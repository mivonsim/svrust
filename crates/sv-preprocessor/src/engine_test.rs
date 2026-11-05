// Tanggung jawab: unit test preprocessor (define, ifdef, include, substitusi).
use crate::{preprocess, preprocess_with_dirs};

fn run(source: &str) -> String {
    preprocess(source, None).unwrap().source
}

fn run_err(source: &str) -> String {
    preprocess(source, None).unwrap_err().to_string()
}

#[test]
fn define_dan_substitusi_sederhana() {
    let out = run("`define WIDTH 8\nmodule m; localparam W = `WIDTH; endmodule");
    assert!(out.contains("localparam W = 8;"));
    assert!(!out.contains("`WIDTH"));
}

#[test]
fn define_tanpa_ekstensi_hilang() {
    let out = run("`define FLAG\nmodule m; wire a = `FLAG; endmodule");
    assert!(out.contains("wire a = ;"));
}

#[test]
fn define_lanjutan_baris_backslash() {
    let out = run("`define ADD_EXPR a + \\\n  b\nmodule m;\nwire [3:0] s = `ADD_EXPR;\nendmodule");
    assert!(out.contains("s = a +"), "hasil: {out}");
    assert!(out.contains("b;"), "hasil: {out}");
}

#[test]
fn ifdef_seleksi_cabang() {
    let src = "`define A\n`ifdef A\nfirst\n`else\nsecond\n`endif\nafter";
    let out = run(src);
    assert!(out.contains("first"));
    assert!(!out.contains("second"));
    assert!(out.contains("after"));
}

#[test]
fn ifndef_dan_elsif() {
    let src = "`ifndef NONE\nb0\n`elsif A\nb1\n`else\nb2\n`endif";
    assert!(run(src).contains("b0"));
    let src2 = "`define A\n`ifndef A\nb0\n`elsif A\nb1\n`else\nb2\n`endif";
    let out2 = run(src2);
    assert!(!out2.contains("b0"));
    assert!(out2.contains("b1"));
    assert!(!out2.contains("b2"));
}

#[test]
fn nested_ifdef_inactive_inner() {
    let src = "`undef OUT\n`ifdef OUT\nouter_yes\n`ifdef A\ninner\n`endif\n`endif\ndone";
    let out = run(src);
    assert!(!out.contains("outer_yes"));
    assert!(!out.contains("inner"));
    assert!(out.contains("done"));
}

#[test]
fn endif_tanpa_ifdef_diterima() {
    // `endif tanpa `ifdef` yang cocok — dilewati (bisa terjadi bila makro
    // bersyarat melintasi berkas include).
    let out = run("`endif\nmodule m; endmodule");
    assert!(out.contains("module m; endmodule"));
}

#[test]
fn define_tidak_diproses_saat_inaktif() {
    let src = "`ifdef NEHIDUP\n`define X 1\n`endif\n`ifdef X\nada\n`endif";
    assert!(!run(src).contains("ada"));
}

#[test]
fn substitusi_lewati_string() {
    let out = run("`define N 5\nmodule m; string s = \"a `N b\"; wire w = `N; endmodule");
    assert!(out.contains("\"a `N b\""));
    assert!(out.contains("w = 5;"));
}

#[test]
fn undef_menghapus_makro() {
    let out = run("`define X 1\n`undef X\n`ifdef X\nada\n`endif\ntidak");
    assert!(!out.contains("ada"));
    assert!(out.contains("tidak"));
}

#[test]
fn include_dan_timescale() {
    let dir = std::env::temp_dir().join("svpp-test-include");
    std::fs::create_dir_all(&dir).ok();
    std::fs::write(dir.join("defs.svh"), "`define WIDTH 4\n").unwrap();
    let main =
        "`timescale 1ns/1ps\n`include \"defs.svh\"\nmodule m; localparam W = `WIDTH; endmodule"
            .to_string();
    let pp = preprocess(&main, Some(&dir)).unwrap();
    assert_eq!(pp.timescale, "1ns/1ps");
    assert!(pp.source.contains("localparam W = 4;"));
}

#[test]
fn include_dengan_komentar_trailing() {
    // `include "file.svh"  // komentar` harus tetap dapat menemukan berkas.
    let dir = std::env::temp_dir().join("svpp-test-include-comment");
    std::fs::create_dir_all(&dir).ok();
    std::fs::write(dir.join("defs.svh"), "`define W 8\n").unwrap();
    let main =
        "`include \"defs.svh\"  // komentar trailing\nmodule m; localparam W = `W; endmodule";
    let pp = preprocess_with_dirs(main, &[dir.as_path()], "test.sv").unwrap();
    assert!(pp.source.contains("localparam W = 8;"));
}

#[test]
fn include_siklus_terdeteksi() {
    let dir = std::env::temp_dir().join("svpp-test-cycle");
    std::fs::create_dir_all(&dir).ok();
    std::fs::write(dir.join("a.svh"), "`include \"b.svh\"\n").unwrap();
    std::fs::write(dir.join("b.svh"), "`include \"a.svh\"\n").unwrap();
    let main = "`include \"a.svh\"\nendmodule";
    let err = preprocess(main, Some(&dir)).unwrap_err();
    assert!(err.to_string().contains("siklus"));
}

#[test]
fn include_tidak_ditemukan_membawa_baris_direktif() {
    let error = preprocess_with_dirs(
        "module m;\n`include \"__svrust_missing_for_test__.svh\"\nendmodule",
        &[],
        "test.sv",
    )
    .expect_err("include yang tidak ada harus gagal");

    assert_eq!(error.file, "test.sv");
    assert_eq!(error.line, 2);
}

// --- Fitur: `include `MAKRO (nama file lewat makro) ---

#[test]
fn include_melalui_makro_dikenal() {
    let dir = std::env::temp_dir().join("svpp-test-include-macro");
    std::fs::create_dir_all(&dir).ok();
    std::fs::write(dir.join("defs.svh"), "`define WIDTH 4\n").unwrap();
    let main =
        "`define FILE \"defs.svh\"\n`include `FILE\nmodule m; localparam W = `WIDTH; endmodule";
    let pp = preprocess_with_dirs(main, &[&dir], "test.sv").unwrap();
    assert!(pp.source.contains("localparam W = 4;"));
}

#[test]
fn include_melalui_makro_tidak_dikenal_dilewati() {
    // Makro yang tidak didefinisikan: skip include, preprocessor tidak error.
    let out = run("module m;\n`include `REQUESTS_FILE\nendmodule");
    assert!(out.contains("module m;"));
    assert!(out.contains("endmodule"));
    assert!(!out.contains("`include"));
}

// --- Fitur: pemanggilan makro function-like yang melintasi baris ---

#[test]
fn pemanggilan_makro_multi_line() {
    // Pemanggilan makro yang argumennya melintasi beberapa baris (kurung belum
    // seimbang pada baris pertama) harus digabung sebelum diekspansi.
    let src = r#"`define INFO(g, m, v) `g: m = v
`INFO(`gfn,
      "hello",
      UVM_DEBUG)
module m; endmodule"#;
    let out = run(src);
    assert!(out.contains("`gfn: \"hello\" = UVM_DEBUG"), "hasil: {out}");
    assert!(out.contains("module m; endmodule"));
}

// Pemanggilan makro UVM/DV yang melintasi baris dengan nested parens (perti
// kasus OpenTitan `uvm_info(`gfn, $sformatf("...[0x%x...]..."), UVM_DEBUG)`).
#[test]
fn pemanggilan_uvm_info_multi_line() {
    let src = r#"`define uvm_info(id, msg, verb) id: msg = verb
`uvm_info(`gfn,
      $sformatf("range [0x%0x,0x%0x)"),
      UVM_DEBUG)
module m; endmodule"#;
    let out = run(src);
    assert!(
        out.contains("gfn: $sformatf(\"range [0x%0x,0x%0x)\") = UVM_DEBUG"),
        "hasil: {out}"
    );
}

// Pemanggilan makro OpenTitan ASSERT_* yang melintasi baris dengan indeks [2].
#[test]
fn pemanggilan_assert_multi_line() {
    let src = r#"`define ASSERT_X(a, b, c, d) a, b, c, d
`ASSERT_X(IbexPcMismatchCheck_A, alert_tx_o[2],
      u_ibex_core.if_stage_i, pc_mismatch_alert_o)
module m; endmodule"#;
    let out = run(src);
    assert!(
        out.contains(
            "IbexPcMismatchCheck_A, alert_tx_o[2], u_ibex_core.if_stage_i, pc_mismatch_alert_o"
        ),
        "hasil: {out}"
    );
}

// Kasus nyata OpenTitan: macro didefinisikan lewat `include "prim_assert.sv"`
// di awal berkas, baru dipakai di baris jauh setelahnya, multi-line.
#[test]
fn pemanggilan_assert_multi_line_lewat_include() {
    let dir = std::env::temp_dir().join("svpp-test-assert-include");
    std::fs::create_dir_all(&dir).ok();
    std::fs::write(
        dir.join("prim_assert.sv"),
        "`define ASSERT_X(a, b, c, d) a, b, c, d\n",
    )
    .unwrap();
    let main = r#"`include "prim_assert.sv"
module rv_core_ibex;
  `ASSERT_X(IbexPcMismatchCheck_A, alert_tx_o[2],
      u_ibex_core.if_stage_i, pc_mismatch_alert_o)
endmodule"#;
    let pp = preprocess_with_dirs(main, &[dir.as_path()], "rv_core_ibex.sv").unwrap();
    assert!(
        pp.source.contains(
            "IbexPcMismatchCheck_A, alert_tx_o[2], u_ibex_core.if_stage_i, pc_mismatch_alert_o"
        ),
        "hasil: {}",
        pp.source
    );
    assert!(!pp.source.contains("`ASSERT_X"));
}

#[test]
fn timescale_default_kosong() {
    assert_eq!(
        preprocess("module m; endmodule", None).unwrap().timescale,
        ""
    );
}

// --- Bug: function-like makro tidak pernah menyubstitusi argumen ---

#[test]
fn function_like_makro_substitusi_argumen() {
    let out = run("`define ADD(a, b) a + b\nwire [7:0] x = `ADD(1, 2);");
    assert_eq!(out.trim(), "wire [7:0] x = 1 + 2;");
}

#[test]
fn function_like_argumen_string_dengan_koma() {
    // Koma di dalam string literal tidak boleh memecah argumen.
    let out = run("`define D(s) s\nwire w = `D(\"x,y\");");
    assert_eq!(out.trim(), "wire w = \"x,y\";");
}

#[test]
fn function_like_argumen_nested_brace() {
    // Koma di dalam {..} tetap satu argumen.
    let out = run("`define C(a, b) {a, b}\nwire [3:0] w = `C(4'h1, 4'h2);");
    assert_eq!(out.trim(), "wire [3:0] w = {4'h1, 4'h2};");
}

#[test]
fn function_like_argumen_lebih_error() {
    let e = run_err("`define F(a) a\nwire w = `F(1,2);");
    assert!(e.contains("terlalu banyak argumen"), "hasil: {e}");
}

#[test]
fn function_like_argumen_kurang_tanpa_default_error() {
    let e = run_err("`define F(a, b) a\nwire w = `F(1);");
    assert!(
        e.contains("tidak diberikan dan tanpa default"),
        "hasil: {e}"
    );
}

#[test]
fn function_like_tanpa_panggilan_error() {
    let e = run_err("`define F(a) a\nwire w = `F;");
    assert!(e.contains("memerlukan argumen"), "hasil: {e}");
}

// --- Bug: makro bersarang tidak diekspansi ---

#[test]
fn makro_bersarang_diekspansi() {
    let out = run("`define A 42\n`define B `A + 1\nwire w = `B;");
    assert_eq!(out.trim(), "wire w = 42 + 1;");
}

#[test]
fn makro_bersarang_lewat_argumen() {
    // Makro dipakai di dalam argumen makro lain.
    let out = run("`define TWO 2\n`define MUL(a) a * a\nwire w = `MUL(`TWO);");
    assert_eq!(out.trim(), "wire w = 2 * 2;");
}

#[test]
fn makro_rekursi_terdeteksi() {
    let e = run_err("`define A `A\nwire w = `A;");
    assert!(e.contains("rekursi makro"), "hasil: {e}");
}

// --- Bug: pemanggilan makro di awal baris dianggap direktif asing ---

#[test]
fn pemanggilan_makro_awal_baris() {
    let out = run("`define W 8\n`W\nwire w = 1;");
    assert_eq!(out.trim(), "8\nwire w = 1;");
}

#[test]
fn pemanggilan_makro_awal_baris_function_like() {
    let out = run("`define ADD(a, b) a + b\n`ADD(1, 2)\nwire w = 1;");
    assert_eq!(out.trim(), "1 + 2\nwire w = 1;");
}

#[test]
fn direktif_tidak_dikenal_dilewati() {
    // Direktif tak dikenal (mis. `ASSERT` dari OpenTitan) dilewati ke output
    // agar parser/lexer yang menanggungnya dapat memprosesnya.
    let out = run("`bukan_direktif x\nmodule m; endmodule");
    assert!(out.contains("`bukan_direktif x"), "hasil: {out}");
    assert!(out.contains("module m; endmodule"), "hasil: {out}");
}

// --- Fase 2: direktif preprocessor lanjutan ---

#[test]
fn undefineall_menghapus_semua_makro() {
    let out = run("`define A 1\n`define B 2\n`undefineall\n`ifdef A\nada_A\n`endif\n`ifdef B\nada_B\n`endif\ndone");
    assert!(!out.contains("ada_A"), "hasil: {out}");
    assert!(!out.contains("ada_B"), "hasil: {out}");
    assert!(out.contains("done"), "hasil: {out}");
}

#[test]
fn default_nettype_wire_menghapus_pengaturan_none() {
    // default_nettype wire = bawaan, jadi makro masih bisa dipakai.
    let out = run("`default_nettype wire\n`define W 8\n`ifdef W\nada_W\n`endif\ndone");
    assert!(out.contains("ada_W"), "hasil: {out}");
    assert!(out.contains("done"), "hasil: {out}");
}

#[test]
fn default_nettype_none_disimpan() {
    // default_nettype none hanya disimpan, tidak mengubah output preprocessor.
    let out = run("`default_nettype none\nmodule m; endmodule");
    assert_eq!(out.trim(), "module m; endmodule");
}

#[test]
fn line_dan_pragma_diterima_tanpa_error() {
    // `line dan `pragma diterima preprocessor tanpa mengubah source.
    let out = run("`line 10 \"file.sv\"\n`pragma protect begin\nmodule m; endmodule");
    assert!(out.contains("module m; endmodule"), "hasil: {out}");
}

// --- Bug: modul komentar tidak pernah dideklarasikan, direktif di dalam
// komentar tetap diproses ---

#[test]
fn direktif_di_dalam_komentar_blok_diabaikan() {
    let out = run("/*\n`ifdef HOAX\n*/\nmodule m; endmodule");
    assert_eq!(out.trim(), "module m; endmodule");
}

#[test]
fn direktif_di_dalam_komentar_baris_diabaikan() {
    let out = run("// `define X 9\nmodule m; endmodule");
    assert_eq!(out.trim(), "module m; endmodule");
}

#[test]
fn makro_di_dalam_komentar_tidak_diekspansi() {
    let out = run("`define N 5\n// pakai `N di sini\nmodule m; endmodule");
    // Komentar dibuang, dan `N di dalamnya tidak pernah diekspansi.
    assert!(!out.contains("`N"), "hasil: {out}");
    assert!(!out.contains("pakai 5"), "hasil: {out}");
    assert!(out.contains("module m; endmodule"), "hasil: {out}");
}

// --- Fitur: nilai default argumen makro ---

#[test]
fn default_argumen_dipakai_bila_tidak_diberikan() {
    let out = run("`define F(a, b = 5) a * b\nwire w = `F(3);");
    assert_eq!(out.trim(), "wire w = 3 * 5;");
}

#[test]
fn default_argumen_tertimpa_argumen_eksplisit() {
    let out = run("`define F(a, b = 5) a * b\nwire w = `F(3, 2);");
    assert_eq!(out.trim(), "wire w = 3 * 2;");
}

#[test]
fn default_argumen_bertingkat_di_body() {
    // Body berisi makro lain + default.
    let out = run("`define BIAS 1\n`define F(a, b = `BIAS) a + b\nwire w = `F(3);");
    assert_eq!(out.trim(), "wire w = 3 + 1;");
}

// --- Bug: argumen kosong eksplisit (`F(1, , 3)`) tidak boleh memicu default ---

#[test]
fn argumen_kosong_eksplisit_bukan_default() {
    // `with_c_` kosong yang ditulis secara eksplisit (`F(a, , c, d)`) harus
    // bernilai string kosong, TIDAK memicu default (karena parameter
    // `with_c_` di-definisi tanpa default).
    let src = "`define F(a, b = 5, c) a + b + c\nwire w = `F(1, , 3);";
    let out = run(src);
    assert!(out.contains("1 +  + 3"), "hasil: {out}");
}

// --- Bug: `define dengan nested parens di parameter default ---

#[test]
fn define_parameter_default_dengan_nested_parens() {
    // `tl_host_custom_seq #(cip_tl_seq_item)` mengandung `( ... )` di dalam
    // default parameter `define`. `)` pertama di default tidak boleh ditanggap
    // sebagai penutup daftar parameter `define`.
    let src = "`define M(a, b = x #(y), c = 9) a + b + c\nwire w = `M(1);";
    let out = run(src);
    assert!(out.contains("1 + x #(y) + 9"), "hasil: {out}");
}

#[test]
fn define_parameter_default_dengan_nested_parens_dan_koma() {
    // Koma di dalam nested parens di default juga tidak boleh memecah parameter.
    let src = "`define M(a, b = x #(y, z), c = 9) a + b + c\nwire w = `M(1);";
    let out = run(src);
    assert!(out.contains("1 + x #(y, z) + 9"), "hasil: {out}");
}
