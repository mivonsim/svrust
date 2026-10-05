// Tanggung jawab: integration test typedef dan enumerated types (LRM §8.20, §6.7).
mod support;

use support::{build, build_top, dengan_sinyal, evaluasi, nilai_rhs};
use sv_elaborator::ElaborateError;
use sv_ir::process::Statement;
use sv_ir::{DataType, Expr};

fn err(source: &str) -> ElaborateError {
    let tokens = sv_lexer::lex(source).expect("lex");
    let ast = sv_parser::parse_module(&tokens).expect("parse");
    match sv_elaborator::elaborate(&ast) {
        Ok(_) => panic!("sumber seharusnya gagal dielaborasi:\n{source}"),
        Err(e) => e,
    }
}

/// Lebar sinyal pada design.
fn lebar(design: &sv_ir::Design, nama: &str) -> u32 {
    design
        .find_variable(nama)
        .unwrap_or_else(|| panic!("sinyal '{nama}' ada"))
        .data_type
        .width
}

/// Signedness sinyal pada design.
fn signed(design: &sv_ir::Design, nama: &str) -> bool {
    design
        .find_variable(nama)
        .unwrap_or_else(|| panic!("sinyal '{nama}' ada"))
        .data_type
        .signed
}

/// Kumpulkan seluruh statement proses pertama beserta isi blok.
///
/// `always_comb begin ... end` dirakit parser sebagai daftar statement biasa
/// tanpa node `Block`, jadi blok tidak pernah muncul di sini.
fn semua_stmt(design: &sv_ir::Design) -> Vec<Statement> {
    let mut out = Vec::new();
    for stmt in &design.processes[0].body {
        match stmt {
            Statement::Block { body, .. } => out.extend(body.iter().cloned()),
            other => out.push(other.clone()),
        }
    }
    out
}

// --- Typedef skalar ---

/// LRM §8.20: nama tipe hasil typedef dipakai sebagai tipe deklarasi.
#[test]
fn typedef_skalar_memberi_lebar_kepada_variabel() {
    let src = "module m;\n\
               typedef logic [7:0] byte_t;\n\
               byte_t data;\n\
               always_comb data = 8'hA5;\n\
               endmodule\n";
    let design = build(src);
    assert_eq!(lebar(&design, "data"), 8);
    let mut signals = vec![0u64; design.variables.len()];
    signals[design.find_variable("data").unwrap().signal_id as usize] = 0xA5;
    assert_eq!(evaluasi(&nilai_rhs(&design), &mut signals), 0xA5);
}

/// LRM §6.2.1: signedness ikut terbawa dari typedef.
#[test]
fn typedef_skalar_bawa_signedness() {
    let src = "module m;\n\
               typedef logic signed [15:0] word_t;\n\
               word_t acc;\n\
               always_comb acc = 16'hFFFD;\n\
               endmodule\n";
    let design = build(src);
    assert_eq!(lebar(&design, "acc"), 16);
    assert!(signed(&design, "acc"), "signedness harus ikut typedef");
}

/// Typedef boleh memakai parameter modul pada lebarnya.
#[test]
fn typedef_berparameter_terresolve() {
    let src = "module m #(parameter W = 12);\n\
               typedef logic [W-1:0] wide_t;\n\
               wide_t data;\n\
               always_comb data = 12'd0;\n\
               endmodule\n";
    let design = build(src);
    assert_eq!(lebar(&design, "data"), 12);
}

/// LRM §8.20: dimension eksplisit pada deklarasi mempersempit tipe.
#[test]
fn deklarasi_bertipe_dengan_dimension_adalah_packed_array() {
    // LRM §7.3: `byte_t [3:0] nib;` dengan `typedef logic [7:0] byte_t` adalah
    // packed array 4 elemen bertipe `byte_t`, jadi 4 x 8 = 32 bit. Implementasi
    // lama memperlakukannya sebagai "persempit" ke 4 bit, jadi test lama yang
    // mengunci 4 itu justru mengunci perilaku salah.
    // iverilog: nib 32-bit, `nib = 32'hF` tercetak 0000000f
    let src = "module m;\n\
               typedef logic [7:0] byte_t;\n\
               byte_t [3:0] nib;\n\
               always_comb nib = 32'hF;\n\
               endmodule\n";
    let design = build(src);
    assert_eq!(lebar(&design, "nib"), 32);
}

#[test]
fn deklarasi_bertipe_tanpa_dimension_pakai_lebar_typedef() {
    let src = "module m;\n\
               typedef logic [7:0] byte_t;\n\
               byte_t nib;\n\
               always_comb nib = 8'hFF;\n\
               endmodule\n";
    let design = build(src);
    assert_eq!(lebar(&design, "nib"), 8);
}

/// Beberapa deklarator berbagi tipe hasil typedef yang sama.
#[test]
fn daftar_deklarator_beripe_typedef() {
    let src = "module m;\n\
               typedef logic [7:0] byte_t;\n\
               byte_t a, b, c;\n\
               always_comb begin a = 0; b = 0; c = 0; end\n\
               endmodule\n";
    let design = build(src);
    for nama in ["a", "b", "c"] {
        assert_eq!(lebar(&design, nama), 8, "lebar {nama}");
    }
}

/// Lebar typedef ikut terbawa saat nilai ditulis lewat sinyal yang diberikan.
#[test]
fn typedef_skalar_menyimpan_nilai_penuh() {
    let src = "module m;\n\
               typedef logic [7:0] byte_t;\n\
               byte_t data;\n\
               always_comb data = 8'h3C;\n\
               endmodule\n";
    assert_eq!(dengan_sinyal(src, &[("data", 0x3C)]), 0x3C);
}

// --- Enum ---

/// LRM §6.7: variabel bertipe enum memakai lebar tipe dasar enumerasi.
#[test]
fn variabel_enum_memakai_lebar_tipe_dasar() {
    let src = "module m;\n\
               typedef enum logic [2:0] { IDLE, LOAD, RUN, DONE } state_t;\n\
               state_t q;\n\
               always_comb q = RUN;\n\
               endmodule\n";
    let design = build(src);
    assert_eq!(lebar(&design, "q"), 3);
    let mut signals = vec![0u64; design.variables.len()];
    // RUN adalah anggota ketiga tanpa nilai eksplisit, jadi nilainya 2.
    assert_eq!(evaluasi(&nilai_rhs(&design), &mut signals), 2);
}

/// LRM §6.7: enum tanpa lebar eksplisit memakai 1 bit.
#[test]
fn enum_tanpa_lebar_default_satu_bit() {
    let src = "module m;\n\
               typedef enum { OFF, ON } flag_t;\n\
               flag_t f;\n\
               always_comb f = ON;\n\
               endmodule\n";
    let design = build(src);
    assert_eq!(lebar(&design, "f"), 1);
    let mut signals = vec![0u64; design.variables.len()];
    assert_eq!(evaluasi(&nilai_rhs(&design), &mut signals), 1);
}

/// Nilai eksplisit menggeser nilai anggota berikutnya (LRM §6.7).
#[test]
fn enum_nilai_eksplisit_menggeser_urutan() {
    let src = "module m;\n\
               typedef enum logic [3:0] { KILO = 10, MEGA, GIGA } prefix_t;\n\
               prefix_t p;\n\
               always_comb p = MEGA;\n\
               endmodule\n";
    let design = build(src);
    let mut signals = vec![0u64; design.variables.len()];
    assert_eq!(evaluasi(&nilai_rhs(&design), &mut signals), 11);
}

/// Literal enum boleh dipakai di perbandingan dan operator kondisional.
#[test]
fn literal_enum_dalam_perbandingan() {
    let src = "module m(input logic [7:0] din, output logic [7:0] y);\n\
               typedef enum logic [1:0] { A, B, C } e_t;\n\
               e_t q;\n\
               always_comb begin q = din[0] ? B : A; y = (q == B) ? 8'd1 : 8'd0; end\n\
               endmodule\n";
    let design = build(src);
    let body = semua_stmt(&design);
    let Statement::Assign { assignment, .. } = &body[1] else {
        panic!("harus assignment");
    };
    let mut signals = vec![0u64; design.variables.len()];
    // `evaluasi` menghitung satu ekspresi tanpa menjalankan statement, jadi
    // nilai `q` diisi manual dengan hasil statement sebelumnya (B = 1).
    signals[design.find_variable("q").unwrap().signal_id as usize] = 1;
    // q == B benar, jadi hasilnya 1.
    assert_eq!(evaluasi(&assignment.value, &mut signals), 1);
    signals[design.find_variable("q").unwrap().signal_id as usize] = 0;
    // q == A, jadi perbandingannya salah dan hasilnya 0.
    assert_eq!(evaluasi(&assignment.value, &mut signals), 0);
}

/// Literal enum boleh dipakai sebagai argumen system task.
#[test]
fn literal_enum_dalam_argumen_system_task() {
    let src = "module m;\n\
               typedef enum logic [1:0] { A, B, C } e_t;\n\
               e_t q;\n\
               always_comb begin q = C; $display(\"%d\", q); end\n\
               endmodule\n";
    let design = build(src);
    let body = semua_stmt(&design);
    let Statement::Assign { assignment, .. } = &body[0] else {
        panic!("harus assignment");
    };
    let mut signals = vec![0u64; design.variables.len()];
    assert_eq!(evaluasi(&assignment.value, &mut signals), 2);
}

/// Enum di modul anak ikut ter-resolve saat instansiasi.
#[test]
fn enum_modul_anak_ter_resolve() {
    let src = "module anak(input logic [7:0] a, output logic [7:0] y);\n\
               typedef enum logic [1:0] { A, B, C } e_t;\n\
               e_t q;\n\
               always_comb begin q = a[0] ? C : A; y = q; end\n\
               endmodule\n\
               module top(input logic [7:0] a, output logic [7:0] y);\n\
               anak u0(.a(a), .y(y));\n\
               endmodule\n";
    let design = build_top(src, "top");
    assert_eq!(lebar(&design, "y"), 8);
    let proses = design
        .processes
        .iter()
        .find(|p| p.name == "always_comb")
        .expect("proses always_comb anak ada");
    let Statement::Assign { assignment, .. } = &proses.body[0] else {
        panic!("harus assignment");
    };
    let mut signals = vec![0u64; design.variables.len()];
    signals[design.find_variable("a").unwrap().signal_id as usize] = 1;
    assert_eq!(evaluasi(&assignment.value, &mut signals), 2);
}

// --- Validasi ---

#[test]
fn tipe_tak_dideklarasikan_ditolak() {
    let e = err("module m;\n\
                 byte_t data;\n\
                 always_comb data = 0;\n\
                 endmodule\n");
    assert!(
        e.message.contains("undefined type 'byte_t'"),
        "pesan: {}",
        e.message
    );
}

#[test]
fn nilai_enum_melebih_lebar_ditolak() {
    let e = err("module m;\n\
                 typedef enum logic [2:0] { A = 9 } e_t;\n\
                 e_t q;\n\
                 always_comb q = A;\n\
                 endmodule\n");
    assert!(
        e.message.contains("melebihi lebar tipe 3 bit"),
        "pesan: {}",
        e.message
    );
}

#[test]
fn typedef_ganda_ditolak() {
    let e = err("module m;\n\
                 typedef logic [7:0] a_t;\n\
                 typedef logic [3:0] a_t;\n\
                 a_t q;\n\
                 always_comb q = 0;\n\
                 endmodule\n");
    assert!(
        e.message.contains("duplicate typedef 'a_t'"),
        "pesan: {}",
        e.message
    );
}

#[test]
fn anggota_enum_ganda_ditolak() {
    let e = err("module m;\n\
                 typedef enum logic [1:0] { A, A } e_t;\n\
                 e_t q;\n\
                 always_comb q = A;\n\
                 endmodule\n");
    assert!(
        e.message.contains("duplicate enum member 'A'"),
        "pesan: {}",
        e.message
    );
}

#[test]
fn deklarasi_lebih_lebar_dari_typedef_adalah_packed_array() {
    // BUG-10: `nib_t [7:0] x;` dengan `typedef logic [3:0] nib_t` adalah packed
    // array 8 elemen = 32 bit dan SAH secara LRM §7.3. Implementasi lama
    // menolaknya karena membandingkan langsung dengan lebar tipe dasar, jadi
    // RTL yang sah gagal dielaborasi total.
    let src = "module m;\n\
               typedef logic [3:0] nib_t;\n\
               nib_t [7:0] x;\n\
               always_comb x = 32'hF;\n\
               endmodule\n";
    let design = build(src);
    assert_eq!(lebar(&design, "x"), 32);
}

#[test]
fn span_error_tipe_menunjuk_ke_nama_tipe() {
    let e = err("module m;\n\
                 byte_t data;\n\
                 always_comb data = 0;\n\
                 endmodule\n");
    assert_eq!(e.span.line, 2, "span: {:?}", e.span);
}

// --- Interaksi dengan fitur lain ---

/// Literal enum dan literal sized harus dibedakan lebar oleh ekspresi.
#[test]
fn enum_dan_sized_literal_tidak_tercampur() {
    let src = "module m;\n\
               typedef enum logic [3:0] { A = 1 } e_t;\n\
               e_t q;\n\
               always_comb q = A;\n\
               endmodule\n";
    let design = build(src);
    let expr = nilai_rhs(&design);
    assert_eq!(
        expr,
        Expr::constant_unknown(1, DataType::logic(4), 0),
        "literal enum harus sized sesuai lebar tipe enum"
    );
}

/// Typedef tidak boleh bocor antar modul: enum anak tak terlihat di induk.
#[test]
fn enum_modul_anak_tidak_bocor_ke_induk() {
    let src = "module anak;\n\
               typedef enum logic [1:0] { A, B, C } e_t;\n\
               e_t q;\n\
               always_comb q = C;\n\
               endmodule\n\
               module top;\n\
               logic [7:0] y;\n\
               always_comb y = C;\n\
               endmodule\n";
    let e = match sv_elaborator::elaborate_top(
        &{
            let tokens = sv_lexer::lex(src).expect("lex");
            sv_parser::parse_file(&tokens).expect("parse")
        },
        "top",
    ) {
        Ok(_) => panic!("literal enum anak seharusnya tidak dikenal di induk"),
        Err(e) => e,
    };
    assert!(
        e.message.contains("undefined signal 'C'"),
        "pesan: {}",
        e.message
    );
}
