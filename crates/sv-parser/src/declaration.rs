// Tanggung jawab: parse deklarasi signal (logic/wire/reg/bit).
use crate::pesan;
use sv_ast::declaration::Declaration;
use sv_ast::width::WidthExpr;
use sv_lexer::span::Span;
use sv_lexer::spanned::SpannedToken;
use sv_lexer::token::Token;

/// Parse satu pernyataan deklarasi, termasuk daftar deklarator.
///
/// `logic [7:0] a, b, c;` mendeklarasikan tiga signal yang berbagi tipe,
/// lebar, dan signedness yang sama (LRM §6.2), jadi hasilnya berupa vektor.
/// Parse satu atau lebih dimensi unpacked (LRM §7.8) pada `idx`.
///
/// Bentuk yang diterima: `[N]` (`size = N`, indeks mulai 0 menaik) dan
/// `[MSB:LSB]` dengan kedua batas literal. Beberapa dimensi boleh ditulis
/// berturut-turut, misal `mem [0:3][0:15]`.
fn parse_unpacked(
    tokens: &[SpannedToken],
    idx: usize,
) -> Result<(Option<sv_ast::declaration::UnpackedDims>, usize), String> {
    let mut cursor = idx;
    let mut dims = Vec::new();
    while tokens.get(cursor).map(|t| &t.token) == Some(&Token::LBracket) {
        // `None` berarti batas bukan literal. Literal `0` BUKAN penanda
        // non-literal: `[0:3]` adalah dimensi 4 elemen yang sah. Versi lama
        // memperlakukan 0 sebagai "batas tak diketahui", mencatat size 0, dan
        // membuat setiap `mem[2]` ditolak sebagai di luar jangkauan.
        let angka = |at: usize| -> Option<u64> {
            match tokens.get(at).map(|t| &t.token) {
                Some(Token::Number(n)) => Some(*n),
                _ => None,
            }
        };
        let msb = angka(cursor + 1);
        let mut c = cursor + 1; // mulai dari isi '['
        let Some(msb) = msb else {
            // Batas non-literal (misal parameter atau ekspresi): lewati
            // seluruh dimensi hingga ']'. Lebarnya belum bisa dihitung di
            // sini; elaborator yang menentukan.
            {
                let mut depth = 1u32;
                while c < tokens.len() && depth > 0 {
                    match tokens.get(c).map(|t| &t.token) {
                        Some(Token::LBracket) => depth += 1,
                        Some(Token::RBracket) => {
                            depth -= 1;
                            if depth == 0 {
                                c += 1;
                                break;
                            }
                        }
                        _ => {}
                    }
                    c += 1;
                }
                dims.push(sv_ast::declaration::UnpackedDim {
                    size: 0,
                    first: 0,
                    descending: false,
                });
                cursor = c;
                continue;
            }
        };
        // `c` masih menunjuk batas msb; maju satu token agar menunjuk ':'
        // atau ']' penutup.
        c += 1;
        if tokens.get(c).map(|t| &t.token) == Some(&Token::Colon) {
            let Some(lsb) = angka(c + 1) else {
                return Err(pesan::harap(
                    tokens,
                    c + 1,
                    "expected angka lsb dimensi unpacked",
                ));
            };
            c += 2;
            if tokens.get(c).map(|t| &t.token) != Some(&Token::RBracket) {
                return Err(pesan::harap(
                    tokens,
                    c,
                    "expected ']' pada dimensi unpacked",
                ));
            }
            c += 1;
            let size = msb.abs_diff(lsb) + 1;
            // `[left:right]`: `left` selalu indeks PERTAMA yang tertulis.
            // `left >= right` berarti menurun (`[3:0]`), selain itu menaik
            // (`[0:3]`).
            let descending = msb >= lsb;
            dims.push(sv_ast::declaration::UnpackedDim {
                size,
                first: msb,
                descending,
            });
        } else if tokens.get(c).map(|t| &t.token) == Some(&Token::RBracket) {
            c += 1;
            // `[N]` berarti N elemen dengan indeks 0..N-1 menaik.
            dims.push(sv_ast::declaration::UnpackedDim {
                size: msb,
                first: 0,
                descending: false,
            });
        } else {
            return Err(pesan::harap(
                tokens,
                c,
                "expected ']' pada dimensi unpacked",
            ));
        }
        cursor = c;
    }
    if dims.is_empty() {
        return Ok((None, 0));
    }
    Ok((
        Some(sv_ast::declaration::UnpackedDims { dims }),
        cursor - idx,
    ))
}

pub fn parse_declaration(tokens: &[SpannedToken]) -> Result<(Vec<Declaration>, usize), String> {
    if tokens.is_empty() {
        return Err("empty tokens".to_string());
    }

    let span = tokens[0].span;
    let mut idx = 1; // skip type keyword (atau ident pertama dari nama tipe hierarkis)
    // Lewati sisa nama tipe hierarkis: `pkg::subpkg::nama_tipe` — setiap `::`
    // (dua token Colon) diikuti oleh ident adalah bagian dari nama tipe.
    while idx + 2 < tokens.len()
        && matches!(tokens[idx].token, Token::Colon)
        && matches!(tokens[idx + 1].token, Token::Colon)
        && matches!(tokens[idx + 2].token, Token::Ident(_))
    {
        idx += 3; // lewati `:`, `:`, dan nama bagian
    }

    // `struct` / `union` packed (LRM §6.7, §8): body berisi daftar field
    // di antara kurung kurawal, diikuti nama variabel dan ';'.
    if matches!(tokens[0].token, Token::Struct | Token::Union) {
        return parse_struct_union(tokens, span);
    }

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
        Token::Real => Some((64, false)),
        Token::Supply0 => Some((1, false)),
        Token::Supply1 => Some((1, false)),
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

    // Lewati nama tipe bertipe paket setelah tipe net: `wire pkg::type nama;`
    // atau `logic pkg::type nama;`. Bentuk: Ident `::` Ident (:: = dua Colon).
    while idx + 2 < tokens.len()
        && matches!(tokens[idx].token, Token::Ident(_))
        && matches!(tokens[idx + 1].token, Token::Colon)
        && matches!(tokens[idx + 2].token, Token::Colon)
        && tokens.get(idx + 3).is_some_and(|t| matches!(&t.token, Token::Ident(_)))
    {
        idx += 4; // lewati pkg :: type
        // Bisa ada `::` lagi untuk nested package: `pkg::sub::type`
        while idx + 2 < tokens.len()
            && matches!(tokens[idx].token, Token::Colon)
            && matches!(tokens[idx + 1].token, Token::Colon)
            && matches!(tokens[idx + 2].token, Token::Ident(_))
        {
            idx += 3;
        }
    }

    // Additional packed dimensions for multi-dimensional declarations,
    // misal `logic [N_CLASSES-1:0][NAlerts-1:0] class_masks;`. SVRust tidak
    // mendukung multi-dimensional penuh, jadi dimensi tambahan hanya
    // dikonsumsi tanpa diproses.
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

    // LRM §7.8: dimensi unpacked menyusul packed dimension, misal
    // `logic [7:0] mem [0:15];`. Paket packed DAN unpacked (`logic [7:0]
    // x [3:0]`) ditolak karena belum ada modelnya.
    let width = match lebar_bulat {
        Some(w) => WidthExpr::Literal(w),
        None => lebar_tertulis,
    };

    // Nama tipe eksplisit untuk tipe non-bit-vector (mis. `real`) dan tipe
    // net dengan resolusi multi-driver (mis. `wand`/`wor`/`tri`) yang dipakai
    // oleh elaborator untuk membangun `DataType` yang benar.
    let type_nama = match tokens[0].token {
        Token::Real => Some("real".to_string()),
        Token::String => Some("string".to_string()),
        Token::Supply0 => Some("supply0".to_string()),
        Token::Supply1 => Some("supply1".to_string()),
        Token::Tri => Some("tri".to_string()),
        Token::Wor => Some("wor".to_string()),
        Token::Wand => Some("wand".to_string()),
        Token::Wire => Some("wire".to_string()),
        _ => None,
    };

    // Daftar deklarator dipisahkan koma dan berbagi tipe yang sama.
    let mut declarations = Vec::new();
    loop {
        if idx >= tokens.len() {
            return Err("expected signal name".to_string());
        }
        let name = match &tokens[idx].token {
            Token::Ident(n) => n.clone(),
            _ => return Err(pesan::harap(tokens, idx, "expected signal name")),
        };
        idx += 1;

        // LRM §7.8: dimensi unpacked menyusul nama deklarator, misal
        // `logic [7:0] mem [0:15];` atau `logic [7:0] mem [0:3][0:15];` —
        // bukan setelah packed dimension.
        let (unpacked, dim_pakai) = parse_unpacked(tokens, idx)?;
        if unpacked.is_some() {
            idx += dim_pakai;
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

        // Instansiasi interface/modul: `clk_rst_if main_clk (.clk(clk), ...)`.
        // Lewati daftar koneksi port hingga ')' penutup.
        if tokens.get(idx).map(|t| &t.token) == Some(&Token::LParen) {
            let mut depth = 1u32;
            idx += 1;
            while idx < tokens.len() && depth > 0 {
                match &tokens[idx].token {
                    Token::LParen | Token::LBracket | Token::LBrace => depth += 1,
                    Token::RParen | Token::RBracket | Token::RBrace => {
                        depth = depth.saturating_sub(1);
                    }
                    _ => {}
                }
                idx += 1;
            }
        }

        declarations.push(Declaration {
            name,
            // Semua deklarator berbagi tipe, jadi lebar disalin per signal.
            width: width.clone(),
            signed,
            init,
            type_name: type_nama.clone(),
            unpacked,
            struct_fields: None,
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
        return Err(pesan::harap(tokens, idx, "expected ';'"));
    }
    idx += 1;

    Ok((declarations, idx))
}

/// Parse `struct` / `union` packed (LRM §6.7, §8).
///
/// Bentuk: `struct { tipe nama [, tipe nama, ...] } nama_variabel;`
/// atau `union { ... } nama_variabel;`
///
/// Setiap field dipisah koma, dengan format `tipe [lebar] nama` atau
/// `nama : tipe [lebar]`.
fn parse_struct_union(
    tokens: &[SpannedToken],
    span: Span,
) -> Result<(Vec<Declaration>, usize), String> {
    let is_struct = matches!(tokens[0].token, Token::Struct);
    let jenis = if is_struct { "struct" } else { "union" };
    let mut idx = 1; // skip `struct`/`union`

    // Expect `{`
    if tokens.get(idx).map(|t| &t.token) != Some(&Token::LBrace) {
        return Err(crate::pesan::tandai(
            tokens,
            idx,
            format!("expected '{{' after {}", jenis),
        ));
    }
    idx += 1;

    let mut fields: Vec<sv_ast::declaration::StructField> = Vec::new();
    loop {
        if tokens.get(idx).map(|t| &t.token) == Some(&Token::RBrace) {
            idx += 1;
            break;
        }
        if idx >= tokens.len() {
            return Err(format!("unterminated {}", jenis));
        }

        // Baca tipe field (bisa berupa keyword tipe atau ident untuk typedef)
        let tipe_token = tokens[idx].token.clone();
        let _tipe_nama = match &tipe_token {
            Token::Ident(n) => n.clone(),
            t => {
                // Konversi token keyword menjadi string nama
                match t {
                    Token::Logic => "logic".to_string(),
                    Token::Bit => "bit".to_string(),
                    Token::Wire => "wire".to_string(),
                    Token::Reg => "reg".to_string(),
                    Token::Integer => "integer".to_string(),
                    Token::Int => "int".to_string(),
                    Token::Byte => "byte".to_string(),
                    Token::ShortInt => "shortint".to_string(),
                    Token::LongInt => "longint".to_string(),
                    Token::Time => "time".to_string(),
                    Token::Real => "real".to_string(),
                    _ => return Err(format!("tipe field tidak dikenal: {:?}", t)),
                }
            }
        };
        idx += 1;

        // Opsional packed dimension [msb:lsb]
        let (lebar_opt, consumed) = super::dimension::parse_dimension(tokens, idx)?;
        idx += consumed;
        let lebar = lebar_opt;

        // Field name: ident setelah tipe, atau `nama :` jika formatnya `nama : tipe`
        let field_nama = match &tokens[idx].token {
            Token::Ident(n) => n.clone(),
            _ => return Err(format!("expected field name in {}", jenis)),
        };
        idx += 1;

        fields.push(sv_ast::declaration::StructField {
            name: field_nama,
            width: lebar,
            signed: false,
        });

        // Koma atau }
        match tokens.get(idx).map(|t| &t.token) {
            Some(&Token::Comma) => {
                idx += 1;
            }
            Some(&Token::RBrace) => {
                // Akan ditangani di iterasi berikutnya
            }
            _ => return Err(format!("expected ',' or '{{' in {}", jenis)),
        }
    }

    // Expect nama variabel setelah `}`
    if idx >= tokens.len() {
        return Err(format!("expected variable name after {}", jenis));
    }
    let var_nama = match &tokens[idx].token {
        Token::Ident(n) => n.clone(),
        _ => return Err("expected variable name".to_string()),
    };
    idx += 1;

    // Expect `;`
    if idx >= tokens.len() || tokens[idx].token != Token::Semi {
        return Err(pesan::harap(tokens, idx, "expected ';'"));
    }
    idx += 1;

    let lebar_total = fields
        .iter()
        .map(|f| match f.width {
            WidthExpr::Literal(w) => w,
            _ => 0,
        })
        .sum();

    Ok((
        vec![Declaration {
            name: var_nama,
            width: WidthExpr::Literal(lebar_total),
            signed: false,
            init: None,
            type_name: Some(jenis.to_string()),
            unpacked: None,
            struct_fields: Some(fields),
            span,
        }],
        idx,
    ))
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

    #[test]
    fn deklarasi_real() {
        // LRM §5.5: `real` adalah floating point 64-bit.
        let d = parse_one("real r;");
        assert_eq!(d.name, "r");
        assert_eq!(d.width, WidthExpr::Literal(64));
        assert!(!d.signed);
        assert_eq!(d.type_name.as_deref(), Some("real"));
    }

    #[test]
    fn deklarasi_supply0_supply1() {
        // LRM §6.6: `supply0`/`supply1` adalah net konstan 1-bit.
        let d0 = parse_one("supply0 z;");
        assert_eq!(d0.name, "z");
        assert_eq!(d0.width, WidthExpr::Literal(1));
        assert!(!d0.signed);
        assert_eq!(d0.type_name.as_deref(), Some("supply0"));

        let d1 = parse_one("supply1 o;");
        assert_eq!(d1.name, "o");
        assert_eq!(d1.width, WidthExpr::Literal(1));
        assert!(!d1.signed);
        assert_eq!(d1.type_name.as_deref(), Some("supply1"));
    }

    #[test]
    fn deklarasi_string() {
        // LRM §6.14: `string` adalah tipe dinamis tanpa lebar bit bawaan.
        let d = parse_one("string s;");
        assert_eq!(d.name, "s");
        assert_eq!(d.type_name.as_deref(), Some("string"));
    }

    #[test]
    fn deklarasi_tri_wor_wand() {
        // LRM §6.6: `tri`/`wor`/`wand` adalah net dengan resolusi multi-driver.
        let d0 = parse_one("tri t;");
        assert_eq!(d0.name, "t");
        assert_eq!(d0.width, WidthExpr::Literal(1));
        assert_eq!(d0.type_name.as_deref(), Some("tri"));

        let d1 = parse_one("wor w;");
        assert_eq!(d1.name, "w");
        assert_eq!(d1.width, WidthExpr::Literal(1));
        assert_eq!(d1.type_name.as_deref(), Some("wor"));

        let d2 = parse_one("wand a;");
        assert_eq!(d2.name, "a");
        assert_eq!(d2.width, WidthExpr::Literal(1));
        assert_eq!(d2.type_name.as_deref(), Some("wand"));

        // Dengan lebar eksplisit
        let d3 = parse_one("tri [7:0] bus;");
        assert_eq!(d3.name, "bus");
        assert_eq!(d3.width, WidthExpr::Literal(8));
        assert_eq!(d3.type_name.as_deref(), Some("tri"));
    }

    #[test]
    fn deklarasi_string_dengan_init() {
        let d = parse_one("string s = \"hello\";");
        assert_eq!(d.name, "s");
        assert!(d.init.is_some());
    }

    #[test]
    fn deklarasi_struct_union() {
        // LRM §6.7, §8: `struct` dan `union` packed.
        let d = parse_one("struct { logic [7:0] a, logic [3:0] b } s;");
        assert_eq!(d.name, "s");
        assert_eq!(d.type_name.as_deref(), Some("struct"));
        assert_eq!(d.struct_fields.as_ref().unwrap().len(), 2);
        assert_eq!(
            d.struct_fields.as_ref().unwrap()[0],
            sv_ast::declaration::StructField {
                name: "a".to_string(),
                width: WidthExpr::Literal(8),
                signed: false,
            }
        );
        assert_eq!(
            d.struct_fields.as_ref().unwrap()[1],
            sv_ast::declaration::StructField {
                name: "b".to_string(),
                width: WidthExpr::Literal(4),
                signed: false,
            }
        );

        let u = parse_one("union { logic [7:0] x, logic [7:0] y } u;");
        assert_eq!(u.name, "u");
        assert_eq!(u.type_name.as_deref(), Some("union"));
        assert_eq!(u.struct_fields.as_ref().unwrap().len(), 2);
    }
}
