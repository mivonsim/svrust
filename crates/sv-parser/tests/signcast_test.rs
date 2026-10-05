// Tanggung jawab: test parsing `$signed` / `$unsigned` (LRM §11.4.7, §20).
//
// Keduanya identik dengan `signed'(x)` / `unsigned'(x)` pada LRM §6.14: hanya
// mengubah signedness, lebar TIDAK berubah. Keyword-nya sudah ada, jadi bentuk
// system function-nya sempat ditolak parser.
//
// Nilai hasil evaluasi diuji di `sv-elaborator` (`cast_test.rs`) karena
// `sv-parser` tidak boleh depend ke `sv-elaborator`.
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

#[test]
fn signed_diparse_sebagai_sign_cast() {
    match parse_expr("$signed(a)") {
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
fn unsigned_diparse_sebagai_sign_cast() {
    match parse_expr("$unsigned(a)") {
        AstExpr::SignCast { signed, .. } => assert!(!signed),
        other => panic!("expected SignCast, dapat {other:?}"),
    }
}

#[test]
fn signed_setara_dengan_bentuk_cast() {
    // LRM §6.14 dan §11.4.7 menyatakan keduanya identik, jadi AST-nya harus
    // sama juga — bukan dua node berbeda yang bisa menyimpang diam-diam.
    //
    // Span dilewati karena `$signed` (7 karakter) dan `signed` (6) punya
    // panjang berbeda; yang dibandingkan adalah strukturnya.
    fn tanpa_span(e: &AstExpr) -> String {
        let s = format!("{e:?}");
        // Buang setiap `span: Span { ... }` sebelum membandingkan.
        let mut out = String::new();
        let mut rest = s.as_str();
        while let Some(i) = rest.find("span: Span {") {
            out.push_str(&rest[..i]);
            match rest[i..].find("}") {
                Some(j) => rest = &rest[i + j + 1..],
                None => break,
            }
        }
        out.push_str(rest);
        out
    }
    assert_eq!(
        tanpa_span(&parse_expr("$signed(a)")),
        tanpa_span(&parse_expr("signed'(a)"))
    );
    assert_eq!(
        tanpa_span(&parse_expr("$unsigned(a)")),
        tanpa_span(&parse_expr("unsigned'(a)"))
    );
}

#[test]
fn signed_operand_boleh_ekspresi_lengkap() {
    match parse_expr("$signed(a + b)") {
        AstExpr::SignCast { operand, .. } => {
            assert!(matches!(*operand, AstExpr::Binary { .. }));
        }
        other => panic!("expected SignCast, dapat {other:?}"),
    }
}

#[test]
fn signed_tanpa_kurung_ditolak() {
    let tokens =
        sv_lexer::lex("module m(output [31:0] y); assign y = $signed; endmodule").expect("lex");
    assert!(
        parse_module(&tokens).is_err(),
        "$signed tanpa kurung harus ditolak"
    );
}

#[test]
fn signed_kurung_tidak_tertutup_ditolak() {
    let tokens =
        sv_lexer::lex("module m(output [31:0] y); assign y = $signed(1; endmodule").expect("lex");
    assert!(
        parse_module(&tokens).is_err(),
        "$signed dengan kurung tak tertutup harus ditolak"
    );
}

#[test]
fn bits_yang_boleh_diparse_tidak_tertangkap_sebagai_sign_cast() {
    // `$bits(x)` tetap harus jadi node `Bits`, bukan `SignCast`.
    assert!(matches!(parse_expr("$bits(a)"), AstExpr::Bits { .. }));
    assert!(matches!(parse_expr("$time"), AstExpr::SystemTime { .. }));
}
