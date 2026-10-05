// Tanggung jawab: integration test deklarasi tipe integer atom (LRM §6.16).
use sv_ast::declaration::Declaration;
use sv_lexer::SpannedToken;
use sv_parser::parse_module;

fn parse_decl(source: &str) -> (Vec<Declaration>, u32, bool) {
    let sv = format!("module m; {source} endmodule");
    let tokens: Vec<SpannedToken> = sv_lexer::lex(&sv).expect("lex");
    let module = parse_module(&tokens).expect("parse");
    let decls = &module.declarations;
    let d = &decls[0];
    let lebar = match &d.width {
        sv_ast::width::WidthExpr::Literal(n) => *n as u32,
        sv_ast::width::WidthExpr::Param(_)
        | sv_ast::width::WidthExpr::ParamMinusOne(_)
        | sv_ast::width::WidthExpr::Expr { .. } => {
            panic!("lebar harus literal, dapat {:?}", d.width)
        }
    };
    (decls.clone(), lebar, d.signed)
}

#[test]
fn byte_berlebar_8_bit_signed() {
    // LRM Tabel 6-22: `byte` 8 bit bertanda. iverilog: `$display("%0d", byte'(8'hA5))`
    // = -91.
    let (decls, lebar, signed) = parse_decl("byte b;");
    assert_eq!(decls.len(), 1);
    assert_eq!(lebar, 8);
    assert!(signed, "byte bertanda");
}

#[test]
fn shortint_berlebar_16_bit_signed() {
    let (_, lebar, signed) = parse_decl("shortint s;");
    assert_eq!(lebar, 16);
    assert!(signed, "shortint bertanda");
}

#[test]
fn longint_berlebar_64_bit_signed() {
    let (_, lebar, signed) = parse_decl("longint l;");
    assert_eq!(lebar, 64);
    assert!(signed, "longint bertanda");
}

#[test]
fn time_berlebar_64_bit_unsigned() {
    // iverilog: `time'(-1)` = 18446744073709551615, jadi tanpa tanda.
    let (_, lebar, signed) = parse_decl("time t;");
    assert_eq!(lebar, 64);
    assert!(!signed, "time tanpa tanda");
}

#[test]
fn integer_dan_int_tetap_32_bit_signed() {
    // Regresi: `integer`/`int` sudah ada sebelum tipe atom baru.
    for kw in ["integer", "int"] {
        let (_, lebar, signed) = parse_decl(&format!("{kw} v;"));
        assert_eq!(lebar, 32, "{kw}");
        assert!(signed, "{kw} bertanda");
    }
}

#[test]
fn logic_polos_tetap_membutuhkan_dimension() {
    // LRM §6.16: `logic` tanpa `[msb:lsb]` adalah vektor 1 bit, jadi tipe atom
    // dengan lebar tetap TIDAK boleh memuatnya. Kalau `logic` ikut daftar tipe
    // atom, `logic [7:0] a;` akan kehilangan lebarnya.
    let (_, lebar, _) = parse_decl("logic [7:0] a;");
    assert_eq!(lebar, 8, "dimension eksplisit harus dipakai");
}

#[test]
fn logic_polos_adalah_satu_bit() {
    // Tanpa dimension, `logic` default 1 bit.
    let (decls, _, _) = parse_decl("logic a;");
    // Tanpa dimension, `logic` tidak punya lebar eksplisit; lebar default 1 bit
    // dipakai saat elaborasi. Yang diuji di sini hanya bahwa `logic` TIDAK
    // masuk daftar tipe atom — kalau iya, `lebar` akan bernilai 1 dari
    // `declaration.rs` dan test ini tidak bisa membedakan.
    let (_, lebar, _) = parse_decl("logic a;");
    assert_eq!(lebar, 1, "lebar default `logic` polos");
    assert_eq!(decls[0].width, sv_ast::width::WidthExpr::Literal(1));
}

#[test]
fn tipe_atom_boleh_ikut_signedness_eksplisit() {
    // `byte unsigned b;` men overrid
    // signedness bawaan.
    let (_, lebar, signed) = parse_decl("byte unsigned b;");
    assert_eq!(lebar, 8);
    assert!(!signed, "qualifier eksplisit menang");
}

#[test]
fn tipe_atom_tidak_boleh_ikut_dimension() {
    // `byte [7:0] b;` tidak sah: lebarnya sudah pasti. Parser mengabaikan
    // dimension, jadi yang tersisa harus tetap deklarasi yang valid.
    let (_, lebar, _) = parse_decl("byte [7:0] b;");
    assert_eq!(lebar, 8, "lebar tipe atom tidak boleh ditimpa dimension");
}

#[test]
fn tipe_atom_sebagai_port_diterima() {
    // LRM §6.6: tipe integer atom boleh muncul sebagai qualifier tipe port.
    let sv = "module m(input byte b, input time t, output [31:0] y); endmodule";
    let tokens: Vec<SpannedToken> = sv_lexer::lex(sv).expect("lex");
    let module = parse_module(&tokens).expect("parse");
    assert_eq!(module.ports.len(), 3);
}

#[test]
fn tipe_atom_sebagai_argumen_subrutin_diterima() {
    let sv = "module m;\n\
              task automatic tg(output byte q);\n\
                q = 8'h01;\n\
              endtask\n\
              endmodule";
    let tokens: Vec<SpannedToken> = sv_lexer::lex(sv).expect("lex");
    parse_module(&tokens).expect("parse");
}

#[test]
fn tipe_atom_sebagai_tipe_balik_function_diterima() {
    let sv = "module m;\n\
              function automatic byte f(input [7:0] x);\n\
                f = x;\n\
              endfunction\n\
              endmodule";
    let tokens: Vec<SpannedToken> = sv_lexer::lex(sv).expect("lex");
    let module = parse_module(&tokens).expect("parse");
    assert_eq!(module.routines.len(), 1);
}

#[test]
fn tipe_atom_didalam_blok_diterima() {
    // LRM §12.8: deklarasi lokal di dalam `begin...end`.
    let sv = "module m;\n\
              initial begin\n\
                byte b;\n\
                shortint s;\n\
                b = 8'h01;\n\
                s = 16'h01;\n\
              end\n\
              endmodule";
    let tokens: Vec<SpannedToken> = sv_lexer::lex(sv).expect("lex");
    parse_module(&tokens).expect("parse");
}

#[test]
fn tipe_atom_dalam_typedef_diterima() {
    // `typedef byte small_t;` — LRM §8.20.
    let sv = "module m;\n  typedef byte small_t;\nendmodule";
    let tokens: Vec<SpannedToken> = sv_lexer::lex(sv).expect("lex");
    let module = parse_module(&tokens).expect("parse");
    assert_eq!(module.typedefs.len(), 1);
}
