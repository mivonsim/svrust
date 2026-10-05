// Tanggung jawab: parse daftar parameter module #(parameter W = 8).
use sv_ast::width::ParamDecl;
use sv_lexer::spanned::SpannedToken;
use sv_lexer::token::Token;

/// Parse `#(parameter A = 1, parameter B = 2)` setelah nama module.
/// Kembalikan daftar kosong bila tidak ada `#`. Konsumsi termasuk `#(...)`.
pub fn parse_param_list(tokens: &[SpannedToken]) -> Result<(Vec<ParamDecl>, usize), String> {
    if tokens.is_empty() || tokens[0].token != Token::Hash {
        return Ok((Vec::new(), 0));
    }
    let mut idx = 1;

    if idx >= tokens.len() || tokens[idx].token != Token::LParen {
        return Err("expected '(' setelah '#'".to_string());
    }
    idx += 1;

    let mut params = Vec::new();
    while idx < tokens.len() && tokens[idx].token != Token::RParen {
        if tokens[idx].token != Token::Parameter {
            return Err("expected 'parameter'".to_string());
        }
        idx += 1;

        if idx >= tokens.len() {
            return Err("expected nama parameter".to_string());
        }
        let (nama, span) = match &tokens[idx].token {
            Token::Ident(n) => (n.clone(), tokens[idx].span),
            _ => return Err("expected nama parameter".to_string()),
        };
        idx += 1;

        if idx >= tokens.len() || tokens[idx].token != Token::Eq {
            return Err("expected '=' pada parameter".to_string());
        }
        idx += 1;

        if idx >= tokens.len() {
            return Err("expected nilai default parameter".to_string());
        }
        let default = match &tokens[idx].token {
            Token::Number(n) => *n,
            // Sized literal juga sah sebagai nilai parameter, misal `8'd4`.
            Token::BasedNumber { value, .. } => *value,
            _ => return Err("nilai default parameter harus literal angka".to_string()),
        };
        idx += 1;

        if params.iter().any(|p: &ParamDecl| p.name == nama) {
            return Err(format!("parameter duplikat: {}", nama));
        }
        params.push(ParamDecl {
            name: nama,
            default,
            span,
        });

        if idx < tokens.len() && tokens[idx].token == Token::Comma {
            idx += 1;
        }
    }

    if idx >= tokens.len() {
        return Err("expected ')' penutup parameter".to_string());
    }
    idx += 1; // lewati ')'

    Ok((params, idx))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sv_lexer::lex;

    fn parse(sumber: &str) -> (Vec<ParamDecl>, usize) {
        let tokens = lex(sumber).unwrap();
        parse_param_list(&tokens).unwrap()
    }

    #[test]
    fn tanpa_hash_hasil_kosong() {
        let tokens = lex("module m;").unwrap();
        let (params, makan) = parse_param_list(&tokens).unwrap();
        assert!(params.is_empty());
        assert_eq!(makan, 0);
    }

    #[test]
    fn satu_parameter() {
        let (params, _) = parse("#(parameter WIDTH = 8)");
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].name, "WIDTH");
        assert_eq!(params[0].default, 8);
    }

    #[test]
    fn dua_parameter() {
        let (params, _) = parse("#(parameter WIDTH = 8, parameter DEPTH = 4)");
        assert_eq!(params.len(), 2);
        assert_eq!(params[1].default, 4);
    }

    #[test]
    fn duplikat_ditolak() {
        let tokens = lex("#(parameter W = 8, parameter W = 4)").unwrap();
        assert!(parse_param_list(&tokens).is_err());
    }

    #[test]
    fn default_harus_angka() {
        let tokens = lex("#(parameter W = foo)").unwrap();
        assert!(parse_param_list(&tokens).is_err());
    }

    #[test]
    fn sized_literal_sah_sebagai_default() {
        // BUG-11: `8'd4` sebelumnya ditolak karena hanya `Token::Number` dibaca.
        let (params, _) = parse("#(parameter W = 8'd4)");
        assert_eq!(params[0].default, 4);
    }

    #[test]
    fn sized_literal_lebar_panjang_sah() {
        let (params, _) = parse("#(parameter W = 32'h10)");
        assert_eq!(params[0].default, 16);
    }

    #[test]
    fn dua_parameter_satu_sized_literal() {
        let (params, _) = parse("#(parameter W = 8'd8, parameter D = 4'd2)");
        assert_eq!(params[0].default, 8);
        assert_eq!(params[1].default, 2);
    }
}
