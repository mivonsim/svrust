// Tanggung jawab: parse sisi kiri assignment berupa sinyal dengan irisan opsional.
use crate::pesan;
use sv_ast::lvalue::{Arah, Lvalue, PartDinamis, Slice};
use sv_ast::width::WidthExpr;
use sv_lexer::spanned::SpannedToken;
use sv_lexer::token::Token;

/// Parse LHS: `nama`, `nama[msb:lsb]`, `nama[bit]`, atau `nama[genvar]`.
/// Mengembalikan `(Lvalue, jumlah token yang dikonsumsi)`.
pub fn parse_lvalue(tokens: &[SpannedToken]) -> Result<(Lvalue, usize), String> {
    if tokens.is_empty() {
        return Err(pesan::harap(tokens, 0, "expected LHS signal name"));
    }
    let name = match &tokens[0].token {
        Token::Ident(n) => n.clone(),
        other => {
            return Err(pesan::tandai(
                tokens,
                0,
                format!("expected LHS signal name, ditemukan {:?}", other),
            ))
        }
    };
    let span = tokens[0].span;

    let mut cursor = 1;

    let mut slice: Option<Slice> = None;
    let mut extra_slices: Vec<Slice> = Vec::new();
    let mut genvar_index: Option<String> = None;
    let mut members: Vec<String> = Vec::new();

    // Parse kurung siku pada nama dasar: `nama[...]` atau `nama[genvar]`.
    if cursor < tokens.len() && tokens[cursor].token == Token::LBracket {
        let closing_awal = (cursor + 1..=tokens.len())
            .find(|&i| tokens.get(i).map(|t| &t.token) == Some(&Token::RBracket));
        let Some(first_closing) = closing_awal else {
            return Err(pesan::harap(
                tokens,
                cursor,
                "expected ']' closing LHS select",
            ));
        };
        if first_closing == cursor + 1 {
            return Err(pesan::harap(
                tokens,
                cursor,
                "kurung siku seleksi kosong `[]` belum didukung",
            ));
        }
        if tokens[cursor + 1].token == Token::Colon {
            return Err(pesan::harap(
                tokens,
                cursor + 1,
                "part-select dengan batas bawah kosong `[:lsb]` belum didukung",
            ));
        }
        if tokens.get(first_closing - 1).map(|t| &t.token) == Some(&Token::Colon) {
            return Err(pesan::harap(
                tokens,
                first_closing - 1,
                "part-select dengan batas atas kosong `[msb:]` belum didukung",
            ));
        }

        // Indexed part-select `a[base +: width]` / `[base -: width]` (LRM §7.8)
        // pada LHS. Base dan width diekspresikan sebagai ekspresi yang
        // diselesaikan saat elaborasi.
        if let Some(pos) = (cursor + 2..first_closing).find(|&i| tokens[i].token == Token::Colon) {
            let arah = match tokens[pos - 1].token {
                Token::Plus => Some(Arah::Plus),
                Token::Minus => Some(Arah::Minus),
                _ => None,
            };
            if let Some(arah) = arah {
                let base_tokens = &tokens[cursor + 2..pos - 1];
                let width_tokens = &tokens[pos + 1..first_closing];
                let (base_expr, _) = super::expression::parse_expression(base_tokens)?;
                let (width_expr, _) = super::expression::parse_expression(width_tokens)?;
                let span = tokens[cursor].span;
                let lvalue = Lvalue {
                    name,
                    slice: None,
                    extra_slices: Vec::new(),
                    members: Vec::new(),
                    genvar_index: None,
                    part_dinamis: Some(PartDinamis {
                        base: base_expr,
                        width: width_expr,
                        arah,
                    }),
                    span,
                };
                return Ok((lvalue, first_closing + 1));
            }
        }

        // Bentuk `nama[genvar]` atau `nama[bit]`: isi kurung siku satu token.
        if first_closing == cursor + 2 {
            match &tokens[cursor + 1].token {
                Token::Ident(genvar) => {
                    genvar_index = Some(genvar.clone());
                    cursor = first_closing + 1;
                    // Dimensi tambahan setelah genvar: data[genvar][0]
                    while cursor < tokens.len() && tokens[cursor].token == Token::LBracket {
                        let second_closing = (cursor + 1..=tokens.len())
                            .find(|&i| tokens.get(i).map(|t| &t.token) == Some(&Token::RBracket));
                        let Some(second_closing) = second_closing else {
                            return Err("expected ']' closing LHS select".to_string());
                        };
                        if second_closing == cursor + 1 {
                            return Err(
                                "kurung siku seleksi kosong `[]` belum didukung".to_string()
                            );
                        }
                        if tokens[cursor + 1].token == Token::Colon {
                            return Err(
                                "part-select dengan batas bawah kosong `[:lsb]` belum didukung"
                                    .to_string(),
                            );
                        }
                        if tokens.get(second_closing - 1).map(|t| &t.token) == Some(&Token::Colon) {
                            return Err(
                                "part-select dengan batas atas kosong `[msb:]` belum didukung"
                                    .to_string(),
                            );
                        }
                        if let Token::Number(n) | Token::BasedNumber { value: n, .. } =
                            &tokens[cursor + 1].token
                        {
                            extra_slices.push(Slice {
                                msb: WidthExpr::Literal(u32::try_from(*n).unwrap_or(0) as usize),
                                lsb: WidthExpr::Literal(u32::try_from(*n).unwrap_or(0) as usize),
                            });
                            cursor = second_closing + 1;
                        } else {
                            // Indeks dinamis setelah genvar: data[i][j] — simpan sebagai ekspresi.
                            let (expr, consumed) = super::expression::parse_expression(
                                &tokens[cursor + 1..second_closing],
                            )?;
                            extra_slices.push(Slice {
                                msb: WidthExpr::Expr {
                                    msb: Box::new(expr.clone()),
                                    lsb: None,
                                },
                                lsb: WidthExpr::Literal(0),
                            });
                            cursor = second_closing + 1;
                            let _ = consumed;
                        }
                    }
                }
                Token::Number(n) | Token::BasedNumber { value: n, .. } => {
                    let val = u32::try_from(*n).unwrap_or(0) as usize;
                    slice = Some(Slice {
                        msb: WidthExpr::Literal(val),
                        lsb: WidthExpr::Literal(val),
                    });
                    cursor = first_closing + 1;
                }
                other => {
                    // Indeks non-literal: `a[N]` di mana N parameter.
                    let (expr, consumed) =
                        super::expression::parse_expression(&tokens[cursor + 1..first_closing])?;
                    slice = Some(Slice {
                        msb: WidthExpr::Expr {
                            msb: Box::new(expr),
                            lsb: None,
                        },
                        lsb: WidthExpr::Literal(0),
                    });
                    cursor = first_closing + 1;
                    let _ = consumed;
                }
            }
        } else {
            // Jika isi bracket lebih dari satu token, bisa jadi part-select
            // variabel (`i:0`) atau ekspresi (`W-1`). Coba parse sebagai ekspresi.
            let bracket_isi = &tokens[cursor + 1..first_closing];
            if bracket_isi.len() > 1
                && bracket_isi
                    .iter()
                    .any(|t| matches!(t.token, Token::Ident(_)))
            {
                let ada_titik_dua = bracket_isi.iter().any(|t| t.token == Token::Colon);
                if ada_titik_dua {
                    // Part-select variabel: `a[i:j]` atau `a[i +: w]`.
                    // Sudah ditangani di atas (indexed part-select). Kalau sampai
                    // sini berarti bentuk `a[msb:lsb]` dengan ekspresi non-literal.
                    let colon_pos = cursor + 1 + pos_colon(bracket_isi);
                    let (msb_expr, consumed) =
                        super::expression::parse_expression(&tokens[cursor + 1..colon_pos])?;
                    let after_colon = &tokens[colon_pos + 1..first_closing];
                    let (lsb_expr, _) = super::expression::parse_expression(after_colon)?;
                    slice = Some(Slice {
                        msb: WidthExpr::Expr {
                            msb: Box::new(msb_expr),
                            lsb: Some(Box::new(lsb_expr)),
                        },
                        lsb: WidthExpr::Literal(0),
                    });
                    cursor = first_closing + 1;
                } else {
                    // Ekspresi tunggal: `a[N-1]`
                    let (expr, consumed) =
                        super::expression::parse_expression(&tokens[cursor + 1..first_closing])?;
                    slice = Some(Slice {
                        msb: WidthExpr::Expr {
                            msb: Box::new(expr),
                            lsb: None,
                        },
                        lsb: WidthExpr::Literal(0),
                    });
                    cursor = first_closing + 1;
                    let _ = consumed;
                }
            } else {
                // Part-select atau bit-select pada nama dasar.
                let (slice_val, consumed) = parse_slice_index(&tokens[cursor + 1..first_closing])?;
                if consumed != first_closing - cursor - 1 {
                    return Err(pesan::tandai(
                        tokens,
                        cursor + 1 + consumed,
                        format!(
                            "sisa token tak dikenal pada indeks LHS: {:?}",
                            tokens[cursor + 1 + consumed..first_closing]
                                .iter()
                                .map(|t| format!("{:?}", t.token))
                                .collect::<Vec<_>>()
                                .join(" ")
                        ),
                    ));
                }
                slice = Some(slice_val);
                cursor = first_closing + 1;
            }
        }

        // Dimensi tambahan setelah indeks: `data[0][0]`, `data[3:0][1]`.
        // Setelah genvar (`data[i][0]`), berhenti — genvar tidak boleh nested slice.
        while genvar_index.is_none()
            && cursor < tokens.len()
            && tokens[cursor].token == Token::LBracket
        {
            let start2 = cursor + 1;
            if start2 >= tokens.len() {
                return Err("expected index in nested select".to_string());
            }
            let closing2 = (start2..=tokens.len())
                .find(|&i| tokens.get(i).map(|t| &t.token) == Some(&Token::RBracket));
            let Some(closing2) = closing2 else {
                return Err("expected ']' closing nested select".to_string());
            };
            if closing2 == start2 {
                return Err("kurung siku seleksi kosong `[]` belum didukung".to_string());
            }
            if tokens[start2].token == Token::Colon {
                return Err("nested part-select belum didukung".to_string());
            }
            if let Token::Ident(_nama) = &tokens[start2].token {
                if closing2 != start2 + 1 {
                    return Err(crate::pesan::tandai(
                        tokens,
                        cursor,
                        "nested genvar index harus satu token".to_string(),
                    ));
                }
                extra_slices.push(Slice {
                    msb: WidthExpr::Literal(0),
                    lsb: WidthExpr::Literal(0),
                });
            } else {
                let (slice_val, consumed) = parse_slice_index(&tokens[start2..closing2])?;
                if consumed != closing2 - start2 {
                    return Err(format!(
                        "sisa token tak dikenal pada indeks nested: {:?}",
                        tokens[start2 + consumed..closing2]
                            .iter()
                            .map(|t| format!("{:?}", t.token))
                            .collect::<Vec<_>>()
                            .join(" ")
                    ));
                }
                extra_slices.push(slice_val);
            }
            cursor = closing2 + 1;
        }

        // Seleksi bertingkat: `y[3:0][1]`, `data[genvar][0]`.
        while cursor < tokens.len() && tokens[cursor].token == Token::LBracket {
            let start2 = cursor + 1;
            if start2 >= tokens.len() {
                return Err("expected index in nested select".to_string());
            }
            let closing2 = (start2..=tokens.len())
                .find(|&i| tokens.get(i).map(|t| &t.token) == Some(&Token::RBracket));
            let Some(closing2) = closing2 else {
                return Err("expected ']' first_closing nested select".to_string());
            };
            if closing2 == start2 {
                return Err("kurung siku seleksi kosong `[]` belum didukung".to_string());
            }
            if tokens[start2].token == Token::Colon {
                return Err("nested part-select belum didukung".to_string());
            }
            if let Token::Ident(_nama) = &tokens[start2].token {
                if closing2 != start2 + 1 {
                    return Err(crate::pesan::tandai(
                        tokens,
                        cursor,
                        "nested genvar index harus satu token".to_string(),
                    ));
                }
                extra_slices.push(Slice {
                    msb: WidthExpr::Literal(0),
                    lsb: WidthExpr::Literal(0),
                });
            } else {
                let (slice_val, consumed) = parse_slice_index(&tokens[start2..closing2])?;
                if consumed != closing2 - start2 {
                    return Err(format!(
                        "sisa token tak dikenal pada indeks nested: {:?}",
                        tokens[start2 + consumed..closing2]
                            .iter()
                            .map(|t| format!("{:?}", t.token))
                            .collect::<Vec<_>>()
                            .join(" ")
                    ));
                }
                extra_slices.push(slice_val);
            }
            cursor = closing2 + 1;
        }
    }

    // Consume dot-separated member path dan optional slice setelah setiap member:
    // `a.b.c`, `a[0].b`, `a.b[0]`, `a.b[3:0]`.
    while cursor < tokens.len() && tokens[cursor].token == Token::Dot {
        cursor += 1; // skip '.'
        if cursor >= tokens.len() {
            return Err("expected nama setelah '.' pada referensi hierarkis".to_string());
        }
        let part = match &tokens[cursor].token {
            Token::Ident(n) => n.clone(),
            other => return Err(format!("expected nama setelah '.', dapat {:?}", other)),
        };
        cursor += 1;
        members.push(part);

        // Optional slice setelah member: `a.b[0]` atau `a.b[3:0]`.
        if cursor < tokens.len() && tokens[cursor].token == Token::LBracket {
            let closing = (cursor + 1..=tokens.len())
                .find(|&i| tokens.get(i).map(|t| &t.token) == Some(&Token::RBracket));
            let Some(closing) = closing else {
                return Err("expected ']' closing member select".to_string());
            };
            if closing == cursor + 1 {
                return Err("kurung siku seleksi kosong `[]` belum didukung".to_string());
            }
            if tokens[cursor + 1].token == Token::Colon {
                return Err(
                    "part-select dengan batas bawah kosong `[:lsb]` belum didukung".to_string(),
                );
            }
            if tokens.get(closing - 1).map(|t| &t.token) == Some(&Token::Colon) {
                return Err(
                    "part-select dengan batas atas kosong `[msb:]` belum didukung".to_string(),
                );
            }

            let mut next_cursor = closing + 1;
            while next_cursor < tokens.len() && tokens[next_cursor].token == Token::LBracket {
                let start2 = next_cursor + 1;
                if start2 >= tokens.len() {
                    return Err("expected index in nested select".to_string());
                }
                let closing2 = (start2..=tokens.len())
                    .find(|&i| tokens.get(i).map(|t| &t.token) == Some(&Token::RBracket));
                let Some(closing2) = closing2 else {
                    return Err("expected ']' nested select".to_string());
                };
                if closing2 == start2 {
                    return Err("kurung siku seleksi kosong `[]` belum didukung".to_string());
                }
                if tokens[start2].token == Token::Colon {
                    return Err("nested part-select belum didukung".to_string());
                }
                if let Token::Ident(_nama) = &tokens[start2].token {
                    if closing2 != start2 + 1 {
                        return Err(crate::pesan::tandai(
                            tokens,
                            next_cursor,
                            "nested genvar index harus satu token".to_string(),
                        ));
                    }
                    extra_slices.push(Slice {
                        msb: WidthExpr::Literal(0),
                        lsb: WidthExpr::Literal(0),
                    });
                } else {
                    let (slice_val, consumed) = parse_slice_index(&tokens[start2..closing2])?;
                    if consumed != closing2 - start2 {
                        return Err(format!(
                            "sisa token tak dikenal pada indeks nested: {:?}",
                            tokens[start2 + consumed..closing2]
                                .iter()
                                .map(|t| format!("{:?}", t.token))
                                .collect::<Vec<_>>()
                                .join(" ")
                        ));
                    }
                    extra_slices.push(slice_val);
                }
                next_cursor = closing2 + 1;
            }

            let slice_tokens = &tokens[cursor + 1..closing];
            let (slice_val, consumed) = parse_slice_index(slice_tokens)?;
            if consumed != closing - cursor - 1 {
                return Err(format!(
                    "sisa token tak dikenal pada indeks member: {:?}",
                    tokens[cursor + 1 + consumed..closing]
                        .iter()
                        .map(|t| format!("{:?}", t.token))
                        .collect::<Vec<_>>()
                        .join(" ")
                ));
            }
            extra_slices.push(slice_val);
            cursor = next_cursor;
        }
    }

    Ok((
        Lvalue {
            name,
            slice,
            extra_slices,
            members,
            genvar_index,
            part_dinamis: None,
            span,
        },
        cursor,
    ))
}

/// Cari posisi `:` dalam slice tokens untuk memisahkan msb dan lsb.
fn pos_colon(tokens: &[SpannedToken]) -> usize {
    tokens
        .iter()
        .position(|t| t.token == Token::Colon)
        .unwrap_or(0)
}

/// Parse isi `[ ... ]`: `msb:lsb` untuk part-select, `i` untuk bit-select.
/// Hanya literal yang diterima untuk `Slice::as_literal()`; ekspresi non-literal
/// disimpan sebagai `WidthExpr::Expr` dan di-resolve saat elaborasi.
fn parse_slice_index(tokens: &[SpannedToken]) -> Result<(Slice, usize), String> {
    let msb = parse_index(tokens, 0, "msb")?;
    let is_part_select = tokens.get(1).map(|t| &t.token) == Some(&Token::Colon);
    if is_part_select {
        let lsb = parse_index(tokens, 2, "lsb")?;
        // Validasi lsb <= msb hanya bila kedua batas literal.
        match (&msb, &lsb) {
            (WidthExpr::Literal(m), WidthExpr::Literal(l)) if *l > *m => {
                return Err(format!("part-select tidak valid: [{m}:{l}]"));
            }
            _ => {}
        }
        // msb, ':', lsb
        return Ok((Slice { msb, lsb }, 3));
    }
    Ok((
        Slice {
            msb: msb.clone(),
            lsb: msb,
        },
        1,
    ))
}

fn parse_index(tokens: &[SpannedToken], at: usize, posisi: &str) -> Result<WidthExpr, String> {
    match tokens.get(at).map(|t| &t.token) {
        Some(Token::Number(n)) => Ok(WidthExpr::Literal(u32::try_from(*n).unwrap_or(0) as usize)),
        Some(Token::BasedNumber { value, .. }) => Ok(WidthExpr::Literal(
            u32::try_from(*value).unwrap_or(0) as usize,
        )),
        // Indeks non-literal: parameter atau ekspresi. Simpan sebagai Expr
        // agar elaborator bisa mengevaluasinya ke literal nanti.
        Some(Token::Ident(n)) => {
            let (expr, consumed) = super::expression::parse_expression(&tokens[at..])?;
            let _ = consumed;
            Ok(WidthExpr::Expr {
                msb: Box::new(expr),
                lsb: None,
            })
        }
        other => Err(pesan::tandai(
            tokens,
            at,
            format!("indeks {posisi} harus literal, ditemukan {:?}", other),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sv_lexer::lex;

    fn parse(src: &str) -> Lvalue {
        let tokens = lex(src).expect("lex");
        let (lhs, _) = parse_lvalue(&tokens).expect("parse");
        lhs
    }

    #[test]
    fn sinyal_polos_tanpa_irisan() {
        let lhs = parse("y");
        assert_eq!(lhs.name, "y");
        assert!(!lhs.is_sliced());
    }

    #[test]
    fn bit_select_satu_bit() {
        let lhs = parse("y[0]");
        assert!(lhs.is_sliced());
        assert_eq!(
            lhs.slice.unwrap(),
            Slice {
                msb: WidthExpr::Literal(0),
                lsb: WidthExpr::Literal(0)
            }
        );
    }

    #[test]
    fn part_select_berentang() {
        let lhs = parse("y[7:4]");
        assert_eq!(
            lhs.slice.unwrap(),
            Slice {
                msb: WidthExpr::Literal(7),
                lsb: WidthExpr::Literal(4)
            }
        );
    }

    #[test]
    fn lsb_lebih_besar_dari_msb_ditolak() {
        let tokens = lex("y[3:7]").unwrap();
        assert!(parse_lvalue(&tokens).is_err());
    }

    #[test]
    fn indeks_dinamis_parameter_diterima() {
        // `a[N-1]` di mana N parameter — indeks dinamis diterima sebagai Expr.
        let lhs = parse("a[N-1]");
        assert!(lhs.is_sliced());
        match lhs.slice.unwrap().msb {
            WidthExpr::Expr { .. } => {}
            other => panic!("harus WidthExpr::Expr, dapat {:?}", other),
        }
    }

    #[test]
    fn indeks_genvar_diterima() {
        let lhs = parse("y[i]");
        assert_eq!(lhs.genvar_index, Some("i".to_string()));
    }

    #[test]
    fn bentuk_sah_tetap_diterima() {
        for src in ["y[0]", "y[7:4]", "y[i]"] {
            let tokens = lex(src).expect("lex");
            parse_lvalue(&tokens).unwrap_or_else(|e| panic!("{src} seharusnya sah: {e}"));
        }
    }

    #[test]
    fn kurung_tidak_tertutup_ditolak() {
        let tokens = lex("y[7:4").unwrap();
        assert!(parse_lvalue(&tokens).is_err());
    }

    #[test]
    fn indexed_part_select_pesan_jelas() {
        for src in ["y[b +: 4]", "y[b -: 4]"] {
            assert!(parse_lvalue(&lex(src).unwrap()).is_ok(), "{src}");
        }
    }
}
