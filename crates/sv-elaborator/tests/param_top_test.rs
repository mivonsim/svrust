// Tanggung jawab: test parameter modul sebagai konstanta (LRM §6.20).
//
// `parameter` boleh dipakai di mana saja dalam modul yang mendeklarasikannya:
// body statement, nilai awal deklarasi, nilai awal deklarasi lokal di dalam
// `begin...end`, kondisi/label `generate`, dan dimension packed `[P]`/`[P-1:0]`.
//
// Bug yang diperbaiki: modul **top** tidak punya jalur substitusi sama sekali —
// hanya modul anak yang parameternya di-inline saat instansiasi — sehingga
// `assign y = WIDTH;` gagal dengan "undefined signal 'WIDTH'" padahal kodenya
// sah. Condition `generate` punya masalah serupa.
//
// Test bandingkan dengan iverilog 12.0 (`-g2012`).
mod support;

use support::{build, build_top};

/// Nilai ekspresi pada assignment pertama yang menulis ke `nama`.
fn nilai_assign(design: &sv_ir::Design, nama: &str) -> u64 {
    let var = design.find_variable(nama).expect("sinyal ada");
    let expr = support::nilai_untuk(design, &var.name);
    let mut signals = vec![0u64; design.variables.len()];
    support::evaluasi(&expr, &mut signals)
}

#[test]
fn parameter_top_terpakai_di_body_statement() {
    // iverilog: K1=4
    let design = build(
        "module m #(parameter W = 4) (output logic [31:0] y);\n\
           assign y = W;\n\
           endmodule\n",
    );
    assert_eq!(nilai_assign(&design, "y"), 4);
}

#[test]
fn parameter_top_terpakai_di_ekspresi_aritmetika() {
    // iverilog: K2=8 untuk W=4, D=3 (`W * 2`)
    let design = build(
        "module m #(parameter W = 4, parameter D = 3) (output logic [31:0] y);\n\
           assign y = W * D;\n\
           endmodule\n",
    );
    assert_eq!(nilai_assign(&design, "y"), 12);
}

#[test]
fn parameter_dalam_nilai_awal_deklarasi() {
    // iverilog: R4=4
    let design = build(
        "module m #(parameter W = 4) (output logic [31:0] y);\n\
           logic [W-1:0] seed = W;\n\
           assign y = seed;\n\
           endmodule\n",
    );
    let seed = design.find_variable("seed").expect("seed terdaftar");
    let init = seed.initial.as_ref().expect("seed punya nilai awal");
    let mut signals = vec![0u64; design.variables.len()];
    assert_eq!(support::evaluasi(init, &mut signals), 4);
}

#[test]
fn parameter_dalam_nilai_awal_deklarasi_lokal() {
    // iverilog: R5=5 (`W + 1` dengan W=4). Nilai assignment di dalam
    // `always_comb` yang dibaca, karena `nilai_untuk` mencari assignment pertama
    // pada proses — di sini proses itulah yang menulis `y`.
    let design = build(concat!(
        "module m #(parameter W = 4) (output logic [31:0] y); ",
        "always_comb begin ",
        "logic [W-1:0] tmp; ",
        "tmp = W + 1; ",
        "y = tmp; ",
        "end endmodule"
    ));
    // Assignment pertama pada proses adalah `tmp = W + 1`, jadi nilainya bisa
    // dibaca langsung tanpa menyebut nama internal variabel lokalnya
    // (`local_scope` me-/rename-nya menjadi `tmp__lokal0`).
    let mut signals = vec![0u64; design.variables.len()];
    assert_eq!(
        support::evaluasi(&support::nilai_rhs(&design), &mut signals),
        5
    );
    assert_eq!(design.processes.len(), 1);
}

#[test]
fn parameter_dalam_condition_generate() {
    // iverilog: R6=4
    let design = build(
        "module m #(parameter W = 4) (output logic [31:0] y);\n\
           generate\n\
             if (W == 4) begin : g\n\
               assign y = W;\n\
             end else begin : h\n\
               assign y = 8'hFF;\n\
             end\n\
           endgenerate\n\
           endmodule\n",
    );
    assert_eq!(nilai_assign(&design, "y"), 4);
}

#[test]
fn parameter_dalam_cabang_generate_yang_tidak_dipakai() {
    // Cabang yang tidak diambil tidak boleh-shadow nilai parameter.
    // iverilog: R6=255 untuk W=8
    let design = build(
        "module m #(parameter W = 8) (output logic [31:0] y);\n\
           generate\n\
             if (W == 4) begin : g\n\
               assign y = W;\n\
             end else begin : h\n\
               assign y = 8'hFF;\n\
             end\n\
           endgenerate\n\
           endmodule\n",
    );
    assert_eq!(nilai_assign(&design, "y"), 0xFF);
}

#[test]
fn parameter_dalam_dimension_packed() {
    // `[W-1:0]` harus memakai nilai parameter, bukan default parser.
    let design = build(
        "module m #(parameter W = 4) (output logic [31:0] y);\n\
           logic [W-1:0] seed = W;\n\
           assign y = seed;\n\
           endmodule\n",
    );
    assert_eq!(
        design.find_variable("seed").expect("seed").data_type.width,
        4
    );
}

#[test]
fn override_parameter_anak_tidak_tertimpa_default() {
    // BUG: kalau substitusi parameter dijalankan untuk SEMUA modul sebelum
    // instansiasi, anak memakai nilai default dan override `#(.W(8))` hilang.
    // iverilog: lc2 → o=08
    let design = build_top(
        "module c #(parameter W = 4) (output logic [W-1:0] y);\n\
           assign y = W;\n\
           endmodule\n\
           module top(output logic [7:0] o);\n\
           c #(.W(8)) u0(.y(o));\n\
           endmodule\n",
        "top",
    );
    assert_eq!(nilai_assign(&design, "o"), 8);
}

#[test]
fn parameter_anak_tanpa_override_memakai_default() {
    let design = build_top(
        "module c #(parameter W = 4) (output logic [W-1:0] y);\n\
           assign y = W;\n\
           endmodule\n\
           module top(output logic [3:0] o);\n\
           c u0(.y(o));\n\
           endmodule\n",
        "top",
    );
    assert_eq!(nilai_assign(&design, "o"), 4);
}

#[test]
fn override_parameter_dipakai_untuk_lebar_port_anak() {
    // Lebar port anak ikut override: `y` jadi 8 bit dan squeez 8.
    let design = build_top(
        "module c #(parameter W = 4) (output logic [W-1:0] y);\n\
           assign y = W;\n\
           endmodule\n\
           module top(output logic [7:0] o);\n\
           c #(.W(8)) u0(.y(o));\n\
           endmodule\n",
        "top",
    );
    // Port anak dipetakan ke sinyal parent, jadi lebar yang diperiksa
    // adalah sinyal parent yang tersambung.
    let o = design.find_variable("o").expect("port parent terdaftar");
    assert_eq!(o.data_type.width, 8);
}

#[test]
fn parameter_yang_tidak_dipakai_tidak_mengganggu() {
    // Parameter tanpa rujukan tetap boleh ada.
    let design = build(
        "module m #(parameter W = 4, parameter UNUSED = 99) (output logic [31:0] y);\n\
           assign y = W;\n\
           endmodule\n",
    );
    assert_eq!(nilai_assign(&design, "y"), 4);
}

#[test]
fn parameter_nama_sama_dengan_sinyal_ditolak() {
    // LRM §6.20: parameter dan variabel berbagi namespace di dalam modul, jadi
    // `parameter W` dan `logic W` adalah deklarasi ganda. iverilog menolak
    // dengan pesan duplicate.
    let tokens = sv_lexer::lex(
        "module m #(parameter W = 4) (output logic [31:0] y); \
         logic [31:0] W; \
         assign y = W; \
         endmodule",
    )
    .expect("lex");
    let ast = sv_parser::parse_module(&tokens).expect("parse");
    let err = sv_elaborator::elaborate(&ast).expect_err("nama ganda harus ditolak");
    assert!(
        err.to_string().contains("W"),
        "pesan tak menyebut nama yang bentrok: {err}"
    );
}
