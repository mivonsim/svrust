// Tanggung jawab: pub API crate sv-opt.
mod fold;
mod pass;

pub use fold::fold_expr;
pub use pass::{optimize, Report};

#[cfg(test)]
mod tests {
    use super::*;
    use sv_ir::process::{Process, ProcessKind, Statement};
    use sv_ir::system_task::{SystemArg, SystemTaskKind};
    use sv_ir::{AssignStyle, Assignment, BinOp, DataType, Design, Expr};

    fn kon(v: u64, w: u32) -> Expr {
        Expr::constant(v, DataType::logic(w))
    }

    fn bin(op: BinOp, a: Expr, b: Expr, w: u32) -> Expr {
        Expr::Bin {
            op,
            lhs: Box::new(a),
            rhs: Box::new(b),
            data_type: DataType::logic(w),
        }
    }

    fn blocking(target: u32, value: Expr) -> Assignment {
        Assignment {
            target,
            slice: None,
            value,
            style: AssignStyle::Blocking,
        }
    }

    fn desain_dengan(body: Vec<Statement>) -> Design {
        let mut design = sv_ir::Design::new("m");
        design.processes.push(Process {
            name: "p0".to_string(),
            kind: ProcessKind::Combinational,
            sensitivity: Vec::new(),
            body,
            span: Default::default(),
        });
        design
    }

    fn nilai_rhs(d: &Design) -> Expr {
        match &d.processes[0].body[0] {
            Statement::Assign { assignment, .. } => assignment.value.clone(),
            other => panic!("harus assignment, dapat {other:?}"),
        }
    }

    #[test]
    fn konstanta_pada_assign_dilipat() {
        let mut d = desain_dengan(vec![Statement::Assign {
            assignment: blocking(0, bin(BinOp::Add, kon(2, 8), kon(3, 8), 8)),
            span: Default::default(),
        }]);
        let laporan = optimize(&mut d);
        assert_eq!(laporan.folded, 1);
        assert_eq!(laporan.processes, 1);
        assert_eq!(nilai_rhs(&d), kon(5, 8));
    }

    #[test]
    fn kondisi_if_dilipat() {
        let mut d = desain_dengan(vec![Statement::If {
            condition: bin(BinOp::Eq, kon(3, 8), kon(3, 8), 1),
            then_branch: vec![],
            else_branch: vec![],
            span: Default::default(),
        }]);
        let laporan = optimize(&mut d);
        assert_eq!(laporan.folded, 1);
        match &d.processes[0].body[0] {
            Statement::If { condition, .. } => assert_eq!(*condition, kon(1, 1)),
            other => panic!("harus If, dapat {other:?}"),
        }
    }

    #[test]
    fn label_case_dilipat() {
        let mut d = desain_dengan(vec![Statement::Case {
            selector: kon(0, 8),
            arms: vec![sv_ir::CaseArm {
                labels: vec![bin(BinOp::Add, kon(1, 8), kon(2, 8), 8)],
                is_default: false,
                body: vec![],
            }],
            kind: sv_ir::CaseKind::Exact,
            span: Default::default(),
        }]);
        assert_eq!(optimize(&mut d).folded, 1);
        match &d.processes[0].body[0] {
            Statement::Case { arms, .. } => assert_eq!(arms[0].labels[0], kon(3, 8)),
            other => panic!("harus Case, dapat {other:?}"),
        }
    }

    #[test]
    fn argumen_system_task_dilipat() {
        let mut d = desain_dengan(vec![Statement::SystemTask {
            kind: SystemTaskKind::Display,
            args: vec![
                SystemArg::Format("v=%d".to_string()),
                SystemArg::Value(bin(BinOp::Mul, kon(3, 8), kon(4, 8), 8)),
            ],
            condition: None,
            time_scale: Default::default(),
            span: Default::default(),
        }]);
        assert_eq!(optimize(&mut d).folded, 1);
        match &d.processes[0].body[0] {
            Statement::SystemTask { args, .. } => match &args[1] {
                SystemArg::Value(e) => assert_eq!(*e, kon(12, 8)),
                other => panic!("harus Value, dapat {other:?}"),
            },
            other => panic!("harus SystemTask, dapat {other:?}"),
        }
    }

    #[test]
    fn statement_dengan_sinyal_tidak_diubah() {
        let mut d = desain_dengan(vec![Statement::Assign {
            assignment: blocking(
                0,
                bin(
                    BinOp::Add,
                    kon(1, 8),
                    Expr::signal_ref(2, DataType::logic(8)),
                    8,
                ),
            ),
            span: Default::default(),
        }]);
        assert_eq!(optimize(&mut d).folded, 0);
        assert!(matches!(nilai_rhs(&d), Expr::Bin { .. }));
    }

    #[test]
    fn design_kosong_lolos() {
        let mut d = Design::new("m");
        let laporan = optimize(&mut d);
        assert_eq!(
            laporan,
            Report {
                folded: 0,
                processes: 0
            }
        );
    }
}
