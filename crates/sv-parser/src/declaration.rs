// Tanggung jawab: parse deklarasi signal (logic/wire/reg/bit).
use sv_ast::declaration::Declaration;
use sv_ast::width::WidthExpr;
use sv_lexer::spanned::SpannedToken;
use sv_lexer::token::Token;

/// Parse satu pernyataan deklarasi, termasuk daftar deklarator.
///
/// `logic [7:0] a, b, c;` mendeklarasikan tiga signal yang berbagi tipe,
/// lebar, dan signedness yang sama (LRM §6.2), jadi hasilnya berupa vektor.
/// Parse satu dimensi unpacked (LRM §7.8) pada `idx`.
///
/// Bentuk yang diterima: `[N]` (`size = N`, indeks mulai 0 menaik) dan
/// `[MSB:LSB]` dengan kedua batas literal.
fn parse_unpacked(
    tokens: &[SpannedToken],
    idx: usize,
) -> Result<(Option<sv_ast::declaration::UnpackedDim>, usize), String> {
    if tokens.get(idx).map(|t| &t.token) != Some(&Token::LBracket) {
        return Ok((None, 0));
    }
    let angka = |at: usize| -> Result<u64, String> {
        match tokens.get(at).map(|t| &t.token) {
            Some(Token::Number(n)) => Ok(*n),
            _ => Err(format!(
                "batas dimensi unpacked harus literal, ditemukan {:?}",
                tokens.get(at).map(|t| &t.token)
            )),
        }
    };
    let msb = angka(idx + 1)?;
    let mut cursor = idx + 2;
    if tokens.get(cursor).map(|t| &t.token) == Some(&Token::Colon) {
        let lsb = angka(cursor + 1)?;
        cursor += 2;
        if tokens.get(cursor).map(|t| &t.token) != Some(&Token::RBracket) {
            return Err("expected ']' pada dimensi unpacked".to_string());
        }
        cursor += 1;
        let size = msb.abs_diff(lsb) + 1;
        // `[left:right]`: `left` selalu indeks PERTAMA yang tertulis.
        // `left >= right` berarti menurun (`[3:0]`), selain itu menaik
        // (`[0:3]`).
        let descending = msb >= lsb;
        return Ok((
            Some(sv_ast::declaration::UnpackedDim {
                size,
                first: msb,
                descending,
            }),
            cursor - idx,
        ));
    }
    if tokens.get(cursor).map(|t| &t.token) != Some(&Token::RBracket) {
        return Err("expected ']' pada dimensi unpacked".to_string());
    }
    cursor += 1;
    // `[N]` berarti N elemen dengan indeks 0..N-1 menaik.
    Ok((
        Some(sv_ast::declaration::UnpackedDim {
            size: msb,
            first: 0,
            descending: false,
        }),
        cursor - idx,
    ))
}

pub fn parse_declaration(tokens: &[SpannedToken]) -> Result<(Vec<Declaration>, usize), String> {
    if tokens.is_empty() {
        return Err("empty tokens".to_string());
    }

    let span = tokens[0].span;
    let mut idx = 1; // skip type keyword

    // LRM §6.16 Tabel 6-22: tipe integer atom punya lebar dan signedness tetap
    // sehingga tidak memerlukan packed dimension. `integer`/`int` 32-bit
    // signed, `byte` 8-bit signed, `shortint` 16-bit signed, `longint` 64-bit
    // signed, `time` 64-bit unsigned.
    //
    // `logic` dan `bit` TIDAK termasuk daftar ini: keduanya memerlukan
    // `[msb:lsb]` untuk punya lebar lebih dari 1 bit, jadi `logic` polos tetap
    // 1 bit seperti LRM §6.16.
    let tipe_bulat = match tokens[0].token {
        Token::Integer | Token::Int => Some((32, true)),
        Token::Byte => Some((8, true)),
        Token::ShortInt => Some((16, true)),
        Token::LongInt => Some((64, true)),
        Token::Time => Some((64, false)),
        _ => None,
    };
    let lebar_bulat = tipe_bulat.map(|(w, _)| w);

    // Qualifier signedness opsional: `signed`/`unsigned` (LRM §6.2.1).
    let signed = match tokens.get(idx).map(|t| &t.token) {
        Some(&Token::Signed) => true,
        Some(&Token::Unsigned) => false,
        _ => tipe_bulat.is_some_and(|(_, s)| s),
    };
    if idx < tokens.len() && matches!(tokens[idx].token, Token::Signed | Token::Unsigned) {
        idx += 1;
    }

    // Optional packed dimension: [N] atau [MSB:LSB]
    let (lebar_tertulis, consumed) = super::dimension::parse_dimension(tokens, idx)?;
    idx += consumed;

    // LRM §7.8: dimensi unpacked menyusul packed dimension, misal
    // `logic [7:0] mem [0:15];`. Paket packed DAN unpacked (`logic [7:0]
    // x [3:0]`) ditolak karena belum ada modelnya.
    let width = match lebar_bulat {
        Some(w) => WidthExpr::Literal(w),
        None => lebar_tertulis,
    };

    // Daftar deklarator dipisahkan koma dan berbagi tipe yang sama.
    let mut declarations = Vec::new();
    loop {
        if idx >= tokens.len() {
            return Err("expected signal name".to_string());
        }
        let name = match &tokens[idx].token {
            Token::Ident(n) => n.clone(),
            _ => return Err("expected signal name".to_string()),
        };
        idx += 1;

        // LRM §7.8: dimensi unpacked menyusul nama deklarator, misal
        // `logic [7:0] mem [0:15];` — bukan setelah packed dimension.
        let (unpacked, dim_pakai) = parse_unpacked(tokens, idx)?;
        if unpacked.is_some() {
            idx += dim_pakai;
            if matches!(tokens.get(idx).map(|t| &t.token), Some(Token::LBracket)) {
                return Err("dimensi unpacked bertingkat belum didukung".to_string());
            }
        }

        // Inisialisasi opsional per deklarator: `logic a = 1, b = 2;`
        let init = if tokens.get(idx).map(|t| &t.token) == Some(&Token::Eq) {
            idx += 1;
            let (expr, consumed) = super::expression::parse_expression(&tokens[idx..])?;
            idx += consumed;
            Some(expr)
        } else {
            None
        };

        declarations.push(Declaration {
            name,
            // Semua deklarator berbagi tipe, jadi lebar disalin per signal.
            width: width.clone(),
            signed,
            init,
            type_name: None,
            unpacked,
            span,
        });

        // Koma berarti deklarator berikutnya masih bertipe sama.
        match tokens.get(idx).map(|t| &t.token) {
            Some(&Token::Comma) => {
                idx += 1;
            }
            _ => break,
        }
    }

    // Expect ';'
    if idx >= tokens.len() || tokens[idx].token != Token::Semi {
        return Err("expected ';'".to_string());
    }
    idx += 1;

    Ok((declarations, idx))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sv_ast::width::WidthExpr;

    fn parse(src: &str) -> Vec<Declaration> {
        let tokens = sv_lexer::lex(src).expect("lex");
        let (decls, _) = parse_declaration(&tokens).expect("parse");
        decls
    }

    /// Deklarasi tunggal; helper untuk pengujian deklarator tunggal.
    fn parse_one(src: &str) -> Declaration {
        let mut decls = parse(src);
        assert_eq!(decls.len(), 1, "harus tepat satu deklarasi");
        decls.remove(0)
    }

    #[test]
    fn deklarasi_tanpa_qualifier_unsigned() {
        let d = parse_one("logic [3:0] t;");
        assert_eq!(d.name, "t");
        assert!(!d.signed, "default LRM §6.2.1 adalah unsigned");
        assert_eq!(d.width, WidthExpr::Literal(4));
    }

    #[test]
    fn deklarasi_signed_ditandai() {
        let d = parse_one("logic signed [7:0] t;");
        assert!(d.signed);
        assert_eq!(d.width, WidthExpr::Literal(8));
    }

    #[test]
    fn deklarasi_unsigned_eksplisit() {
        assert!(!parse_one("logic unsigned [7:0] t;").signed);
    }

    #[test]
    fn deklarasi_signed_tanpa_dimension() {
        let d = parse_one("logic signed t;");
        assert!(d.signed);
        assert_eq!(d.width, WidthExpr::Literal(1));
    }

    #[test]
    fn deklarasi_tanpa_dimension_default_satu_bit() {
        assert_eq!(parse_one("logic t;").width, WidthExpr::Literal(1));
    }

    #[test]
    fn daftar_deklarator_bersama_tipe() {
        // BUG-18: `logic [7:0] a, b, c;` harus menghasilkan tiga signal
        // yang berbagi lebar dan signedness yang sama.
        let decls = parse("logic [7:0] a, b, c;");
        let nama: Vec<&str> = decls.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(nama, vec!["a", "b", "c"]);
        for d in &decls {
            assert_eq!(d.width, WidthExpr::Literal(8));
        }
    }

    #[test]
    fn daftar_deklarator_tanpa_dimension() {
        // Tanpa packed dimension tiap signal tetap 1 bit.
        let decls = parse("logic flag_a, flag_b;");
        assert_eq!(decls.len(), 2);
        for d in &decls {
            assert_eq!(d.width, WidthExpr::Literal(1));
        }
    }

    #[test]
    fn daftar_deklarator_bersama_signed() {
        // Qualifier signed berlaku ke seluruh deklarator.
        let decls = parse("logic signed [15:0] hi, lo;");
        assert_eq!(decls.len(), 2);
        assert!(decls.iter().all(|d| d.signed));
    }
}
