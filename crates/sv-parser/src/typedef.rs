// Tanggung jawab: parse deklarasi `typedef` skalar, `typedef enum`, dan
// `typedef struct packed`.
use sv_ast::expression::Expr;
use sv_ast::typedef::{EnumMember, StructFieldDef, TypeDefBody, TypeDefDecl};
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

    // LRM §6.16 / §8.20: `typedef class Nama;` — alias kelas OOP. Lewati
    // `class` lalu baca nama.
    if tokens.get(idx).map(|t| &t.token) == Some(&Token::Class) {
        idx += 1; // skip 'class'
        let nama = baca_nama(tokens, idx)?;
        idx += 1;
        if tokens.get(idx).map(|t| &t.token) != Some(&Token::Semi) {
            return Err("expected ';' menutup typedef class".to_string());
        }
        idx += 1;
        return Ok((
            TypeDefDecl {
                name: nama,
                body: TypeDefBody::Scalar {
                    width: WidthExpr::Literal(1),
                    signed: false,
                },
                span,
            },
            idx,
        ));
    }

    // Bentuk enumerasi punya daftar anggota di dalam kurung kurawal.
    if tokens.get(idx).map(|t| &t.token) == Some(&Token::Enum) {
        idx += 1;
        let tipe_atom = tokens
            .get(idx)
            .and_then(|t| sv_lexer::token::tipe::atom(&t.token));
        // Apakah enum menulis tipe dasar eksplisit (`enum logic [1:0]`, `enum
        // int`, ...)? Bila tidak ada sama sekali (langsung `{`), lebarnya
        // di-auto-size saat elaborasi (WidthExpr::Auto, LRM §6.7).
        let ada_tipe_dasar = tipe_atom.is_some()
            || tokens
                .get(idx)
                .is_some_and(|t| sv_lexer::token::tipe::bisa_mulai_deklarasi(&t.token));
        if ada_tipe_dasar {
            idx += 1;
        }
        // Qualifier signedness opsional pada tipe dasar enum.
        if matches!(
            tokens.get(idx).map(|t| &t.token),
            Some(Token::Signed) | Some(Token::Unsigned)
        ) {
            idx += 1;
        }
        // LRM §6.7: lebar enum boleh ditulis eksplisit; tanpa itu default 1 bit.
        let (base_width, consumed) = if ada_tipe_dasar {
            match tipe_atom {
                Some((w, _)) => (WidthExpr::Literal(w as usize), 0),
                None => super::dimension::parse_dimension(tokens, idx)?,
            }
        } else {
            // LRM §6.7: enum tanpa tipe dasar eksplisit default 1 bit.
            (WidthExpr::Literal(1), 0)
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
            return Err(super::pesan::harap(
                tokens,
                idx,
                "expected ';' menutup typedef",
            ));
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

    // Bentuk struct: `typedef struct packed { ... } nama;` (LRM §6.7).
    //
    // `struct` adalah token khusus; `packed` diperlakukan lexer sebagai Ident,
    // begitu juga qualifier signedness opsional. Setiap field berupa tipe atom
    // (`logic`/`bit`/`wire`/`reg`/tipe integer) atau nama typedef, dengan
    // dimension packed opsional. Lebar total dihitung saat elaborasi dengan
    // menjumlahkan lebar tiap field.
    if tokens.get(idx).map(|t| &t.token) == Some(&Token::Struct) {
        idx += 1;
        // Qualifier `packed` (Ident) opsional.
        if tokens
            .get(idx)
            .map(|t| &t.token)
            .is_some_and(|t| matches!(t, Token::Ident(n) if n == "packed"))
        {
            idx += 1;
        }
        // Qualifier signedness opsional.
        if matches!(
            tokens.get(idx).map(|t| &t.token),
            Some(Token::Signed) | Some(Token::Unsigned)
        ) {
            idx += 1;
        }
        if tokens.get(idx).map(|t| &t.token) != Some(&Token::LBrace) {
            return Err("expected '{' membuka struct typedef".to_string());
        }
        idx += 1;

        let mut fields = Vec::new();
        while idx < tokens.len() && tokens[idx].token != Token::RBrace {
            let tk = tokens.get(idx).map(|t| t.token.clone());
            let (mut lebar, mut signed, type_name): (WidthExpr, bool, Option<String>) = match &tk {
                Some(t) if sv_lexer::token::tipe::atom(t).is_some() => {
                    let (w, s) = sv_lexer::token::tipe::atom(t).unwrap();
                    (WidthExpr::Literal(w as usize), s, None)
                }
                Some(Token::Logic) | Some(Token::Bit) | Some(Token::Wire) | Some(Token::Reg) => {
                    (WidthExpr::Literal(1), false, None)
                }
                Some(Token::Ident(n)) => (WidthExpr::Literal(1), false, Some(n.clone())),
                other => return Err(format!("tipe field struct tidak dikenal: {:?}", other)),
            };
            idx += 1;
            // Qualifier signedness opsional setelah tipe field.
            if matches!(
                tokens.get(idx).map(|t| &t.token),
                Some(Token::Signed) | Some(Token::Unsigned)
            ) {
                signed = matches!(tokens[idx].token, Token::Signed);
                idx += 1;
            }
            // Dimension opsional. Untuk tipe atom berarti packed width; untuk
            // typedef dilewati (lebarnya di-resolve lewat typedef lain).
            if tokens.get(idx).map(|t| &t.token) == Some(&Token::LBracket) {
                if type_name.is_none() {
                    let (w, consumed) = super::dimension::parse_dimension(tokens, idx)?;
                    lebar = w;
                    idx += consumed;
                } else {
                    let mut depth = 1u32;
                    idx += 1;
                    while idx < tokens.len() && depth > 0 {
                        match tokens[idx].token {
                            Token::LBracket => depth += 1,
                            Token::RBracket => depth -= 1,
                            _ => {}
                        }
                        idx += 1;
                    }
                }
            }
            // Nama field.
            let field_nama = match tokens.get(idx).map(|t| &t.token) {
                Some(Token::Ident(n)) => n.clone(),
                other => return Err(format!("expected nama field struct, dapat {other:?}")),
            };
            idx += 1;
            // Pemisah `,` atau `;` antar field.
            if matches!(
                tokens.get(idx).map(|t| &t.token),
                Some(Token::Comma) | Some(Token::Semi)
            ) {
                idx += 1;
            }
            fields.push(StructFieldDef {
                name: field_nama,
                width: lebar,
                signed,
                type_name,
            });
        }
        if tokens.get(idx).map(|t| &t.token) != Some(&Token::RBrace) {
            return Err("expected '}' menutup struct typedef".to_string());
        }
        idx += 1;
        let name = baca_nama(tokens, idx)?;
        idx += 1;
        if tokens.get(idx).map(|t| &t.token) != Some(&Token::Semi) {
            return Err(super::pesan::harap(
                tokens,
                idx,
                "expected ';' menutup typedef",
            ));
        }
        idx += 1;
        return Ok((
            TypeDefDecl {
                name,
                body: TypeDefBody::Struct { fields },
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
    } else if tokens
        .get(idx)
        .is_some_and(|t| matches!(&t.token, Token::Ident(_)))
    {
        // `typedef Ident #(...) Nama;` atau `typedef Ident Nama;` — tipe
        // referensi dengan optional parameter override (LRM §8.20).
        idx += 1; // skip nama tipe referensi
                  // Lewati parameter override opsional #(...)
        if idx < tokens.len() && tokens[idx].token == Token::Hash {
            idx += 1; // skip '#'
            if idx < tokens.len() && tokens[idx].token == Token::LParen {
                idx += 1; // skip '('
                let mut paren_depth = 1u32;
                while idx < tokens.len() && paren_depth > 0 {
                    match tokens[idx].token {
                        Token::LParen => paren_depth += 1,
                        Token::RParen => paren_depth -= 1,
                        _ => {}
                    }
                    idx += 1;
                }
            }
        }
        // Lewati dimensi packed opsional setelah tipe referensi
        while idx < tokens.len() && tokens[idx].token == Token::LBracket {
            let mut depth = 1u32;
            idx += 1;
            while idx < tokens.len() && depth > 0 {
                match tokens[idx].token {
                    Token::LBracket => depth += 1,
                    Token::RBracket => depth -= 1,
                    _ => {}
                }
                idx += 1;
            }
        }
        // Signedness qualifier opsional
        if matches!(
            tokens.get(idx).map(|t| &t.token),
            Some(Token::Signed) | Some(Token::Unsigned)
        ) {
            idx += 1;
        }
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
        return Err(super::pesan::harap(
            tokens,
            idx,
            "expected ';' menutup typedef",
        ));
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
    let mut type_name = match &tokens[0].token {
        Token::Ident(n) => n.clone(),
        other => return Err(format!("expected nama tipe, dapat {other:?}")),
    };
    let mut idx = 1;
    // Nama tipe berkualifikasi paket: `pkg::nama_tipe` (mis.
    // `prim_alert_pkg::alert_tx_t`). `::` adalah dua token `Colon`.
    if tokens.get(idx).map(|t| &t.token) == Some(&Token::Colon)
        && tokens.get(idx + 1).map(|t| &t.token) == Some(&Token::Colon)
        && tokens
            .get(idx + 2)
            .is_some_and(|t| matches!(&t.token, Token::Ident(_)))
    {
        if let Token::Ident(n) = &tokens[idx + 2].token {
            type_name = format!("{}::{}", type_name, n);
        }
        idx += 3;
    }

    let signed = matches!(tokens.get(idx).map(|t| &t.token), Some(Token::Signed));
    if matches!(
        tokens.get(idx).map(|t| &t.token),
        Some(Token::Signed) | Some(Token::Unsigned)
    ) {
        idx += 1;
    }

    let mut width = WidthExpr::Literal(1);
    let (width_parsed, consumed) = super::dimension::parse_dimension(tokens, idx)?;
    idx += consumed;
    if consumed > 0 {
        width = width_parsed;
    }

    let span = tokens[0].span;
    let name = match tokens.get(idx).map(|t| &t.token) {
        Some(Token::Ident(n)) => n.clone(),
        other => return Err(format!("expected nama signal, dapat {other:?}")),
    };
    idx += 1;

    // Packed dimension setelah nama: `tlul_pkg::tl_h2d_t tl_demux_h2d[32'd2];`
    let (width_packed, consumed_packed) = super::dimension::parse_dimension(tokens, idx)?;
    if consumed_packed > 0 {
        idx += consumed_packed;
        // Override width dari packed dimension setelah nama
        width = width_packed;
    }

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
        struct_fields: None,
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
            struct_fields: None,
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
