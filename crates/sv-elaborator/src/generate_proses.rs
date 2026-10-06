// Tanggung jawab: substitusi genvar dan scope pada proses di dalam generate.
use crate::error::ElaborateError;
use crate::generate::{GenvarEnv, ScopeBlok};
use crate::param::ParamTable;
use crate::symbol::{Symbol, SymbolTable};
use sv_ast::combinational::CombinationalStatement;
use sv_ast::declaration::Declaration;
use sv_ast::expression::Expr as AstExpr;
use sv_ast::generate::GenerateItem;
use sv_ast::lvalue::Lvalue;
use sv_ast::statement::{SequentialStatement, Statement};
use sv_ir::datatype::DataType;
use sv_ir::scope::ScopePath;
use sv_ir::variable::{VarDecl, VarKind};
use sv_ir::Design;

/// Kumpulkan seluruh deklarasi yang muncul di dalam item generate.
///
/// Termasuk deklarasi di dalam badan proses (`always_ff`/`always_comb`/
/// `initial`), karena semuanya berada dalam scope blok yang sama sehingga
/// harus mendapat prefix iterasi yang sama juga (LRM §27.4).
pub(crate) fn kumpulkan_deklarasi(items: &[GenerateItem]) -> Vec<Declaration> {
    let mut out = Vec::new();
    for item in items {
        match item {
            GenerateItem::Decl(decls) => out.extend(decls.iter().cloned()),
            GenerateItem::Process(stmt) => kumpulkan_dari_statement(stmt, &mut out),
            GenerateItem::For(for_gen) => out.extend(kumpulkan_deklarasi(&for_gen.body)),
            GenerateItem::If(if_gen) => {
                out.extend(kumpulkan_deklarasi(&if_gen.then_branch));
                if let Some(else_branch) = &if_gen.else_branch {
                    out.extend(kumpulkan_deklarasi(else_branch));
                }
            }
            GenerateItem::Case(case_gen) => {
                for arm in &case_gen.arms {
                    out.extend(kumpulkan_deklarasi(&arm.body));
                }
            }
            GenerateItem::Assign { .. } | GenerateItem::Instance(_) => {}
        }
    }
    out
}

/// Kumpulkan deklarasi dari satu statement modul.
fn kumpulkan_dari_statement(stmt: &Statement, out: &mut Vec<Declaration>) {
    match stmt {
        Statement::AlwaysTimed { body, .. }
        | Statement::AlwaysComb { body, .. }
        | Statement::Initial { body, .. } => kumpulan_comb(body, out),
        Statement::AlwaysFf { body, .. } => kumpulan_seq(body, out),
        Statement::ContinuousAssign { .. } => {}
    }
}

fn kumpulan_comb(body: &[CombinationalStatement], out: &mut Vec<Declaration>) {
    for stmt in body {
        match stmt {
            CombinationalStatement::Decl(decl) => out.push(decl.clone()),
            CombinationalStatement::Block { body, .. } => kumpulan_comb(body, out),
            // Task call dan `return` sudah ter-expand sebelum elaborasi, jadi tidak
            // pernah membawa deklarasi lokal baru.
            CombinationalStatement::BlockingAssign { .. }
            | CombinationalStatement::NonBlockingAssign { .. }
            | CombinationalStatement::CompoundAssign { .. }
            | CombinationalStatement::SystemTask(_)
            | CombinationalStatement::Delay { .. }
            | CombinationalStatement::TaskCall(_)
            | CombinationalStatement::Return { .. } => {}
            // Variabel loop `for` tidak dikumpulkan sebagai deklarasi, sama seperti
            // `local_decl` pada modul biasa.
            CombinationalStatement::For { body, .. } => kumpulan_comb(body, out),
            CombinationalStatement::Repeat { body, .. } => kumpulan_comb(body, out),
            CombinationalStatement::While { body, .. } => kumpulan_comb(body, out),
            CombinationalStatement::EventControl { body, .. } => kumpulan_comb(body, out),
            CombinationalStatement::IfElse {
                then_branch,
                else_branch,
                ..
            } => {
                kumpulan_comb(then_branch, out);
                if let Some(branch) = else_branch {
                    kumpulan_comb(branch, out);
                }
            }
            CombinationalStatement::Case { arms, .. } => {
                for arm in arms {
                    kumpulan_comb(&arm.body, out);
                }
            }
        }
    }
}

fn kumpulan_seq(body: &[SequentialStatement], out: &mut Vec<Declaration>) {
    for stmt in body {
        match stmt {
            SequentialStatement::Decl(decl) => out.push(decl.clone()),
            // Lihat catatan pada `kumpulan_comb`: node ini sudah ter-expand.
            SequentialStatement::NonBlockingAssign { .. }
            | SequentialStatement::BlockingAssign { .. }
            | SequentialStatement::SystemTask(_)
            | SequentialStatement::TaskCall(_)
            | SequentialStatement::Return { .. } => {}
            SequentialStatement::IfElse {
                then_branch,
                else_branch,
                ..
            } => {
                kumpulan_seq(then_branch, out);
                if let Some(branch) = else_branch {
                    kumpulan_seq(branch, out);
                }
            }
        }
    }
}

/// Daftarkan deklarasi lokal proses dengan prefix scope blok generate.
///
/// Harus dipanggil sebelum statement dielaborasi karena ekspresi di dalam
/// blok sudah merujuk nama tersebut.
pub(crate) fn daftarkan(
    design: &mut Design,
    symbols: &mut SymbolTable,
    stmt: &Statement,
    scope: &ScopeBlok,
    params: &ParamTable,
) -> Result<(), ElaborateError> {
    let mut decls = Vec::new();
    kumpulkan_dari_statement(stmt, &mut decls);
    for decl in decls {
        let bit = params.lebar(&decl.width, decl.span)?;
        let tipe = DataType::logic(bit as u32).with_signed(decl.signed);
        let nama = scope.nama_luar(&decl.name, decl.span);
        let id = symbols.insert(Symbol {
            name: nama.clone(),
            signal_id: 0,
            data_type: tipe,
            kind: VarKind::Variable,
            unpacked: None,
            span: decl.span,
        })?;
        design.add_variable(VarDecl::new(
            nama.clone(),
            ScopePath::root().child(nama),
            tipe,
            VarKind::Variable,
            id,
        ));
    }
    Ok(())
}

/// Substitusi genvar dan scope pada statement modul di dalam generate.
pub(crate) fn subst_statement(stmt: &Statement, env: &GenvarEnv, scope: &ScopeBlok) -> Statement {
    match stmt {
        Statement::ContinuousAssign { lhs, rhs, span } => Statement::ContinuousAssign {
            lhs: subst_lvalue(lhs, scope, env),
            rhs: subst_expr(rhs, env, scope),
            span: *span,
        },
        Statement::AlwaysTimed { body, span } => Statement::AlwaysTimed {
            body: subst_comb(body, env, scope),
            span: *span,
        },
        Statement::AlwaysComb { body, span } => Statement::AlwaysComb {
            body: subst_comb(body, env, scope),
            span: *span,
        },
        Statement::Initial { body, span } => Statement::Initial {
            body: subst_comb(body, env, scope),
            span: *span,
        },
        Statement::AlwaysFf { events, body, span } => Statement::AlwaysFf {
            events: events
                .iter()
                .map(|item| item.dengan_nama(scope.nama_luar(&item.signal, item.span)))
                .collect(),
            body: subst_seq(body, env, scope),
            span: *span,
        },
    }
}

/// Substitusi pada LHS.
///
/// Indeks genvar diselesaikan menjadi irisan konkret di sini karena nilai
/// genvar sudah diketahui pada iterasi ini. Bila genvarnya tidak aktif,
/// indeks dibiarkan apa adanya agar ditolak `lookup_lvalue` dengan pesan yang
/// menyebut activate-nya.
fn subst_lvalue(lhs: &Lvalue, scope: &ScopeBlok, env: &GenvarEnv) -> Lvalue {
    let mut hasil = match scope.qualify_nama(&lhs.name) {
        Some(baru) => lhs.dengan_nama(baru),
        None => lhs.dengan_nama(scope.nama_luar(&lhs.name, lhs.span)),
    };
    if let Some(genvar) = hasil.genvar_index.clone() {
        if let Some(nilai) = env.cari(&genvar) {
            if let Ok(bit) = u32::try_from(nilai) {
                hasil.resolve_genvar(bit);
            }
        }
    }
    hasil
}

/// Substitusi pada seluruh cabang ekspresi.
pub(crate) fn subst_expr(expr: &AstExpr, env: &GenvarEnv, scope: &ScopeBlok) -> AstExpr {
    match expr {
        AstExpr::Ident { name: nama, span } => {
            if let Some(nilai) = env.cari(nama) {
                return AstExpr::Number(nilai);
            }
            match scope.qualify(nama, *span) {
                Some(baru) => baru,
                None => AstExpr::ident(scope.nama_luar(nama, *span), *span),
            }
        }
        // Nama fungsi bukan sinyal, jadi tidak di-qualify; argumennya yang perlu.
        AstExpr::FunctionCall { name, args, span } => AstExpr::FunctionCall {
            name: name.clone(),
            args: args.iter().map(|a| subst_expr(a, env, scope)).collect(),
            span: *span,
        },
        AstExpr::Binary { op, lhs, rhs, span } => AstExpr::Binary {
            op: *op,
            lhs: Box::new(subst_expr(lhs, env, scope)),
            rhs: Box::new(subst_expr(rhs, env, scope)),
            span: *span,
        },
        AstExpr::Unary { op, operand, span } => AstExpr::Unary {
            op: *op,
            operand: Box::new(subst_expr(operand, env, scope)),
            span: *span,
        },
        AstExpr::Ternary {
            condition,
            when_true,
            when_false,
            span,
        } => AstExpr::Ternary {
            condition: Box::new(subst_expr(condition, env, scope)),
            when_true: Box::new(subst_expr(when_true, env, scope)),
            when_false: Box::new(subst_expr(when_false, env, scope)),
            span: *span,
        },
        AstExpr::Select {
            base,
            msb,
            lsb,
            span,
        } => AstExpr::Select {
            base: Box::new(subst_expr(base, env, scope)),
            msb: *msb,
            lsb: *lsb,
            span: *span,
        },
        AstExpr::IndexDynamic { base, index, span } => AstExpr::IndexDynamic {
            base: Box::new(subst_expr(base, env, scope)),
            index: Box::new(subst_expr(index, env, scope)),
            span: *span,
        },
        AstExpr::Concat { items, span } => AstExpr::Concat {
            items: items.iter().map(|i| subst_expr(i, env, scope)).collect(),
            span: *span,
        },
        AstExpr::Replicate { count, value, span } => AstExpr::Replicate {
            count: Box::new(subst_expr(count, env, scope)),
            value: Box::new(subst_expr(value, env, scope)),
            span: *span,
        },
        AstExpr::Sized {
            value,
            width,
            signed,
            unknown_mask,
            zmask,
        } => AstExpr::Sized {
            value: *value,
            width: *width,
            signed: *signed,
            unknown_mask: *unknown_mask,
            zmask: *zmask,
        },
        AstExpr::Number(v) => AstExpr::Number(*v),
        AstExpr::SystemTime { unit, span } => AstExpr::SystemTime {
            unit: *unit,
            span: *span,
        },
        // LRM §6.14 + §8.20: nama tipe bukan sinyal lokal scope ini, jadi tidak
        // ikut prefix iterasi — tapi typedef module-scoped, jadi tetap perlu
        // prefix instans supaya tidak tertukar dengan typedef modul lain.
        // LRM §6.14: cast ke tipe bawaan dan size cast tidak punya nama tipe
        // yang perlu di-prefix, jadi hanya operandnya yang ikut substitusi.
        AstExpr::BuiltinCast {
            type_name,
            width,
            signed,
            operand,
            span,
        } => AstExpr::BuiltinCast {
            type_name: type_name.clone(),
            width: *width,
            signed: *signed,
            operand: Box::new(subst_expr(operand, env, scope)),
            span: *span,
        },
        AstExpr::SizeCast {
            width,
            operand,
            span,
        } => AstExpr::SizeCast {
            width: *width,
            operand: Box::new(subst_expr(operand, env, scope)),
            span: *span,
        },
        AstExpr::Cast {
            type_name,
            operand,
            span,
        } => AstExpr::Cast {
            type_name: format!("{}{}", scope.inst_prefix, type_name),
            operand: Box::new(subst_expr(operand, env, scope)),
            span: *span,
        },
        AstExpr::SignCast {
            signed,
            operand,
            span,
        } => AstExpr::SignCast {
            signed: *signed,
            operand: Box::new(subst_expr(operand, env, scope)),
            span: *span,
        },
        AstExpr::Bits { operand, span } => AstExpr::Bits {
            operand: Box::new(subst_expr(operand, env, scope)),
            span: *span,
        },
    }
}

/// Substitusi pada rangkaian statement combinational.
pub(crate) fn subst_comb(
    body: &[CombinationalStatement],
    env: &GenvarEnv,
    scope: &ScopeBlok,
) -> Vec<CombinationalStatement> {
    body.iter()
        .map(|stmt| match stmt {
            CombinationalStatement::Decl(decl) => {
                let mut decl = decl.clone();
                decl.name = scope.nama_luar(&decl.name, decl.span);
                CombinationalStatement::Decl(decl)
            }
            // Task call sudah ter-expand sebelum elaborasi; argumennya tetap
            // disubstitusi agar jalur ini benar bila someday tidak lagi.
            CombinationalStatement::TaskCall(call) => {
                CombinationalStatement::TaskCall(sv_ast::routine::TaskCall {
                    name: call.name.clone(),
                    args: call
                        .args
                        .iter()
                        .map(|a| subst_expr(a, env, scope))
                        .collect(),
                    span: call.span,
                })
            }
            CombinationalStatement::Return { value, span } => CombinationalStatement::Return {
                value: value.as_ref().map(|v| subst_expr(v, env, scope)),
                span: *span,
            },
            CombinationalStatement::BlockingAssign { lhs, rhs, span } => {
                CombinationalStatement::BlockingAssign {
                    lhs: subst_lvalue(lhs, scope, env),
                    rhs: subst_expr(rhs, env, scope),
                    span: *span,
                }
            }
            // NBA harus tetap NBA: kalau VARI-variannya diturunkan ke
            // `BlockingAssign`, penjadwalan `<=` jadi `=` di dalam `initial`.
            CombinationalStatement::NonBlockingAssign { lhs, rhs, span } => {
                CombinationalStatement::NonBlockingAssign {
                    lhs: subst_lvalue(lhs, scope, env),
                    rhs: subst_expr(rhs, env, scope),
                    span: *span,
                }
            }
            CombinationalStatement::CompoundAssign { lhs, op, rhs, span } => {
                CombinationalStatement::CompoundAssign {
                    lhs: subst_lvalue(lhs, scope, env),
                    op: *op,
                    rhs: subst_expr(rhs, env, scope),
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
                    lhs: subst_lvalue(&init.lhs, scope, env),
                    rhs: subst_expr(&init.rhs, env, scope),
                    span: init.span,
                },
                condition: subst_expr(condition, env, scope),
                step: sv_ast::combinational::ForStep {
                    lhs: subst_lvalue(&step.lhs, scope, env),
                    rhs: subst_expr(&step.rhs, env, scope),
                    span: step.span,
                },
                body: subst_comb(body, env, scope),
                span: *span,
            },
            CombinationalStatement::SystemTask(task) => {
                CombinationalStatement::SystemTask(subst_task(task, env, scope))
            }
            CombinationalStatement::Repeat { count, body, span } => {
                CombinationalStatement::Repeat {
                    count: subst_expr(count, env, scope),
                    body: subst_comb(body, env, scope),
                    span: *span,
                }
            }
            CombinationalStatement::Delay {
                amount,
                unit,
                body,
                span,
            } => CombinationalStatement::Delay {
                amount: subst_expr(amount, env, scope),
                unit: *unit,
                body: Box::new(
                    subst_comb(std::slice::from_ref(body), env, scope)
                        .into_iter()
                        .next()
                        .expect("satu statement menghasilkan satu statement"),
                ),
                span: *span,
            },
            CombinationalStatement::While {
                condition,
                body,
                span,
            } => CombinationalStatement::While {
                condition: subst_expr(condition, env, scope),
                body: subst_comb(body, env, scope),
                span: *span,
            },
            CombinationalStatement::Block { body, span } => CombinationalStatement::Block {
                body: subst_comb(body, env, scope),
                span: *span,
            },
            CombinationalStatement::EventControl { events, body, span } => {
                CombinationalStatement::EventControl {
                    events: events
                        .iter()
                        .map(|item| item.dengan_nama(scope.nama_luar(&item.signal, item.span)))
                        .collect(),
                    body: subst_comb(body, env, scope),
                    span: *span,
                }
            }
            CombinationalStatement::IfElse {
                condition,
                then_branch,
                else_branch,
                span,
            } => CombinationalStatement::IfElse {
                condition: subst_expr(condition, env, scope),
                then_branch: subst_comb(then_branch, env, scope),
                else_branch: else_branch.as_ref().map(|b| subst_comb(b, env, scope)),
                span: *span,
            },
            CombinationalStatement::Case {
                selector,
                arms,
                kind,
                span,
            } => CombinationalStatement::Case {
                selector: subst_expr(selector, env, scope),
                arms: arms
                    .iter()
                    .map(|arm| sv_ast::combinational::CaseArm {
                        labels: arm
                            .labels
                            .iter()
                            .map(|l| subst_expr(l, env, scope))
                            .collect(),
                        body: subst_comb(&arm.body, env, scope),
                        is_default: arm.is_default,
                        span: arm.span,
                    })
                    .collect(),
                kind: *kind,
                span: *span,
            },
        })
        .collect()
}

/// Substitusi pada rangkaian statement sekuensial.
pub(crate) fn subst_seq(
    body: &[SequentialStatement],
    env: &GenvarEnv,
    scope: &ScopeBlok,
) -> Vec<SequentialStatement> {
    body.iter()
        .map(|stmt| match stmt {
            SequentialStatement::Decl(decl) => {
                let mut decl = decl.clone();
                decl.name = scope.nama_luar(&decl.name, decl.span);
                SequentialStatement::Decl(decl)
            }
            SequentialStatement::TaskCall(call) => {
                SequentialStatement::TaskCall(sv_ast::routine::TaskCall {
                    name: call.name.clone(),
                    args: call
                        .args
                        .iter()
                        .map(|a| subst_expr(a, env, scope))
                        .collect(),
                    span: call.span,
                })
            }
            SequentialStatement::Return { value, span } => SequentialStatement::Return {
                value: value.as_ref().map(|v| subst_expr(v, env, scope)),
                span: *span,
            },
            SequentialStatement::NonBlockingAssign { lhs, rhs, span } => {
                SequentialStatement::NonBlockingAssign {
                    lhs: subst_lvalue(lhs, scope, env),
                    rhs: subst_expr(rhs, env, scope),
                    span: *span,
                }
            }
            SequentialStatement::BlockingAssign { lhs, rhs, span } => {
                SequentialStatement::BlockingAssign {
                    lhs: subst_lvalue(lhs, scope, env),
                    rhs: subst_expr(rhs, env, scope),
                    span: *span,
                }
            }
            SequentialStatement::IfElse {
                condition,
                then_branch,
                else_branch,
                span,
            } => SequentialStatement::IfElse {
                condition: subst_expr(condition, env, scope),
                then_branch: subst_seq(then_branch, env, scope),
                else_branch: else_branch.as_ref().map(|b| subst_seq(b, env, scope)),
                span: *span,
            },
            SequentialStatement::SystemTask(task) => {
                SequentialStatement::SystemTask(subst_task(task, env, scope))
            }
        })
        .collect()
}

/// Substitusi genvar dan scope pada argumen system task. Dipakai jalur
/// sekuensial dan combinational agar keduanya tidak berbeda perilaku.
fn subst_task(
    task: &sv_ast::system_task::SystemTask,
    env: &GenvarEnv,
    scope: &ScopeBlok,
) -> sv_ast::system_task::SystemTask {
    sv_ast::system_task::SystemTask {
        kind: task.kind.clone(),
        args: task
            .args
            .iter()
            .map(|a| match a {
                sv_ast::system_task::SystemArg::Format(t) => {
                    sv_ast::system_task::SystemArg::Format(t.clone())
                }
                sv_ast::system_task::SystemArg::Value(e) => {
                    sv_ast::system_task::SystemArg::Value(subst_expr(e, env, scope))
                }
            })
            .collect(),
        condition: task.condition.as_ref().map(|c| subst_expr(c, env, scope)),
        time_scale: task.time_scale,
        span: task.span,
    }
}
