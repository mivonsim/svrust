// Tanggung jawab: integration test driver Rust yang dihasilkan untuk simulasi.
use std::path::Path;

use sv_codegen_rust::{generate_module, module_name::module_names_unik};
use sv_ir::Design;
use svrust::driver_gen::generate_driver;
use svrust::pipeline::compile_semua_root;

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
    // Sisa statement proses ini dihentikan `return` di dalam badan proses.
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

/// Driver untuk desain dengan proses waktu `always #N` TANPA blok `initial`
/// dan TANPA proses sekuensial harus memanggil `eval_timed()` di dalam loop
/// scheduler, agar penundaan di dalam body benar-benar menunda penulangan
/// (LRM §9.2.1, §11.2). Tanpa ini counter tetap 0.
#[test]
fn driver_tanpa_initial_panggil_eval_timed_untuk_proses_waktu() {
    let src = "module tb(output logic [7:0] cnt_o); \
               logic [7:0] cnt = 0; \
               always begin #2 cnt = cnt + 1; end \
               assign cnt_o = cnt; endmodule";
    let kode = driver(src);
    assert!(
        kode.contains("design.eval_timed();"),
        "driver plain tanpa initial harus memanggil eval_timed untuk proses waktu"
    );
    assert!(
        kode.contains("design.step_scheduler()"),
        "driver plain tanpa initial harus menggunakan step_scheduler"
    );
}

/// Driver untuk desain dengan blok `initial` + proses waktu `always #N`
/// TANPA proses sekuensial harus memanggil `eval_timed()` di dalam langkah
/// driver (cabang "has initial, no sequential"). Tanpa ini proses waktu
/// mati dan `#delay` di dalam `if`/`loop` tidak menangguhkan proses — bahkan
/// dapat menyebabkan infinite loop karena scheduler tidak pernah idle.
#[test]
fn driver_dengan_initial_tanpa_sequential_panggil_eval_timed_untuk_proses_waktu() {
    let src = "module tb(output logic [7:0] cnt_o); \
               logic [7:0] cnt = 0; \
               initial begin #10 $finish; end \
               always begin if (cnt < 5) begin #2 cnt = cnt + 1; end end \
               assign cnt_o = cnt; endmodule";
    let kode = driver(src);
    assert!(
        kode.contains("design.eval_timed();"),
        "driver dengan initial tanpa sequential harus memanggil eval_timed untuk proses waktu"
    );
    assert!(
        kode.contains("design.eval_initial(__step);"),
        "driver dengan initial harus memanggil eval_initial per langkah"
    );
    assert!(
        !kode.contains("while design.step_scheduler()"),
        "driver dengan initial tanpa sequential tidak boleh pakai while step_scheduler (infinite loop saat proses waktu reschedule)"
    );
}

/// `compile_semua_root` (parallel.md fase 5/§23.1) harus menghasilkan SATU
/// design per modul root tanpa `--top`, dan setiap design menghasilkan kode
// Rust yang bisa dikodekan dengan nama modul unik (tidak bentrok).
#[test]
fn compile_semua_root_hasil_satu_design_per_root_dengan_nama_unik() {
    let dir = std::env::temp_dir().join(format!(
        "svrust-parallel-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("jam sistem valid")
            .as_nanos()
    ));
    std::fs::create_dir(&dir).expect("buat direktori fixture");
    let a = dir.join("a.sv");
    let b = dir.join("b.sv");
    let c = dir.join("c.sv");
    std::fs::write(
        &a,
        "module root_a(output logic [7:0] y); assign y = 8'd1; endmodule\n",
    )
    .expect("tulis a");
    std::fs::write(
        &b,
        "module root_b(output logic [7:0] z); assign z = 8'd2; endmodule\n",
    )
    .expect("tulis b");
    std::fs::write(
        &c,
        "module root_c(output logic [7:0] w); assign w = 8'd3; endmodule\n",
    )
    .expect("tulis c");

    let (designs, _sumber_hash) = compile_semua_root(&[a.clone(), b.clone(), c.clone()], None, &[])
        .expect("tiga root harus dielaborasi");
    assert_eq!(designs.len(), 3, "harus ada 3 design untuk 3 root");

    let nama: Vec<_> = designs.iter().map(|d| d.name.clone()).collect();
    for n in &["root_a", "root_b", "root_c"] {
        assert!(
            nama.contains(&n.to_string()),
            "root {n} harus ada: {nama:?}"
        );
    }

    // Setiap design bisa dikodekan dan nama Rust unik (tidak bentrok).
    let kode: Vec<_> = designs.iter().map(generate_module).collect();
    let nama_rust = module_names_unik(&nama);
    let mut unik = std::collections::HashSet::new();
    for n in &nama_rust {
        assert!(unik.insert(n.clone()), "nama Rust ganda: {n}");
    }
    assert_eq!(kode.len(), 3, "harus ada 3 kode generate untuk 3 root");

    std::fs::remove_file(a).expect("hapus a");
    std::fs::remove_file(b).expect("hapus b");
    std::fs::remove_file(c).expect("hapus c");
    std::fs::remove_dir(dir).expect("hapus direktori fixture");
}

/// §25 determinism scheduling: dua kali pipeline pada desain yang sama dengan
// banyak proses waktu harus menghasilkan kode driver yang identik. Urutan proses
// pada waktu yang sama ditetapkan deterministic (FIFO per region + seq tiebreak
// di BinaryHeap), walaupun LRM §4.4 tidak menspesifikasikannya sepenuhnya.
#[test]
fn driver_kode_deterministik_saat_desain_sama_diproses_dua_kali() {
    let src = "module tb(output logic [7:0] y, output logic [7:0] z); \
               logic [7:0] a = 0; logic [7:0] b = 0; \
               always begin #2 a = a + 1; end \
               always begin #3 b = b + 1; end \
               initial begin #10 $finish; end \
               assign y = a; assign z = b; endmodule";
    let des = design(src);
    let kode1 = generate_driver(&des, 20, None);
    let kode2 = generate_driver(&des, 20, None);
    assert_eq!(
        kode1, kode2,
        "driver harus deterministik: dua kali generate pada desain sama harus kode identik (LRM §4.4 / §25)"
    );
    // Scheduler harus ada dalam kode (proses waktu dievaluasi via step_scheduler).
    assert!(
        kode1.contains("design.step_scheduler();"),
        "driver harus memakai step_scheduler untuk urutan proses per region event"
    );
    // Proses waktu harus dievaluasi via eval_timed.
    assert!(
        kode1.contains("design.eval_timed();"),
        "driver harus memanggil eval_timed untuk proses waktu `always #N`"
    );
}
