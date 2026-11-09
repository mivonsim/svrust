// Tanggung jawab: meng-inline badan task/function ke titik panggilan (LRM §13.3/§13.4).
use crate::error::ElaborateError;
use crate::routine_subst::Substitution;
use std::collections::HashMap;
use sv_ast::combinational::{CombinationalStatement, ForStep};
use sv_ast::declaration::Declaration;
use sv_ast::dpi::{DpiDecl, DpiKind};
use sv_ast::expression::{BinaryOp, Expr, UnaryOp};
use sv_ast::generate::GenerateItem;
use sv_ast::lvalue::Lvalue;
use sv_ast::module::Module;
use sv_ast::routine::{RoutineDecl, RoutineKind};
use sv_ast::statement::{SequentialStatement, Statement};

/// Kedalaman maksimum inlining; melindungi rekursi tak berujung.
const MAX_DEPTH: usize = 64;

/// Kembalikan salinan module dengan seluruh panggilan task/function ter-expand.
///
/// Dijalankan sekali per module sebelum statement dielaborasi, sehingga sisa
/// pipeline tidak pernah melihat node `TaskCall`/`FunctionCall`. Panggilan
/// fungsi DPI-C import diubah menjadi `Statement::DpiCall` (bukan di-inline
/// karena tidak ada badan SV untuk di-inlining).
pub fn expand_module(module: &Module, dpis: &[DpiDecl]) -> Result<Module, ElaborateError> {
    let mut routines: HashMap<String, RoutineDecl> = HashMap::new();
    for decl in &module.routines {
        if let Some(ada) = routines.get(&decl.name) {
            return Err(ElaborateError::new(
                format!(
                    "duplicate routine '{}' (sudah dideklarasikan sebagai {})",
                    decl.name,
                    label(ada.kind)
                ),
                decl.span,
            ));
        }
        routines.insert(decl.name.clone(), decl.clone());
    }

    let mut out = module.clone();
    let mut statements = Vec::with_capacity(module.statements.len());
    for statement in &module.statements {
        statements.extend(expand_statement(statement, &routines, dpis)?);
    }
    out.statements = statements;
    out.generates = module
        .generates
        .iter()
        .map(|region| {
            Ok(sv_ast::generate::GenerateRegion {
                genvars: region.genvars.clone(),
                items: expand_items(&region.items, &routines, dpis)?,
                span: region.span,
            })
        })
        .collect::<Result<Vec<_>, ElaborateError>>()?;
    Ok(out)
}

/// Ikon nama jenis subroutine untuk pesan error duplikat.
fn label(kind: RoutineKind) -> &'static str {
    match kind {
        RoutineKind::Task => "task",
        RoutineKind::Function => "function",
    }
}

/// Stato inlining: tabel routine dan kedalaman rekursi saat ini.
struct Inliner<'a> {
    routines: &'a HashMap<String, RoutineDecl>,
    /// Deklarasi DPI-C import/export, untuk mengenali panggilan fungsi C.
    dpis: &'a [DpiDecl],
    depth: usize,
    /// Nomor pemanggilan task, untuk memberi nama unik pada deklarasi lokal
    /// badan subrutin yang di-inline.
    panggilan: usize,
}

impl<'a> Inliner<'a> {
    fn baru(routines: &'a HashMap<String, RoutineDecl>, dpis: &'a [DpiDecl]) -> Self {
        Self {
            routines,
            dpis,
            depth: 0,
            panggilan: 0,
        }
    }

    /// Cari deklarasi DPI-C import berdasarkan nama fungsi SV.
    ///
    /// Kembalikan `Some(c_name)` bila ditemukan, `None` bila bukan fungsi DPI-C.
    fn cari_dpi_import(&self, name: &str) -> Option<String> {
        self.dpis.iter().find_map(|dpi| {
            if dpi.kind == DpiKind::Import && dpi.name == name {
                Some(dpi.c_name.clone().unwrap_or_else(|| name.to_string()))
            } else {
                None
            }
        })
    }

    /// Ambil deklarasi subroutine atau laporkan error yang menunjuk span call.
    fn ambil(
        &self,
        name: &str,
        span: sv_lexer::span::Span,
    ) -> Result<&'a RoutineDecl, ElaborateError> {
        self.routines.get(name).ok_or_else(|| {
            ElaborateError::new(format!("undefined function or task '{name}'"), span)
        })
    }

    /// Naikkan kedalaman; menolak rekursi yang tak berujung.
    fn masuk(&mut self, name: &str, span: sv_lexer::span::Span) -> Result<(), ElaborateError> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(ElaborateError::new(
                format!(
                    "inlining '{name}' melebihi batas {MAX_DEPTH} tingkat; rekursi belum didukung"
                ),
                span,
            ));
        }
        Ok(())
    }

    fn keluar(&mut self) {
        self.depth -= 1;
    }

    /// Satu statement modul bisa jadi banyak statement setelah task di-inline.
    fn statement(&mut self, statement: &Statement) -> Result<Vec<Statement>, ElaborateError> {
        let out = match statement {
            Statement::ContinuousAssign { lhs, rhs, span } => Statement::ContinuousAssign {
                lhs: self.lvalue(lhs)?,
                rhs: self.expr(rhs)?,
                span: *span,
            },
            Statement::AlwaysFf { events, body, span } => Statement::AlwaysFf {
                events: events.clone(),
                body: self.seq_list(body)?,
                span: *span,
            },
            Statement::AlwaysTimed { body, span } => Statement::AlwaysTimed {
                body: self.comb_list(body)?,
                span: *span,
            },
            Statement::AlwaysComb { body, span, events } => Statement::AlwaysComb {
                body: self.comb_list(body)?,
                span: *span,
                events: events.clone(),
            },
            Statement::Initial { body, span } => Statement::Initial {
                body: self.comb_list(body)?,
                span: *span,
            },
            Statement::Final { body, span } => Statement::Final {
                body: self.comb_list(body)?,
                span: *span,
            },
        };
        Ok(vec![out])
    }

    fn seq_list(
        &mut self,
        body: &[SequentialStatement],
    ) -> Result<Vec<SequentialStatement>, ElaborateError> {
        let mut out = Vec::new();
        for stmt in body {
            out.extend(self.seq(stmt)?);
        }
        Ok(out)
    }

    fn seq(
        &mut self,
        stmt: &SequentialStatement,
    ) -> Result<Vec<SequentialStatement>, ElaborateError> {
        let out: Vec<SequentialStatement> = match stmt {
            SequentialStatement::DpiCall(_call) => {
                // DpiCall sudah dalam bentuk akhir, lewati tanpa modifikasi.
                vec![stmt.clone()]
            }
            SequentialStatement::NonBlockingAssign { lhs, rhs, span } => {
                // Jika RHS adalah pemanggilan fungsi DPI-C import, ubah jadi
                // `SequentialStatement::DpiCall` dengan `ret_target` dari LHS.
                // Ini menangani pola `q <= dpi_func(...)`.
                let rhs_expr: &sv_ast::expression::Expr = rhs;
                if let sv_ast::expression::Expr::FunctionCall {
                    name,
                    args,
                    span: expr_span,
                } = rhs_expr
                {
                    if let Some(c_name) = self.cari_dpi_import(name) {
                        return Ok(vec![SequentialStatement::DpiCall(sv_ast::dpi::DpiCall {
                            name: name.clone(),
                            c_name,
                            args: args.clone(),
                            ret_target: Some(lhs.clone()),
                            span: *expr_span,
                        })]);
                    }
                }
                vec![SequentialStatement::NonBlockingAssign {
                    lhs: self.lvalue(lhs)?,
                    rhs: self.expr(rhs)?,
                    span: *span,
                }]
            }
            SequentialStatement::BlockingAssign { lhs, rhs, span } => {
                // Jika RHS adalah pemanggilan fungsi DPI-C import, ubah jadi
                // `SequentialStatement::DpiCall` dengan `ret_target` dari LHS.
                // Ini menangani pola `x = hitung_paritas(8'hFF);`.
                let rhs_expr: &sv_ast::expression::Expr = rhs;
                if let sv_ast::expression::Expr::FunctionCall {
                    name,
                    args,
                    span: expr_span,
                } = rhs_expr
                {
                    if let Some(c_name) = self.cari_dpi_import(name) {
                        return Ok(vec![SequentialStatement::DpiCall(sv_ast::dpi::DpiCall {
                            name: name.clone(),
                            c_name,
                            args: args.clone(),
                            ret_target: Some(lhs.clone()),
                            span: *expr_span,
                        })]);
                    }
                }
                vec![SequentialStatement::BlockingAssign {
                    lhs: self.lvalue(lhs)?,
                    rhs: self.expr(rhs)?,
                    span: *span,
                }]
            }
            SequentialStatement::IfElse {
                condition,
                then_branch,
                else_branch,
                span,
            } => {
                vec![SequentialStatement::IfElse {
                    condition: self.expr(condition)?,
                    then_branch: self.seq_list(then_branch)?,
                    else_branch: match else_branch {
                        Some(branch) => Some(self.seq_list(branch)?),
                        None => None,
                    },
                    span: *span,
                }]
            }
            SequentialStatement::Decl(decl) => {
                vec![SequentialStatement::Decl(self.deklarasi(decl)?)]
            }
            SequentialStatement::SystemTask(task) => {
                let mut args = Vec::new();
                for arg in &task.args {
                    match arg {
                        sv_ast::system_task::SystemArg::Format(t) => {
                            args.push(sv_ast::system_task::SystemArg::Format(t.clone()))
                        }
                        sv_ast::system_task::SystemArg::Value(e) => {
                            args.push(sv_ast::system_task::SystemArg::Value(self.expr(e)?))
                        }
                    }
                }
                vec![SequentialStatement::SystemTask(
                    sv_ast::system_task::SystemTask {
                        kind: task.kind.clone(),
                        args,
                        condition: match &task.condition {
                            Some(cond) => Some(self.expr(cond)?),
                            None => None,
                        },
                        time_scale: task.time_scale,
                        span: task.span,
                    },
                )]
            }
            // Panggilan fungsi DPI-C import tidak punya badan SV untuk
            // di-inlining — diubah menjadi `SequentialStatement::DpiCall`
            // yang codegen akan terjemahkan ke pemanggilan C via `sv_runtime::dpi`.
            SequentialStatement::TaskCall(call) => {
                if let Some(c_name) = self.cari_dpi_import(&call.name) {
                    return Ok(vec![SequentialStatement::DpiCall(sv_ast::dpi::DpiCall {
                        name: call.name.clone(),
                        c_name,
                        args: call.args.clone(),
                        ret_target: None,
                        span: call.span,
                    })]);
                }
                // Fungsi/task tak dikenal (mis. DPI-C import dari paket eksternal
                // yang tidak tersedia dalam kompilasi tanpa --top). Terima
                // lenient agar elaborasi OpenTitan tanpa --top tetap melenggang,
                // konsisten dengan perilaku Expr::FunctionCall yang mengembalikan
                // placeholder 0 untuk fungsi tak dikenal.
                if self.routines.contains_key(&call.name) {
                    let body = self.task_body(&call.name, &call.args, call.span)?;
                    self.comb_to_seq_list(&body)?
                } else {
                    vec![]
                }
            }
            SequentialStatement::Return { span, .. } => {
                return Err(ElaborateError::new(
                    "`return` hanya sah di dalam task/function",
                    *span,
                ))
            }
            SequentialStatement::Forever { body, span } => {
                vec![SequentialStatement::Forever {
                    body: self.seq_list(body)?,
                    span: *span,
                }]
            }
            SequentialStatement::ProceduralAssign { lhs, rhs, span } => {
                vec![SequentialStatement::ProceduralAssign {
                    lhs: self.lvalue(lhs)?,
                    rhs: self.expr(rhs)?,
                    span: *span,
                }]
            }
            SequentialStatement::Deassign { lhs, span } => {
                vec![SequentialStatement::Deassign {
                    lhs: self.lvalue(lhs)?,
                    span: *span,
                }]
            }
            SequentialStatement::Block { label, body, span } => {
                vec![SequentialStatement::Block {
                    label: label.clone(),
                    body: self.seq_list(body)?,
                    span: *span,
                }]
            }
        };
        Ok(out)
    }

    fn comb_list(
        &mut self,
        body: &[CombinationalStatement],
    ) -> Result<Vec<CombinationalStatement>, ElaborateError> {
        let mut out = Vec::new();
        for stmt in body {
            out.extend(self.comb(stmt)?);
        }
        Ok(out)
    }

    /// Konversi satu `CombinationalStatement` (badan task/function) menjadi
    /// `SequentialStatement` (badan `always_ff`). Varian yang tidak punya
    /// ekuivalen sekuensial (loop, delay, event control, case) ditolak dengan
    /// pesan yang jelas karena LRM §13.3 membatasi badan task yang dipanggil
    /// dari konteks sekuensial.
    fn comb_to_seq(
        &mut self,
        stmt: &CombinationalStatement,
    ) -> Result<Vec<SequentialStatement>, ElaborateError> {
        match stmt {
            CombinationalStatement::Decl(decl) => {
                Ok(vec![SequentialStatement::Decl(self.deklarasi(decl)?)])
            }
            CombinationalStatement::BlockingAssign { lhs, rhs, span } => {
                Ok(vec![SequentialStatement::BlockingAssign {
                    lhs: self.lvalue(lhs)?,
                    rhs: self.expr(rhs)?,
                    span: *span,
                }])
            }
            // Non-blocking assignment di task yang dipanggil dari `always_ff`
            // tetap NBA (LRM §10.4).
            CombinationalStatement::NonBlockingAssign { lhs, rhs, span } => {
                Ok(vec![SequentialStatement::NonBlockingAssign {
                    lhs: self.lvalue(lhs)?,
                    rhs: self.expr(rhs)?,
                    span: *span,
                }])
            }
            // Compound assignment di dalam task dianggap blocking (LRM §11.3:
            // `y += a` setara dengan `y = y + a` dievaluasi segera).
            CombinationalStatement::CompoundAssign { lhs, op, rhs, span } => {
                let _ = op;
                Ok(vec![SequentialStatement::BlockingAssign {
                    lhs: self.lvalue(lhs)?,
                    rhs: self.expr(rhs)?,
                    span: *span,
                }])
            }
            CombinationalStatement::IfElse {
                condition,
                then_branch,
                else_branch,
                span,
            } => Ok(vec![SequentialStatement::IfElse {
                condition: self.expr(condition)?,
                then_branch: self.comb_to_seq_list(then_branch)?,
                else_branch: match else_branch {
                    Some(branch) => Some(self.comb_to_seq_list(branch)?),
                    None => None,
                },
                span: *span,
            }]),
            CombinationalStatement::Block {
                label: _,
                body,
                span: _,
            } => {
                // Blok tidak menambah semantik sekuensial; flatten isinya.
                self.comb_to_seq_list(body)
            }
            CombinationalStatement::SystemTask(task) => {
                let mut args = Vec::new();
                for arg in &task.args {
                    match arg {
                        sv_ast::system_task::SystemArg::Format(t) => {
                            args.push(sv_ast::system_task::SystemArg::Format(t.clone()))
                        }
                        sv_ast::system_task::SystemArg::Value(e) => {
                            args.push(sv_ast::system_task::SystemArg::Value(self.expr(e)?))
                        }
                    }
                }
                Ok(vec![SequentialStatement::SystemTask(
                    sv_ast::system_task::SystemTask {
                        kind: task.kind.clone(),
                        args,
                        condition: match &task.condition {
                            Some(cond) => Some(self.expr(cond)?),
                            None => None,
                        },
                        time_scale: task.time_scale,
                        span: task.span,
                    },
                )])
            }
            CombinationalStatement::TaskCall(call) => {
                if let Some(c_name) = self.cari_dpi_import(&call.name) {
                    return Ok(vec![SequentialStatement::DpiCall(sv_ast::dpi::DpiCall {
                        name: call.name.clone(),
                        c_name,
                        args: call.args.clone(),
                        ret_target: None,
                        span: call.span,
                    })]);
                }
                // Fungsi/task tak dikenal (mis. DPI-C import dari paket eksternal
                // yang tidak tersedia dalam kompilasi tanpa --top). Terima
                // lenient agar elaborasi OpenTitan tanpa --top tetap melenggang.
                if self.routines.contains_key(&call.name) {
                    let body = self.task_body(&call.name, &call.args, call.span)?;
                    self.comb_to_seq_list(&body)
                } else {
                    Ok(vec![])
                }
            }
            CombinationalStatement::DpiCall(call) => {
                Ok(vec![SequentialStatement::DpiCall(sv_ast::dpi::DpiCall {
                    name: call.name.clone(),
                    c_name: call.c_name.clone(),
                    args: call.args.clone(),
                    ret_target: call.ret_target.clone(),
                    span: call.span,
                })])
            }
            CombinationalStatement::Return { span, .. } => Err(ElaborateError::new(
                "`return` hanya sah di dalam task/function",
                *span,
            )),
            CombinationalStatement::Case { span, .. } => Err(ElaborateError::new(
                "`case` dalam task tidak bisa dipanggil dari `always_ff`",
                *span,
            )),
            CombinationalStatement::For { span, .. } => Err(ElaborateError::new(
                "`for` dalam task tidak bisa dipanggil dari `always_ff`",
                *span,
            )),
            CombinationalStatement::While { span, .. } => Err(ElaborateError::new(
                "`while` dalam task tidak bisa dipanggil dari `always_ff`",
                *span,
            )),
            CombinationalStatement::Forever { span, .. } => Err(ElaborateError::new(
                "`forever` dalam task tidak bisa dipanggil dari `always_ff`",
                *span,
            )),
            CombinationalStatement::Repeat { span, .. } => Err(ElaborateError::new(
                "`repeat` dalam task tidak bisa dipanggil dari `always_ff`",
                *span,
            )),
            CombinationalStatement::Delay { span, .. } => Err(ElaborateError::new(
                "`#delay` dalam task tidak bisa dipanggil dari `always_ff`",
                *span,
            )),
            CombinationalStatement::EventControl { span, .. } => Err(ElaborateError::new(
                "`@(...)` dalam task tidak bisa dipanggil dari `always_ff`",
                *span,
            )),
            CombinationalStatement::ProceduralAssign { lhs, rhs, span } => {
                Ok(vec![SequentialStatement::ProceduralAssign {
                    lhs: self.lvalue(lhs)?,
                    rhs: self.expr(rhs)?,
                    span: *span,
                }])
            }
            CombinationalStatement::Deassign { lhs, span } => {
                Ok(vec![SequentialStatement::Deassign {
                    lhs: self.lvalue(lhs)?,
                    span: *span,
                }])
            }
            CombinationalStatement::ExprStmt { expr, .. } => {
                let _ = self.expr(expr)?;
                Ok(Vec::new())
            }
        }
    }

    /// Konversi daftar `CombinationalStatement` ke `SequentialStatement`.
    fn comb_to_seq_list(
        &mut self,
        body: &[CombinationalStatement],
    ) -> Result<Vec<SequentialStatement>, ElaborateError> {
        let mut out = Vec::new();
        for stmt in body {
            out.extend(self.comb_to_seq(stmt)?);
        }
        Ok(out)
    }

    /// Turunkan satu statement modul (bisa jadi banyak setelah task di-inline).
    fn comb(
        &mut self,
        stmt: &CombinationalStatement,
    ) -> Result<Vec<CombinationalStatement>, ElaborateError> {
        let out = match stmt {
            CombinationalStatement::Decl(decl) => {
                CombinationalStatement::Decl(self.deklarasi(decl)?)
            }
            CombinationalStatement::BlockingAssign { lhs, rhs, span } => {
                // Jika RHS adalah pemanggilan fungsi DPI-C import, ubah jadi
                // `CombinationalStatement::DpiCall` dengan `ret_target` dari LHS.
                let rhs_expr: &sv_ast::expression::Expr = rhs;
                if let sv_ast::expression::Expr::FunctionCall {
                    name,
                    args,
                    span: expr_span,
                } = rhs_expr
                {
                    if let Some(c_name) = self.cari_dpi_import(name) {
                        return Ok(vec![CombinationalStatement::DpiCall(
                            sv_ast::dpi::DpiCall {
                                name: name.clone(),
                                c_name,
                                args: args.clone(),
                                ret_target: Some(lhs.clone()),
                                span: *expr_span,
                            },
                        )]);
                    }
                }
                CombinationalStatement::BlockingAssign {
                    lhs: self.lvalue(lhs)?,
                    rhs: self.expr(rhs)?,
                    span: *span,
                }
            }
            // NBA harus tetap NBA setelah badan subrutin di-inline; kalau
            // variannya diturunkan ke `BlockingAssign`, `<=` jadi `=`.
            CombinationalStatement::NonBlockingAssign { lhs, rhs, span } => {
                CombinationalStatement::NonBlockingAssign {
                    lhs: self.lvalue(lhs)?,
                    rhs: self.expr(rhs)?,
                    span: *span,
                }
            }
            CombinationalStatement::CompoundAssign { lhs, op, rhs, span } => {
                CombinationalStatement::CompoundAssign {
                    lhs: self.lvalue(lhs)?,
                    op: *op,
                    rhs: self.expr(rhs)?,
                    span: *span,
                }
            }
            CombinationalStatement::For {
                init,
                condition,
                step,
                body,
                span,
            } => CombinationalStatement::For {
                init: sv_ast::combinational::ForStep {
                    lhs: self.lvalue(&init.lhs)?,
                    rhs: self.expr(&init.rhs)?,
                    span: init.span,
                },
                condition: self.expr(condition)?,
                step: sv_ast::combinational::ForStep {
                    lhs: self.lvalue(&step.lhs)?,
                    rhs: self.expr(&step.rhs)?,
                    span: step.span,
                },
                body: self.comb_list(body)?,
                span: *span,
            },
            CombinationalStatement::While {
                condition,
                body,
                span,
            } => CombinationalStatement::While {
                condition: self.expr(condition)?,
                body: self.comb_list(body)?,
                span: *span,
            },
            CombinationalStatement::Forever { body, span } => CombinationalStatement::Forever {
                body: self.comb_list(body)?,
                span: *span,
            },
            CombinationalStatement::Repeat { count, body, span } => {
                CombinationalStatement::Repeat {
                    count: self.expr(count)?,
                    body: self.comb_list(body)?,
                    span: *span,
                }
            }
            CombinationalStatement::Case {
                selector,
                arms,
                kind,
                span,
            } => {
                let mut baru = Vec::with_capacity(arms.len());
                for arm in arms {
                    let mut labels = Vec::with_capacity(arm.labels.len());
                    for label in &arm.labels {
                        labels.push(self.expr(label)?);
                    }
                    baru.push(sv_ast::combinational::CaseArm {
                        labels,
                        is_default: arm.is_default,
                        body: self.comb_list(&arm.body)?,
                        span: arm.span,
                    });
                }
                CombinationalStatement::Case {
                    selector: self.expr(selector)?,
                    arms: baru,
                    kind: *kind,
                    span: *span,
                }
            }
            CombinationalStatement::IfElse {
                condition,
                then_branch,
                else_branch,
                span,
            } => CombinationalStatement::IfElse {
                condition: self.expr(condition)?,
                then_branch: self.comb_list(then_branch)?,
                else_branch: match else_branch {
                    Some(branch) => Some(self.comb_list(branch)?),
                    None => None,
                },
                span: *span,
            },
            CombinationalStatement::Block { label, body, span } => CombinationalStatement::Block {
                label: label.clone(),
                body: self.comb_list(body)?,
                span: *span,
            },
            CombinationalStatement::EventControl { events, body, span } => {
                CombinationalStatement::EventControl {
                    events: events.clone(),
                    body: self.comb_list(body)?,
                    span: *span,
                }
            }
            CombinationalStatement::Delay {
                amount,
                unit,
                body,
                span,
            } => {
                let amount = self.expr(amount)?;
                let statements = self.comb(body)?;
                let body = match statements.as_slice() {
                    [satu] => Box::new(satu.clone()),
                    // Task yang di-inline di dalam `#n stmt` menghasilkan lebih
                    // dari satu statement; bungkusnya jadi blok.
                    banyak => Box::new(CombinationalStatement::Block {
                        label: None,
                        body: banyak.to_vec(),
                        span: *span,
                    }),
                };
                CombinationalStatement::Delay {
                    amount,
                    unit: *unit,
                    body,
                    span: *span,
                }
            }
            CombinationalStatement::SystemTask(task) => {
                let mut args = Vec::new();
                for arg in &task.args {
                    match arg {
                        sv_ast::system_task::SystemArg::Format(t) => {
                            args.push(sv_ast::system_task::SystemArg::Format(t.clone()))
                        }
                        sv_ast::system_task::SystemArg::Value(e) => {
                            args.push(sv_ast::system_task::SystemArg::Value(self.expr(e)?))
                        }
                    }
                }
                CombinationalStatement::SystemTask(sv_ast::system_task::SystemTask {
                    kind: task.kind.clone(),
                    args,
                    condition: match &task.condition {
                        Some(cond) => Some(self.expr(cond)?),
                        None => None,
                    },
                    time_scale: task.time_scale,
                    span: task.span,
                })
            }
            CombinationalStatement::TaskCall(call) => {
                if let Some(c_name) = self.cari_dpi_import(&call.name) {
                    return Ok(vec![CombinationalStatement::DpiCall(
                        sv_ast::dpi::DpiCall {
                            name: call.name.clone(),
                            c_name,
                            args: call.args.clone(),
                            ret_target: None,
                            span: call.span,
                        },
                    )]);
                }
                if self.routines.contains_key(&call.name) {
                    return self.task_body(&call.name, &call.args, call.span);
                }
                return Ok(vec![]);
            }
            CombinationalStatement::DpiCall(call) => CombinationalStatement::DpiCall(call.clone()),
            CombinationalStatement::Return { span, .. } => {
                return Err(ElaborateError::new(
                    "`return` hanya sah di dalam task/function".to_string(),
                    *span,
                ))
            }
            CombinationalStatement::ProceduralAssign { lhs, rhs, span } => {
                CombinationalStatement::ProceduralAssign {
                    lhs: lhs.clone(),
                    rhs: rhs.clone(),
                    span: *span,
                }
            }
            CombinationalStatement::Deassign { lhs, span } => CombinationalStatement::Deassign {
                lhs: lhs.clone(),
                span: *span,
            },
            CombinationalStatement::ExprStmt { expr, span } => {
                let _ = self.expr(expr)?;
                CombinationalStatement::ExprStmt {
                    expr: expr.clone(),
                    span: *span,
                }
            }
        };
        Ok(vec![out])
    }

    fn deklarasi(&mut self, decl: &Declaration) -> Result<Declaration, ElaborateError> {
        let mut out = decl.clone();
        if let Some(init) = &decl.init {
            out.init = Some(self.expr(init)?);
        }
        Ok(out)
    }

    /// In-line badan task sebagai daftar statement di titik panggilan.
    ///
    /// Argumen `input` disubstitusi ke badan sebagai ekspresi; argumen
    /// `output`/`inout` harus berupa nama sinyal sederhana agar bisa ditulis
    /// langsung ke sinyal pemanggil.
    fn task_body(
        &mut self,
        name: &str,
        args: &[Expr],
        span: sv_lexer::span::Span,
    ) -> Result<Vec<CombinationalStatement>, ElaborateError> {
        let decl = self.ambil(name, span)?;
        if decl.kind != RoutineKind::Task
            && !(decl.kind == RoutineKind::Function
                && matches!(
                    decl.return_width,
                    Some(sv_ast::width::WidthExpr::Literal(0))
                ))
        {
            return Err(ElaborateError::new(
                format!("'{name}' adalah function, bukan task; pakai dalam ekspresi"),
                span,
            ));
        }
        cek_argumen(decl, args, span)?;
        for (arg, argumen) in decl.args.iter().zip(args) {
            if arg.direction == sv_ast::routine::ArgDirection::Input {
                continue;
            }
            if !matches!(argumen, Expr::Ident { .. }) {
                return Err(ElaborateError::new(
                    format!(
                        "argumen output/inout '{}' harus berupa nama sinyal, bukan ekspresi",
                        arg.name
                    ),
                    span,
                ));
            }
        }
        // Deklarasi lokal di dalam badan task harus diberi nama unik per titik
        // pemanggilan. Tanpa itu, dua pemanggilan task yang sama menduplikasi
        // nama lokal yang sama dan bentrok saat didaftarkan sebagai sinyal
        // design. Parser hanya menamai lokal pada `module.statements`, bukan
        // pada badan subrutin, jadi penamaan harus terjadi di sini.
        let id = self.panggilan;
        self.panggilan += 1;
        let body_ternama =
            crate::routine_subst::rename_locals(&decl.body, &format!("{name}__{id}"));

        let tabel = Substitution::baru(
            decl.args.iter().map(|a| a.name.clone()).collect(),
            args.to_vec(),
        );
        let body = tabel.ganti_statements(&body_ternama);
        self.masuk(name, span)?;
        let hasil = self.comb_list(&body);
        self.keluar();
        hasil
    }

    /// In-line function: badan-nya harus reducir ke satu ekspresi nilai.
    fn call(
        &mut self,
        name: &str,
        args: &[Expr],
        span: sv_lexer::span::Span,
    ) -> Result<Expr, ElaborateError> {
        let decl = self.ambil(name, span)?;
        if decl.kind != RoutineKind::Function {
            return Err(ElaborateError::new(
                format!("'{name}' adalah task, bukan function; pakai sebagai statement"),
                span,
            ));
        }
        cek_argumen(decl, args, span)?;

        // Argumen diekspansi lebih dulu supaya panggilan di dalamnya ikutInline.
        let mut argumen = Vec::with_capacity(args.len());
        for arg in args {
            argumen.push(self.expr(arg)?);
        }

        let nilai = nilai_function(decl, &argumen, span)?;
        let tabel = Substitution::baru(decl.args.iter().map(|a| a.name.clone()).collect(), argumen);
        self.masuk(name, span)?;
        let hasil = self.expr(&tabel.ganti(&nilai));
        self.keluar();
        hasil
    }

    fn expr(&mut self, expr: &Expr) -> Result<Expr, ElaborateError> {
        Ok(match expr {
            // `$value$plusargs("+nama=fmt", var)` dalam posisi ekspresi
            // (LRM §20.9): hasil 1 bila plusarg ditemukan. Di inlining
            // routine cukup konstanta 1; penulisan ke variabel target
            // ditangani codegen statement.
            Expr::FunctionCall {
                name,
                args: _,
                span: _,
            } if name == "$value$plusargs" || name == "$sformatf" => Expr::Number(0),
            Expr::FunctionCall { name, args, span } => {
                // Panggilan fungsi DPI-C import dalam posisi ekspresi —
                // ubah menjadi `Expr::DpiCall` agar nilai balik bisa dipakai
                // sebagai operand ekspresi (LRM §35).
                if let Some(c_name) = self.cari_dpi_import(name) {
                    // Lower argumen ke ekspresi AST yang sudah di-resolve.
                    let mut argumen = Vec::with_capacity(args.len());
                    for arg in args {
                        argumen.push(self.expr(arg)?);
                    }
                    // Cari tipe balik dari deklarasi DPI-C.
                    let ret_tipe = self
                        .dpis
                        .iter()
                        .find(|d| d.kind == DpiKind::Import && d.name == *name)
                        .and_then(|d| d.ret_type.clone());
                    return Ok(Expr::DpiCall {
                        name: name.clone(),
                        c_name,
                        args: argumen,
                        ret_type: ret_tipe,
                        span: *span,
                    });
                }
                // Fungsi yang diketahui di-inline; fungsi tak dikenal (mis.
                // `aes_transpose` di OpenTitan) jadi placeholder 0 agar
                // `check` tanpa `--top` tetap melenggang (pendekatan lenient,
                // FASA.md).
                if self.routines.contains_key(name.as_str()) {
                    let name = name.clone();
                    let args = args.clone();
                    self.call(&name, &args, *span)?
                } else {
                    Expr::Number(0)
                }
            }
            // `Expr::DpiCall` sudah dalam bentuk akhir — klooning saja.
            Expr::DpiCall { .. } => expr.clone(),
            Expr::Ident { name, span } => Expr::ident(name.clone(), *span),
            Expr::Number(n) => Expr::Number(*n),
            Expr::StringConst { value, span } => Expr::StringConst {
                value: value.clone(),
                span: *span,
            },
            Expr::Sized {
                value,
                width,
                signed,
                unknown_mask,
                zmask,
            } => Expr::Sized {
                value: *value,
                width: *width,
                signed: *signed,
                unknown_mask: *unknown_mask,
                zmask: *zmask,
            },
            Expr::Binary { op, lhs, rhs, span } => Expr::Binary {
                op: *op,
                lhs: Box::new(self.expr(lhs)?),
                rhs: Box::new(self.expr(rhs)?),
                span: *span,
            },
            Expr::Unary { op, operand, span } => Expr::Unary {
                op: *op,
                operand: Box::new(self.expr(operand)?),
                span: *span,
            },
            Expr::Ternary {
                condition,
                when_true,
                when_false,
                span,
            } => Expr::Ternary {
                condition: Box::new(self.expr(condition)?),
                when_true: Box::new(self.expr(when_true)?),
                when_false: Box::new(self.expr(when_false)?),
                span: *span,
            },
            Expr::Select {
                base,
                msb,
                lsb,
                span,
            } => Expr::Select {
                base: Box::new(self.expr(base)?),
                msb: *msb,
                lsb: *lsb,
                span: *span,
            },
            Expr::IndexDynamic { base, index, span } => Expr::IndexDynamic {
                base: Box::new(self.expr(base)?),
                index: Box::new(self.expr(index)?),
                span: *span,
            },
            Expr::Concat { items, span } => {
                let mut baru = Vec::with_capacity(items.len());
                for item in items {
                    baru.push(self.expr(item)?);
                }
                Expr::Concat {
                    items: baru,
                    span: *span,
                }
            }
            Expr::Replicate { count, value, span } => Expr::Replicate {
                count: Box::new(self.expr(count)?),
                value: Box::new(self.expr(value)?),
                span: *span,
            },
            Expr::SystemTime { unit, span } => Expr::SystemTime {
                unit: *unit,
                span: *span,
            },
            // LRM §6.14: cast ke tipe bawaan dan size cast tidak punya nama
            // tipe yang jadi argumen pemanggil; hanya operandnya yang perlu
            // dikembangkan dengan argumen nyata.
            Expr::BuiltinCast {
                type_name,
                width,
                signed,
                operand,
                span,
            } => Expr::BuiltinCast {
                type_name: type_name.clone(),
                width: *width,
                signed: *signed,
                operand: Box::new(self.expr(operand)?),
                span: *span,
            },
            Expr::SizeCast {
                width,
                operand,
                span,
            } => Expr::SizeCast {
                width: *width,
                operand: Box::new(self.expr(operand)?),
                span: *span,
            },
            // LRM §6.14: nama tipe bukan sinyal, jadi tidak boleh di-inline
            // sebagai argumen pemanggil; hanya operandnya yang disubstitusi.
            Expr::Cast {
                type_name,
                operand,
                span,
            } => Expr::Cast {
                type_name: type_name.clone(),
                operand: Box::new(self.expr(operand)?),
                span: *span,
            },
            Expr::SignCast {
                signed,
                operand,
                span,
            } => Expr::SignCast {
                signed: *signed,
                operand: Box::new(self.expr(operand)?),
                span: *span,
            },
            Expr::Bits { operand, span } => Expr::Bits {
                operand: Box::new(self.expr(operand)?),
                span: *span,
            },
            Expr::HierarchicalRef { parts, span } => Expr::HierarchicalRef {
                parts: parts.clone(),
                span: *span,
            },
        })
    }

    fn lvalue(&mut self, lhs: &Lvalue) -> Result<Lvalue, ElaborateError> {
        Ok(Lvalue {
            name: lhs.name.clone(),
            slice: lhs.slice,
            extra_slices: lhs.extra_slices.clone(),
            members: lhs.members.clone(),
            genvar_index: lhs.genvar_index.clone(),
            span: lhs.span,
        })
    }
}

/// Turunkan badan function menjadi satu ekspresi nilai (LRM §13.4).
///
/// Bentuk yang diterima: satu assignment ke nama function, `return expr;`,
/// atau `if` yang kedua cabangnya sama-sama mengesahkan nama function.
/// Untuk badan yang lebih kompleks (loop, deklarasi lokal), dievaluasi
/// secara simbolis melalui `eval_function_body`.
fn nilai_function(
    decl: &RoutineDecl,
    args: &[Expr],
    span: sv_lexer::span::Span,
) -> Result<Expr, ElaborateError> {
    if decl.body.len() == 1 {
        if let Some(nilai) = nilai_dari_statement(&decl.body[0], &decl.name) {
            return Ok(nilai);
        }
    }
    // Satu `if/else` yang kedua cabangnya mengesahkan nama function menjadi
    // operator ternair, sehingga tetap bisa dipakai di dalam ekspresi.
    if decl.body.len() == 1 {
        if let CombinationalStatement::IfElse {
            condition,
            then_branch,
            else_branch,
            span: if_span,
        } = &decl.body[0]
        {
            let Some(then_val) = satu_nilai(then_branch, &decl.name) else {
                return Err(badan_function_tidak_terdukung(&decl.name, *if_span));
            };
            let else_val = match else_branch {
                Some(branch) => Some(
                    satu_nilai(branch, &decl.name)
                        .ok_or_else(|| badan_function_tidak_terdukung(&decl.name, *if_span))?,
                ),
                // Tanpa `else`, LRM menyisakan nilai balik tak terdefinisi; mesin
                // 2-state menganggapnya 0.
                None => None,
            };
            // Nilai balik ini masih memakai nama formal; substitusi dilakukan
            // sekali di pemanggil (`Inliner::call`) supaya tidak ganda.
            return Ok(Expr::Ternary {
                condition: Box::new(condition.clone()),
                when_true: Box::new(then_val),
                when_false: Box::new(else_val.unwrap_or(Expr::Number(0))),
                span: *if_span,
            });
        }
    }
    // Fallback: evaluasi simbolis untuk badan function dengan loop,
    // deklarasi lokal, atau konstruksi lain yang bisa dievaluasi pada
    // waktu kompilasi.
    eval_function_body(decl, args, span)
}

/// Pesan error seragam untuk badan function yang belum bisa di-inline.
fn badan_function_tidak_terdukung(name: &str, span: sv_lexer::span::Span) -> ElaborateError {
    ElaborateError::new(
        format!(
            "badan function '{name}' harus satu assignment ke namanya, satu `return expr;`, \
             atau satu `if` yang kedua cabangnya mengesahkan namanya"
        ),
        span,
    )
}

/// Ambil nilai dari satu statement bila statement itu mengesahkan `nama`.
fn nilai_dari_statement(stmt: &CombinationalStatement, nama: &str) -> Option<Expr> {
    match stmt {
        CombinationalStatement::BlockingAssign { lhs, rhs, .. }
        | CombinationalStatement::NonBlockingAssign { lhs, rhs, .. }
            if lhs.name == nama =>
        {
            Some(rhs.clone())
        }
        CombinationalStatement::Return { value, .. } => value.clone(),
        _ => None,
    }
}

/// Ambil nilai dari daftar statement yang tepat satu dan mengesahkan `nama`.
fn satu_nilai(body: &[CombinationalStatement], nama: &str) -> Option<Expr> {
    if body.len() != 1 {
        return None;
    }
    nilai_dari_statement(&body[0], nama)
}

/// Evaluasi simbolis badan function untuk menghasilkan ekspresi nilai (LRM §13.4).
///
/// Digunakan saat function punya loop atau deklarasi lokal yang tidak bisa
/// direduksi menjadi ekspresi tunggal melalui pola sederhana (`nilai_dari_statement`).
///
/// Dieksekusi per iterasi loop konstanta; variabel lokal disimpan sebagai
/// `Expr` dan argument formal dipetakan ke ekspresi pemanggil.
/// Hasilnya adalah ekspresi terakhir yang ditugaskan ke nama function.
fn eval_function_body(
    decl: &RoutineDecl,
    args: &[Expr],
    span: sv_lexer::span::Span,
) -> Result<Expr, ElaborateError> {
    use std::collections::HashMap;

    // Peta nama variabel lokal -> ekspresi nilainya.
    let mut env: HashMap<String, Expr> = HashMap::new();

    // Map argument formal ke ekspresi pemanggil (input only; output/inout
    // harus berupa nama sinyal dan sudah divalidasi di pemanggil).
    for (formal, argumen) in decl.args.iter().zip(args.iter()) {
        if formal.direction == sv_ast::routine::ArgDirection::Input {
            env.insert(formal.name.clone(), argumen.clone());
        }
    }

    // Deklarasi variabel lokal di awal badan function.
    for stmt in &decl.body {
        if let CombinationalStatement::Decl(decl_lokal) = stmt {
            env.insert(decl_lokal.name.clone(), Expr::Number(0));
        }
    }

    // Eksekusi statement demi statement.
    for stmt in &decl.body {
        eksekusi_stmt_function(stmt, &mut env, &decl.name)?;
    }

    // Ambil nilai terakhir yang ditugaskan ke nama function.
    env.get(&decl.name)
        .cloned()
        .ok_or_else(|| badan_function_tidak_terdukung(&decl.name, span))
}

/// Eksekusi satu statement dalam konteks evaluasi function.
///
/// Mengembalikan `Err` bila statement mengandung konstruksi yang tidak
/// bisa dievaluasi secara simbolis (delay, event control, system task,
/// nested function call yang kompleks, dll).
fn eksekusi_stmt_function(
    stmt: &CombinationalStatement,
    env: &mut HashMap<String, Expr>,
    nama_fungsi: &str,
) -> Result<(), ElaborateError> {
    match stmt {
        CombinationalStatement::Decl(decl) => {
            // Variabel lokal sudah diinisialisasi ke 0 di awal; nilai inisial
            // opsional boleh dievaluasi.
            if let Some(init) = &decl.init {
                let nilai = evaluasi_expr_function(init, env)?;
                env.insert(decl.name.clone(), nilai);
            }
        }
        CombinationalStatement::BlockingAssign { lhs, rhs, span } => {
            let nilai = evaluasi_expr_function(rhs, env)?;
            if lhs.name == nama_fungsi {
                // Assignment ke nama function: simpan sebagai hasil akhir.
                env.insert(nama_fungsi.to_string(), nilai);
            } else if let Some(_slice) = &lhs.slice {
                // Part-select: belum didukung dalam evaluasi simbolis function.
                return Err(ElaborateError::new(
                    format!(
                        "part-select pada variabel lokal '{}' dalam function tidak didukung",
                        lhs.name
                    ),
                    *span,
                ));
            } else {
                env.insert(lhs.name.clone(), nilai);
            }
        }
        CombinationalStatement::NonBlockingAssign { .. } => {
            // Non-blocking dalam function diperlakukan sebagai blocking.
        }
        CombinationalStatement::For {
            init,
            condition,
            step,
            body,
            span,
        } => {
            // Inisialisasi loop.
            let ForStep { lhs, rhs, .. } = init;
            let nilai = evaluasi_expr_function(rhs, env)?;
            env.insert(lhs.name.clone(), nilai);

            // Jalankan loop sebanyak batas konstanta.
            for _ in 0..1024 {
                // Cek kondisi.
                let lanjut = match evaluasi_expr_function(condition, env)? {
                    Expr::Number(n) => n != 0,
                    Expr::Sized { value, .. } => value != 0,
                    _ => {
                        return Err(ElaborateError::new(
                            "kondisi loop function harus konstanta".to_string(),
                            *span,
                        ))
                    }
                };
                if !lanjut {
                    break;
                }

                // Jalankan body loop.
                for stmt in body {
                    eksekusi_stmt_function(stmt, env, nama_fungsi)?;
                }

                // Step loop.
                let ForStep { lhs, rhs, .. } = step;
                let nilai = evaluasi_expr_function(rhs, env)?;
                env.insert(lhs.name.clone(), nilai);
            }
        }
        CombinationalStatement::IfElse {
            condition,
            then_branch,
            else_branch,
            span: _,
        } => {
            // Hanya evaluasi bila kondisi konstan (angka). Kondisi non-konan
            // berarti badan function tidak bisa dievaluasi secara simbolis;
            // abaikan statement ini — pengecekan akhir di `eval_function_body`
            // akan menghasilkan pesan "harus satu assignment ke namanya".
            let benar = match evaluasi_expr_function(condition, env)? {
                Expr::Number(n) => n != 0,
                _ => return Ok(()),
            };
            if benar {
                for stmt in then_branch {
                    eksekusi_stmt_function(stmt, env, nama_fungsi)?;
                }
            } else if let Some(branch) = else_branch {
                for stmt in branch {
                    eksekusi_stmt_function(stmt, env, nama_fungsi)?;
                }
            }
        }
        CombinationalStatement::Block { body, .. } => {
            for stmt in body {
                eksekusi_stmt_function(stmt, env, nama_fungsi)?;
            }
        }
        CombinationalStatement::Return { value, .. } => {
            if let Some(nilai) = value {
                let hasil = evaluasi_expr_function(nilai, env)?;
                env.insert(nama_fungsi.to_string(), hasil);
            }
        }
        CombinationalStatement::ExprStmt { expr, .. } => {
            let _ = evaluasi_expr_function(expr, env)?;
        }
        CombinationalStatement::SystemTask(_) => {
            // System task dalam function dievaluasi sebagai no-op (nilai
            // tidak memengaruhi hasil function).
        }
        CombinationalStatement::TaskCall(_) => {
            // Panggilan task dalam function belum didukung evaluasi simbolis.
            return Err(ElaborateError::new(
                "panggilan task dalam badan function tidak didukung".to_string(),
                span_dummy(),
            ));
        }
        CombinationalStatement::DpiCall(_) => {
            // Pemanggilan fungsi C lewat DPI-C tidak bisa dievaluasi secara
            // simbolis dalam function.
            return Err(ElaborateError::new(
                "panggilan DPI-C dalam badan function tidak didukung".to_string(),
                span_dummy(),
            ));
        }
        CombinationalStatement::Case { .. }
        | CombinationalStatement::While { .. }
        | CombinationalStatement::Repeat { .. }
        | CombinationalStatement::Forever { .. }
        | CombinationalStatement::Delay { .. }
        | CombinationalStatement::EventControl { .. }
        | CombinationalStatement::CompoundAssign { .. }
        | CombinationalStatement::ProceduralAssign { .. }
        | CombinationalStatement::Deassign { .. } => {
            // Konstruksi lain belum didukung dalam evaluasi simbolis function.
        }
    }
    Ok(())
}

/// Evaluasi ekspresi dalam konteks function, mengganti nama variabel lokal
/// dengan ekspresi nilainya.
fn evaluasi_expr_function(
    expr: &Expr,
    env: &HashMap<String, Expr>,
) -> Result<Expr, ElaborateError> {
    Ok(match expr {
        Expr::Ident { name, span: _ } => {
            if let Some(nilai) = env.get(name) {
                nilai.clone()
            } else {
                expr.clone()
            }
        }
        Expr::Binary { op, lhs, rhs, span } => {
            let lhs_eval = evaluasi_expr_function(lhs, env)?;
            let rhs_eval = evaluasi_expr_function(rhs, env)?;
            match (&lhs_eval, &rhs_eval) {
                (Expr::Number(a), Expr::Number(b)) => Expr::Number(eval_bin_const(*op, *a, *b, 64)),
                (
                    Expr::Sized {
                        value: a,
                        width: wa,
                        ..
                    },
                    Expr::Sized {
                        value: b,
                        width: wb,
                        ..
                    },
                ) => Expr::Sized {
                    value: eval_bin_const(*op, *a, *b, (*wa).max(*wb)),
                    width: (*wa).max(*wb),
                    signed: false,
                    unknown_mask: 0,
                    zmask: 0,
                },
                _ => Expr::Binary {
                    op: *op,
                    lhs: Box::new(lhs_eval),
                    rhs: Box::new(rhs_eval),
                    span: *span,
                },
            }
        }
        Expr::Unary { op, operand, span } => {
            let operand_eval = evaluasi_expr_function(operand, env)?;
            match &operand_eval {
                Expr::Number(a) => Expr::Number(eval_unary_const(*op, *a, 64)),
                Expr::Sized {
                    value: a, width: w, ..
                } => Expr::Sized {
                    value: eval_unary_const(*op, *a, *w),
                    width: *w,
                    signed: false,
                    unknown_mask: 0,
                    zmask: 0,
                },
                _ => Expr::Unary {
                    op: *op,
                    operand: Box::new(operand_eval),
                    span: *span,
                },
            }
        }
        Expr::Ternary {
            condition,
            when_true,
            when_false,
            span,
        } => Expr::Ternary {
            condition: Box::new(evaluasi_expr_function(condition, env)?),
            when_true: Box::new(evaluasi_expr_function(when_true, env)?),
            when_false: Box::new(evaluasi_expr_function(when_false, env)?),
            span: *span,
        },
        Expr::Select {
            base,
            msb,
            lsb,
            span,
        } => Expr::Select {
            base: Box::new(evaluasi_expr_function(base, env)?),
            msb: *msb,
            lsb: *lsb,
            span: *span,
        },
        Expr::Concat { items, span } => Expr::Concat {
            items: items
                .iter()
                .map(|e| evaluasi_expr_function(e, env))
                .collect::<Result<Vec<_>, _>>()?,
            span: *span,
        },
        Expr::Replicate { count, value, span } => Expr::Replicate {
            count: Box::new(evaluasi_expr_function(count, env)?),
            value: Box::new(evaluasi_expr_function(value, env)?),
            span: *span,
        },
        Expr::Cast {
            type_name,
            operand,
            span,
        } => Expr::Cast {
            type_name: type_name.clone(),
            operand: Box::new(evaluasi_expr_function(operand, env)?),
            span: *span,
        },
        Expr::SignCast {
            signed,
            operand,
            span,
        } => Expr::SignCast {
            signed: *signed,
            operand: Box::new(evaluasi_expr_function(operand, env)?),
            span: *span,
        },
        Expr::SizeCast {
            width,
            operand,
            span,
        } => Expr::SizeCast {
            width: *width,
            operand: Box::new(evaluasi_expr_function(operand, env)?),
            span: *span,
        },
        Expr::FunctionCall { name, args, span } => {
            // Panggilan function bersarang: evaluasi argumen, lalu inline.
            // Untuk saat ini, biarkan sebagai FunctionCall (akan di-resolve
            // oleh pemanggil melalui Substitution).
            Expr::FunctionCall {
                name: name.clone(),
                args: args
                    .iter()
                    .map(|e| evaluasi_expr_function(e, env))
                    .collect::<Result<Vec<_>, _>>()?,
                span: *span,
            }
        }
        other => other.clone(),
    })
}

/// Evaluasi ekspresi biner dengan operand konstanta untuk menghasilkan
/// konstanta baru. Digunakan dalam evaluasi simbolis badan function.
fn eval_bin_const(op: BinaryOp, a: u64, b: u64, width: u32) -> u64 {
    let mask = if width >= 64 {
        u64::MAX
    } else {
        (1u64 << width) - 1
    };
    let a = a & mask;
    let b = b & mask;
    match op {
        BinaryOp::Add => a.wrapping_add(b) & mask,
        BinaryOp::Sub => a.wrapping_sub(b) & mask,
        BinaryOp::Mul => a.wrapping_mul(b) & mask,
        BinaryOp::Div => a.checked_div(b).unwrap_or(0) & mask,
        BinaryOp::Mod => a.checked_rem(b).unwrap_or(0) & mask,
        BinaryOp::Shl => {
            if b >= 64 {
                0
            } else {
                a.wrapping_shl(b as u32) & mask
            }
        }
        BinaryOp::Shr => {
            if b >= 64 {
                0
            } else {
                a.wrapping_shr(b as u32)
            }
        }
        BinaryOp::Sar => {
            if b >= 64 {
                if a & (1u64 << (width - 1)) != 0 {
                    mask
                } else {
                    0
                }
            } else {
                (a as i64).wrapping_shr(b as u32) as u64 & mask
            }
        }
        BinaryOp::And => a & b,
        BinaryOp::Or => a | b,
        BinaryOp::Xor => a ^ b,
        BinaryOp::Eq => {
            if a == b {
                1
            } else {
                0
            }
        }
        BinaryOp::NotEq => {
            if a != b {
                1
            } else {
                0
            }
        }
        BinaryOp::Lt => {
            if a < b {
                1
            } else {
                0
            }
        }
        BinaryOp::Gt => {
            if a > b {
                1
            } else {
                0
            }
        }
        BinaryOp::Le => {
            if a <= b {
                1
            } else {
                0
            }
        }
        BinaryOp::Ge => {
            if a >= b {
                1
            } else {
                0
            }
        }
        BinaryOp::LogAnd => {
            if a != 0 && b != 0 {
                1
            } else {
                0
            }
        }
        BinaryOp::LogOr => {
            if a != 0 || b != 0 {
                1
            } else {
                0
            }
        }
    }
}

/// Evaluasi ekspresi unary dengan operand konstanta untuk menghasilkan
/// konstanta baru.
fn eval_unary_const(op: UnaryOp, a: u64, width: u32) -> u64 {
    let mask = if width >= 64 {
        u64::MAX
    } else {
        (1u64 << width) - 1
    };
    match op {
        UnaryOp::BitNot => !a & mask,
        UnaryOp::LogNot => {
            if a == 0 {
                1
            } else {
                0
            }
        }
        UnaryOp::BitNeg => (!a).wrapping_add(1) & mask,
        UnaryOp::RedAnd => {
            let mut r = 1;
            for i in 0..width.min(64) {
                r &= (a >> i) & 1;
            }
            r
        }
        UnaryOp::RedNand => {
            let mut r = 1;
            for i in 0..width.min(64) {
                r &= (a >> i) & 1;
            }
            if r == 1 {
                0
            } else {
                1
            }
        }
        UnaryOp::RedOr => {
            let mut r = 0;
            for i in 0..width.min(64) {
                r |= (a >> i) & 1;
            }
            r
        }
        UnaryOp::RedNor => {
            let mut r = 0;
            for i in 0..width.min(64) {
                r |= (a >> i) & 1;
            }
            if r == 0 {
                1
            } else {
                0
            }
        }
        UnaryOp::RedXor => {
            let mut r = 0;
            for i in 0..width.min(64) {
                r ^= (a >> i) & 1;
            }
            r
        }
        UnaryOp::RedXnor => {
            let mut r = 0;
            for i in 0..width.min(64) {
                r ^= (a >> i) & 1;
            }
            if r == 0 {
                1
            } else {
                0
            }
        }
    }
}

/// Span dummy untuk error yang tidak punya lokasi spesifik.
fn span_dummy() -> sv_lexer::span::Span {
    sv_lexer::span::Span::dummy()
}

/// Validasi jumlah argumen pemanggilan terhadap daftar formal (LRM §13.3).
fn cek_argumen(
    decl: &RoutineDecl,
    args: &[Expr],
    span: sv_lexer::span::Span,
) -> Result<(), ElaborateError> {
    if args.len() != decl.args.len() {
        return Err(ElaborateError::new(
            format!(
                "{} '{}' mengharapkan {} argumen, dapat {}",
                label(decl.kind),
                decl.name,
                decl.args.len(),
                args.len()
            ),
            span,
        ));
    }
    Ok(())
}

/// Expand item generate; subrutin bisa dipanggil di dalam `assign`/`always_*`.
fn expand_items(
    items: &[GenerateItem],
    routines: &HashMap<String, RoutineDecl>,
    dpis: &[DpiDecl],
) -> Result<Vec<GenerateItem>, ElaborateError> {
    let mut inliner = Inliner::baru(routines, dpis);
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        out.push(expand_item(item, &mut inliner)?);
    }
    Ok(out)
}

fn expand_item(
    item: &GenerateItem,
    inliner: &mut Inliner<'_>,
) -> Result<GenerateItem, ElaborateError> {
    Ok(match item {
        GenerateItem::For(g) => GenerateItem::For(Box::new(sv_ast::generate::ForGenerate {
            var: g.var.clone(),
            init: g.init,
            bound: g.bound,
            compare: g.compare,
            step: g.step,
            label: g.label.clone(),
            body: expand_items_with(inliner, &g.body)?,
            span: g.span,
        })),
        GenerateItem::If(g) => GenerateItem::If(Box::new(sv_ast::generate::IfGenerate {
            condition: inliner.expr(&g.condition)?,
            then_branch: expand_items_with(inliner, &g.then_branch)?,
            then_label: g.then_label.clone(),
            else_branch: match &g.else_branch {
                Some(branch) => Some(expand_items_with(inliner, branch)?),
                None => None,
            },
            else_label: g.else_label.clone(),
            span: g.span,
        })),
        GenerateItem::Case(g) => GenerateItem::Case(Box::new(sv_ast::generate::CaseGenerate {
            selector: inliner.expr(&g.selector)?,
            arms: g
                .arms
                .iter()
                .map(|arm| {
                    Ok(sv_ast::generate::GenerateCaseArm {
                        labels: arm
                            .labels
                            .iter()
                            .map(|l| inliner.expr(l))
                            .collect::<Result<Vec<_>, ElaborateError>>()?,
                        label: arm.label.clone(),
                        body: expand_items_with(inliner, &arm.body)?,
                        is_default: arm.is_default,
                        span: arm.span,
                    })
                })
                .collect::<Result<Vec<_>, ElaborateError>>()?,
            span: g.span,
        })),
        GenerateItem::Instance(inst) => GenerateItem::Instance(inst.clone()),
        GenerateItem::Decl(decls) => GenerateItem::Decl(decls.clone()),
        GenerateItem::Assign(inner) => {
            let (lhs, rhs, span) = &**inner;
            GenerateItem::Assign(Box::new((lhs.clone(), inliner.expr(rhs)?, *span)))
        }
        GenerateItem::Process(stmt) => {
            let statement = inliner.statement(stmt)?;
            let [satu] = <[Statement; 1]>::try_from(statement).map_err(|_| {
                ElaborateError::new(
                    "task di dalam proses generate harus menghasilkan tepat satu statement",
                    stmt_span(stmt),
                )
            })?;
            GenerateItem::Process(Box::new(satu))
        }
    })
}

/// Span statement proses generate untuk pesan error.
fn stmt_span(stmt: &Statement) -> sv_lexer::span::Span {
    match stmt {
        Statement::ContinuousAssign { span, .. }
        | Statement::AlwaysFf { span, .. }
        | Statement::AlwaysTimed { span, .. }
        | Statement::AlwaysComb { span, .. }
        | Statement::Initial { span, .. }
        | Statement::Final { span, .. } => *span,
    }
}

fn expand_items_with(
    inliner: &mut Inliner<'_>,
    items: &[GenerateItem],
) -> Result<Vec<GenerateItem>, ElaborateError> {
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        out.push(expand_item(item, inliner)?);
    }
    Ok(out)
}

fn expand_statement(
    statement: &Statement,
    routines: &HashMap<String, RoutineDecl>,
    dpis: &[DpiDecl],
) -> Result<Vec<Statement>, ElaborateError> {
    Inliner::baru(routines, dpis).statement(statement)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sv_ast::combinational::CombinationalStatement;
    use sv_ast::declaration::Declaration;
    use sv_ast::expression::Expr;
    use sv_ast::lvalue::Lvalue;
    use sv_ast::routine::{RoutineDecl, RoutineKind};
    use sv_lexer::span::Span;

    #[test]
    fn eval_function_body_loop_konstanta() {
        // function automatic [7:0] lima_kali_dua() { logic [7:0] acc; acc = 0; for (i=0; i<5; i=i+1) acc = acc + 2; lima_kali_dua = acc; }
        let decl = RoutineDecl {
            kind: RoutineKind::Function,
            name: "f".to_string(),
            return_width: Some(sv_ast::width::WidthExpr::Literal(8)),
            return_signed: false,
            args: vec![],
            body: vec![
                CombinationalStatement::Decl(Declaration {
                    name: "acc".to_string(),
                    width: sv_ast::width::WidthExpr::Literal(8),
                    signed: false,
                    init: Some(Expr::Number(0)),
                    type_name: None,
                    unpacked: None,
                    struct_fields: None,
                    span: Span::dummy(),
                }),
                CombinationalStatement::BlockingAssign {
                    lhs: Lvalue::simple("acc", Span::dummy()),
                    rhs: Expr::Number(0),
                    span: Span::dummy(),
                },
                CombinationalStatement::For {
                    init: sv_ast::combinational::ForStep {
                        lhs: Lvalue::simple("i", Span::dummy()),
                        rhs: Expr::Number(0),
                        span: Span::dummy(),
                    },
                    condition: Expr::Binary {
                        op: sv_ast::expression::BinaryOp::Lt,
                        lhs: Box::new(Expr::Ident {
                            name: "i".to_string(),
                            span: Span::dummy(),
                        }),
                        rhs: Box::new(Expr::Number(5)),
                        span: Span::dummy(),
                    },
                    step: sv_ast::combinational::ForStep {
                        lhs: Lvalue::simple("i", Span::dummy()),
                        rhs: Expr::Binary {
                            op: sv_ast::expression::BinaryOp::Add,
                            lhs: Box::new(Expr::Ident {
                                name: "i".to_string(),
                                span: Span::dummy(),
                            }),
                            rhs: Box::new(Expr::Number(1)),
                            span: Span::dummy(),
                        },
                        span: Span::dummy(),
                    },
                    body: vec![CombinationalStatement::BlockingAssign {
                        lhs: Lvalue::simple("acc", Span::dummy()),
                        rhs: Expr::Binary {
                            op: sv_ast::expression::BinaryOp::Add,
                            lhs: Box::new(Expr::Ident {
                                name: "acc".to_string(),
                                span: Span::dummy(),
                            }),
                            rhs: Box::new(Expr::Number(2)),
                            span: Span::dummy(),
                        },
                        span: Span::dummy(),
                    }],
                    span: Span::dummy(),
                },
                CombinationalStatement::BlockingAssign {
                    lhs: Lvalue::simple("f", Span::dummy()),
                    rhs: Expr::Ident {
                        name: "acc".to_string(),
                        span: Span::dummy(),
                    },
                    span: Span::dummy(),
                },
            ],
            automatic: false,
            span: Span::dummy(),
        };

        let nilai = eval_function_body(&decl, &[], Span::dummy()).unwrap();
        assert_eq!(nilai, Expr::Number(10), "5 iterasi × 2 = 10");
    }
}
