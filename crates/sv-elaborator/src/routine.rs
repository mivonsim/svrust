// Tanggung jawab: meng-inline badan task/function ke titik panggilan (LRM §13.3/§13.4).
use crate::error::ElaborateError;
use crate::routine_subst::Substitution;
use std::collections::HashMap;
use sv_ast::combinational::CombinationalStatement;
use sv_ast::declaration::Declaration;
use sv_ast::expression::Expr;
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
/// pipeline tidak pernah melihat node `TaskCall`/`FunctionCall`.
pub fn expand_module(module: &Module) -> Result<Module, ElaborateError> {
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
        statements.extend(expand_statement(statement, &routines)?);
    }
    out.statements = statements;
    out.generates = module
        .generates
        .iter()
        .map(|region| {
            Ok(sv_ast::generate::GenerateRegion {
                genvars: region.genvars.clone(),
                items: expand_items(&region.items, &routines)?,
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
    depth: usize,
    /// Nomor pemanggilan task, untuk memberi nama unik pada deklarasi lokal
    /// badan subrutin yang di-inline.
    panggilan: usize,
}

impl<'a> Inliner<'a> {
    fn baru(routines: &'a HashMap<String, RoutineDecl>) -> Self {
        Self {
            routines,
            depth: 0,
            panggilan: 0,
        }
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
            Statement::AlwaysComb { body, span } => Statement::AlwaysComb {
                body: self.comb_list(body)?,
                span: *span,
            },
            Statement::Initial { body, span } => Statement::Initial {
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
        let out = match stmt {
            SequentialStatement::NonBlockingAssign { lhs, rhs, span } => {
                SequentialStatement::NonBlockingAssign {
                    lhs: self.lvalue(lhs)?,
                    rhs: self.expr(rhs)?,
                    span: *span,
                }
            }
            SequentialStatement::BlockingAssign { lhs, rhs, span } => {
                SequentialStatement::BlockingAssign {
                    lhs: self.lvalue(lhs)?,
                    rhs: self.expr(rhs)?,
                    span: *span,
                }
            }
            SequentialStatement::IfElse {
                condition,
                then_branch,
                else_branch,
                span,
            } => SequentialStatement::IfElse {
                condition: self.expr(condition)?,
                then_branch: self.seq_list(then_branch)?,
                else_branch: match else_branch {
                    Some(branch) => Some(self.seq_list(branch)?),
                    None => None,
                },
                span: *span,
            },
            SequentialStatement::Decl(decl) => SequentialStatement::Decl(self.deklarasi(decl)?),
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
                SequentialStatement::SystemTask(sv_ast::system_task::SystemTask {
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
            // Task di-inline jadi statement di depan titik panggilan. AST
            // sekuensial belum punya loop/case, jadi task dengan badan
            // majemuk hanya boleh dipanggil dari konteks combinational.
            SequentialStatement::TaskCall(call) => {
                return Err(ElaborateError::new(
                    format!(
                        "task '{}' belum bisa dipanggil di dalam `always_ff`; \
                         pindahkan ke `always_comb` atau `initial`",
                        call.name
                    ),
                    call.span,
                ))
            }
            SequentialStatement::Return { span, .. } => {
                return Err(ElaborateError::new(
                    "`return` hanya sah di dalam task/function",
                    *span,
                ))
            }
        };
        Ok(vec![out])
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

    fn comb(
        &mut self,
        stmt: &CombinationalStatement,
    ) -> Result<Vec<CombinationalStatement>, ElaborateError> {
        let out = match stmt {
            CombinationalStatement::Decl(decl) => {
                CombinationalStatement::Decl(self.deklarasi(decl)?)
            }
            CombinationalStatement::BlockingAssign { lhs, rhs, span } => {
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
            CombinationalStatement::Block { body, span } => CombinationalStatement::Block {
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
                return self.task_body(&call.name, &call.args, call.span)
            }
            CombinationalStatement::Return { span, .. } => {
                return Err(ElaborateError::new(
                    "`return` hanya sah di dalam task/function",
                    *span,
                ))
            }
        };
        Ok(vec![out])
    }

    /// Turunkan deklarasi lokal; nilai inisialnya bisa memanggil function.
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
        if decl.kind != RoutineKind::Task {
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

        let nilai = nilai_function(decl, span)?;
        let tabel = Substitution::baru(decl.args.iter().map(|a| a.name.clone()).collect(), argumen);
        self.masuk(name, span)?;
        let hasil = self.expr(&tabel.ganti(&nilai));
        self.keluar();
        hasil
    }

    fn expr(&mut self, expr: &Expr) -> Result<Expr, ElaborateError> {
        Ok(match expr {
            Expr::FunctionCall { name, args, span } => {
                let name = name.clone();
                let args = args.clone();
                self.call(&name, &args, *span)?
            }
            Expr::Ident { name, span } => Expr::ident(name.clone(), *span),
            Expr::Number(n) => Expr::Number(*n),
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
        })
    }

    fn lvalue(&mut self, lhs: &Lvalue) -> Result<Lvalue, ElaborateError> {
        Ok(Lvalue {
            name: lhs.name.clone(),
            slice: lhs.slice,
            genvar_index: lhs.genvar_index.clone(),
            span: lhs.span,
        })
    }
}

/// Turunkan badan function menjadi satu ekspresi nilai (LRM §13.4).
///
/// Bentuk yang diterima: satu assignment ke nama function, `return expr;`,
/// atau `if` yang kedua cabangnya sama-sama mengesahkan nama function.
fn nilai_function(decl: &RoutineDecl, span: sv_lexer::span::Span) -> Result<Expr, ElaborateError> {
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
    Err(badan_function_tidak_terdukung(&decl.name, span))
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
) -> Result<Vec<GenerateItem>, ElaborateError> {
    let mut inliner = Inliner::baru(routines);
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
            else_branch: match &g.else_branch {
                Some(branch) => Some(expand_items_with(inliner, branch)?),
                None => None,
            },
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
        GenerateItem::Assign { lhs, rhs, span } => GenerateItem::Assign {
            lhs: lhs.clone(),
            rhs: inliner.expr(rhs)?,
            span: *span,
        },
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
        | Statement::AlwaysComb { span, .. }
        | Statement::Initial { span, .. } => *span,
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
) -> Result<Vec<Statement>, ElaborateError> {
    Inliner::baru(routines).statement(statement)
}
