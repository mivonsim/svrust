// Tanggung jawab: traversal pass pelipatan konstanta pada seluruh design IR.
use sv_ir::process::Statement;
use sv_ir::system_task::SystemArg;
use sv_ir::{Assignment, Design};

use crate::fold::fold_expr;

/// Ringkasan hasil satu pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Report {
    /// Banyak ekspresi yang berubah menjadi konstanta.
    pub folded: u32,
    /// Banyak proses yang diperiksa.
    pub processes: u32,
}

/// Jalankan pelipatan konstanta pada seluruh design, in-place.
///
/// Aman terhadap semantik: hanya ekspresi yang seluruh operand-nya konstanta
/// yang dilipat, dan perhitungannya meniru evaluator runtime bit demi bit.
/// Syarat "hasil muat dalam lebar tipe" juga diterapkan sebagai pengaman.
pub fn optimize(design: &mut Design) -> Report {
    let mut report = Report::default();
    for process in &mut design.processes {
        report.processes += 1;
        report.folded += fold_statements(&mut process.body);
    }
    report
}

fn fold_statements(body: &mut [Statement]) -> u32 {
    let mut n = 0;
    for stmt in body {
        n += fold_statement(stmt);
    }
    n
}

fn fold_assignment(a: &mut Assignment) -> u32 {
    fold_expr(&mut a.value)
}

fn fold_statement(stmt: &mut Statement) -> u32 {
    match stmt {
        Statement::Assign { assignment, .. } => fold_assignment(assignment),
        Statement::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            let mut n = fold_expr(condition);
            n += fold_statements(then_branch);
            n += fold_statements(else_branch);
            n
        }
        Statement::Case { selector, arms, .. } => {
            let mut n = fold_expr(selector);
            for arm in arms.iter_mut() {
                for label in &mut arm.labels {
                    n += fold_expr(label);
                }
                n += fold_statements(&mut arm.body);
            }
            n
        }
        Statement::For {
            init,
            condition,
            step,
            body,
            ..
        } => {
            let mut n = fold_assignment(init);
            n += fold_assignment(step);
            n += fold_expr(condition);
            n += fold_statements(body);
            n
        }
        Statement::Repeat { count, body, .. } => {
            let mut n = fold_expr(count);
            n += fold_statements(body);
            n
        }
        Statement::While {
            condition, body, ..
        } => {
            let mut n = fold_expr(condition);
            n += fold_statements(body);
            n
        }
        Statement::SystemTask {
            args, condition, ..
        } => {
            let mut n = 0;
            for arg in args {
                if let SystemArg::Value(expr) = arg {
                    n += fold_expr(expr);
                }
            }
            if let Some(cond) = condition {
                n += fold_expr(cond);
            }
            n
        }
        Statement::EventControl { body, .. } => fold_statements(body),
        Statement::Delay { amount, body, .. } => {
            let mut n = fold_expr(amount);
            n += fold_statement(body);
            n
        }
        Statement::Block { body, .. } => fold_statements(body),
        Statement::Noop { .. } => 0,
    }
}
