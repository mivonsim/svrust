// Tanggung jawab: integration test posisi sumber (Span) pada error elaborasi.
use sv_elaborator::ElaborateError;

/// Bentuk error elaborasi dari satu modul, atau `Err` bila parse gagal duluan.
fn err(source: &str) -> ElaborateError {
    let tokens = sv_lexer::lex(source).expect("lex");
    let ast = sv_parser::parse_module(&tokens).expect("parse");
    match sv_elaborator::elaborate(&ast) {
        Ok(_) => panic!("sumber seharusnya gagal dielaborasi:\n{source}"),
        Err(e) => e,
    }
}

// BUG: `AstExpr::Ident` tidak menyimpan span, sehingga `lower_expression`
// melaporkan error dengan `Span::dummy()` (line 1 col 1) dan pengguna tak
// pernah tahu identifier mana yang salah (AGENTS.md aturan 3).

#[test]
fn undefined_signal_di_ekspresi_menunjuk_baris_benar() {
    let e = err(
        "module bad(input logic din, output logic q);\n  always_comb q = a + din;\nendmodule\n",
    );
    assert!(
        e.message.contains("undefined signal 'a'"),
        "pesan tak menyebut nama: {}",
        e.message
    );
    // `a` ada di baris 2 kolom 19.
    assert_eq!(e.span.line, 2, "span: {:?}", e.span);
    assert_eq!(e.span.col, 19, "span: {:?}", e.span);
}

#[test]
fn span_berbeda_untuk_identifier_berbeda() {
    let e =
        err("module bad(input logic din, output logic q);\n  always_comb q = nope;\nendmodule\n");
    assert_eq!(e.span.line, 2, "span: {:?}", e.span);
    assert_eq!(e.span.col, 19, "span: {:?}", e.span);
}

#[test]
fn undefined_signal_di_select_menunjuk_kolom_nama() {
    let e = err("module bad(input logic [3:0] din, output logic q);\n  always_comb q = salah[2];\nendmodule\n");
    assert!(
        e.message.contains("undefined signal 'salah'"),
        "{}",
        e.message
    );
    assert_eq!(e.span.line, 2, "span: {:?}", e.span);
    assert_eq!(e.span.col, 19, "span: {:?}", e.span);
}

#[test]
fn undefined_signal_di_operand_unary_menunjuk_baris_benar() {
    let e = err(
        "module bad(input logic din, output logic q);\n  always_comb q = ~hilang;\nendmodule\n",
    );
    assert!(
        e.message.contains("undefined signal 'hilang'"),
        "{}",
        e.message
    );
    assert_eq!(e.span.line, 2, "span: {:?}", e.span);
    assert_eq!(e.span.col, 20, "span: {:?}", e.span);
}

#[test]
fn undefined_signal_di_always_ff_menunjuk_baris_benar() {
    let e = err("module bad(input logic clk, input logic din, output logic q);\n  always_ff @(posedge clk) q <= absen;\nendmodule\n");
    assert!(
        e.message.contains("undefined signal 'absen'"),
        "{}",
        e.message
    );
    assert_eq!(e.span.line, 2, "span: {:?}", e.span);
}

#[test]
fn undefined_signal_di_rhs_proses_generate_menunjuk_baris_benar() {
    // Ident tak dikenal di RHS proses yang hidup di dalam region generate
    // harus menunjuk tokennya, bukan posisi dummy: substitusi scope di
    // `generate_proses` harus meneruskan span, bukan menggantinya.
    let e = err("module bad(input logic din, output logic [3:0] y);\n  generate\n    for (genvar i = 0; i < 4; i = i + 1) begin : g\n      always_comb y[i] = tidak_ada;\n    end\n  endgenerate\nendmodule\n");
    assert!(
        e.message.contains("undefined signal 'tidak_ada'"),
        "pesan: {}",
        e.message
    );
    assert_eq!(e.span.line, 4, "span: {:?}", e.span);
    assert_eq!(e.span.col, 26, "span: {:?}", e.span);
}

// BUG: `EventItem` tidak menyimpan span, sehingga error `undefined signal`
// pada `@(posedge clk)` memakai span statement. Kolom yang muncul adalah kolom
// `always_ff`, bukan kolom nama clock yang salah.

#[test]
fn undefined_signal_pada_event_always_ff_menunjuk_kolom_clock() {
    let e = err("module bad(input logic din, output logic q);\n  always_ff @(posedge clk_ngada) q <= din;\nendmodule\n");
    assert!(
        e.message.contains("undefined signal 'clk_ngada'"),
        "pesan: {}",
        e.message
    );
    // `clk_ngada` mulai di kolom 23; `always_ff` mulai di kolom 3.
    assert_eq!(e.span.line, 2, "span: {:?}", e.span);
    assert_eq!(e.span.col, 23, "span: {:?}", e.span);
}

#[test]
fn undefined_signal_pada_event_di_generate_menunjuk_kolom_clock() {
    // Substitusi scope di `generate_proses` menyintesis ulang `EventItem`;
    // span token clock harus ikut terbawa, bukan diganti span statement.
    let e = err("module bad(input logic din, output logic q);\n  generate\n    for (genvar i = 0; i < 2; i = i + 1) begin : g\n      always_ff @(posedge clk_ngada) q <= din;\n    end\n  endgenerate\nendmodule\n");
    assert!(
        e.message.contains("undefined signal 'clk_ngada'"),
        "pesan: {}",
        e.message
    );
    assert_eq!(e.span.line, 4, "span: {:?}", e.span);
    assert_eq!(e.span.col, 27, "span: {:?}", e.span);
}

#[test]
fn undefined_signal_pada_event_gabungan_or_menunjuk_kolom_yang_salah() {
    // `@(posedge clk or negedge rst)` — hanya item kedua yang salah, jadi
    // pesan harus menunjuk kolom `rst_ngada`, bukan kolom item pertama.
    let e = err("module bad(input logic clk, input logic q);\n  always_ff @(posedge clk or negedge rst_ngada) q <= 1'b0;\nendmodule\n");
    assert!(
        e.message.contains("undefined signal 'rst_ngada'"),
        "pesan: {}",
        e.message
    );
    assert_eq!(e.span.line, 2, "span: {:?}", e.span);
    assert_eq!(e.span.col, 38, "span: {:?}", e.span);
}

#[test]
fn undefined_signal_pada_event_control_di_initial_menunjuk_kolom_clock() {
    // `EventControl` sebagai statement (bukan `always_ff`) lewat jalur
    // `CombinationalStatement::EventControl` yang sama.
    let e = err("module bad(output logic q);\n  initial begin\n    @(posedge clk_ngada) begin\n      q = 1'b0;\n    end\n  end\nendmodule\n");
    assert!(
        e.message.contains("undefined signal 'clk_ngada'"),
        "pesan: {}",
        e.message
    );
    assert_eq!(e.span.line, 3, "span: {:?}", e.span);
    assert_eq!(e.span.col, 15, "span: {:?}", e.span);
}
