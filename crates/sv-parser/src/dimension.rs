// Tanggung jawab: parse packed dimension menjadi ekspresi lebar.
use sv_ast::width::WidthExpr;
use sv_lexer::spanned::SpannedToken;
use sv_lexer::token::Token;

/// Parse dimension yang seluruh batasnya angka literal.
///
/// Mengembalikan `Ok(None)` bila bentuknya bukan literal murni (mis. `[W+1:0]`)
/// supaya pemanggil beralih ke jalur ekspresi umum. Bentuk literal tetap
/// menghasilkan `WidthExpr::Literal` supaya bentuk AST tidak berubah untuk kasus
/// paling umum — dan perhitungannya selesai di sini karena tidak ada parameter
/// yang perlu di-resolve.
fn parse_literal_dimension(
    tokens: &[SpannedToken],
    idx: usize,
    start: usize,
) -> Result<Option<(WidthExpr, usize)>, String> {
    /// Baca satu batas literal `N` atau `N - M`, advancing `cursor`.
    fn bound(tokens: &[SpannedToken], cursor: &mut usize) -> Result<Option<u64>, String> {
        let Token::Number(_) = &tokens[*cursor].token else {
            return Ok(None);
        };
        let mut v = match &tokens[*cursor].token {
            Token::Number(n) => *n,
            _ => unreachable!("dicek di atas"),
        };
        *cursor += 1;
        // Bentuk `A - B`, misal `[8-1:0]`.
        if tokens.get(*cursor).map(|t| &t.token) == Some(&Token::Minus) {
            if let Some(Token::Number(delta)) = tokens.get(*cursor + 1).map(|t| &t.token) {
                if v < *delta {
                    return Err(format!("invalid range [{}-{}]", v, delta));
                }
                v -= *delta;
                *cursor += 2;
            }
        }
        Ok(Some(v))
    }

    let mut cursor = idx;
    let Some(msb) = bound(tokens, &mut cursor)? else {
        return Ok(None);
    };
    let width = if tokens.get(cursor).map(|t| &t.token) == Some(&Token::Colon) {
        cursor += 1;
        let Some(lsb) = bound(tokens, &mut cursor)? else {
            return Ok(None);
        };
        if msb < lsb {
            return Err(format!("invalid range [{}:{}]", msb, lsb));
        }
        msb - lsb + 1
    } else {
        msb
    };
    if tokens.get(cursor).map(|t| &t.token) != Some(&Token::RBracket) {
        return Err("expected ']'".to_string());
    }
    cursor += 1;
    let width = usize::try_from(width).map_err(|_| format!("dimension too large: {width}"))?;
    Ok(Some((WidthExpr::Literal(width), cursor - start)))
}

/// Membaca dimensi packed starting pada `start`.
/// Mengembalikan (ekspresi_lebar, jumlah_token_konsum).
/// Pola: `[N]`, `[MSB:LSB]`, `[P]`, `[P-1:0]`, dan ekspresi konstanta
/// bebas seperti `[W+1:0]`.
pub fn parse_dimension(
    tokens: &[SpannedToken],
    start: usize,
) -> Result<(WidthExpr, usize), String> {
    if start >= tokens.len() || tokens[start].token != Token::LBracket {
        return Ok((WidthExpr::Literal(1), 0));
    }

    let idx = start + 1;
    if idx >= tokens.len() {
        return Err("expected dimension bound".to_string());
    }

    // Batas angka literal, opsional `A - B` lalu opsional `: C`.
    //
    // Jalur cepat ini menghasilkan `WidthExpr::Literal` supaya bentuk AST tetap
    // sama untuk kasus paling umum; sisanya jatuh ke jalur ekspresi umum di
    // bawah. Perhitungan selesai di sini karena tidak ada parameter yang perlu
    // di-resolve.
    if let Some(hasil) = parse_literal_dimension(tokens, idx, start)? {
        return Ok(hasil);
    }

    // Batas berupa ekspresi konstanta apa pun (LRM §7.3).
    //
    // Bentuk sederhana `[P]` dan `[P-1:0]` ditangani lebih dulu supaya tetap
    // jadi variant khusus; sisanya (`[W+1:0]`, `[W*2-1:0]`, `[7:2]`) disimpan
    // sebagai ekspresi utuh dan di-resolve saat elaborasi, karena parameter
    // modul belum diketahui saat parse.
    if let Token::Ident(name) = &tokens[idx].token {
        let nama = name.clone();
        let Some(after) = tokens.get(idx + 1).map(|t| &t.token) else {
            return Err("expected ']'".to_string());
        };
        match after {
            Token::RBracket => {
                return Ok((WidthExpr::Param(nama), idx + 2 - start));
            }
            // `[P-1:0]` — dibiarkan sebagai `ParamMinusOne` supaya pesan error
            // dan tes yang ada tetap punya bentuknya.
            Token::Minus
                if tokens.get(idx + 2).map(|t| &t.token) == Some(&Token::Number(1))
                    && tokens.get(idx + 3).map(|t| &t.token) == Some(&Token::Colon)
                    && tokens.get(idx + 4).map(|t| &t.token) == Some(&Token::Number(0))
                    && tokens.get(idx + 5).map(|t| &t.token) == Some(&Token::RBracket) =>
            {
                return Ok((WidthExpr::ParamMinusOne(nama), idx + 6 - start));
            }
            _ => {}
        }
    }

    // Bentuk umum: ekspresi konstanta, opsional `: ekspresi`.
    let (msb, eaten) = crate::expression::parse_expression(&tokens[idx..])?;
    let mut cursor = idx + eaten;
    let lsb = if tokens.get(cursor).map(|t| &t.token) == Some(&Token::Colon) {
        cursor += 1;
        let (expr, eaten) = crate::expression::parse_expression(&tokens[cursor..])?;
        cursor += eaten;
        Some(Box::new(expr))
    } else {
        None
    };
    if tokens.get(cursor).map(|t| &t.token) != Some(&Token::RBracket) {
        return Err("expected ']' pada dimension".to_string());
    }
    cursor += 1;
    Ok((
        WidthExpr::Expr {
            msb: Box::new(msb),
            lsb,
        },
        cursor - start,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sv_lexer::lex;

    #[test]
    fn single_bound() {
        let tokens = lex("[8]").unwrap();
        assert_eq!(
            parse_dimension(&tokens, 0).unwrap(),
            (WidthExpr::Literal(8), 3)
        );
    }

    #[test]
    fn range_bound() {
        let tokens = lex("[31:0]").unwrap();
        assert_eq!(
            parse_dimension(&tokens, 0).unwrap(),
            (WidthExpr::Literal(32), 5)
        );
    }

    #[test]
    fn absent_dimension() {
        let tokens = lex("a").unwrap();
        assert_eq!(
            parse_dimension(&tokens, 0).unwrap(),
            (WidthExpr::Literal(1), 0)
        );
    }

    #[test]
    fn reversed_range_rejected() {
        let tokens = lex("[0:7]").unwrap();
        assert!(parse_dimension(&tokens, 0).is_err());
    }

    #[test]
    fn param_pola() {
        let tokens = lex("[WIDTH-1:0]").unwrap();
        assert_eq!(
            parse_dimension(&tokens, 0).unwrap(),
            (WidthExpr::ParamMinusOne("WIDTH".to_string()), 7)
        );

        let tokens = lex("[WIDTH]").unwrap();
        assert_eq!(
            parse_dimension(&tokens, 0).unwrap(),
            (WidthExpr::Param("WIDTH".to_string()), 3)
        );
    }

    #[test]
    fn param_dengan_kurung_ditolak() {
        // Bentuk yang benar-benar tidak sah: `[WIDTH]` tertutup kurung belum
        // selesai. `[WIDTH-2:0]` sendiri SAH — itu ekspresi konstanta biasa
        // (LRM §7.3) dan sebelumnya ikut ditolak karena parser hanya mengenali
        // `[P]` dan `[P-1:0]`.
        let tokens = lex("[WIDTH 0]").unwrap();
        assert!(parse_dimension(&tokens, 0).is_err());
    }

    #[test]
    fn ekspresi_konstanta_diterima_sebagai_dimension() {
        // LRM §7.3: batas boleh ekspresi konstanta apa pun.
        //
        // Batas yang memuat parameter tidak bisa dihitung saat parse karena
        // nilainya baru diketahui saat elaborasi, jadi ekspresinya disimpan utuh
        // sebagai `WidthExpr::Expr`.
        for source in [
            "[W+1:0]",
            "[W*2-1:0]",
            "[W-2:0]",
            "[WIDTH+DEPTH-1:0]",
            "[(W+1)*2-1:0]",
        ] {
            let tokens = lex(source).unwrap();
            let (width, consumed) =
                parse_dimension(&tokens, 0).unwrap_or_else(|e| panic!("{source} gagal: {e}"));
            assert_eq!(consumed, tokens.len(), "{source} tidaknya semua token");
            assert!(
                matches!(width, WidthExpr::Expr { .. }),
                "{source} seharusnya jadi WidthExpr::Expr, dapat {width:?}"
            );
        }
        // Batas literal murni tetap dihitung di parse sebagai `Literal`.
        let tokens = lex("[7:2]").unwrap();
        assert_eq!(
            parse_dimension(&tokens, 0).unwrap(),
            (WidthExpr::Literal(6), 5)
        );
    }

    #[test]
    fn bentuk_sederhana_tetap_literal_atau_param() {
        // Bentuk yang paling umum harus tetap punya variant khusus supaya AST
        // tidak berubah bentuknya.
        let tokens = lex("[8]").unwrap();
        assert_eq!(
            parse_dimension(&tokens, 0).unwrap(),
            (WidthExpr::Literal(8), 3)
        );
        let tokens = lex("[WIDTH]").unwrap();
        assert_eq!(
            parse_dimension(&tokens, 0).unwrap(),
            (WidthExpr::Param("WIDTH".to_string()), 3)
        );
        let tokens = lex("[WIDTH-1:0]").unwrap();
        assert_eq!(
            parse_dimension(&tokens, 0).unwrap(),
            (WidthExpr::ParamMinusOne("WIDTH".to_string()), 7)
        );
    }

    // Bug: ekspresi literal `A - B` di dimension ditolak. Pola ini muncul
    // saat `WIDTH ter-subst jadi angka, mis. `WIDTH-1:0 -> 8-1:0.
    #[test]
    fn literal_minus_pada_range() {
        let tokens = lex("[8-1:0]").unwrap();
        assert_eq!(
            parse_dimension(&tokens, 0).unwrap(),
            (WidthExpr::Literal(8), 7)
        );
    }

    #[test]
    fn literal_minus_tanpa_range() {
        let tokens = lex("[8-1]").unwrap();
        assert_eq!(
            parse_dimension(&tokens, 0).unwrap(),
            (WidthExpr::Literal(7), 5)
        );
    }

    #[test]
    fn literal_minus_negatif_ditolak() {
        let tokens = lex("[1-8:0]").unwrap();
        assert!(parse_dimension(&tokens, 0).is_err());
    }
}
