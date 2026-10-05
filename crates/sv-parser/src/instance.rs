// Tanggung jawab: parse instansiasi module dengan koneksi bernama.
use sv_ast::instance::{Instance, ParamOverride, PortConn, PortIndex};
use sv_lexer::spanned::SpannedToken;
use sv_lexer::token::Token;

/// Parse `modname [#(.P(v))] instname ([.port(sig)]);`.
pub fn parse_instance(tokens: &[SpannedToken]) -> Result<(Instance, usize), String> {
    if tokens.is_empty() {
        return Err("expected module name".to_string());
    }
    let span = tokens[0].span;
    let module_name = match &tokens[0].token {
        Token::Ident(n) => n.clone(),
        _ => return Err("expected module name".to_string()),
    };
    let mut idx = 1;

    // Override parameter opsional: #(.P(v), ...)
    let mut param_overrides = Vec::new();
    if idx < tokens.len() && tokens[idx].token == Token::Hash {
        idx += 1;
        if idx >= tokens.len() || tokens[idx].token != Token::LParen {
            return Err("expected '(' setelah '#'".to_string());
        }
        idx += 1;
        while idx < tokens.len() && tokens[idx].token != Token::RParen {
            if tokens[idx].token != Token::Dot {
                return Err("expected '.NAMA' pada override parameter".to_string());
            }
            idx += 1;
            if idx >= tokens.len() {
                return Err("expected nama parameter".to_string());
            }
            let nama = match &tokens[idx].token {
                Token::Ident(n) => n.clone(),
                _ => return Err("expected nama parameter".to_string()),
            };
            idx += 1;
            if idx >= tokens.len() || tokens[idx].token != Token::LParen {
                return Err("expected '(' setelah nama parameter".to_string());
            }
            idx += 1;
            if idx >= tokens.len() {
                return Err("expected nilai parameter".to_string());
            }
            let nilai = match &tokens[idx].token {
                Token::Number(n) => *n,
                _ => return Err("nilai override harus literal angka".to_string()),
            };
            idx += 1;
            if idx >= tokens.len() || tokens[idx].token != Token::RParen {
                return Err("expected ')' penutup nilai".to_string());
            }
            idx += 1;
            param_overrides.push(ParamOverride {
                name: nama,
                value: nilai,
            });
            if idx < tokens.len() && tokens[idx].token == Token::Comma {
                idx += 1;
            }
        }
        if idx >= tokens.len() {
            return Err("expected ')' penutup override".to_string());
        }
        idx += 1;
    }

    // Nama instansi.
    if idx >= tokens.len() {
        return Err("expected nama instansi".to_string());
    }
    let inst_name = match &tokens[idx].token {
        Token::Ident(n) => n.clone(),
        _ => return Err("expected nama instansi".to_string()),
    };
    idx += 1;

    // Daftar koneksi: (.port(sig), ...).
    if idx >= tokens.len() || tokens[idx].token != Token::LParen {
        return Err("expected '(' daftar koneksi".to_string());
    }
    idx += 1;
    let mut port_conns = Vec::new();
    while idx < tokens.len() && tokens[idx].token != Token::RParen {
        if tokens[idx].token != Token::Dot {
            return Err("expected '.port' pada koneksi".to_string());
        }
        idx += 1;
        if idx >= tokens.len() {
            return Err("expected nama port".to_string());
        }
        let port = match &tokens[idx].token {
            Token::Ident(n) => n.clone(),
            _ => return Err("expected nama port".to_string()),
        };
        idx += 1;
        if idx >= tokens.len() || tokens[idx].token != Token::LParen {
            return Err("expected '(' setelah nama port".to_string());
        }
        idx += 1;
        if idx >= tokens.len() {
            return Err("expected nama sinyal".to_string());
        }
        let signal_span = tokens[idx].span;
        let signal = match &tokens[idx].token {
            Token::Ident(n) => n.clone(),
            _ => return Err("expected nama sinyal".to_string()),
        };
        idx += 1;
        // LRM §23.3.3.7: sinyal koneksi boleh berupa bit/part-select, misal
        // `.y(q[i])` pada pola kanonik loop generate.
        let (index, index_consumed) = parse_port_index(&tokens[idx..])?;
        idx += index_consumed;
        if idx >= tokens.len() || tokens[idx].token != Token::RParen {
            return Err("expected ')' penutup sinyal".to_string());
        }
        idx += 1;
        port_conns.push(PortConn {
            port,
            signal,
            index,
            span: signal_span,
        });
        if idx < tokens.len() && tokens[idx].token == Token::Comma {
            idx += 1;
        }
    }
    if idx >= tokens.len() {
        return Err("expected ')' penutup koneksi".to_string());
    }
    idx += 1;

    if idx >= tokens.len() || tokens[idx].token != Token::Semi {
        return Err("expected ';'".to_string());
    }
    idx += 1;

    Ok((
        Instance {
            module_name,
            inst_name,
            param_overrides,
            port_conns,
            span,
        },
        idx,
    ))
}

/// Parse indeks opsional pada sinyal koneksi port.
///
/// Mengembalikan `(indeks, jumlah_token_konsumsi)`. Tanpa `[` hasilnya
/// `(None, 0)` sehingga pemanggil tidak perlu pengecekan tambahan.
fn parse_port_index(tokens: &[SpannedToken]) -> Result<(Option<PortIndex>, usize), String> {
    if tokens.first().map(|t| &t.token) != Some(&Token::LBracket) {
        return Ok((None, 0));
    }
    if tokens.len() < 2 {
        return Err("expected indeks setelah '['".to_string());
    }

    // Bentuk `q[genvar]` menyimpan nama genvar untuk disubstitusi saat
    // elaborasi; bentuk literal diselesaikan di sini.
    let mut hasil = match &tokens[1].token {
        Token::Number(n) => PortIndex::Bit(u32::try_from(*n).map_err(|_| "index melebihi u32")?),
        Token::Ident(nama) => PortIndex::Genvar(nama.clone()),
        other => return Err(format!("indeks koneksi tidak didukung: {:?}", other)),
    };

    // Bentuk part-select `q[msb:lsb]`.
    if tokens.get(2).map(|t| &t.token) == Some(&Token::Colon) {
        let lsb = match tokens.get(3).map(|t| &t.token) {
            Some(Token::Number(n)) => u32::try_from(*n).map_err(|_| "index melebihi u32")?,
            other => return Err(format!("lsb part-select harus literal: {:?}", other)),
        };
        match hasil {
            PortIndex::Bit(msb) => {
                if msb < lsb {
                    return Err(format!("invalid range [{}:{}]", msb, lsb));
                }
                hasil = PortIndex::Range(msb, lsb);
            }
            other => return Err(format!("part-select tidak bisa memakai {:?}", other)),
        }
    }

    if tokens.get(4).map(|t| &t.token) == Some(&Token::RBracket) {
        return Ok((Some(hasil), 5));
    }
    // Bentuk bit/part-select hanya sepanjang tiga atau lima token.
    if tokens.get(2).map(|t| &t.token) == Some(&Token::RBracket) {
        return Ok((Some(hasil), 3));
    }
    Err("expected ']' pada indeks".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sv_lexer::lex;

    #[test]
    fn dasar_tanpa_override() {
        let tokens = lex("adder u0 (.a(x), .y(z));").unwrap();
        let (inst, _) = parse_instance(&tokens).unwrap();
        assert_eq!(inst.module_name, "adder");
        assert_eq!(inst.inst_name, "u0");
        assert!(inst.param_overrides.is_empty());
        assert_eq!(inst.port_conns.len(), 2);
        assert_eq!(inst.port_conns[0].port, "a");
        assert_eq!(inst.port_conns[0].signal, "x");
    }

    #[test]
    fn dengan_override() {
        let tokens = lex("adder #(.WIDTH(16)) u1 (.a(x), .y(z));").unwrap();
        let (inst, _) = parse_instance(&tokens).unwrap();
        assert_eq!(inst.param_overrides.len(), 1);
        assert_eq!(inst.param_overrides[0].name, "WIDTH");
        assert_eq!(inst.param_overrides[0].value, 16);
    }

    #[test]
    fn koneksi_posisional_ditolak() {
        let tokens = lex("adder u0 (x, z);").unwrap();
        assert!(parse_instance(&tokens).is_err());
    }

    // --- LRM §23.3.3.7: indeks pada sinyal koneksi ---

    #[test]
    fn bit_select_pada_koneksi() {
        let tokens = lex("adder u0 (.y(q[3]));").unwrap();
        let (inst, _) = parse_instance(&tokens).unwrap();
        assert_eq!(inst.port_conns[0].index, Some(PortIndex::Bit(3)));
    }

    #[test]
    fn part_select_pada_koneksi() {
        let tokens = lex("adder u0 (.y(q[7:4]));").unwrap();
        let (inst, _) = parse_instance(&tokens).unwrap();
        assert_eq!(inst.port_conns[0].index, Some(PortIndex::Range(7, 4)));
    }

    #[test]
    fn indeks_genvar_pada_koneksi() {
        // Pola kanonik generate: `.y(q[i])` di dalam loop.
        let tokens = lex("adder u0 (.y(q[i]));").unwrap();
        let (inst, _) = parse_instance(&tokens).unwrap();
        assert_eq!(
            inst.port_conns[0].index,
            Some(PortIndex::Genvar("i".to_string()))
        );
    }

    #[test]
    fn koneksi_tanpa_indeks_tetap_utuh() {
        let tokens = lex("adder u0 (.y(q));").unwrap();
        let (inst, _) = parse_instance(&tokens).unwrap();
        assert_eq!(inst.port_conns[0].index, None);
        assert_eq!(inst.port_conns[0].signal, "q");
    }

    #[test]
    fn indeks_kosong_ditolak() {
        let tokens = lex("adder u0 (.y(q[]));").unwrap();
        assert!(parse_instance(&tokens).is_err());
    }

    #[test]
    fn part_select_terbalik_ditolak() {
        let tokens = lex("adder u0 (.y(q[0:7]));").unwrap();
        assert!(parse_instance(&tokens).is_err());
    }
}
