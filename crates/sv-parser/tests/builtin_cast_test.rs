// Tanggung jawab: integration test parsing cast ke tipe bawaan & size cast (LRM §6.14).
use sv_ast::expression::Expr as AstExpr;
use sv_lexer::SpannedToken;
use sv_parser::parse_module;

fn parse_expr(source: &str) -> AstExpr {
    let sv = format!("module m(output [31:0] y); assign y = {source}; endmodule");
    let tokens: Vec<SpannedToken> = sv_lexer::lex(&sv).expect("lex");
    let module = parse_module(&tokens).expect("parse");
    let sv_ast::statement::Statement::ContinuousAssign { rhs, .. } = &module.statements[0] else {
        panic!("expected continuous assign");
    };
    rhs.clone()
}

/// Pesan error dari lexer maupun parser, apa pun tahap yang menolaknya.
///
/// Bentuk yang salah bisa tertangkap di lexer (mis. `16'a`) atau di parser
/// (mis. `'(a)`), jadi keduanya dikumpulkan di sini supaya test cukup
/// memeriksa pesannya tanpa tahu tahap mana yang menolak.
fn parse_expr_gagal(source: &str) -> String {
    let sv = format!("module m(output [31:0] y); assign y = {source}; endmodule");
    match sv_lexer::lex(&sv) {
        Err(err) => err.to_string(),
        Ok(tokens) => parse_module(&tokens).expect_err("parse harus gagal"),
    }
}

#[test]
fn token_size_cast_menyimpan_lebar() {
    // Token harus membawa lebarnya supaya parser tidak perlu menebak.
    let tokens = sv_lexer::lex("16'(a)").expect("lex");
    assert!(
        tokens
            .iter()
            .any(|t| t.token == sv_lexer::Token::SizeCast(16)),
        "token SizeCast(16) tidak ada: {tokens:?}"
    );
}

#[test]
fn kutip_tanpa_lebar_jadi_token_cast_bukan_size_cast() {
    // `'(a)` tidak punya lebar, jadi tidak boleh jadi `SizeCast`; ia jadi
    // `Token::Cast` dan ditolak parser dengan pesan yang menyebut bentuk benar.
    let tokens = sv_lexer::lex("'(a)").expect("lex");
    assert!(
        tokens.iter().any(|t| t.token == sv_lexer::Token::Cast),
        "token Cast tidak ada: {tokens:?}"
    );
    assert!(
        !tokens
            .iter()
            .any(|t| matches!(t.token, sv_lexer::Token::SizeCast(_))),
        "' tanpa lebar tidak boleh jadi SizeCast: {tokens:?}"
    );
}

// --- LRM §6.14 / Tabel 6-22: cast ke tipe bawaan ---

#[test]
fn integer_dijadi_builtin_cast_32_bit_signed() {
    match parse_expr("integer'(a)") {
        AstExpr::BuiltinCast {
            type_name,
            width,
            signed,
            operand,
            ..
        } => {
            assert_eq!(type_name, "integer");
            assert_eq!(width, 32);
            assert!(signed, "integer bertanda");
            assert!(matches!(*operand, AstExpr::Ident { .. }));
        }
        other => panic!("expected BuiltinCast, dapat {other:?}"),
    }
}

#[test]
fn int_dijadi_builtin_cast_32_bit_signed() {
    // LRM Tabel 6-22: `int` dan `integer` sama-sama 32-bit signed.
    match parse_expr("int'(a)") {
        AstExpr::BuiltinCast { width, signed, .. } => {
            assert_eq!(width, 32);
            assert!(signed);
        }
        other => panic!("expected BuiltinCast, dapat {other:?}"),
    }
}

#[test]
fn byte_dijadi_builtin_cast_8_bit_signed() {
    // LRM Tabel 6-22: `byte` bertanda. Diverifikasi terhadap iverilog 12.0:
    // `$display("%0d", byte'(8'hA5))` = -91, bukan 165.
    match parse_expr("byte'(a)") {
        AstExpr::BuiltinCast { width, signed, .. } => {
            assert_eq!(width, 8);
            assert!(signed, "byte bertanda");
        }
        other => panic!("expected BuiltinCast, dapat {other:?}"),
    }
}

#[test]
fn shortint_dijadi_builtin_cast_16_bit_signed() {
    // LRM Tabel 6-22: `shortint` bertanda. iverilog: `shortint'(-1)` = -1.
    match parse_expr("shortint'(a)") {
        AstExpr::BuiltinCast { width, signed, .. } => {
            assert_eq!(width, 16);
            assert!(signed, "shortint bertanda");
        }
        other => panic!("expected BuiltinCast, dapat {other:?}"),
    }
}

#[test]
fn tipe_bawaan_tanpa_tanda_hanya_bit_logic_time() {
    // Kontras dengan dua test di atas: hanya `bit`, `logic`, dan `time` yang
    // tanpa tanda. iverilog: `time'(-1)` = 18446744073709551615.
    for (nama, lebar) in [("bit", 1u32), ("logic", 1), ("time", 64)] {
        match parse_expr(&format!("{nama}'(a)")) {
            AstExpr::BuiltinCast { width, signed, .. } => {
                assert_eq!(width, lebar, "{nama} lebar {lebar}");
                assert!(!signed, "{nama} tanpa tanda");
            }
            other => panic!("expected BuiltinCast untuk {nama}, dapat {other:?}"),
        }
    }
}

#[test]
fn longint_dijadi_builtin_cast_64_bit_signed() {
    match parse_expr("longint'(a)") {
        AstExpr::BuiltinCast { width, signed, .. } => {
            assert_eq!(width, 64);
            assert!(signed);
        }
        other => panic!("expected BuiltinCast, dapat {other:?}"),
    }
}

#[test]
fn time_dijadi_builtin_cast_64_bit_unsigned() {
    match parse_expr("time'(a)") {
        AstExpr::BuiltinCast { width, signed, .. } => {
            assert_eq!(width, 64);
            assert!(!signed, "waktu simulasi unsigned");
        }
        other => panic!("expected BuiltinCast, dapat {other:?}"),
    }
}

#[test]
fn logic_tanpa_range_adalah_satu_bit() {
    // LRM §6.16: `logic` tanpa `[msb:lsb]` adalah vektor 1 bit, bukan tipe
    // skalar penuh. iverilog: `logic'(8'hA5)` = 1, yaitu LSB-nya.
    match parse_expr("logic'(a)") {
        AstExpr::BuiltinCast { width, signed, .. } => {
            assert_eq!(width, 1);
            assert!(!signed);
        }
        other => panic!("expected BuiltinCast, dapat {other:?}"),
    }
}

#[test]
fn bit_tanpa_range_adalah_satu_bit() {
    match parse_expr("bit'(a)") {
        AstExpr::BuiltinCast { width, .. } => assert_eq!(width, 1),
        other => panic!("expected BuiltinCast, dapat {other:?}"),
    }
}

#[test]
fn builtin_cast_bukan_typedef_cast() {
    // Nama tipe bawaan harus dikenali sebagai keyword, bukan `Ident` yang
    // lalu ditolak `undefined type` saat elaborasi.
    assert!(matches!(
        parse_expr("integer'(a)"),
        AstExpr::BuiltinCast { .. }
    ));
}

#[test]
fn builtin_cast_operand_boleh_ekspresi_lengkap() {
    match parse_expr("integer'(a + b * c)") {
        AstExpr::BuiltinCast { operand, .. } => {
            assert!(matches!(*operand, AstExpr::Binary { .. }));
        }
        other => panic!("expected BuiltinCast, dapat {other:?}"),
    }
}

// --- LRM §6.14: size cast ---

#[test]
fn size_cast_menyimpan_lebar() {
    match parse_expr("16'(a)") {
        AstExpr::SizeCast {
            width,
            operand,
            span,
        } => {
            assert_eq!(width, 16);
            assert!(matches!(*operand, AstExpr::Ident { .. }));
            // Span harus ada: error size cast tanpa kurung harus bisa ditelusuri.
            assert!(span.line > 0);
        }
        other => panic!("expected SizeCast, dapat {other:?}"),
    }
}

#[test]
fn size_cast_lebar_berbagai_diterima() {
    for lebar in [1u32, 4, 8, 32, 64, 128] {
        match parse_expr(&format!("{lebar}'(a)")) {
            AstExpr::SizeCast { width, .. } => assert_eq!(width, lebar),
            other => panic!("expected SizeCast untuk {lebar}, dapat {other:?}"),
        }
    }
}

#[test]
fn size_cast_bukan_typedef_cast() {
    // Size cast tidak punya nama tipe, jadi tidak boleh jadi `Cast`.
    assert!(matches!(parse_expr("16'(a)"), AstExpr::SizeCast { .. }));
}

#[test]
fn size_cast_lebar_nol_ditolak() {
    // LRM §6.14: lebar harus positif; `0'(x)` tidak sah.
    let pesan = parse_expr_gagal("0'(a)");
    assert!(
        pesan.contains("lebar") || pesan.contains("size cast"),
        "pesan tak menjelaskan masalah: {pesan}"
    );
}

#[test]
fn size_cast_tanpa_kurung_ditolak() {
    // `16'a` bukan size cast: operand wajib dalam kurung. iverilog juga
    // menolak (`syntax error`).
    let pesan = parse_expr_gagal("16'a");
    assert!(
        pesan.contains("size cast") || pesan.contains("basis"),
        "pesan tak menjelaskan masalah: {pesan}"
    );
}

#[test]
fn kutip_tanpa_lebar_ditolak() {
    // `'(x)` tanpa lebar eksplisit bukan bentuk yang sah. Pesannya harus
    // menyebut size cast, bukan "expected basis" yang menyesatkan.
    let pesan = parse_expr_gagal("'(a)");
    assert!(
        pesan.contains("size cast") || pesan.contains("lebar"),
        "pesan tak menjelaskan masalah: {pesan}"
    );
}

// --- Regresi: bentuk lama tidak rusak ---

#[test]
fn typedef_cast_tetap_typedef_cast() {
    // Nama non-bawaan harus tetap lewat jalur typedef, bukan BuiltinCast.
    assert!(matches!(parse_expr("byte_t'(a)"), AstExpr::Cast { .. }));
}

#[test]
fn based_literal_tetap_bukan_size_cast() {
    // `8'hFF` punya basis, jadi bukan size cast dan lebarnya 8.
    match parse_expr("8'hFF") {
        AstExpr::Sized { width, value, .. } => {
            assert_eq!(width, 8);
            assert_eq!(value, 0xFF);
        }
        other => panic!("expected Sized, dapat {other:?}"),
    }
}

#[test]
fn unsized_based_literal_tetap_32_bit() {
    // LRM §5.7.1: literal tanpa ukuran lebarnya 32.
    match parse_expr("'hFF") {
        AstExpr::Sized { width, .. } => assert_eq!(width, 32),
        other => panic!("expected Sized, dapat {other:?}"),
    }
}
