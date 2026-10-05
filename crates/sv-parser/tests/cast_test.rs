// Tanggung jawab: integration test parsing type cast dan $bits SystemVerilog.
use sv_ast::expression::Expr as AstExpr;
use sv_lexer::SpannedToken;
use sv_parser::parse_module;

fn parse_expr(source: &str) -> AstExpr {
    // Bungkus ekspresi solitary dalam module supaya parser module yang diuji,
    // lalu ambil RHS `assign y = <ekspresi>` pertama.
    let sv = format!("module m(output [31:0] y); assign y = {source}; endmodule");
    let tokens: Vec<SpannedToken> = sv_lexer::lex(&sv).expect("lex");
    let module = parse_module(&tokens).expect("parse");
    let statement = module.statements.first().expect("ada statement");
    let sv_ast::statement::Statement::ContinuousAssign { rhs, .. } = statement else {
        panic!("expected continuous assign");
    };
    rhs.clone()
}

fn parse_expr_gagal(source: &str) -> String {
    let sv = format!("module m(output [31:0] y); assign y = {source}; endmodule");
    let tokens = sv_lexer::lex(&sv).expect("lex");
    parse_module(&tokens).expect_err("parse harus gagal")
}

// --- LRM §6.14: type cast ke typedef ---

#[test]
fn cast_typedef_menyimpan_nama_tipe() {
    match parse_expr("byte_t'(a)") {
        AstExpr::Cast {
            type_name, operand, ..
        } => {
            assert_eq!(type_name, "byte_t");
            assert!(matches!(*operand, AstExpr::Ident { .. }));
        }
        other => panic!("expected Cast, dapat {other:?}"),
    }
}

#[test]
fn cast_bukan_panggilan_function() {
    // LRM §6.14: pembeda `tipe'(x)` dari `f(x)` adalah tanda `'` di antaranya.
    // Kalau salah dikira function call, argumennya tak akan punya cast.
    let expr = parse_expr("nibble_t'(b[3:0])");
    assert!(
        matches!(expr, AstExpr::Cast { .. }),
        "harus Cast, bukan FunctionCall"
    );
}

#[test]
fn cast_operand_boleh_ekspresi_lengkap() {
    // Operand diparse sebagai ekspresi penuh, jadi operator di dalamnya
    // tetap punya prioritas yang benar.
    match parse_expr("word_t'(a + b * c)") {
        AstExpr::Cast { operand, .. } => {
            assert!(matches!(*operand, AstExpr::Binary { .. }));
        }
        other => panic!("expected Cast, dapat {other:?}"),
    }
}

#[test]
fn cast_tanpa_kurung_ditolak() {
    let pesan = parse_expr_gagal("byte_t'a");
    assert!(
        pesan.contains("'(") || pesan.contains("cast"),
        "pesan tak menjelaskan masalah: {pesan}"
    );
}

// --- LRM §6.14: sign cast ---

#[test]
fn sign_cast_menyimpan_flag_tanda() {
    match parse_expr("signed'(a)") {
        AstExpr::SignCast {
            signed, operand, ..
        } => {
            assert!(signed);
            assert!(matches!(*operand, AstExpr::Ident { .. }));
        }
        other => panic!("expected SignCast, dapat {other:?}"),
    }
}

#[test]
fn unsigned_cast_menyimpan_flag_tanda() {
    match parse_expr("unsigned'(a)") {
        AstExpr::SignCast {
            signed, operand, ..
        } => {
            assert!(!signed);
            assert!(matches!(*operand, AstExpr::Ident { .. }));
        }
        other => panic!("expected SignCast, dapat {other:?}"),
    }
}

#[test]
fn sign_cast_bukan_cast_typedef() {
    // `signed` adalah keyword, bukan nama tipe, jadi harus jadi SignCast
    // (lebar tetap) dan bukan Cast (lebar berubah).
    assert!(matches!(parse_expr("signed'(a)"), AstExpr::SignCast { .. }));
}

// --- LRM §20: $bits ---

#[test]
fn bits_menyimpan_operand() {
    match parse_expr("$bits(a)") {
        AstExpr::Bits { operand, .. } => {
            assert!(matches!(*operand, AstExpr::Ident { .. }));
        }
        other => panic!("expected Bits, dapat {other:?}"),
    }
}

#[test]
fn bits_operand_boleh_select_dan_concat() {
    // `$bits` menerima ekspresi apa pun, jadi select dan concat harus
    // diteruskan apa adanya sebagai operand.
    assert!(matches!(parse_expr("$bits(a[7:0])"), AstExpr::Bits { .. }));
    assert!(matches!(parse_expr("$bits({a, b})"), AstExpr::Bits { .. }));
}

#[test]
fn bits_tanpa_kurung_ditolak() {
    // LRM §20: `$bits` wajib punya argumen; tanpa kurung bukan pemanggilan.
    let pesan = parse_expr_gagal("$bits");
    assert!(
        pesan.contains("$bits") || pesan.contains("LParen"),
        "pesan tak menjelaskan masalah: {pesan}"
    );
}
