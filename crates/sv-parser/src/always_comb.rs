// Tanggung jawab: parse always_comb.
use sv_ast::combinational::CombinationalStatement;
use sv_ast::statement::Statement;
use sv_lexer::spanned::SpannedToken;
use sv_lexer::token::Token;

pub fn parse_always_comb(tokens: &[SpannedToken]) -> Result<(Statement, usize), String> {
    if tokens.is_empty() || tokens[0].token != Token::AlwaysComb {
        return Err("expected 'always_comb'".to_string());
    }
    let span = tokens[0].span;
    let mut idx = 1;

    // `always_comb @(a, b)` — daftar sensitivitas boleh ditulis tapi tidak
    // mengubah perilaku: simulator mengevaluasi ulang pada setiap langkah.
    if idx < tokens.len() && tokens[idx].token == Token::At {
        idx += 1;
        if idx >= tokens.len() || tokens[idx].token != Token::LParen {
            return Err("expected '(' after @".to_string());
        }
        idx += 1;
        while idx < tokens.len() && tokens[idx].token != Token::RParen {
            idx += 1;
        }
        if idx >= tokens.len() {
            return Err("expected ')'".to_string());
        }
        idx += 1;
    }

    let (body, next) = parse_comb_body(tokens, idx)?;
    Ok((Statement::AlwaysComb { body, span }, next))
}

/// Parse body proses combinational: blok `begin...end` atau satu statement.
///
/// Mengembalikan daftar statement dan jumlah token yang dikonsumsi. Dipakai
/// oleh `parse_always_comb` dan parser `always` polos supaya aturan body
/// hanya ditulis sekali.
pub(super) fn parse_comb_body(
    tokens: &[SpannedToken],
    mut idx: usize,
) -> Result<(Vec<CombinationalStatement>, usize), String> {
    if idx >= tokens.len() {
        return Err("expected body".to_string());
    }

    if tokens[idx].token == Token::Begin {
        idx += 1;
        let (stmts, consumed) = super::combinational::parse_combinational_block(&tokens[idx..])?;
        idx += consumed;
        if idx >= tokens.len() || tokens[idx].token != Token::End {
            return Err("expected 'end'".to_string());
        }
        idx += 1;
        return Ok((stmts, idx));
    }

    let (stmt, consumed) = super::combinational::parse_combinational_statement(&tokens[idx..])?;
    idx += consumed;
    Ok((vec![stmt], idx))
}
