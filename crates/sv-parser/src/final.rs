// Tanggung jawab: parse blok final untuk testbench.
use sv_ast::statement::Statement;
use sv_lexer::spanned::SpannedToken;
use sv_lexer::token::Token;

/// Parse `final begin ... end` atau `final <statement>`.
pub fn parse_final(tokens: &[SpannedToken]) -> Result<(Statement, usize), String> {
    if tokens.is_empty() || tokens[0].token != Token::Final {
        return Err("expected 'final'".to_string());
    }
    let span = tokens[0].span;
    let mut idx = 1;

    if idx >= tokens.len() {
        return Err("expected final body".to_string());
    }

    let body = if tokens[idx].token == Token::Begin {
        idx += 1;
        let (stmts, consumed) = super::combinational::parse_combinational_block(&tokens[idx..])?;
        idx += consumed;
        if idx >= tokens.len() || tokens[idx].token != Token::End {
            return Err("expected 'end'".to_string());
        }
        idx += 1;
        stmts
    } else {
        let (stmt, consumed) = super::combinational::parse_combinational_statement(&tokens[idx..])?;
        idx += consumed;
        vec![stmt]
    };

    Ok((Statement::Final { body, span }, idx))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sv_lexer::lex;

    #[test]
    fn final_blok_diparse() {
        let tokens = lex("final begin y = 8'd1; end").expect("lex");
        let (stmt, _) = parse_final(&tokens).expect("parse");
        match stmt {
            Statement::Final { body, .. } => assert_eq!(body.len(), 1),
            lain => panic!("dapat {:?}", lain),
        }
    }

    #[test]
    fn pertama_token_harus_final() {
        let tokens = lex("initial begin y = 1; end").expect("lex");
        let err = parse_final(&tokens).expect_err("harus gagal");
        assert!(err.contains("expected 'final'"), "pesan: {err}");
    }
}
