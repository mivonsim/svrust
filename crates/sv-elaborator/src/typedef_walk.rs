// Tanggung jawab: menyubstitusi literal enum pada seluruh statement module.
use crate::error::ElaborateError;
use crate::typedef::TypeTable;
use sv_ast::combinational::CombinationalStatement;
use sv_ast::module::Module;
use sv_ast::statement::{SequentialStatement, Statement};

/// Kembalikan salinan module dengan setiap nama anggota enum pada statement
/// diganti menjadi konstanta berukuran lebar tipe enum (LRM §6.7).
///
/// Dijalankan sekali per module sebelum statement dielaborasi, sehingga
/// `lower_expression` tidak perlu tahu soal enumerasi sama sekali.
pub fn expand_literals(module: &Module, table: &TypeTable) -> Result<Module, ElaborateError> {
    let mut out = module.clone();
    out.statements = module
        .statements
        .iter()
        .map(|stmt| statement(stmt, table))
        .collect::<Result<Vec<_>, ElaborateError>>()?;
    Ok(out)
}

fn statement(stmt: &Statement, table: &TypeTable) -> Result<Statement, ElaborateError> {
    Ok(match stmt {
        Statement::ContinuousAssign { lhs, rhs, span } => Statement::ContinuousAssign {
            lhs: lhs.clone(),
            rhs: table.ganti_literal(rhs),
            span: *span,
        },
        Statement::AlwaysFf { events, body, span } => Statement::AlwaysFf {
            events: events.clone(),
            body: body
                .iter()
                .map(|s| seq(s, table))
                .collect::<Result<Vec<_>, ElaborateError>>()?,
            span: *span,
        },
        Statement::AlwaysComb { body, span } => Statement::AlwaysComb {
            body: comb_list(body, table)?,
            span: *span,
        },
        Statement::Initial { body, span } => Statement::Initial {
            body: comb_list(body, table)?,
            span: *span,
        },
    })
}

fn seq_list(
    body: &[SequentialStatement],
    table: &TypeTable,
) -> Result<Vec<SequentialStatement>, ElaborateError> {
    body.iter().map(|s| seq(s, table)).collect()
}

fn seq(
    stmt: &SequentialStatement,
    table: &TypeTable,
) -> Result<SequentialStatement, ElaborateError> {
    Ok(match stmt {
        SequentialStatement::NonBlockingAssign { lhs, rhs, span } => {
            SequentialStatement::NonBlockingAssign {
                lhs: lhs.clone(),
                rhs: table.ganti_literal(rhs),
                span: *span,
            }
        }
        SequentialStatement::BlockingAssign { lhs, rhs, span } => {
            SequentialStatement::BlockingAssign {
                lhs: lhs.clone(),
                rhs: table.ganti_literal(rhs),
                span: *span,
            }
        }
        SequentialStatement::IfElse {
            condition,
            then_branch,
            else_branch,
            span,
        } => SequentialStatement::IfElse {
            condition: table.ganti_literal(condition),
            then_branch: seq_list(then_branch, table)?,
            else_branch: match else_branch {
                Some(branch) => Some(seq_list(branch, table)?),
                None => None,
            },
            span: *span,
        },
        SequentialStatement::Decl(decl) => {
            let mut baru = decl.clone();
            baru.init = baru.init.as_ref().map(|e| table.ganti_literal(e));
            SequentialStatement::Decl(baru)
        }
        SequentialStatement::SystemTask(task) => {
            SequentialStatement::SystemTask(task_literal(task, table))
        }
        SequentialStatement::TaskCall(call) => {
            SequentialStatement::TaskCall(sv_ast::routine::TaskCall {
                name: call.name.clone(),
                args: call.args.iter().map(|a| table.ganti_literal(a)).collect(),
                span: call.span,
            })
        }
        SequentialStatement::Return { value, span } => SequentialStatement::Return {
            value: value.as_ref().map(|v| table.ganti_literal(v)),
            span: *span,
        },
    })
}

fn comb_list(
    body: &[CombinationalStatement],
    table: &TypeTable,
) -> Result<Vec<CombinationalStatement>, ElaborateError> {
    body.iter().map(|s| comb(s, table)).collect()
}

fn comb(
    stmt: &CombinationalStatement,
    table: &TypeTable,
) -> Result<CombinationalStatement, ElaborateError> {
    use sv_ast::combinational::CaseArm;
    Ok(match stmt {
        CombinationalStatement::Decl(decl) => {
            let mut baru = decl.clone();
            baru.init = baru.init.as_ref().map(|e| table.ganti_literal(e));
            CombinationalStatement::Decl(baru)
        }
        CombinationalStatement::BlockingAssign { lhs, rhs, span } => {
            CombinationalStatement::BlockingAssign {
                lhs: lhs.clone(),
                rhs: table.ganti_literal(rhs),
                span: *span,
            }
        }
        CombinationalStatement::NonBlockingAssign { lhs, rhs, span } => {
            CombinationalStatement::NonBlockingAssign {
                lhs: lhs.clone(),
                rhs: table.ganti_literal(rhs),
                span: *span,
            }
        }
        CombinationalStatement::CompoundAssign { lhs, op, rhs, span } => {
            CombinationalStatement::CompoundAssign {
                lhs: lhs.clone(),
                op: *op,
                rhs: table.ganti_literal(rhs),
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
                lhs: init.lhs.clone(),
                rhs: table.ganti_literal(&init.rhs),
                span: init.span,
            },
            condition: table.ganti_literal(condition),
            step: sv_ast::combinational::ForStep {
                lhs: step.lhs.clone(),
                rhs: table.ganti_literal(&step.rhs),
                span: step.span,
            },
            body: comb_list(body, table)?,
            span: *span,
        },
        CombinationalStatement::While {
            condition,
            body,
            span,
        } => CombinationalStatement::While {
            condition: table.ganti_literal(condition),
            body: comb_list(body, table)?,
            span: *span,
        },
        CombinationalStatement::Repeat { count, body, span } => CombinationalStatement::Repeat {
            count: table.ganti_literal(count),
            body: comb_list(body, table)?,
            span: *span,
        },
        CombinationalStatement::Case {
            selector,
            arms,
            kind,
            span,
        } => CombinationalStatement::Case {
            selector: table.ganti_literal(selector),
            arms: arms
                .iter()
                .map(|arm| {
                    Ok(CaseArm {
                        labels: arm.labels.iter().map(|l| table.ganti_literal(l)).collect(),
                        is_default: arm.is_default,
                        body: comb_list(&arm.body, table)?,
                        span: arm.span,
                    })
                })
                .collect::<Result<Vec<_>, ElaborateError>>()?,
            kind: *kind,
            span: *span,
        },
        CombinationalStatement::IfElse {
            condition,
            then_branch,
            else_branch,
            span,
        } => CombinationalStatement::IfElse {
            condition: table.ganti_literal(condition),
            then_branch: comb_list(then_branch, table)?,
            else_branch: match else_branch {
                Some(branch) => Some(comb_list(branch, table)?),
                None => None,
            },
            span: *span,
        },
        CombinationalStatement::Block { body, span } => CombinationalStatement::Block {
            body: comb_list(body, table)?,
            span: *span,
        },
        CombinationalStatement::EventControl { events, body, span } => {
            CombinationalStatement::EventControl {
                events: events.clone(),
                body: comb_list(body, table)?,
                span: *span,
            }
        }
        CombinationalStatement::Delay {
            amount,
            unit,
            body,
            span,
        } => CombinationalStatement::Delay {
            amount: table.ganti_literal(amount),
            unit: *unit,
            body: Box::new(comb(body, table)?),
            span: *span,
        },
        CombinationalStatement::SystemTask(task) => {
            CombinationalStatement::SystemTask(task_literal(task, table))
        }
        CombinationalStatement::TaskCall(call) => {
            CombinationalStatement::TaskCall(sv_ast::routine::TaskCall {
                name: call.name.clone(),
                args: call.args.iter().map(|a| table.ganti_literal(a)).collect(),
                span: call.span,
            })
        }
        CombinationalStatement::Return { value, span } => CombinationalStatement::Return {
            value: value.as_ref().map(|v| table.ganti_literal(v)),
            span: *span,
        },
    })
}

fn task_literal(
    task: &sv_ast::system_task::SystemTask,
    table: &TypeTable,
) -> sv_ast::system_task::SystemTask {
    sv_ast::system_task::SystemTask {
        kind: task.kind.clone(),
        args: task
            .args
            .iter()
            .map(|arg| match arg {
                sv_ast::system_task::SystemArg::Format(t) => {
                    sv_ast::system_task::SystemArg::Format(t.clone())
                }
                sv_ast::system_task::SystemArg::Value(e) => {
                    sv_ast::system_task::SystemArg::Value(table.ganti_literal(e))
                }
            })
            .collect(),
        condition: task.condition.as_ref().map(|c| table.ganti_literal(c)),
        time_scale: task.time_scale,
        span: task.span,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sv_ast::typedef::{EnumMember, TypeDefBody, TypeDefDecl};
    use sv_ast::width::WidthExpr;
    use sv_lexer::span::Span;
    use sv_lexer::spanned::SpannedToken;

    fn enum_table() -> TypeTable {
        let decl = TypeDefDecl {
            name: "state_t".to_string(),
            body: TypeDefBody::Enum {
                base_width: WidthExpr::Literal(2),
                members: vec![
                    EnumMember {
                        name: "IDLE".to_string(),
                        value: None,
                        span: Span::dummy(),
                    },
                    EnumMember {
                        name: "RUN".to_string(),
                        value: None,
                        span: Span::dummy(),
                    },
                ],
            },
            span: Span::dummy(),
        };
        let params = crate::param::ParamTable::baru(&[]).expect("params");
        TypeTable::baru(&[decl], &params).expect("tabel")
    }

    fn module_dari(src: &str) -> Module {
        let tokens: Vec<SpannedToken> = sv_lexer::lex(src).expect("lex");
        sv_parser::parse_module(&tokens).expect("parse")
    }

    /// Ambil RHS statement pertama pada proses/module pertama.
    fn rhs_pertama(module: &Module) -> sv_ast::expression::Expr {
        match &module.statements[0] {
            Statement::ContinuousAssign { rhs, .. } => rhs.clone(),
            Statement::Initial { body, .. } => match &body[0] {
                CombinationalStatement::BlockingAssign { rhs, .. } => rhs.clone(),
                other => panic!("harus blocking assign, dapat {other:?}"),
            },
            other => panic!("harus statement, dapat {other:?}"),
        }
    }

    #[test]
    fn literal_enum_pada_continuous_assign_terganti() {
        let src = "module m(output logic [7:0] y);\n\
                   assign y = RUN;\n\
                   endmodule\n";
        let out = expand_literals(&module_dari(src), &enum_table()).expect("expand");
        assert_eq!(
            rhs_pertama(&out),
            sv_ast::expression::Expr::Sized {
                value: 1,
                width: 2,
                signed: false,
                unknown_mask: 0,
                zmask: 0,
            }
        );
    }

    #[test]
    fn literal_enum_pada_initial_terganti() {
        let src = "module m;\n\
                   logic [7:0] y;\n\
                   initial y = IDLE;\n\
                   endmodule\n";
        let out = expand_literals(&module_dari(src), &enum_table()).expect("expand");
        assert_eq!(
            rhs_pertama(&out),
            sv_ast::expression::Expr::Sized {
                value: 0,
                width: 2,
                signed: false,
                unknown_mask: 0,
                zmask: 0,
            }
        );
    }

    #[test]
    fn literal_enum_dalam_perbandingan_terganti_semua_sisi() {
        let src = "module m(output logic [7:0] y);\n\
                   assign y = (q == RUN) ? RUN : IDLE;\n\
                   endmodule\n";
        let out = expand_literals(&module_dari(src), &enum_table()).expect("expand");
        // Tidak boleh ada `Ident` bernama RUN/IDLE yang tersisa.
        let teks = format!("{:?}", rhs_pertama(&out));
        assert!(!teks.contains("RUN"), "sisa: {teks}");
        assert!(!teks.contains("IDLE"), "sisa: {teks}");
    }

    #[test]
    fn nama_bukan_enum_tidak_diubah() {
        let src = "module m(output logic [7:0] y);\n\
                   assign y = q;\n\
                   endmodule\n";
        let module = module_dari(src);
        let out = expand_literals(&module, &enum_table()).expect("expand");
        assert_eq!(rhs_pertama(&out), rhs_pertama(&module));
    }

    #[test]
    fn literal_enum_dalam_argumen_task_terganti() {
        let src = "module m(output logic [7:0] y);\n\
                   task t(input [7:0] v);\n\
                   endtask\n\
                   initial t(RUN);\n\
                   endmodule\n";
        let out = expand_literals(&module_dari(src), &enum_table()).expect("expand");
        let Statement::Initial { body, .. } = &out.statements[0] else {
            panic!("harus initial");
        };
        match &body[0] {
            CombinationalStatement::TaskCall(call) => {
                assert_eq!(
                    call.args[0],
                    sv_ast::expression::Expr::Sized {
                        value: 1,
                        width: 2,
                        signed: false,
                        unknown_mask: 0,
                        zmask: 0,
                    }
                );
            }
            other => panic!("harus task call, dapat {other:?}"),
        }
    }

    #[test]
    fn literal_enum_dalam_argumen_system_task_terganti() {
        let src = "module m;\n\
                   initial $display(\"%d\", RUN);\n\
                   endmodule\n";
        let out = expand_literals(&module_dari(src), &enum_table()).expect("expand");
        let Statement::Initial { body, .. } = &out.statements[0] else {
            panic!("harus initial");
        };
        match &body[0] {
            CombinationalStatement::SystemTask(task) => {
                assert_eq!(
                    task.args[1],
                    sv_ast::system_task::SystemArg::Value(sv_ast::expression::Expr::Sized {
                        value: 1,
                        width: 2,
                        signed: false,
                        unknown_mask: 0,
                        zmask: 0,
                    })
                );
            }
            other => panic!("harus system task, dapat {other:?}"),
        }
    }
}
