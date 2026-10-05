// Tanggung jawab: test NBA di dalam `initial` dan ekspresi konstanta di
// kondisi `generate`.
//
// Dua gap yang ditutup:
//   - LRM §15.2 tidak membatasi `<=` pada blok prosedural khusus, jadi
//     `initial q <= 8'hAA;` sah. Parser sempat menolaknya karena AST
//     `CombinationalStatement` tidak punyaNBA.
//   - LRM §27.1 mensyaratkan kondisi `generate` konstan, tapi "konstan" juga
//     mencakup cast (`if (16'(W) > 8)`), select, dan concat — evaluator
//     `konst` hanya mengenali literal/parameter/operator.
//
// Semua nilai diverifikasi terhadap iverilog 12.0 (`-g2012`).
mod support;

use sv_ir::process::AssignStyle;

#[test]
fn nba_di_initial_diperima() {
    let d = support::build(
        "module m(output logic [7:0] q);\n\
           initial q <= 8'hAA;\n\
           endmodule\n",
    );
    let expr = support::nilai_untuk(&d, "q");
    let mut signals = vec![0u64; d.variables.len()];
    assert_eq!(support::evaluasi(&expr, &mut signals), 0xAA);
}

#[test]
fn nba_di_initial_tetap_non_blocking() {
    // Bentuk assignment-nya harus tetap NonBlocking di IR supaya penjadwalan
    // NBA tetap berlaku di dalam `initial`.
    let d = support::build(
        "module m(output logic [7:0] q);\n\
           initial q <= 8'hAA;\n\
           endmodule\n",
    );
    let styles: Vec<AssignStyle> = d
        .processes
        .iter()
        .flat_map(|p| p.body.iter())
        .filter_map(|s| match s {
            sv_ir::process::Statement::Assign { assignment, .. } => Some(assignment.style),
            _ => None,
        })
        .collect();
    assert_eq!(styles, vec![AssignStyle::NonBlocking]);
}

#[test]
fn blocking_di_initial_tetap_blocking() {
    // Penambahan NBA tidak boleh mengubah perilaku `=`.
    let d = support::build(
        "module m(output logic [7:0] q);\n\
           initial q = 8'hAA;\n\
           endmodule\n",
    );
    let styles: Vec<AssignStyle> = d
        .processes
        .iter()
        .flat_map(|p| p.body.iter())
        .filter_map(|s| match s {
            sv_ir::process::Statement::Assign { assignment, .. } => Some(assignment.style),
            _ => None,
        })
        .collect();
    assert_eq!(styles, vec![AssignStyle::Blocking]);
}

#[test]
fn kedua_bentuk_berdampingan_dalam_initial() {
    let d = support::build(
        "module m(output logic [7:0] a, output logic [7:0] b);\n\
           initial begin a <= 8'h11; b = 8'h22; end\n\
           endmodule\n",
    );
    let mut signals = vec![0u64; d.variables.len()];
    let a = support::evaluasi(&support::nilai_untuk(&d, "a"), &mut signals);
    let b = support::evaluasi(&support::nilai_untuk(&d, "b"), &mut signals);
    assert_eq!((a, b), (0x11, 0x22));
}

// --- LRM §27.1: ekspresi konstanta pada kondisi generate ---

fn nilai_generate(src: &str) -> u64 {
    let d = support::build(src);
    let expr = support::nilai_untuk(&d, "y");
    let mut signals = vec![0u64; d.variables.len()];
    support::evaluasi(&expr, &mut signals)
}

#[test]
fn kondisi_generate_dengan_size_cast() {
    // iverilog: G1=000000bb untuk W=4 karena 16'(4) > 8 salah.
    let d = nilai_generate(
        "module m #(parameter W = 4) (output logic [31:0] y);\n\
           generate\n\
             if (16'(W) > 8) begin : g\n\
               assign y = 32'hAA;\n\
             end else begin : h\n\
               assign y = 32'hBB;\n\
             end\n\
           endgenerate\n\
           endmodule\n",
    );
    assert_eq!(d, 0xBB);
}

#[test]
fn kondisi_generate_dengan_size_cast_yang_memenuhi() {
    // iverilog: 8'(W) > 2 benar untuk W=4.
    let d = nilai_generate(
        "module m #(parameter W = 4) (output logic [31:0] y);\n\
           generate\n\
             if (8'(W) > 2) begin : g\n\
               assign y = 32'hAA;\n\
             end else begin : h\n\
               assign y = 32'hBB;\n\
             end\n\
           endgenerate\n\
           endmodule\n",
    );
    assert_eq!(d, 0xAA);
}

#[test]
fn kondisi_generate_dengan_typedef_cast() {
    let d = nilai_generate(
        "module m #(parameter W = 4) (output logic [31:0] y);\n\
           typedef logic [7:0] byte_t;\n\
           generate\n\
             if (byte_t'(W) == 8'h04) begin : g\n\
               assign y = 32'hAA;\n\
             end else begin : h\n\
               assign y = 32'hBB;\n\
             end\n\
           endgenerate\n\
           endmodule\n",
    );
    assert_eq!(d, 0xAA);
}

#[test]
fn kondisi_generate_dengan_sign_cast() {
    let d = nilai_generate(
        "module m #(parameter W = 4) (output logic [31:0] y);\n\
           generate\n\
             if (signed'(W) > 0) begin : g\n\
               assign y = 32'hAA;\n\
             end else begin : h\n\
               assign y = 32'hBB;\n\
             end\n\
           endgenerate\n\
           endmodule\n",
    );
    assert_eq!(d, 0xAA);
}

#[test]
fn kondisi_generate_dengan_select() {
    // LRM §7.4: `a[hi:lo]` boleh dipakai sebagai konstanta.
    let d = nilai_generate(
        "module m #(parameter W = 8) (output logic [31:0] y);\n\
           generate\n\
             if (8'hF0[7:4] == 8'hF) begin : g\n\
               assign y = 32'hAA;\n\
             end else begin : h\n\
               assign y = 32'hBB;\n\
             end\n\
           endgenerate\n\
           endmodule\n",
    );
    assert_eq!(d, 0xAA);
}

#[test]
fn kondisi_generate_dengan_concat() {
    // LRM §11.8.1: `{A, B}` — bagian atas lebih dulu.
    let d = nilai_generate(
        "module m #(parameter W = 4) (output logic [31:0] y);\n\
           generate\n\
             if ({4'hA, 4'h1} == 8'hA1) begin : g\n\
               assign y = 32'hAA;\n\
             end else begin : h\n\
               assign y = 32'hBB;\n\
             end\n\
           endgenerate\n\
           endmodule\n",
    );
    assert_eq!(d, 0xAA);
}

#[test]
fn selektor_case_generate_dengan_cast() {
    // Selektor `case`-generate juga harus bisa menerima cast.
    let d = nilai_generate(
        "module m #(parameter W = 4) (output logic [31:0] y);\n\
           generate\n\
             case (8'(W))\n\
               8'd4: assign y = 32'hAA;\n\
               default: assign y = 32'hBB;\n\
             endcase\n\
           endgenerate\n\
           endmodule\n",
    );
    assert_eq!(d, 0xAA);
}

#[test]
fn kondisi_generate_dengan_sinyal_masih_ditolak() {
    // Yang tetap harus ditolak: nilai yang butuh sinyal, karena sinyal belum
    // punya nilai sebelum simulasi berjalan (LRM §27.1).
    let tokens = sv_lexer::lex(
        "module m #(parameter W = 4) (input logic [7:0] din, output logic [31:0] y);\n\
         generate\n\
           if (din > 8'd4) assign y = 32'hAA; else assign y = 32'hBB;\n\
         endgenerate\n\
         endmodule",
    )
    .expect("lex");
    let ast = sv_parser::parse_module(&tokens).expect("parse");
    let err = sv_elaborator::elaborate(&ast).expect_err("sinyal tak boleh jadi kondisi generate");
    assert!(
        err.to_string().contains("konstan"),
        "pesan tak menjelaskan syaratnya: {err}"
    );
}
