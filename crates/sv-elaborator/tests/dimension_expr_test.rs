// Tanggung jawab: test ekspresi konstanta pada dimension packed (LRM §7.3).
//
// Batas dimension boleh ekspresi konstanta apa pun, bukan hanya `P` atau
// `P-1:0`. Bentuk seperti `[W+1:0]`, `[W*2-1:0]`, dan `[W-2:0]` sangat umum
// di RTL dan sebelumnya ditolak parser dengan pesan
// "pola parameter didukung: [P] atau [P-1:0]".
//
// Semua nilai diverifikasi terhadap iverilog 12.0 (`-g2012`).
mod support;

use sv_ast::width::WidthExpr;

/// Lebar yang di-resolve elaborator untuk sebuah deklarasi.
fn lebar_dari(src: &str, nama: &str) -> u32 {
    let design = support::build(src);
    design
        .find_variable(nama)
        .unwrap_or_else(|| panic!("sinyal '{nama}' tidak terdaftar"))
        .data_type
        .width
}

/// Source standard: modul dengan parameter `W = 4` dan `DEPTH = 3`, lalu satu
/// deklarasi yang lebarnya diukur.
fn src(deklarasi: &str) -> String {
    format!(
        "module m #(parameter W = 4, parameter DEPTH = 3) ();\n\
           {deklarasi}\n\
           endmodule\n"
    )
}

#[test]
fn batas_msb_konstanta_penjumlahan() {
    // LRM §7.3: `[W+1:0]` = msb 5, lsb 0 => 6 bit.
    let d = lebar_dari(&src("logic [W+1:0] a;"), "a");
    assert_eq!(d, 6);
}

#[test]
fn batas_msb_konstanta_perkalian() {
    // `[W*2-1:0]` = msb 7 => 8 bit.
    let d = lebar_dari(&src("logic [W*2-1:0] b;"), "b");
    assert_eq!(d, 8);
}

#[test]
fn batas_dua_parameter_dijumlahkan() {
    // `[W+DEPTH-1:0]` = msb 6 => 7 bit.
    let d = lebar_dari(&src("logic [W+DEPTH-1:0] c;"), "c");
    assert_eq!(d, 7);
}

#[test]
fn batas_kurung_ganda() {
    // `[(W+1)*2-1:0]` = msb 9 => 10 bit.
    let d = lebar_dari(&src("logic [(W+1)*2-1:0] f;"), "f");
    assert_eq!(d, 10);
}

#[test]
fn lsb_ekspresi_bukan_nol() {
    // `[W+1:2]` = msb 5, lsb 2 => 4 bit.
    let d = lebar_dari(&src("logic [W+1:2] h;"), "h");
    assert_eq!(d, 4);
}

#[test]
fn rentang_literal_murni_tetap_dihitung_di_parser() {
    // Batas tanpa parameter bisa langsung dihitung saat parse.
    let tokens = sv_lexer::lex("[7:2]").expect("lex");
    assert_eq!(
        sv_parser::parse_dimension(&tokens, 0).expect("parse").0,
        WidthExpr::Literal(6)
    );
}

#[test]
fn batas_msb_negatif_ditolak() {
    // LRM §7.3: batas dimension harus ekspresi konstanta non-negatif.
    // `W-8` dengan `W = 4` bernilai -4 pada tipe `integer` signed, jadi
    // ditolak.
    //
    // Divergensi dari iverilog perlu dicatat: iverilog lenient dan menghitung
    // -4:0 = 5 bit. SVRust menolak karena `wrapping_sub` akan menghasilkan
    // 2^64-4 dan lebarnya jadi miliaran bit — design absurd tanpa pesan apa
    // pun. Menolak lebih aman daripada menerima rentang yang absurd.
    //
    // `konst_checked` dipakai HANYA untuk dimension; kondisi `generate` tetap
    // memakai `konst` yang wrap, jadi perilakunya tidak berubah.
    let src = "module m #(parameter W = 4) ();\n logic [W-8:0] bad;\nendmodule";
    let tokens = sv_lexer::lex(src).expect("lex");
    let ast = sv_parser::parse_module(&tokens).expect("parse");
    let err = sv_elaborator::elaborate(&ast).expect_err("msb negatif harus ditolak");
    assert!(
        err.to_string().contains("dimension"),
        "pesan tak menjelaskan masalah: {err}"
    );
}

#[test]
fn batas_msb_melapau_ditolak() {
    // 4^32 = 2^64 sudah melewati `u64::MAX`, jadi harus ditolak. Dengan
    // `wrapping_mul` hasilnya 0 dan lebarnya jadi 1 bit — design yang salah
    // diam-diam.
    let faktor = format!("W{}", "*W".repeat(31));
    let src = format!("module m #(parameter W = 4) ();\n logic [{faktor}:0] bad;\nendmodule");
    let tokens = sv_lexer::lex(&src).expect("lex");
    let ast = sv_parser::parse_module(&tokens).expect("parse");
    assert!(
        sv_elaborator::elaborate(&ast).is_err(),
        "perkalian yang meluap harus ditolak"
    );
}

#[test]
fn batas_tidak_tertutup_kurung_ditolak() {
    let tokens = sv_lexer::lex("[W+1:0").expect("lex");
    assert!(sv_parser::parse_dimension(&tokens, 0).is_err());
}

#[test]
fn ekspresi_dimension_terpakai_pada_port_modul() {
    // Port juga boleh memakai ekspresi dimension.
    let design = support::build(
        "module m #(parameter W = 4) (input logic [W+1:0] a, output logic [W-1:0] y);\n\
           assign y = a[3:0];\n\
           endmodule\n",
    );
    assert_eq!(
        design.find_variable("a").expect("port a").data_type.width,
        6
    );
    assert_eq!(
        design.find_variable("y").expect("port y").data_type.width,
        4
    );
}

#[test]
fn ekspresi_dimension_terpakai_pada_instansiasi_anak() {
    // Lebar port anak ikut ter-resolve dari parameter efektifnya.
    let design = support::build_top(
        "module c #(parameter W = 4) (output logic [W+1:0] y);\n\
           assign y = W;\n\
           endmodule\n\
           module top(output logic [7:0] o);\n\
           c #(.W(6)) u0(.y(o));\n\
           endmodule\n",
        "top",
    );
    assert_eq!(
        design.find_variable("o").expect("port o").data_type.width,
        8
    );
}

#[test]
fn ekspresi_dimension_dengan_parameter_tidak_dideklarasikan_ditolak() {
    // `logic [UNDEF+1:0]` harus gagal dengan pesan yang menyebut parameternya.
    let tokens = sv_lexer::lex("module m; logic [UNDEF+1:0] bad; endmodule").expect("lex");
    let ast = sv_parser::parse_module(&tokens).expect("parse");
    let err = sv_elaborator::elaborate(&ast).expect_err("parameter tak dikenal");
    assert!(
        err.to_string().contains("UNDEF"),
        "pesan tak menyebut parameter: {err}"
    );
}
