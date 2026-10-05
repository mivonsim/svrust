// Tanggung jawab: integration test driver Rust yang dihasilkan untuk simulasi.
use std::path::Path;

use cargo_sv::driver_gen::generate_driver;
use sv_ir::Design;

/// Elaborasi sumber SV menjadi design.
fn design(source: &str) -> Design {
    let tokens = sv_lexer::lex(source).expect("lex");
    let ast = sv_parser::parse_module(&tokens).expect("parse");
    sv_elaborator::elaborate(&ast).expect("elaborate")
}

fn driver(source: &str) -> String {
    generate_driver(&design(source), 20, None)
}

/// Kode struct design (bukan driver), tempat field `finished` hidup.
fn struct_design(source: &str) -> String {
    sv_codegen_rust::generate_module(&design(source))
}

// BUG: `finished` dan `is_finished()` hanya di-emit bila design punya blok
// `initial`. Design RTL murni yang memanggil `$finish` di dalam `always_ff`
// lalu gagal dikompilasi karena field itu tidak ada.

// BUG: driver mengecek `is_finished()` hanya di dalam loop, jadi `$finish`
// yang menyala pada tahap `initial` masih menghasilkan satu iterasi clock
// tambahan sebelum berhenti.

/// Design tanpa `initial`, dengan `$finish` di dalam `always_ff`.
const FINISH_DALAM_FF: &str = "module m(input logic clk, input logic din, output logic q); \
     always_ff @(posedge clk) begin q <= din; if (q) begin $finish; end end endmodule";

#[test]
fn field_finished_ada_bahkan_tanpa_blok_initial() {
    let kode = struct_design(FINISH_DALAM_FF);
    assert!(
        kode.contains("finished: bool,"),
        "field `finished` tidak di-emit pada design tanpa initial"
    );
    assert!(
        kode.contains("finished: false,"),
        "inisialisasi `finished` tidak di-emit"
    );
}

#[test]
fn method_is_finished_ada_bahkan_tanpa_blok_initial() {
    let kode = struct_design(FINISH_DALAM_FF);
    assert!(
        kode.contains("pub fn is_finished(&self) -> bool"),
        "method `is_finished` tidak di-emit pada design tanpa initial"
    );
}

#[test]
fn driver_memeriksa_is_finished_tanpa_blok_initial() {
    let kode = driver(FINISH_DALAM_FF);
    assert!(
        kode.contains("design.is_finished()"),
        "driver tidak mengecek flag finished"
    );
}

#[test]
fn loop_clock_dilewati_bila_sudah_finish_di_tahap_initial() {
    // `$finish` pada tahap `initial` harus mencegah loop clock berikutnya,
    // bukan hanya keluar dari loop initial. Desain harus punya clock supaya
    // tahap clock benar-benar di-emit.
    let kode = driver(
        "module m(input logic clk, output logic q); \
         initial begin q = 1'b1; $finish; end \
         always_ff @(posedge clk) q <= 1'b0; endmodule",
    );
    assert!(
        kode.contains("if !design.is_finished() {"),
        "loop clock tidak dijaga oleh flag finished dari tahap initial"
    );
}

#[test]
fn design_tanpa_finish_tanpa_initial_tidak_memakai_flag() {
    // Sebaliknya, design yang tidak pernah memanggil `$finish` tidak perlu
    // field maupun pengecekan; mengembalikannya hanya menambah dead code.
    let kode = struct_design("module m(input logic a, output logic y); assign y = a; endmodule");
    assert!(!kode.contains("finished: bool,"), "field tidak perlu ada");
    assert!(
        !kode.contains("is_finished()"),
        "pemeriksaan tidak perlu ada"
    );
}

#[test]
fn is_finished_hanya_diemit_satu_kali() {
    // Bekas bug: `is_finished` ditulis di dalam `write_eval_initial` pada jalur
    // yang menunggu edge, lalu juga oleh fungsi terpisah, sehingga Rust hasil
    // generate gagal build dengan "duplicate definitions".
    let kode = struct_design(
        "module m(input logic clk, output logic q); \
         always_ff @(posedge clk) q <= 1'b1; \
         initial begin @(posedge clk); q = 1'b0; $finish; end endmodule",
    );
    assert_eq!(
        kode.matches("pub fn is_finished(&self) -> bool").count(),
        1,
        "is_finished harus tepat satu definisi"
    );
}

#[test]
fn segmen_initial_yang_memanggil_finish_berhenti_sebelum_eval_seq() {
    // LRM §20.3: setelah `$finish` tidak boleh ada `eval_comb`/`eval_seq`
    // lagi pada langkah yang sama. Urutan yang benar: cek flag tepat setelah
    // `eval_initial`, baru `eval_comb`, baru `eval_seq`.
    let kode = driver(
        "module m(input logic clk, output logic q); \
         always_ff @(posedge clk) q <= 1'b1; \
         initial begin q = 1'b0; $finish; end endmodule",
    );
    let mulai = kode
        .find("design.eval_initial(__step);")
        .expect("loop initial");
    let cek = kode[mulai..]
        .find("if design.is_finished() {")
        .expect("pengecekan setelah eval_initial")
        + mulai;
    let comb = kode[mulai..]
        .find("design.eval_comb();")
        .expect("eval_comb")
        + mulai;
    assert!(
        cek < comb,
        "pengecekan finished harus mendahului eval_comb pada langkah yang sama"
    );
}

#[test]
fn driver_vcd_juga_mengecek_is_finished() {
    let tokens = sv_lexer::lex(FINISH_DALAM_FF).expect("lex");
    let ast = sv_parser::parse_module(&tokens).expect("parse");
    let des = sv_elaborator::elaborate(&ast).expect("elaborate");
    let kode = generate_driver(&des, 20, Some(Path::new("out.vcd")));
    assert!(
        kode.contains("design.is_finished()"),
        "driver VCD tidak mengecek flag finished"
    );
}

// BUG: `$display` di dalam `always_ff` ditolak parser sebagai "unexpected
// token", padahal LRM §20 tidak membatasi system task pada `initial`.

#[test]
fn display_di_always_ff_diterima_parser() {
    let src = "module m(input logic clk, input logic din, output logic q); \
               always_ff @(posedge clk) begin q <= din; $display(\"q=%b\", q); end endmodule";
    let tokens = sv_lexer::lex(src).expect("lex");
    let ast =
        sv_parser::parse_module(&tokens).expect("parser harus menerima $display di always_ff");
    sv_elaborator::elaborate(&ast).expect("elaborate");
}

#[test]
fn display_di_always_ff_turun_ke_ir() {
    let kode = struct_design(
        "module m(input logic clk, input logic din, output logic q); \
         always_ff @(posedge clk) begin q <= din; $display(\"q=%b\", q); end endmodule",
    );
    assert!(
        kode.contains("sv_format_args"),
        "$display di always_ff tidak ikut dikodekan"
    );
}

#[test]
fn finish_di_always_ff_menghasilkan_flag_pada_design_tanpa_initial() {
    let kode = struct_design(FINISH_DALAM_FF);
    assert!(
        kode.contains("self.finished = true;"),
        "$finish di always_ff tidak menandai flag finished"
    );
    assert!(
        kode.contains("return;"),
        "$finish di always_ff tidak menghentikan sisa proses"
    );
}

#[test]
fn system_task_lain_di_always_ff_diterima() {
    // `$strobe` dan `$monitor` juga sah di badan sekuensial (LRM §20.3/§20.2).
    let src = "module m(input logic clk, input logic din, output logic q); \
               always_ff @(posedge clk) begin \
                 q <= din; $strobe(\"q=%b\", q); $monitor(\"m=%b\", q); \
               end endmodule";
    let tokens = sv_lexer::lex(src).expect("lex");
    let ast = sv_parser::parse_module(&tokens).expect("parse");
    sv_elaborator::elaborate(&ast).expect("elaborate");
}
