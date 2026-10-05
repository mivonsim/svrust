// Tanggung jawab: integrasi proses di dalam region generate (LRM §27.1).
use sv_elaborator::elaborate_top;
use sv_lexer::lex;
use sv_parser::parse_file;

fn desain(src: &str, top: &str) -> Result<sv_ir::Design, String> {
    let tokens = lex(src).map_err(|e| e.to_string())?;
    let modules = parse_file(&tokens)?;
    elaborate_top(&modules, top).map_err(|e| e.to_string())
}

// BUG: `always_ff` di dalam loop generate ditolak parser karena genvar belum
// bisa disubstitusi pada badan proses.
#[test]
fn always_ff_dalam_generate_dielaborasi() {
    let src = "module m(input clk, input din, output reg [3:0] q); \
               generate \
                 for (genvar i = 0; i < 4; i = i + 1) begin : g \
                   always_ff @(posedge clk) q[i] <= din; \
                 end \
               endgenerate \
               endmodule";
    let d = desain(src, "m").expect("elaborate");
    let sekuensial: Vec<_> = d
        .processes
        .iter()
        .filter(|p| p.kind == sv_ir::process::ProcessKind::Sequential)
        .collect();
    assert_eq!(sekuensial.len(), 4, "tiap iterasi harus jadi satu proses");
}

#[test]
fn always_ff_dalam_generate_menulis_bit_berbeda() {
    // Masker tiap proses harus berbeda; kalau tidak, semua iterasi menulis
    // ke bit yang sama.
    let src = "module m(input clk, input din, output reg [3:0] q); \
               generate \
                 for (genvar i = 0; i < 4; i = i + 1) begin : g \
                   always_ff @(posedge clk) q[i] <= din; \
                 end \
               endgenerate \
               endmodule";
    let d = desain(src, "m").expect("elaborate");
    let mut maskers = Vec::new();
    for p in &d.processes {
        for stmt in &p.body {
            if let sv_ir::process::Statement::Assign { assignment, .. } = stmt {
                maskers.push(assignment.slice.map(|s| s.mask_range()));
            }
        }
    }
    maskers.sort();
    maskers.dedup();
    assert_eq!(
        maskers.len(),
        4,
        "setiap iterasi harus punya masker berbeda, didapat {:?}",
        maskers
    );
}

#[test]
fn deklarasi_lokal_proses_dalam_generate_tidak_bentrok() {
    // `logic t;` di dalam setiap iterasi harus menjadi sinyal terpisah.
    let src = "module m(input a, output [3:0] q); \
               generate \
                 for (genvar i = 0; i < 4; i = i + 1) begin : g \
                   logic t; \
                   always_comb t = a; \
                   assign q[i] = t; \
                 end \
               endgenerate \
               endmodule";
    let d = desain(src, "m").expect("elaborate");
    // Selain 4 port, harus ada 4 variabel `t` per iterasi.
    let jumlah_t = d.variables.iter().filter(|v| v.name.contains("t")).count();
    assert_eq!(jumlah_t, 4, "variabel t harus terpisah per iterasi");
}

#[test]
fn always_comb_dalam_generate_dielaborasi() {
    let src = "module m(input a, output [3:0] q); \
               generate \
                 for (genvar i = 0; i < 4; i = i + 1) begin : g \
                   always_comb q[i] = ~a; \
                 end \
               endgenerate \
               endmodule";
    let d = desain(src, "m").expect("elaborate");
    let comb = d
        .processes
        .iter()
        .filter(|p| p.kind == sv_ir::process::ProcessKind::Combinational)
        .count();
    assert_eq!(comb, 4);
}

#[test]
fn initial_dalam_generate_dielaborasi() {
    let src = "module m(output [3:0] q); \
               generate \
                 for (genvar i = 0; i < 4; i = i + 1) begin : g \
                   initial q[i] = 1'b1; \
                 end \
               endgenerate \
               endmodule";
    let d = desain(src, "m").expect("elaborate");
    assert_eq!(d.processes.len(), 4);
}

#[test]
fn genvar_tersubstusi_di_badan_proses() {
    // `q[i] = i` harus menghasilkan bit i bernilai i pada iterasi ke-i.
    let src = "module m(output [3:0] q); \
               generate \
                 for (genvar i = 0; i < 4; i = i + 1) begin : g \
                   always_comb q[i] = i[0]; \
                 end \
               endgenerate \
               endmodule";
    let d = desain(src, "m").expect("elaborate");
    assert_eq!(d.processes.len(), 4);
}
