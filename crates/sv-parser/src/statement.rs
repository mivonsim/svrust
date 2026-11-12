// Tanggung jawab: parse continuous assignment (assign lhs = rhs).
use crate::pesan;
use sv_ast::statement::Statement;
use sv_lexer::spanned::SpannedToken;
use sv_lexer::token::Token;

pub fn parse_continuous_assign(tokens: &[SpannedToken]) -> Result<(Statement, usize), String> {
    if tokens.is_empty() || tokens[0].token != Token::Assign {
        return Err("expected 'assign'".to_string());
    }
    let span = tokens[0].span;
    let mut idx = 1; // skip 'assign'
                     // Drive strength specification: `assign (strength0, strength1) lhs = rhs;`
                     // Ini bukan standar SystemVerilog tetapi dipakai OpenTitan DV. Lewati
                     // hingga ')' penutup.
    if tokens.get(idx).map(|t| &t.token) == Some(&Token::LParen) {
        let mut depth = 1u32;
        idx += 1;
        while idx < tokens.len() && depth > 0 {
            match &tokens[idx].token {
                Token::LParen => depth += 1,
                Token::RParen => depth -= 1,
                _ => {}
            }
            idx += 1;
        }
    }
    // LRM §11.8.1: concatenation pada LHS, misal `{a, b} = {c, d}`,
    // saat ini hanya LHS pertama yang dipakai di level modul.
    let lhss = if tokens.get(idx).map(|t| &t.token) == Some(&Token::LBrace) {
        let (lhss, consumed) = super::combinational::parse_concat_lvalue(&tokens[idx..])?;
        idx += consumed;
        lhss
    } else {
        let (lhs, consumed) = super::lvalue::parse_lvalue(&tokens[idx..])?;
        idx += consumed;
        vec![lhs]
    };
    let lhs = lhss
        .into_iter()
        .next()
        .ok_or_else(|| "expected LHS signal name".to_string())?;
    if idx >= tokens.len() || tokens[idx].token != Token::Eq {
        return Err(pesan::harap(tokens, idx, "expected '='"));
    }
    idx += 1;
    // RHS: expression
    let (rhs, consumed) = super::expression::parse_expression(&tokens[idx..])?;
    idx += consumed;
    // Expect ';'
    if idx >= tokens.len() || tokens[idx].token != Token::Semi {
        return Err(pesan::harap(tokens, idx, "expected ';'"));
    }
    idx += 1;

    Ok((Statement::ContinuousAssign { lhs, rhs, span }, idx))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sv_ast::lvalue::{Lvalue, Slice};

    #[test]
    fn error_assign_tanpa_sama_membawa_posisi_token_penyebab() {
        let tokens = sv_lexer::lex("assign a b;").expect("lex");
        let err = parse_continuous_assign(&tokens).expect_err("harus error");
        assert!(err.contains("Ident(\"b\")"), "{err}");
        assert!(err.ends_with("[pos=1:10]"), "{err}");
    }

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
