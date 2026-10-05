// Tanggung jawab: unit test preprocessor (define, ifdef, include, substitusi).
use crate::preprocess;

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
fn endif_tanpa_ifdef_error() {
    assert!(run_err("`endif").contains("`endif tanpa `ifdef"));
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
fn direktif_tetap_error_bila_tidak_dikenal() {
    let e = run_err("`bukan_direktif x\nmodule m; endmodule");
    assert!(e.contains("tidak dikenal"), "hasil: {e}");
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
