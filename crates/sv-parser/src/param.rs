// Tanggung jawab: parse daftar parameter module #(parameter W = 8).
use crate::pesan;
use sv_ast::width::ParamDecl;
use sv_lexer::spanned::SpannedToken;
use sv_lexer::token::Token;

/// Parse `#(parameter A = 1, localparam B = A*2)` setelah nama module.
///
/// `localparam` boleh muncul di daftar parameter port (LRM §6.20.1). Nilainya
/// adalah ekspresi yang dievaluasi elaborator setelah parameter yang
/// mendahuluinya, jadi hasilnya dikembalikan terpisah dari `ParamDecl`.
///
/// Kembalikan daftar kosong bila tidak ada `#`. Konsumsi termasuk `#(...)`.
pub fn parse_param_list(
    tokens: &[SpannedToken],
) -> Result<(Vec<ParamDecl>, Vec<sv_ast::width::LocalParamDecl>, usize), String> {
    if tokens.is_empty() || tokens[0].token != Token::Hash {
        return Ok((Vec::new(), Vec::new(), 0));
    }
    let mut localparams: Vec<sv_ast::width::LocalParamDecl> = Vec::new();
    let mut idx = 1;

    if idx >= tokens.len() || tokens[idx].token != Token::LParen {
        return Err(pesan::harap(tokens, idx, "expected '(' setelah '#'"));
    }
    idx += 1;

    let mut params = Vec::new();
    while idx < tokens.len() && tokens[idx].token != Token::RParen {
        // `localparam` di dalam `#(...)` adalah konstanta terkunci yang tidak
        // boleh di-override saat instansiasi (LRM §6.20.1).
        // SV juga mengizinkan parameter tanpa kata kunci `parameter`,
        // misal `#(lc_tx_t ActVal = On)` (LRM §6.20.1).
        let is_local = match tokens[idx].token {
            Token::Parameter => false,
            Token::LocalParam => true,
            ref lain => {
                // Kalau bukan `parameter`/`localparam`, coba deteksi apakah
                // ini parameter tanpa kata kunci: `tipe nama = nilai` atau
                // `tipe nama[dim] = nilai`.
                let looks_like_param = matches!(lain, Token::Ident(_))
                    || sv_lexer::token::tipe::bisa_mulai_deklarasi(lain);
                if looks_like_param {
                    // Parameter tanpa kata kunci — tidak ada `is_local`.
                    false
                } else {
                    return Err(pesan::tandai(
                        tokens,
                        idx,
                        format!(
                            "expected 'parameter' atau 'localparam', ditemukan {:?}",
                            lain
                        ),
                    ));
                }
            }
        };
        // Kalau token awal bukan `parameter`/`localparam`, jangan maju idx
        // (biarkan parsing tipe di bawah menangani).
        if matches!(tokens[idx].token, Token::Parameter | Token::LocalParam) {
            idx += 1;
        }

        if idx >= tokens.len() {
            return Err(pesan::harap(
                tokens,
                idx,
                "expected nama parameter, tetapi input habis",
            ));
        }
        // Parameter boleh bermula dengan tipe, misal `parameter string Name`
        // atau `parameter int ListenPort` (LRM §6.20.1).
        // Heuristik: tipe nama diikuti oleh Ident lain (nama parameter)
        // lalu `=`. Kalau hanya satu Ident lalu `=`, itu nama parameter
        // langsung, bukan tipe.
        let type_name = if tokens.get(idx).is_some_and(|t| {
            matches!(
                &t.token,
                Token::Int
                    | Token::Bit
                    | Token::Logic
                    | Token::Integer
                    | Token::Byte
                    | Token::ShortInt
                    | Token::LongInt
                    | Token::Time
                    | Token::Real
                    | Token::String
                    | Token::StringLiteral(_)
            )
        }) {
            // Kata kunci tipe pasti diikuti nama parameter.
            let tn = match &tokens[idx].token {
                Token::Int => Some("int".to_string()),
                Token::Bit => Some("bit".to_string()),
                Token::Logic => Some("logic".to_string()),
                Token::Integer => Some("integer".to_string()),
                Token::Byte => Some("byte".to_string()),
                Token::ShortInt => Some("shortint".to_string()),
                Token::LongInt => Some("longint".to_string()),
                Token::Time => Some("time".to_string()),
                Token::Real => Some("real".to_string()),
                Token::String => Some("string".to_string()),
                Token::StringLiteral(s) => Some(s.clone()),
                _ => None,
            };
            idx += 1;
            // Lewati signedness opsional setelah tipe: `parameter int unsigned x`
            if idx < tokens.len() && matches!(tokens[idx].token, Token::Signed | Token::Unsigned) {
                idx += 1;
            }
            tn
        } else if tokens
            .get(idx)
            .is_some_and(|t| matches!(&t.token, Token::Ident(_)))
            && tokens
                .get(idx + 1)
                .is_some_and(|t| matches!(&t.token, Token::Ident(_)))
            && tokens
                .get(idx + 2)
                .is_some_and(|t| t.token == Token::Eq || t.token == Token::LBracket)
        {
            // `parameter string Name = ...` atau `parameter string Name[dim] = ...`
            // `string` adalah tipe, `Name` adalah nama.
            let tn = match &tokens[idx].token {
                Token::Ident(n) => Some(n.clone()),
                _ => None,
            };
            idx += 1;
            tn
        } else if tokens
            .get(idx)
            .is_some_and(|t| matches!(&t.token, Token::Ident(_)))
            && tokens
                .get(idx + 1)
                .is_some_and(|t| t.token == Token::LBracket)
            && tokens
                .get(idx + 2)
                .is_some_and(|t| matches!(&t.token, Token::Ident(_)))
        {
            // `parameter tipe [dim] Nama = ...` — tipe dengan packed dimension
            // sebelum nama parameter (misal `parameter pad_type_e [N] x = ...`).
            let tn = match &tokens[idx].token {
                Token::Ident(n) => Some(n.clone()),
                _ => None,
            };
            idx += 1;
            // Lewati packed dimension `[N]`.
            idx = crate::module::lewati_kurung_siku(tokens, idx);
            tn
        } else if tokens
            .get(idx)
            .is_some_and(|t| matches!(&t.token, Token::Ident(_)))
            && tokens
                .get(idx + 1)
                .is_some_and(|t| t.token == Token::ColonColon)
            && tokens
                .get(idx + 2)
                .is_some_and(|t| matches!(&t.token, Token::Ident(_)))
            && tokens
                .get(idx + 3)
                .is_some_and(|t| matches!(&t.token, Token::Ident(_)))
            && tokens.get(idx + 4).is_some_and(|t| {
                t.token == Token::Eq || t.token == Token::LBracket || t.token == Token::Comma
            })
        {
            // `parameter pkg::type Nama = ...` — tipe dengan scope paket (LRM §6.20.1).
            // Bentuk: `Ident` `::` `Ident` `Ident` `=`. `::` adalah satu token `ColonColon`.
            let tn = match &tokens[idx].token {
                Token::Ident(pkg) => {
                    let tipe = match &tokens[idx + 2].token {
                        Token::Ident(t) => t.clone(),
                        _ => String::new(),
                    };
                    Some(format!("{}::{}", pkg, tipe))
                }
                _ => None,
            };
            idx += 3; // lewati pkg :: type
            tn
        } else {
            None
        };
        // Lewati packed dimension setelah tipe, misal `logic [15:0] nama`.
        while idx < tokens.len() && tokens[idx].token == Token::LBracket {
            let mut depth = 1u32;
            idx += 1;
            while idx < tokens.len() && depth > 0 {
                match &tokens[idx].token {
                    Token::LBracket => depth += 1,
                    Token::RBracket => depth -= 1,
                    _ => {}
                }
                idx += 1;
            }
        }
        let (nama, span) = match &tokens[idx].token {
            Token::Ident(n) => (n.clone(), tokens[idx].span),
            _ => return Err(pesan::harap(tokens, idx, "expected nama parameter")),
        };
        idx += 1;

        // Dimensi unpacked setelah nama parameter, misal `KmacAppCfg[N]`.
        // SVRust tidak memproses dimensi ini seuhnya; cukup dikonsumsi.
        while idx < tokens.len() && tokens[idx].token == Token::LBracket {
            let mut depth = 1u32;
            idx += 1;
            while idx < tokens.len() && depth > 0 {
                match &tokens[idx].token {
                    Token::LBracket => depth += 1,
                    Token::RBracket => depth -= 1,
                    _ => {}
                }
                idx += 1;
            }
        }

        // `=` dan nilai default opsional: parameter tanpa default diperbolehkan
        // (misal `parameter int unsigned NumDATEntries,`).
        let (default, default_str) = if idx < tokens.len() && tokens[idx].token == Token::Eq {
            idx += 1;
            if idx >= tokens.len() {
                return Err(pesan::harap(
                    tokens,
                    idx,
                    "expected nilai default parameter, tetapi input habis",
                ));
            }
            // Nilai `localparam` adalah ekspresi konstanta, bukan hanya literal:
            // `localparam int Share = (EnMasking) ? 2 : 1`. Ekspresinya disimpan
            // utuh dan dievaluasi elaborator setelah parameter sebelumnya.
            if is_local {
                let (value, eaten) = super::expression::parse_expression(&tokens[idx..])?;
                idx += eaten;
                if localparams.iter().any(|p| p.name == nama)
                    || params.iter().any(|p: &ParamDecl| p.name == nama)
                {
                    return Err(pesan::tandai(
                        tokens,
                        idx,
                        format!("parameter duplikat: {}", nama),
                    ));
                }
                localparams.push(sv_ast::width::LocalParamDecl {
                    name: nama,
                    value,
                    span,
                });
                if idx < tokens.len() && tokens[idx].token == Token::Comma {
                    idx += 1;
                    continue;
                }
                break;
            }

            // Nilai default boleh angka, string literal, atau identifier
            // (untuk konstanta paket seperti `spi_device_pkg::SramType1r1w`).
            // Cek identifier dengan scope paket terlebih dahulu (lebih spesifik).
            let scoped = tokens
                .get(idx)
                .is_some_and(|t| matches!(&t.token, Token::Ident(_)))
                && tokens
                    .get(idx + 1)
                    .is_some_and(|t| t.token == Token::ColonColon)
                && tokens
                    .get(idx + 2)
                    .is_some_and(|t| matches!(&t.token, Token::Ident(_)));
            if scoped {
                let pkg = match &tokens[idx].token {
                    Token::Ident(n) => n.clone(),
                    _ => String::new(),
                };
                let nama = match &tokens[idx + 2].token {
                    Token::Ident(n) => n.clone(),
                    _ => String::new(),
                };
                (0, Some(format!("{}::{}", pkg, nama)))
            } else {
                match &tokens[idx].token {
                    Token::Number(n) => (*n, None),
                    // Sized literal juga sah sebagai nilai parameter, misal `8'd4`.
                    Token::BasedNumber { value, .. } => (*value, None),
                    Token::StringLiteral(s) => (0, Some(s.clone())),
                    Token::Ident(n) => {
                        // Identifier tunggal sebagai default (konstanta paket).
                        (0, Some(n.clone()))
                    }
                    _ => (0, None),
                }
            }
        } else {
            // Parameter tanpa default value (hanya nama dan tipe).
            (0, None)
        };
        // Lewati token-token nilai default hingga koma atau ')' pada kedalaman
        // 0, agar ekspresi default yang melibatkan operator (misal `a + b`) atau
        // concatenation ('{...}') dapat dilewati dengan benar. Jangan maju
        // melewati `)` penutup parameter list — biarkan loop luar yang menangani.
        let mut depth = 0u32;
        while idx < tokens.len() {
            match &tokens[idx].token {
                Token::LBrace | Token::LParen | Token::LBracket => depth += 1,
                Token::RBrace | Token::RBracket => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        idx += 1;
                        break;
                    }
                }
                Token::RParen => {
                    if depth == 0 {
                        break; // jangan maju melewati `)` penutup parameter list
                    }
                    depth = depth.saturating_sub(1);
                }
                Token::Comma if depth == 0 => break,
                _ => {}
            }
            idx += 1;
        }

        if params.iter().any(|p: &ParamDecl| p.name == nama) {
            return Err(pesan::tandai(
                tokens,
                idx,
                format!("parameter duplikat: {}", nama),
            ));
        }
        params.push(ParamDecl {
            name: nama.clone(),
            type_name,
            default,
            default_str,
            span,
        });

        if idx < tokens.len() && tokens[idx].token == Token::Comma {
            idx += 1;
        }
    }

    if tokens.get(idx).map(|t| &t.token) != Some(&Token::RParen) {
        return Err(pesan::harap(tokens, idx, "expected ')' penutup parameter"));
    }
    idx += 1; // lewati ')'

    Ok((params, localparams, idx))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sv_lexer::lex;

    fn parse(sumber: &str) -> (Vec<ParamDecl>, usize) {
        let tokens = lex(sumber).unwrap();
        let (params, _localparams, makan) = parse_param_list(&tokens).unwrap();
        (params, makan)
    }

    #[test]
    fn tanpa_hash_hasil_kosong() {
        let tokens = lex("module m;").unwrap();
        let (params, localparams, makan) = parse_param_list(&tokens).unwrap();
        assert!(params.is_empty());
        assert!(localparams.is_empty());
        assert_eq!(makan, 0);
    }

    // BUG: `localparam` di dalam `#(...)` ditolak sebagai "expected
    // 'parameter'", padahal OpenTitan memakai pola ini untuk menurunkan
    // lebar port dari parameter (`localparam int Share = (EnMasking) ? 2 : 1`).
    #[test]
    fn localparam_di_dalam_daftar_parameter_diterima() {
        let tokens = lex("#(parameter int A = 4, localparam int B = (A > 2) ? 8 : 4)").unwrap();
        let (params, localparams, _) = parse_param_list(&tokens).unwrap();
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].name, "A");
        assert_eq!(localparams.len(), 1);
        assert_eq!(localparams[0].name, "B");
        assert!(matches!(
            localparams[0].value,
            sv_ast::expression::Expr::Ternary { .. }
        ));
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
    fn default_ident_diterima() {
        // Identifier sebagai default (konstanta paket) sekarang diterima.
        let (params, _) = parse("#(parameter W = foo)");
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].name, "W");
        assert_eq!(params[0].default_str.as_deref(), Some("foo"));
        assert_eq!(params[0].default, 0);
    }

    #[test]
    fn default_ident_ber_scope_paket_diterima() {
        // Identifier dengan scope paket sebagai default.
        let (params, _) = parse("#(parameter W = spi_device_pkg::SramType1r1w)");
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].name, "W");
        assert_eq!(
            params[0].default_str.as_deref(),
            Some("spi_device_pkg::SramType1r1w")
        );
        assert_eq!(params[0].default, 0);
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
    fn parameter_string_sah() {
        // LRM §6.20.1: parameter boleh bertipe `string`.
        let (params, _) = parse("#(parameter string Name = \"dmi0\")");
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].name, "Name");
        assert_eq!(params[0].type_name.as_deref(), Some("string"));
        assert_eq!(params[0].default_str.as_deref(), Some("dmi0"));
    }

    #[test]
    fn parameter_tipe_ber_scope_paket_sah() {
        // Tipe dengan scope paket: `parameter pkg::type Nama = ...`.
        let (params, _) = parse("#(parameter spi_device_pkg::sram_type_e SpiDeviceSramType = 0)");
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].name, "SpiDeviceSramType");
        assert_eq!(
            params[0].type_name.as_deref(),
            Some("spi_device_pkg::sram_type_e")
        );
    }

    #[test]
    fn dua_parameter_satu_sized_literal() {
        let (params, _) = parse("#(parameter W = 8'd8, parameter D = 4'd2)");
        assert_eq!(params[0].default, 8);
        assert_eq!(params[1].default, 2);
    }

    #[test]
    fn repro_earlgrey_pd_main_full() {
        use crate::module::parse_module;
        use std::path::Path;
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../opentitan/hw/top_earlgrey/rtl/autogen/earlgrey_pd_main.sv"
        );
        let text = std::fs::read_to_string(path).expect("read");
        let dir = Path::new(path).parent().unwrap();
        // OpenTitan menyimpan `prim_assert.sv` di `hw/ip/prim/rtl`; tambahkan
        // agar `include` dari earlgrey_pd_main.sv dapat ditemukan tanpa
        // --incdir eksplisit (mirip alur `check --rtl opentitan.f`).
        let rtl_dir = dir.parent().unwrap();
        let prim_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../opentitan/hw/ip/prim/rtl");
        let dirs = [dir, rtl_dir, prim_dir.as_ref()];
        let pp = sv_preprocessor::preprocess_with_dirs(&text, &dirs, path).expect("preprocess");
        let tokens = lex(&pp.source).expect("lex");
        match parse_module(&tokens) {
            Ok(m) => {
                let _ = m;
            }
            Err(e) => {
                let _ = e;
            }
        }
    }
}
