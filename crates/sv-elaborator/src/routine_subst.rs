// Tanggung jawab: substitusi nama formal task/function dengan argumen nyata.
use std::collections::HashMap;
use sv_ast::combinational::{CaseArm, CombinationalStatement, ForStep};
use sv_ast::expression::Expr;
use sv_ast::lvalue::Lvalue;
use sv_ast::statement::SequentialStatement;
use sv_ast::system_task::{SystemArg, SystemTask};

/// Pasangan nama formal dan argumen nyata pada satu pemanggilan.
pub struct Substitution {
    formal: Vec<String>,
    argumen: Vec<Expr>,
}

impl Substitution {
    pub fn baru(formal: Vec<String>, argumen: Vec<Expr>) -> Self {
        Self { formal, argumen }
    }

    fn argumen_for(&self, name: &str) -> Option<&Expr> {
        self.formal
            .iter()
            .position(|f| f == name)
            .and_then(|i| self.argumen.get(i))
    }

    /// Ganti identifier formal di dalam ekspresi dengan argumennya.
    ///
    /// Argument `output` juga disubstitusi sebagai nilai baca; penulisan ke
    /// formal ditangani oleh [`Self::ganti_lvalue`].
    pub fn ganti(&self, expr: &Expr) -> Expr {
        match expr {
            Expr::Ident { name, span } => match self.argumen_for(name) {
                Some(arg) => arg.clone(),
                None => Expr::ident(name.clone(), *span),
            },
            Expr::Binary { op, lhs, rhs, span } => Expr::Binary {
                op: *op,
                lhs: Box::new(self.ganti(lhs)),
                rhs: Box::new(self.ganti(rhs)),
                span: *span,
            },
            Expr::Unary { op, operand, span } => Expr::Unary {
                op: *op,
                operand: Box::new(self.ganti(operand)),
                span: *span,
            },
            Expr::Ternary {
                condition,
                when_true,
                when_false,
                span,
            } => Expr::Ternary {
                condition: Box::new(self.ganti(condition)),
                when_true: Box::new(self.ganti(when_true)),
                when_false: Box::new(self.ganti(when_false)),
                span: *span,
            },
            Expr::Select {
                base,
                msb,
                lsb,
                span,
            } => Expr::Select {
                base: Box::new(self.ganti(base)),
                msb: *msb,
                lsb: *lsb,
                span: *span,
            },
            Expr::IndexDynamic { base, index, span } => Expr::IndexDynamic {
                base: Box::new(self.ganti(base)),
                index: Box::new(self.ganti(index)),
                span: *span,
            },
            Expr::Concat { items, span } => Expr::Concat {
                items: items.iter().map(|i| self.ganti(i)).collect(),
                span: *span,
            },
            Expr::Replicate { count, value, span } => Expr::Replicate {
                count: Box::new(self.ganti(count)),
                value: Box::new(self.ganti(value)),
                span: *span,
            },
            // Pemanggilan function bersarang tetap perlu argumennya disubstitusi:
            // nama formal fungsi luar bisa muncul sebagai argumen fungsi
            // dalam, dan itu baru bernilai setelah argumen nyata disisipkan.
            Expr::FunctionCall { name, args, span } => Expr::FunctionCall {
                name: name.clone(),
                args: args.iter().map(|a| self.ganti(a)).collect(),
                span: *span,
            },
            // BUG-3: node cast/`$bits` dulu tertangkap wildcard di bawah, jadi
            // argumen formal yang muncul di dalam operand tidak pernah
            // disubstitusi dan bocor ke IR sebagai "undefined signal".
            // Nama tipe sendiri bukan formal, jadi tidak ikut diganti.
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
                operand: Box::new(self.ganti(operand)),
                span: *span,
            },
            Expr::SizeCast {
                width,
                operand,
                span,
            } => Expr::SizeCast {
                width: *width,
                operand: Box::new(self.ganti(operand)),
                span: *span,
            },
            // BUG-3: node cast/`$bits` dulu tertangkap wildcard di bawah, jadi
            // argumen formal yang muncul di dalam operand tidak pernah
            // disubstitusi dan bocor ke IR sebagai "undefined signal".
            // Nama tipe sendiri bukan formal, jadi tidak ikut diganti.
            Expr::Cast {
                type_name,
                operand,
                span,
            } => Expr::Cast {
                type_name: type_name.clone(),
                operand: Box::new(self.ganti(operand)),
                span: *span,
            },
            Expr::SignCast {
                signed,
                operand,
                span,
            } => Expr::SignCast {
                signed: *signed,
                operand: Box::new(self.ganti(operand)),
                span: *span,
            },
            Expr::Bits { operand, span } => Expr::Bits {
                operand: Box::new(self.ganti(operand)),
                span: *span,
            },
            // Constant, select, replicate, dan system function tidak punya
            // identifier yang bisa berupa formal.
            other => other.clone(),
        }
    }

    /// Ganti nama formal pada target assignment dengan nama sinyal nyata.
    ///
    /// Slice ikut dibawa agar `q[3:0]` pada argumen tetap terpotong sama.
    pub fn ganti_lvalue(&self, lhs: &Lvalue) -> Lvalue {
        match self.argumen_for(&lhs.name) {
            // Hanya nama sinyal sederhana yang bisa jadi target; elaborator
            // sudah menolak bentuk lain untuk argumen output/inout.
            Some(Expr::Ident { name, .. }) => Lvalue {
                name: name.clone(),
                slice: lhs.slice,
                // `genvar_index` harus ikut dibawa. Kalau dipaksa jadi `None`,
                // LHS `q[i]` berubah jadi `q` polos sehingga seluruh sinyal
                // ditulis — nilai simulasi salah tanpa pesan apa pun.
                genvar_index: lhs.genvar_index.clone(),
                span: lhs.span,
            },
            _ => lhs.clone(),
        }
    }

    /// Substitusi seluruh statement combinational dalam badan task/function.
    pub fn ganti_statements(&self, body: &[CombinationalStatement]) -> Vec<CombinationalStatement> {
        body.iter().map(|stmt| self.ganti_statement(stmt)).collect()
    }

    fn ganti_statement(&self, stmt: &CombinationalStatement) -> CombinationalStatement {
        match stmt {
            CombinationalStatement::BlockingAssign { lhs, rhs, span } => {
                CombinationalStatement::BlockingAssign {
                    lhs: self.ganti_lvalue(lhs),
                    rhs: self.ganti(rhs),
                    span: *span,
                }
            }
            CombinationalStatement::NonBlockingAssign { lhs, rhs, span } => {
                CombinationalStatement::NonBlockingAssign {
                    lhs: self.ganti_lvalue(lhs),
                    rhs: self.ganti(rhs),
                    span: *span,
                }
            }
            CombinationalStatement::CompoundAssign { lhs, op, rhs, span } => {
                CombinationalStatement::CompoundAssign {
                    lhs: self.ganti_lvalue(lhs),
                    op: *op,
                    rhs: self.ganti(rhs),
                    span: *span,
                }
            }
            CombinationalStatement::IfElse {
                condition,
                then_branch,
                else_branch,
                span,
            } => CombinationalStatement::IfElse {
                condition: self.ganti(condition),
                then_branch: self.ganti_statements(then_branch),
                else_branch: else_branch.as_ref().map(|b| self.ganti_statements(b)),
                span: *span,
            },
            CombinationalStatement::For {
                init,
                condition,
                step,
                body,
                span,
            } => CombinationalStatement::For {
                init: self.ganti_step(init),
                condition: self.ganti(condition),
                step: self.ganti_step(step),
                body: self.ganti_statements(body),
                span: *span,
            },
            CombinationalStatement::While {
                condition,
                body,
                span,
            } => CombinationalStatement::While {
                condition: self.ganti(condition),
                body: self.ganti_statements(body),
                span: *span,
            },
            CombinationalStatement::Repeat { count, body, span } => {
                CombinationalStatement::Repeat {
                    count: self.ganti(count),
                    body: self.ganti_statements(body),
                    span: *span,
                }
            }
            CombinationalStatement::Case {
                selector,
                arms,
                kind,
                span,
            } => CombinationalStatement::Case {
                selector: self.ganti(selector),
                arms: arms.iter().map(|a| self.ganti_arm(a)).collect(),
                kind: *kind,
                span: *span,
            },
            CombinationalStatement::Block { body, span } => CombinationalStatement::Block {
                body: self.ganti_statements(body),
                span: *span,
            },
            CombinationalStatement::EventControl { events, body, span } => {
                CombinationalStatement::EventControl {
                    events: events.clone(),
                    body: self.ganti_statements(body),
                    span: *span,
                }
            }
            CombinationalStatement::Delay {
                amount,
                unit,
                body,
                span,
            } => CombinationalStatement::Delay {
                amount: self.ganti(amount),
                unit: *unit,
                body: Box::new(self.ganti_statement(body)),
                span: *span,
            },
            CombinationalStatement::SystemTask(task) => {
                CombinationalStatement::SystemTask(self.ganti_task(task))
            }
            CombinationalStatement::TaskCall(call) => {
                CombinationalStatement::TaskCall(sv_ast::routine::TaskCall {
                    name: call.name.clone(),
                    args: call.args.iter().map(|a| self.ganti(a)).collect(),
                    span: call.span,
                })
            }
            CombinationalStatement::Return { value, span } => CombinationalStatement::Return {
                value: value.as_ref().map(|v| self.ganti(v)),
                span: *span,
            },
            // Deklarasi lokal di dalam badan subrutin tidak boleh ada: nama
            // formal dan nama lokal bisa bentrok setelah di-inline. Parser
            // dan elaborator menolaknya lebih awal.
            CombinationalStatement::Decl(decl) => CombinationalStatement::Decl(decl.clone()),
        }
    }

    fn ganti_arm(&self, arm: &CaseArm) -> CaseArm {
        CaseArm {
            labels: arm.labels.iter().map(|l| self.ganti(l)).collect(),
            is_default: arm.is_default,
            body: self.ganti_statements(&arm.body),
            span: arm.span,
        }
    }

    fn ganti_step(&self, step: &ForStep) -> ForStep {
        ForStep {
            lhs: self.ganti_lvalue(&step.lhs),
            rhs: self.ganti(&step.rhs),
            span: step.span,
        }
    }

    fn ganti_task(&self, task: &SystemTask) -> SystemTask {
        SystemTask {
            kind: task.kind.clone(),
            args: task
                .args
                .iter()
                .map(|arg| match arg {
                    SystemArg::Format(t) => SystemArg::Format(t.clone()),
                    SystemArg::Value(e) => SystemArg::Value(self.ganti(e)),
                })
                .collect(),
            condition: task.condition.as_ref().map(|c| self.ganti(c)),
            span: task.span,
        }
    }

    /// Substitusi statement sekuensial; dipakai untuk badan task yang dipanggil
    /// dari konteks `initial`.
    pub fn ganti_seq(&self, stmt: &SequentialStatement) -> SequentialStatement {
        match stmt {
            SequentialStatement::NonBlockingAssign { lhs, rhs, span } => {
                SequentialStatement::NonBlockingAssign {
                    lhs: self.ganti_lvalue(lhs),
                    rhs: self.ganti(rhs),
                    span: *span,
                }
            }
            SequentialStatement::BlockingAssign { lhs, rhs, span } => {
                SequentialStatement::BlockingAssign {
                    lhs: self.ganti_lvalue(lhs),
                    rhs: self.ganti(rhs),
                    span: *span,
                }
            }
            SequentialStatement::IfElse {
                condition,
                then_branch,
                else_branch,
                span,
            } => SequentialStatement::IfElse {
                condition: self.ganti(condition),
                then_branch: then_branch.iter().map(|s| self.ganti_seq(s)).collect(),
                else_branch: else_branch
                    .as_ref()
                    .map(|b| b.iter().map(|s| self.ganti_seq(s)).collect()),
                span: *span,
            },
            SequentialStatement::SystemTask(task) => {
                SequentialStatement::SystemTask(self.ganti_task(task))
            }
            SequentialStatement::TaskCall(call) => {
                SequentialStatement::TaskCall(sv_ast::routine::TaskCall {
                    name: call.name.clone(),
                    args: call.args.iter().map(|a| self.ganti(a)).collect(),
                    span: call.span,
                })
            }
            SequentialStatement::Return { value, span } => SequentialStatement::Return {
                value: value.as_ref().map(|v| self.ganti(v)),
                span: *span,
            },
            SequentialStatement::Decl(decl) => SequentialStatement::Decl(decl.clone()),
        }
    }
}

/// Beri nama unik pada deklarasi lokal di dalam badan subrutin.
///
/// Nama lokal pada badan task akan disalin ke setiap titik pemanggilan, jadi
/// dua pemanggilan task yang sama akan menduplikasi nama yang sama. Tanpa
/// penamaan ulang, keduanya bentrok saat didaftarkan sebagai sinyal design.
/// Suffix `suffix` harus berbeda per titik pemanggilan.
pub fn rename_locals(body: &[CombinationalStatement], suffix: &str) -> Vec<CombinationalStatement> {
    let mut peta: HashMap<String, String> = HashMap::new();
    for (urut, lokal) in kumpulkan_lokal(body).into_iter().enumerate() {
        peta.insert(lokal, format!("{suffix}__{urut}"));
    }
    body.iter()
        .map(|stmt| rename_statement(stmt, &peta))
        .collect()
}

/// Rename satu statement beserta seluruh sub-statement-nya.
fn rename_statement(
    stmt: &CombinationalStatement,
    peta: &HashMap<String, String>,
) -> CombinationalStatement {
    let r = |nama: &str| peta.get(nama).cloned().unwrap_or_else(|| nama.to_string());
    match stmt {
        CombinationalStatement::Decl(decl) => {
            let mut baru = decl.clone();
            baru.name = r(&decl.name);
            CombinationalStatement::Decl(baru)
        }
        CombinationalStatement::BlockingAssign { lhs, rhs, span } => {
            CombinationalStatement::BlockingAssign {
                lhs: rename_lvalue(lhs, peta),
                rhs: rename_expr(rhs, peta),
                span: *span,
            }
        }
        CombinationalStatement::NonBlockingAssign { lhs, rhs, span } => {
            CombinationalStatement::NonBlockingAssign {
                lhs: rename_lvalue(lhs, peta),
                rhs: rename_expr(rhs, peta),
                span: *span,
            }
        }
        CombinationalStatement::CompoundAssign { lhs, op, rhs, span } => {
            CombinationalStatement::CompoundAssign {
                lhs: rename_lvalue(lhs, peta),
                op: *op,
                rhs: rename_expr(rhs, peta),
                span: *span,
            }
        }
        CombinationalStatement::IfElse {
            condition,
            then_branch,
            else_branch,
            span,
        } => CombinationalStatement::IfElse {
            condition: rename_expr(condition, peta),
            then_branch: rename_locals(then_branch, ""),
            else_branch: else_branch.as_ref().map(|b| rename_locals(b, "")),
            span: *span,
        },
        CombinationalStatement::For {
            init,
            condition,
            step,
            body,
            span,
        } => CombinationalStatement::For {
            init: rename_step(init, peta),
            condition: rename_expr(condition, peta),
            step: rename_step(step, peta),
            body: rename_locals(body, ""),
            span: *span,
        },
        CombinationalStatement::While {
            condition,
            body,
            span,
        } => CombinationalStatement::While {
            condition: rename_expr(condition, peta),
            body: rename_locals(body, ""),
            span: *span,
        },
        CombinationalStatement::Repeat { count, body, span } => CombinationalStatement::Repeat {
            count: rename_expr(count, peta),
            body: rename_locals(body, ""),
            span: *span,
        },
        CombinationalStatement::Case {
            selector,
            arms,
            kind,
            span,
        } => CombinationalStatement::Case {
            selector: rename_expr(selector, peta),
            arms: arms
                .iter()
                .map(|arm| CaseArm {
                    labels: arm.labels.iter().map(|l| rename_expr(l, peta)).collect(),
                    is_default: arm.is_default,
                    body: rename_locals(&arm.body, ""),
                    span: arm.span,
                })
                .collect(),
            kind: *kind,
            span: *span,
        },
        CombinationalStatement::Block { body, span } => CombinationalStatement::Block {
            body: rename_locals(body, ""),
            span: *span,
        },
        CombinationalStatement::EventControl { events, body, span } => {
            let mut baru = events.clone();
            for item in &mut baru {
                item.signal = r(&item.signal);
            }
            CombinationalStatement::EventControl {
                events: baru,
                body: rename_locals(body, ""),
                span: *span,
            }
        }
        CombinationalStatement::Delay {
            amount,
            unit,
            body,
            span,
        } => CombinationalStatement::Delay {
            amount: rename_expr(amount, peta),
            unit: *unit,
            body: Box::new(rename_statement(body, peta)),
            span: *span,
        },
        CombinationalStatement::SystemTask(task) => {
            CombinationalStatement::SystemTask(rename_task(task, peta))
        }
        CombinationalStatement::TaskCall(call) => {
            CombinationalStatement::TaskCall(sv_ast::routine::TaskCall {
                name: call.name.clone(),
                args: call.args.iter().map(|a| rename_expr(a, peta)).collect(),
                span: call.span,
            })
        }
        CombinationalStatement::Return { value, span } => CombinationalStatement::Return {
            value: value.as_ref().map(|v| rename_expr(v, peta)),
            span: *span,
        },
    }
}

fn rename_lvalue(lhs: &Lvalue, peta: &HashMap<String, String>) -> Lvalue {
    Lvalue {
        name: peta
            .get(&lhs.name)
            .cloned()
            .unwrap_or_else(|| lhs.name.clone()),
        slice: lhs.slice,
        genvar_index: lhs.genvar_index.clone(),
        span: lhs.span,
    }
}

fn rename_step(step: &ForStep, peta: &HashMap<String, String>) -> ForStep {
    ForStep {
        lhs: rename_lvalue(&step.lhs, peta),
        rhs: rename_expr(&step.rhs, peta),
        span: step.span,
    }
}

fn rename_task(task: &SystemTask, peta: &HashMap<String, String>) -> SystemTask {
    SystemTask {
        kind: task.kind.clone(),
        args: task
            .args
            .iter()
            .map(|arg| match arg {
                SystemArg::Format(t) => SystemArg::Format(t.clone()),
                SystemArg::Value(e) => SystemArg::Value(rename_expr(e, peta)),
            })
            .collect(),
        condition: task.condition.as_ref().map(|c| rename_expr(c, peta)),
        span: task.span,
    }
}

/// Rename seluruh identifier di dalam ekspresi.
fn rename_expr(expr: &Expr, peta: &HashMap<String, String>) -> Expr {
    let nama = |n: &str| peta.get(n).cloned().unwrap_or_else(|| n.to_string());
    match expr {
        Expr::Ident { name, span } => Expr::ident(nama(name), *span),
        Expr::Number(v) => Expr::Number(*v),
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
            lhs: Box::new(rename_expr(lhs, peta)),
            rhs: Box::new(rename_expr(rhs, peta)),
            span: *span,
        },
        Expr::Unary { op, operand, span } => Expr::Unary {
            op: *op,
            operand: Box::new(rename_expr(operand, peta)),
            span: *span,
        },
        Expr::Ternary {
            condition,
            when_true,
            when_false,
            span,
        } => Expr::Ternary {
            condition: Box::new(rename_expr(condition, peta)),
            when_true: Box::new(rename_expr(when_true, peta)),
            when_false: Box::new(rename_expr(when_false, peta)),
            span: *span,
        },
        Expr::Select {
            base,
            msb,
            lsb,
            span,
        } => Expr::Select {
            base: Box::new(rename_expr(base, peta)),
            msb: *msb,
            lsb: *lsb,
            span: *span,
        },
        Expr::IndexDynamic { base, index, span } => Expr::IndexDynamic {
            base: Box::new(rename_expr(base, peta)),
            index: Box::new(rename_expr(index, peta)),
            span: *span,
        },
        Expr::Concat { items, span } => Expr::Concat {
            items: items.iter().map(|i| rename_expr(i, peta)).collect(),
            span: *span,
        },
        Expr::Replicate { count, value, span } => Expr::Replicate {
            count: Box::new(rename_expr(count, peta)),
            value: Box::new(rename_expr(value, peta)),
            span: *span,
        },
        Expr::SystemTime { span } => Expr::SystemTime { span: *span },
        Expr::FunctionCall { name, args, span } => Expr::FunctionCall {
            name: name.clone(),
            args: args.iter().map(|a| rename_expr(a, peta)).collect(),
            span: *span,
        },
        // LRM §6.14: nama tipe bawaan bukan sinyal, jadi tidak ikut di-rename.
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
            operand: Box::new(rename_expr(operand, peta)),
            span: *span,
        },
        Expr::SizeCast {
            width,
            operand,
            span,
        } => Expr::SizeCast {
            width: *width,
            operand: Box::new(rename_expr(operand, peta)),
            span: *span,
        },
        // LRM §6.14: nama tipe bukan sinyal, jadi tidak ikut di-rename; hanya
        // operandnya yang mengikuti substitusi argumen.
        Expr::Cast {
            type_name,
            operand,
            span,
        } => Expr::Cast {
            type_name: type_name.clone(),
            operand: Box::new(rename_expr(operand, peta)),
            span: *span,
        },
        Expr::SignCast {
            signed,
            operand,
            span,
        } => Expr::SignCast {
            signed: *signed,
            operand: Box::new(rename_expr(operand, peta)),
            span: *span,
        },
        Expr::Bits { operand, span } => Expr::Bits {
            operand: Box::new(rename_expr(operand, peta)),
            span: *span,
        },
    }
}

/// Kumpulkan nama seluruh deklarasi lokal di dalam badan statement.
fn kumpulkan_lokal(body: &[CombinationalStatement]) -> Vec<String> {
    let mut out = Vec::new();
    kumpulkan_lokal_dalam(body, &mut out);
    out
}

fn kumpulkan_lokal_dalam(body: &[CombinationalStatement], out: &mut Vec<String>) {
    for stmt in body {
        match stmt {
            CombinationalStatement::Decl(decl) => {
                if !out.contains(&decl.name) {
                    out.push(decl.name.clone());
                }
            }
            CombinationalStatement::Block { body, .. } => kumpulkan_lokal_dalam(body, out),
            CombinationalStatement::IfElse {
                then_branch,
                else_branch,
                ..
            } => {
                kumpulkan_lokal_dalam(then_branch, out);
                if let Some(branch) = else_branch {
                    kumpulkan_lokal_dalam(branch, out);
                }
            }
            CombinationalStatement::For { body, .. }
            | CombinationalStatement::While { body, .. }
            | CombinationalStatement::Repeat { body, .. }
            | CombinationalStatement::EventControl { body, .. } => kumpulkan_lokal_dalam(body, out),
            CombinationalStatement::Delay { body, .. } => {
                kumpulkan_lokal_dalam(std::slice::from_ref(body.as_ref()), out)
            }
            CombinationalStatement::Case { arms, .. } => {
                for arm in arms {
                    kumpulkan_lokal_dalam(&arm.body, out);
                }
            }
            CombinationalStatement::BlockingAssign { .. }
            | CombinationalStatement::NonBlockingAssign { .. }
            | CombinationalStatement::CompoundAssign { .. }
            | CombinationalStatement::SystemTask(_)
            | CombinationalStatement::TaskCall(_)
            | CombinationalStatement::Return { .. } => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sv_lexer::span::Span;

    fn ident(nama: &str) -> Expr {
        Expr::ident(nama, Span::dummy())
    }

    fn lvalue(nama: &str) -> Lvalue {
        Lvalue::simple(nama.to_string(), Span::dummy())
    }

    fn tabel() -> Substitution {
        Substitution::baru(
            vec!["a".to_string(), "q".to_string()],
            vec![ident("x"), ident("y")],
        )
    }

    #[test]
    fn formal_input_diganti_argumen() {
        let hasil = tabel().ganti(&ident("a"));
        assert_eq!(hasil, ident("x"));
    }

    #[test]
    fn nama_bukan_formal_tetap_dipertahankan() {
        let hasil = tabel().ganti(&ident("lain"));
        assert_eq!(hasil, ident("lain"));
    }

    #[test]
    fn substitusi_masuk_ke_seluruh_sub_ekspresi() {
        let expr = Expr::Binary {
            op: sv_ast::expression::BinaryOp::Add,
            lhs: Box::new(ident("a")),
            rhs: Box::new(Expr::Number(1)),
            span: Span::dummy(),
        };
        match tabel().ganti(&expr) {
            Expr::Binary { lhs, .. } => assert_eq!(*lhs, ident("x")),
            other => panic!("harus binary, dapat {other:?}"),
        }
    }

    #[test]
    fn lvalue_output_ditulis_ke_sinyal_pemanggil() {
        let hasil = tabel().ganti_lvalue(&lvalue("q"));
        assert_eq!(hasil.name, "y");
    }

    #[test]
    fn lvalue_bukan_formal_tetap_dipertahankan() {
        let hasil = tabel().ganti_lvalue(&lvalue("tmp"));
        assert_eq!(hasil.name, "tmp");
    }

    #[test]
    fn statement_task_menyerap_argumen_output() {
        let body = vec![CombinationalStatement::BlockingAssign {
            lhs: lvalue("q"),
            rhs: ident("a"),
            span: Span::dummy(),
        }];
        let hasil = tabel().ganti_statements(&body);
        match &hasil[0] {
            CombinationalStatement::BlockingAssign { lhs, rhs, .. } => {
                assert_eq!(lhs.name, "y");
                assert_eq!(*rhs, ident("x"));
            }
            other => panic!("harus blocking assign, dapat {other:?}"),
        }
    }

    #[test]
    fn argumen_panggilan_task_tidak_tersentuh_berulang() {
        // Argumen task sudah diekspansi oleh Inliner sebelum substitusi,
        // sehingga tidak perlu disubstitusi dua kali.
        let body = vec![CombinationalStatement::TaskCall(
            sv_ast::routine::TaskCall {
                name: "lain".to_string(),
                args: vec![ident("a")],
                span: Span::dummy(),
            },
        )];
        let hasil = tabel().ganti_statements(&body);
        match &hasil[0] {
            CombinationalStatement::TaskCall(call) => assert_eq!(call.args[0], ident("x")),
            other => panic!("harus task call, dapat {other:?}"),
        }
    }

    #[test]
    fn statement_sekuensial_tersubstitusi() {
        let stmt = SequentialStatement::BlockingAssign {
            lhs: lvalue("q"),
            rhs: ident("a"),
            span: Span::dummy(),
        };
        let hasil = tabel().ganti_seq(&stmt);
        match hasil {
            SequentialStatement::BlockingAssign { lhs, rhs, .. } => {
                assert_eq!(lhs.name, "y");
                assert_eq!(rhs, ident("x"));
            }
            other => panic!("harus blocking assign, dapat {other:?}"),
        }
    }
}
