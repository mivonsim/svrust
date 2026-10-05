// Tanggung jawab: parse continuous assignment (assign lhs = rhs).
use sv_ast::statement::Statement;
use sv_lexer::spanned::SpannedToken;
use sv_lexer::token::Token;

pub fn parse_continuous_assign(tokens: &[SpannedToken]) -> Result<(Statement, usize), String> {
    if tokens.is_empty() || tokens[0].token != Token::Assign {
        return Err("expected 'assign'".to_string());
    }
    let span = tokens[0].span;
    let mut idx = 1;

    // LHS: signal name, opsional dengan irisan `nama[msb:lsb]`.
    let (lhs, consumed) = super::lvalue::parse_lvalue(&tokens[idx..])?;
    idx += consumed;

    // Expect '='
    if idx >= tokens.len() || tokens[idx].token != Token::Eq {
        return Err("expected '='".to_string());
    }
    idx += 1;

    // RHS: expression
    let (rhs, consumed) = super::expression::parse_expression(&tokens[idx..])?;
    idx += consumed;

    // Expect ';'
    if idx >= tokens.len() || tokens[idx].token != Token::Semi {
        return Err("expected ';'".to_string());
    }
    idx += 1;

    Ok((Statement::ContinuousAssign { lhs, rhs, span }, idx))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sv_ast::lvalue::{Lvalue, Slice};

    fn parse(src: &str) -> Statement {
        let tokens = sv_lexer::lex(src).expect("lex");
        let (stmt, _) = parse_continuous_assign(&tokens).expect("parse");
        stmt
    }

    fn lhs_dari(stmt: Statement) -> Lvalue {
        match stmt {
            Statement::ContinuousAssign { lhs, .. } => lhs,
            other => panic!("expected continuous assign, dapat {other:?}"),
        }
    }

    #[test]
    fn assign_sinyal_penuh() {
        let lhs = lhs_dari(parse("assign y = a;"));
        assert_eq!(lhs.name, "y");
        assert!(!lhs.is_sliced());
    }

    #[test]
    fn assign_part_select_lhs() {
        let lhs = lhs_dari(parse("assign y[7:4] = a;"));
        assert_eq!(lhs.name, "y");
        assert_eq!(lhs.slice, Some(Slice { msb: 7, lsb: 4 }));
    }

    #[test]
    fn assign_bit_select_lhs() {
        let lhs = lhs_dari(parse("assign y[0] = a;"));
        assert_eq!(lhs.slice, Some(Slice { msb: 0, lsb: 0 }));
    }

    #[test]
    fn assign_irisan_dengan_rhs_select() {
        let src = "assign y[3:0] = a[7:4];";
        match parse(src) {
            Statement::ContinuousAssign { lhs, rhs, .. } => {
                assert_eq!(lhs.slice, Some(Slice { msb: 3, lsb: 0 }));
                assert!(matches!(rhs, sv_ast::expression::Expr::Select { .. }));
            }
            other => panic!("expected continuous assign, dapat {other:?}"),
        }
    }

    #[test]
    fn assign_lhs_tidak_tertutup_ditolak() {
        let tokens = sv_lexer::lex("assign y[3:0 = a;").expect("lex");
        assert!(parse_continuous_assign(&tokens).is_err());
    }

    #[test]
    fn assign_lhs_irisan_terbalik_ditolak() {
        let tokens = sv_lexer::lex("assign y[0:3] = a;").expect("lex");
        assert!(parse_continuous_assign(&tokens).is_err());
    }
}
