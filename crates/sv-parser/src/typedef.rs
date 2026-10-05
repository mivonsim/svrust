// Tanggung jawab: parse deklarasi `typedef` skalar dan `typedef enum`.
use sv_ast::expression::Expr;
use sv_ast::typedef::{EnumMember, TypeDefBody, TypeDefDecl};
use sv_ast::width::WidthExpr;
use sv_lexer::spanned::SpannedToken;
use sv_lexer::token::Token;

/// Parse `typedef ... nama;` (LRM §8.20 dan §6.7).
///
/// Bentuk yang didukung:
/// ```text
/// typedef logic [7:0] byte_t;
/// typedef enum logic [1:0] { IDLE, RUN, DONE = 3 } state_t;
/// ```
pub fn parse_typedef(tokens: &[SpannedToken]) -> Result<(TypeDefDecl, usize), String> {
    if tokens.is_empty() || tokens[0].token != Token::Typedef {
        return Err("expected 'typedef'".to_string());
    }
    let span = tokens[0].span;
    let mut idx = 1;

    // Bentuk enumerasi punya daftar anggota di dalam kurung kurawal.
    if tokens.get(idx).map(|t| &t.token) == Some(&Token::Enum) {
        idx += 1;
        // Tipe atom (Tabel 6-22) sudah menentukan lebar dan signedness-nya.
        let tipe_atom = tokens
            .get(idx)
            .and_then(|t| sv_lexer::token::tipe::atom(&t.token));
        if tokens
            .get(idx)
            .is_some_and(|t| sv_lexer::token::tipe::bisa_mulai_deklarasi(&t.token))
        {
            idx += 1;
        }
        // Qualifier signedness opsional pada tipe dasar enum.
        if matches!(
            tokens.get(idx).map(|t| &t.token),
            Some(Token::Signed) | Some(Token::Unsigned)
        ) {
            idx += 1;
        }
        // LRM §6.7: lebar enum boleh ditulis eksplisit; tanpa itu 1 bit.
        // Tipe atom memakai lebarnya sendiri dan tidak boleh diikuti dimension.
        let (base_width, consumed) = match tipe_atom {
            Some((w, _)) => (WidthExpr::Literal(w as usize), 0),
            None => super::dimension::parse_dimension(tokens, idx)?,
        };
        idx += consumed;

        if tokens.get(idx).map(|t| &t.token) != Some(&Token::LBrace) {
            return Err("expected '{' membuka daftar enum".to_string());
        }
        idx += 1;
        let mut members = Vec::new();
        while idx < tokens.len() && tokens[idx].token != Token::RBrace {
            let member_span = tokens[idx].span;
            let name = match &tokens[idx].token {
                Token::Ident(n) => n.clone(),
                other => return Err(format!("expected nama anggota enum, dapat {other:?}")),
            };
            idx += 1;
            // Nilai eksplisit opsional: `RUN = 3` (LRM §6.7).
            let value = if tokens.get(idx).map(|t| &t.token) == Some(&Token::Eq) {
                idx += 1;
                let (expr, consumed) = super::expression::parse_expression(&tokens[idx..])?;
                idx += consumed;
                Some(expr)
            } else {
                None
            };
            members.push(EnumMember {
                name,
                value,
                span: member_span,
            });
            if tokens.get(idx).map(|t| &t.token) == Some(&Token::Comma) {
                idx += 1;
            }
        }
        if tokens.get(idx).map(|t| &t.token) != Some(&Token::RBrace) {
            return Err("expected '}' menutup daftar enum".to_string());
        }
        idx += 1;

        let name = baca_nama(tokens, idx)?;
        idx += 1;
        // Tutup dengan `;`, sama seperti deklarasi module item lainnya.
        if tokens.get(idx).map(|t| &t.token) != Some(&Token::Semi) {
            return Err("expected ';' menutup typedef".to_string());
        }
        idx += 1;
        return Ok((
            TypeDefDecl {
                name,
                body: TypeDefBody::Enum {
                    base_width,
                    members,
                },
                span,
            },
            idx,
        ));
    }

    // Bentuk skalar: alias dari tipe yang sudah ada (LRM §8.20).
    let tipe_atom = tokens
        .get(idx)
        .and_then(|t| sv_lexer::token::tipe::atom(&t.token));
    if tokens
        .get(idx)
        .is_some_and(|t| sv_lexer::token::tipe::bisa_mulai_deklarasi(&t.token))
    {
        idx += 1;
    } else {
        return Err(format!(
            "expected tipe setelah 'typedef', dapat {:?}",
            tokens.get(idx).map(|t| &t.token)
        ));
    }

    let signed = tipe_atom.is_some_and(|(_, s)| s)
        || matches!(tokens.get(idx).map(|t| &t.token), Some(Token::Signed));
    if matches!(
        tokens.get(idx).map(|t| &t.token),
        Some(Token::Signed) | Some(Token::Unsigned)
    ) {
        idx += 1;
    }

    let (width, consumed) = match tipe_atom {
        Some((w, _)) => (WidthExpr::Literal(w as usize), 0),
        None => super::dimension::parse_dimension(tokens, idx)?,
    };
    idx += consumed;

    let name = baca_nama(tokens, idx)?;
    idx += 1;
    // Tutup dengan `;` supaya pemanggil module.rs bisa lanjut ke item berikutnya.
    if tokens.get(idx).map(|t| &t.token) != Some(&Token::Semi) {
        return Err("expected ';' menutup typedef".to_string());
    }
    idx += 1;
    Ok((
        TypeDefDecl {
            name,
            body: TypeDefBody::Scalar { width, signed },
            span,
        },
        idx,
    ))
}

/// Baca nama tipe hasil typedef; menolak nama yang bukan identifier.
fn baca_nama(tokens: &[SpannedToken], idx: usize) -> Result<String, String> {
    match tokens.get(idx).map(|t| &t.token) {
        Some(Token::Ident(n)) => Ok(n.clone()),
        other => Err(format!("expected nama tipe, dapat {other:?}")),
    }
}

/// True bila token adalah salah satu keyword qualifier tipe yang boleh
/// muncul sebelum nama tipe pada deklarasi bertipe (`byte_t data;`).
pub fn qualifies_as_type(token: &Token) -> bool {
    sv_lexer::token::tipe::bisa_mulai_deklarasi(token)
}

/// Baca daftar deklarator untuk tipe hasil typedef.
///
/// Bentuk: `nama_tipe [signed|unsigned] [[dim]] nama [, nama]* ;`
///
/// Lebar yang sudah diketahui di-resolve lebih dulu oleh elaborator lewat
/// `TypeTable`, sehingga parser cukup menyimpan dimensi tertulis apa adanya.
pub fn parse_typed_declaration(
    tokens: &[SpannedToken],
) -> Result<(String, sv_ast::declaration::Declaration, usize), String> {
    if tokens.is_empty() {
        return Err("expected tipe".to_string());
    }
    let type_name = match &tokens[0].token {
        Token::Ident(n) => n.clone(),
        other => return Err(format!("expected nama tipe, dapat {other:?}")),
    };
    let mut idx = 1;

    let signed = matches!(tokens.get(idx).map(|t| &t.token), Some(Token::Signed));
    if matches!(
        tokens.get(idx).map(|t| &t.token),
        Some(Token::Signed) | Some(Token::Unsigned)
    ) {
        idx += 1;
    }

    let (width, consumed) = super::dimension::parse_dimension(tokens, idx)?;
    idx += consumed;

    let span = tokens[0].span;
    let name = match tokens.get(idx).map(|t| &t.token) {
        Some(Token::Ident(n)) => n.clone(),
        other => return Err(format!("expected nama signal, dapat {other:?}")),
    };
    idx += 1;

    let init = if tokens.get(idx).map(|t| &t.token) == Some(&Token::Eq) {
        idx += 1;
        let (expr, consumed) = super::expression::parse_expression(&tokens[idx..])?;
        idx += consumed;
        Some(expr)
    } else {
        None
    };

    let decl = sv_ast::declaration::Declaration {
        name,
        width,
        signed,
        init,
        type_name: Some(type_name.clone()),
        unpacked: None,
        span,
    };
    Ok((type_name, decl, idx))
}

/// Baca daftar deklarator tambahan setelah koma pada deklarasi bertipe.
///
/// Mengembalikan daftar deklarasi tambahan; pemanggil menggabungkannya dengan
/// deklarator pertama.
pub fn parse_more_typed_declarators(
    tokens: &[SpannedToken],
    start: usize,
    type_name: &str,
    width: &WidthExpr,
    signed: bool,
    span: sv_lexer::span::Span,
) -> Result<(Vec<sv_ast::declaration::Declaration>, usize), String> {
    let mut out = Vec::new();
    let mut idx = start;
    while idx < tokens.len() && tokens[idx].token == Token::Comma {
        idx += 1;
        let name = match tokens.get(idx).map(|t| &t.token) {
            Some(Token::Ident(n)) => n.clone(),
            other => return Err(format!("expected nama signal, dapat {other:?}")),
        };
        idx += 1;
        let init = if tokens.get(idx).map(|t| &t.token) == Some(&Token::Eq) {
            idx += 1;
            let (expr, consumed) = super::expression::parse_expression(&tokens[idx..])?;
            idx += consumed;
            Some(expr)
        } else {
            None
        };
        out.push(sv_ast::declaration::Declaration {
            name,
            width: width.clone(),
            signed,
            init,
            type_name: Some(type_name.to_string()),
            unpacked: None,
            span,
        });
    }
    Ok((out, idx))
}

/// True bila ekspresi adalah literal konstan yang boleh dipakai sebagai nilai
/// anggota enum (LRM §6.7 membolehkan ekspresi konstanta).
pub fn literal_konstan(expr: &Expr) -> bool {
    match expr {
        Expr::Number(_) | Expr::Sized { .. } => true,
        Expr::Unary { operand, .. } => literal_konstan(operand),
        Expr::Binary { lhs, rhs, .. } => literal_konstan(lhs) && literal_konstan(rhs),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sv_lexer::lex;

    fn parse(src: &str) -> TypeDefDecl {
        let tokens = lex(src).expect("lex");
        parse_typedef(&tokens).expect("parse").0
    }

    #[test]
    fn typedef_skalar_dengan_packed_dimension() {
        let decl = parse("typedef logic [7:0] byte_t;");
        assert_eq!(decl.name, "byte_t");
        assert_eq!(
            decl.body,
            TypeDefBody::Scalar {
                width: WidthExpr::Literal(8),
                signed: false
            }
        );
    }

    #[test]
    fn typedef_skalar_signed() {
        let decl = parse("typedef logic signed [15:0] word_t;");
        match decl.body {
            TypeDefBody::Scalar { width, signed } => {
                assert_eq!(width, WidthExpr::Literal(16));
                assert!(signed);
            }
            other => panic!("harus scalar, dapat {other:?}"),
        }
    }

    #[test]
    fn typedef_integer_berlebar_tiga_puluh_dua() {
        let decl = parse("typedef integer int_t;");
        match decl.body {
            TypeDefBody::Scalar { width, signed } => {
                assert_eq!(width, WidthExpr::Literal(32));
                assert!(signed);
            }
            other => panic!("harus scalar, dapat {other:?}"),
        }
    }

    #[test]
    fn typedef_enum_dengan_anggota_berurutan() {
        let decl = parse("typedef enum logic [1:0] { IDLE, RUN, DONE } state_t;");
        assert_eq!(decl.name, "state_t");
        match decl.body {
            TypeDefBody::Enum {
                base_width,
                members,
            } => {
                assert_eq!(base_width, WidthExpr::Literal(2));
                let nama: Vec<&str> = members.iter().map(|m| m.name.as_str()).collect();
                assert_eq!(nama, vec!["IDLE", "RUN", "DONE"]);
                assert!(members.iter().all(|m| m.value.is_none()));
            }
            other => panic!("harus enum, dapat {other:?}"),
        }
    }

    #[test]
    fn typedef_enum_dengan_nilai_eksplisit() {
        let decl = parse("typedef enum logic [3:0] { A, B = 5, C } e_t;");
        match decl.body {
            TypeDefBody::Enum { members, .. } => {
                assert!(members[0].value.is_none());
                assert_eq!(members[1].value, Some(Expr::Number(5)));
                assert!(members[2].value.is_none());
            }
            other => panic!("harus enum, dapat {other:?}"),
        }
    }

    #[test]
    fn typedef_enum_tanpa_lebar_default_satu_bit() {
        let decl = parse("typedef enum { ON, OFF } flag_t;");
        match decl.body {
            TypeDefBody::Enum { base_width, .. } => {
                assert_eq!(base_width, WidthExpr::Literal(1));
            }
            other => panic!("harus enum, dapat {other:?}"),
        }
    }

    #[test]
    fn typedef_enum_boleh_pakai_tanda_koma_ekstra() {
        // Trailing comma sebelum `}` diterima.
        let decl = parse("typedef enum logic [1:0] { A, B, } e_t;");
        match decl.body {
            TypeDefBody::Enum { members, .. } => assert_eq!(members.len(), 2),
            other => panic!("harus enum, dapat {other:?}"),
        }
    }

    #[test]
    fn enum_tanpa_nama_tipe_ditolak() {
        let tokens = lex("typedef enum logic [1:0] { A, B };").expect("lex");
        assert!(parse_typedef(&tokens).is_err());
    }

    #[test]
    fn enum_tanpa_kurung_kurawal_ditolak() {
        let tokens = lex("typedef enum logic [1:0] A, B e_t;").expect("lex");
        assert!(parse_typedef(&tokens).is_err());
    }

    #[test]
    fn enum_kurung_kurawal_tidak_tertutup_ditolak() {
        let tokens = lex("typedef enum logic [1:0] { A, B e_t;").expect("lex");
        assert!(parse_typedef(&tokens).is_err());
    }

    #[test]
    fn typedef_tanpa_semicolon_ditolak() {
        // Tanpa `;` penutup, parser harus menolak, bukan menelan token
        // module item berikutnya.
        let tokens = lex("typedef logic [7:0] byte_t").expect("lex");
        assert!(parse_typedef(&tokens).is_err());
        let tokens = lex("typedef enum logic [1:0] { A, B } e_t").expect("lex");
        assert!(parse_typedef(&tokens).is_err());
    }

    #[test]
    fn typedef_mengonsumsi_semicolon_penutup() {
        let tokens = lex("typedef logic [7:0] byte_t; logic [7:0] x;").expect("lex");
        let (_, consumed) = parse_typedef(&tokens).expect("parse");
        assert_eq!(consumed, 9, "harus berhenti tepat setelah `;`");
        assert_eq!(tokens[consumed].token, Token::Logic);
    }

    #[test]
    fn typedef_tanpa_kworde_tipe_ditolak() {
        let tokens = lex("typedef byte_t;").expect("lex");
        assert!(parse_typedef(&tokens).is_err());
    }

    #[test]
    fn deklarasi_bertipe_membaca_lebar_dari_typedef() {
        let tokens = lex("byte_t data;").expect("lex");
        let (type_name, decl, consumed) = parse_typed_declaration(&tokens).expect("parse");
        assert_eq!(type_name, "byte_t");
        assert_eq!(decl.name, "data");
        assert_eq!(decl.width, WidthExpr::Literal(1));
        assert_eq!(decl.type_name.as_deref(), Some("byte_t"));
        // Fungsi berhenti sebelum `;`; pemanggil module.rs yang mengonsumsinya.
        assert_eq!(consumed, tokens.len() - 1);
    }

    #[test]
    fn deklarasi_bertipe_boleh_timpa_lebar() {
        // `byte_t [3:0] nib;` Persempit tipe yang di-typedef.
        let tokens = lex("byte_t [3:0] nib;").expect("lex");
        let (_, decl, _) = parse_typed_declaration(&tokens).expect("parse");
        assert_eq!(decl.width, WidthExpr::Literal(4));
    }

    #[test]
    fn deklarasi_bertipe_punya_nilai_awal() {
        let tokens = lex("byte_t data = 8'd7;").expect("lex");
        let (_, decl, _) = parse_typed_declaration(&tokens).expect("parse");
        assert!(decl.init.is_some());
    }

    #[test]
    fn deklarator_tambahan_bertipe_dibaca() {
        let tokens = lex("b_t x, y;").expect("lex");
        let (_, first, consumed) = parse_typed_declaration(&tokens).expect("parse");
        assert_eq!(first.name, "x");
        let (tambahan, akhir) =
            parse_more_typed_declarators(&tokens, consumed, "b_t", &first.width, false, first.span)
                .expect("parse");
        assert_eq!(tambahan.len(), 1);
        assert_eq!(tambahan[0].name, "y");
        assert_eq!(tambahan[0].type_name.as_deref(), Some("b_t"));
        // Seperti di atas, `;` terakhir tidak termasuk hasil.
        assert_eq!(akhir, tokens.len() - 1);
    }

    #[test]
    fn deklarator_tambahan_boleh_punya_nilai_awal() {
        let tokens = lex("b_t x = 1, y = 2;").expect("lex");
        let (_, first, consumed) = parse_typed_declaration(&tokens).expect("parse");
        let (tambahan, _) =
            parse_more_typed_declarators(&tokens, consumed, "b_t", &first.width, false, first.span)
                .expect("parse");
        assert!(tambahan[0].init.is_some());
    }

    #[test]
    fn literal_konstan_mengenali_ekspresi_konstan_saja() {
        assert!(literal_konstan(&Expr::Number(3)));
        assert!(literal_konstan(&Expr::Sized {
            value: 1,
            width: 4,
            signed: false,
            unknown_mask: 0,
            zmask: 0,
        }));
        assert!(!literal_konstan(&Expr::ident(
            "a",
            sv_lexer::span::Span::dummy()
        )));
    }
}
