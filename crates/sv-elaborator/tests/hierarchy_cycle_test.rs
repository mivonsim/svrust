// Tanggung jawab: integrasi test batas rekursi hierarki instansiasi (LRM §23.1.1).
mod support;

use sv_elaborator::ElaborateError;

/// Parse multi-file lalu elabore dengan `top` yang diberikan.
fn err_top(sumber: &str, top: &str) -> ElaborateError {
    let tokens = sv_lexer::lex(sumber).expect("lex");
    let modules = sv_parser::parse_file(&tokens).expect("parse");
    sv_elaborator::elaborate_top(&modules, top).expect_err("elaborate harus gagal")
}

// BUG (ditemukan fuzzing): `a` menginstans `b` dan `b` menginstans `a` membuat
// `lower_instance` memanggil dirinya sendiri tanpa henti sampai stack habis —
// `stack-overflow` yang tidak bisa ditangkap `catch_unwind` dan tidak memberi
// pesan apa pun. Instansiasi bersiklus memang tidak sah (LRM §23.1.1: tak ada
// rekursi tak terbatas pada hierarki), jadi harus ditolak dengan pesan yang
// menyebutkan rantainya.
#[test]
fn instansiasi_siklus_ditolak_bukan_stack_overflow() {
    let sumber = "module a; b u0(); endmodule\n\
                  module b; a u0(); endmodule\n";
    let e = err_top(sumber, "a");
    assert!(
        e.message.contains("siklus"),
        "pesan harus menyebut siklus: {}",
        e.message
    );
    // Pesan harus menunjukkan rantainya supaya akar masalahnya jelas.
    assert!(
        e.message.contains("->"),
        "pesan harus menampilkan rantai instansiasi: {}",
        e.message
    );
}

#[test]
fn siklus_tiga_modul_ditolak() {
    let sumber = "module a; b u0(); endmodule\n\
                  module b; c u0(); endmodule\n\
                  module c; a u0(); endmodule\n";
    let e = err_top(sumber, "a");
    assert!(
        e.message.contains("siklus"),
        "pesan harus menyebut siklus: {}",
        e.message
    );
}

// Instansiasi yang sah dan dalam harus tetap jalan — penjaga kedalaman tidak
// boleh memotong hierarki yang legitimate.
#[test]
fn hierarki_legitimate_berhasil_dielaborasi() {
    let mut sumber = String::from("module m0(input a, output y); assign y = a; endmodule\n");
    for i in 1..8 {
        sumber.push_str(&format!(
            "module m{i}(input a, output y); m{} u0(.a(a), .y(y)); endmodule\n",
            i - 1
        ));
    }
    let tokens = sv_lexer::lex(&sumber).expect("lex");
    let modules = sv_parser::parse_file(&tokens).expect("parse");
    sv_elaborator::elaborate_top(&modules, "m7").expect("hierarki 8 tingkat harus sah");
}

#[test]
fn instansiasi_langsung_tidak_menghapus_modul_induk() {
    // Regression: penjaga rantai tidak boleh salah pop dan membuat modul
    // setelahnya ikut tersingkir.
    let sumber = "module leaf(input a, output y); assign y = a; endmodule\n\
                  module mid(input a, output y); leaf u0(.a(a), .y(y)); endmodule\n\
                  module top(input a, output y); mid u0(.a(a), .y(y)); endmodule\n";
    let tokens = sv_lexer::lex(sumber).expect("lex");
    let modules = sv_parser::parse_file(&tokens).expect("parse");
    let design = sv_elaborator::elaborate_top(&modules, "top").expect("elaborate");
    // Sinyal `y` top harus tetap ada.
    assert!(
        design.variables.iter().any(|v| v.name == "y"),
        "sinyal top hilang setelah instansiasi"
    );
}
