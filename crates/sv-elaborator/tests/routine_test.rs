// Tanggung jawab: integration test inlining task/function (LRM §13.3/§13.4).
mod support;

use support::{build, build_top, dengan_sinyal, evaluasi, lebar_mask, nilai_rhs};
use sv_ir::process::{AssignStyle, Statement};
use sv_ir::{DataType, Expr};

/// Elaborate sumber dan kembalikan pesan error, atau panic bila sukses.
fn err(source: &str) -> sv_elaborator::ElaborateError {
    let tokens = sv_lexer::lex(source).expect("lex");
    let ast = sv_parser::parse_module(&tokens).expect("parse");
    match sv_elaborator::elaborate(&ast) {
        Ok(_) => panic!("sumber seharusnya gagal dielaborasi:\n{source}"),
        Err(e) => e,
    }
}

/// BUG: deklarasi lokal di dalam badan task tidak diberi nama unik per
/// pemanggilan. Dua pemanggilan task yang sama menduplikasi nama lokal yang
/// sama, sehingga gagal elaborasi dengan `duplicate signal`.
#[test]
fn deklarasi_lokal_task_dipanggil_ganda_tidak_bentrok() {
    let src = "module m(input logic [7:0] a, output logic [7:0] y, output logic [7:0] z);\n\
               task pakai_lokal(input [7:0] v, output [7:0] hasil);\n\
                 logic [7:0] t;\n\
                 t = v + 1;\n\
                 hasil = t;\n\
               endtask\n\
               always_comb begin pakai_lokal(a, y); pakai_lokal(a, z); end\n\
               endmodule\n";
    let design = build(src);
    let mut signals = vec![0u64; design.variables.len()];
    signals[design.find_variable("a").unwrap().signal_id as usize] = 5;
    let body = semua_stmt(&design);
    // Dua pemanggilan, masing-masing menyumbang `t = ...` lalu `hasil = t`.
    // `Decl` sendiri tidak jadi statement runtime karena sudah terdaftar
    // sebagai sinyal design.
    assert_eq!(body.len(), 4, "dua task inline jadi empat statement");
    let Statement::Assign { assignment: t1, .. } = &body[0] else {
        panic!("harus assignment");
    };
    let Statement::Assign { assignment: h1, .. } = &body[1] else {
        panic!("harus assignment");
    };
    let Statement::Assign { assignment: t2, .. } = &body[2] else {
        panic!("harus assignment");
    };
    let Statement::Assign { assignment: h2, .. } = &body[3] else {
        panic!("harus assignment");
    };
    assert_eq!(h1.target, design.find_variable("y").unwrap().signal_id);
    assert_eq!(h2.target, design.find_variable("z").unwrap().signal_id);
    // Kedua `t` harus sinyal berbeda, kalau tidak design akan bentrok.
    assert_ne!(t1.target, t2.target, "dua pemanggilan punya t berbeda");
    assert_ne!(t1.target, h1.target, "t dan hasil tidak berbagi sinyal");
    assert_eq!(h1.value, Expr::signal_ref(t1.target, DataType::logic(8)));
    assert_eq!(h2.value, Expr::signal_ref(t2.target, DataType::logic(8)));
    assert_eq!(evaluasi(&t1.value, &mut signals), 6);
    assert_eq!(evaluasi(&t2.value, &mut signals), 6);
    signals[t1.target as usize] = 6;
    signals[t2.target as usize] = 6;
    assert_eq!(evaluasi(&h1.value, &mut signals), 6);
    assert_eq!(evaluasi(&h2.value, &mut signals), 6);
}

/// Deklarasi lokal di dalam badan task tetap bisa dibaca dan ditulis.
#[test]
fn deklarasi_lokal_task_ter_daftarkan_sebagai_sinyal() {
    let src = "module m(input logic [7:0] a, output logic [7:0] y);\n\
               task pakai_lokal(input [7:0] v, output [7:0] hasil);\n\
                 logic [7:0] t;\n\
                 t = v + 1;\n\
                 hasil = t;\n\
               endtask\n\
               always_comb begin pakai_lokal(a, y); end\n\
               endmodule\n";
    let design = build(src);
    let mut signals = vec![0u64; design.variables.len()];
    signals[design.find_variable("a").unwrap().signal_id as usize] = 5;
    let body = semua_stmt(&design);
    assert_eq!(body.len(), 2, "satu task inline jadi dua statement");
    let Statement::Assign { assignment: t, .. } = &body[0] else {
        panic!("harus assignment");
    };
    let Statement::Assign { assignment, .. } = &body[1] else {
        panic!("harus assignment");
    };
    assert_eq!(
        assignment.target,
        design.find_variable("y").unwrap().signal_id
    );
    // `hasil = t` membaca t, bukan signal modul mana pun.
    assert_eq!(
        assignment.value,
        Expr::signal_ref(t.target, DataType::logic(8))
    );
    // `evaluasi` menghitung satu ekspresi tanpa menjalankan statement, jadi
    // nilai t diisi manual hasil statement pertama (a + 1 = 6).
    assert_eq!(evaluasi(&t.value, &mut signals), 6);
    signals[t.target as usize] = 6;
    assert_eq!(evaluasi(&assignment.value, &mut signals), 6);
    // Sinyal lokal terdaftar dengan nama yang memuat nama task dan nomor
    // pemanggilan, jadi tidak mungkin bentrok dengan sinyal modul.
    let lokal = design
        .variables
        .iter()
        .find(|v| v.name.starts_with("pakai_lokal__"))
        .expect("sinyal lokal task terdaftar");
    assert_ne!(
        lokal.signal_id,
        design.find_variable("y").unwrap().signal_id
    );
}

/// Substitusi formal berlaku juga pada sinyal yang dibaca lewat helper ini.
#[test]
fn function_membaca_nilai_sinyal_pengganti() {
    let src = "module m(input logic [7:0] a, output logic [7:0] y);\n\
               function [7:0] tambah_satu(input [7:0] v);\n\
                 tambah_satu = v + 1;\n\
               endfunction\n\
               always_comb y = tambah_satu(a);\n\
               endmodule\n";
    assert_eq!(dengan_sinyal(src, &[("a", 9)]), 10);
}

/// Nilai balik fungsi mengikuti lebar sinyal argumen yang diberikan.
#[test]
fn argumen_function_memakai_lebar_sinyal_asli() {
    let src = "module m(input logic [3:0] a, output logic [7:0] y);\n\
               function [7:0] lebarkan(input [3:0] v);\n\
                 lebarkan = v;\n\
               endfunction\n\
               always_comb y = lebarkan(a);\n\
               endmodule\n";
    let nilai = dengan_sinyal(src, &[("a", 0xF)]);
    // Sinyal sumber 4 bit, jadi hanya 4 bit bawah yang terbawa.
    assert_eq!(nilai, 0xF);
}

/// Hitung statement pertama pada proses pertama.
fn stmt_pertama(design: &sv_ir::Design) -> Statement {
    design.processes[0].body[0].clone()
}

/// Kumpulkan seluruh statement proses pertama, termasuk isi blok.
///
/// `always_comb begin ... end` dielroutine parser sebagai daftar statement
/// biasa (tanpa node `Block`), jadi isi task muncul langsung di `body`.
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

// --- Function ---

/// LRM §13.4: badan function berupa assignment ke namanya becomes expression.
#[test]
fn function_assignment_di_inline_menjadi_ekspresi() {
    let src = "module m(input logic [7:0] a, input logic [7:0] b, output logic [7:0] y);\n\
               function [7:0] jumlah(input [7:0] x, input [7:0] w);\n\
                 jumlah = x + w;\n\
               endfunction\n\
               always_comb y = jumlah(a, b);\n\
               endmodule\n";
    let design = build(src);
    match stmt_pertama(&design) {
        Statement::Assign { assignment, .. } => {
            assert_eq!(
                assignment.target,
                design.find_variable("y").unwrap().signal_id
            );
            let mut signals = vec![0u64; design.variables.len()];
            let a = design.find_variable("a").unwrap().signal_id as usize;
            let b = design.find_variable("b").unwrap().signal_id as usize;
            signals[a] = 7;
            signals[b] = 5;
            assert_eq!(evaluasi(&assignment.value, &mut signals), 12);
        }
        other => panic!("harus assignment, dapat {other:?}"),
    }
}

/// LRM §13.4: `return expr;` setara assignment ke nama fungsi.
#[test]
fn function_dengan_return_di_inline() {
    let src = "module m(input logic [7:0] a, output logic [7:0] y);\n\
               function [7:0] kali(input [7:0] v);\n\
                 return v + 1;\n\
               endfunction\n\
               always_comb y = kali(a);\n\
               endmodule\n";
    let design = build(src);
    let mut signals = vec![0u64; design.variables.len()];
    signals[design.find_variable("a").unwrap().signal_id as usize] = 41;
    assert_eq!(evaluasi(&nilai_rhs(&design), &mut signals), 42);
}

/// LRM §13.4: fungsi boleh dipanggil tanpa argumen dan membaca sinyal modul.
#[test]
fn function_tanpa_argumen_membaca_sinyal_modul() {
    let src = "module m(input logic [7:0] a, output logic y);\n\
               function paritas;\n\
                 paritas = ^a;\n\
               endfunction\n\
               always_comb y = paritas();\n\
               endmodule\n";
    let design = build(src);
    let mut signals = vec![0u64; design.variables.len()];
    let a = design.find_variable("a").unwrap().signal_id as usize;
    signals[a] = 0b101;
    assert_eq!(evaluasi(&nilai_rhs(&design), &mut signals), 0);
    signals[a] = 0b111;
    assert_eq!(evaluasi(&nilai_rhs(&design), &mut signals), 1);
}

/// Fungsi boleh memanggil fungsi lain; argumennya ikut tersubstitusi.
#[test]
fn function_memanggil_function_lain() {
    let src = "module m(input logic [7:0] a, output logic [7:0] y);\n\
               function [7:0] jumlah(input [7:0] x, input [7:0] w);\n\
                 jumlah = x + w;\n\
               endfunction\n\
               function [7:0] dobel(input [7:0] v);\n\
                 dobel = jumlah(v, v);\n\
               endfunction\n\
               always_comb y = dobel(a);\n\
               endmodule\n";
    let design = build(src);
    let mut signals = vec![0u64; design.variables.len()];
    signals[design.find_variable("a").unwrap().signal_id as usize] = 7;
    assert_eq!(evaluasi(&nilai_rhs(&design), &mut signals), 14);
}

/// Fungsi boleh dipanggil di dalam ekspresi yang lebih besar.
#[test]
fn function_nested_dalam_ekspresi_aritmetika() {
    let src = "module m(input logic [7:0] a, output logic [7:0] y);\n\
               function [7:0] kali(input [7:0] v);\n\
                 kali = v * 2;\n\
               endfunction\n\
               always_comb y = kali(a) + 1;\n\
               endmodule\n";
    let design = build(src);
    let mut signals = vec![0u64; design.variables.len()];
    signals[design.find_variable("a").unwrap().signal_id as usize] = 10;
    assert_eq!(evaluasi(&nilai_rhs(&design), &mut signals), 21);
}

/// Badan function berbentuk `if/else` menjadi operator ternair (LRM §11.4.11).
#[test]
fn function_dengan_if_menjadi_ternary() {
    let src = "module m(input logic [7:0] a, output logic [7:0] y);\n\
               function [7:0] pilih(input [7:0] v);\n\
                 if (v > 4) pilih = 8'hFF; else pilih = 8'h00;\n\
               endfunction\n\
               always_comb y = pilih(a);\n\
               endmodule\n";
    let design = build(src);
    let mut signals = vec![0u64; design.variables.len()];
    let a = design.find_variable("a").unwrap().signal_id as usize;
    signals[a] = 2;
    assert_eq!(evaluasi(&nilai_rhs(&design), &mut signals), 0);
    signals[a] = 9;
    assert_eq!(evaluasi(&nilai_rhs(&design), &mut signals), 0xFF);
}

/// LRM §13.4: nilai balik mengikuti lebar yang ditulis pada header.
#[test]
fn lebar_function_bukan_satu_bit_pada_reduksi() {
    // Tanpa lebar eksplisit, `paritas` hanya 1 bit; lebarnya harus 1.
    let src = "module m(input logic [7:0] a, output logic y);\n\
               function paritas;\n\
                 paritas = ^a;\n\
               endfunction\n\
               always_comb y = paritas();\n\
               endmodule\n";
    let design = build(src);
    assert_eq!(nilai_rhs(&design).data_type().width, 1);
}

// --- Task ---

/// LRM §13.3: badan task di-inline di depan statement pemanggil.
#[test]
fn task_body_di_inline_sebagai_statement() {
    let src = "module m(input logic [7:0] a, output logic [7:0] y);\n\
               task salin(input [7:0] v, output [7:0] hasil);\n\
                 hasil = v;\n\
               endtask\n\
               always_comb begin salin(a, y); end\n\
               endmodule\n";
    let design = build(src);
    let mut signals = vec![0u64; design.variables.len()];
    signals[design.find_variable("a").unwrap().signal_id as usize] = 5;
    let body = semua_stmt(&design);
    assert_eq!(body.len(), 1, "satu statement task harus ter-inline");
    let Statement::Assign { assignment, .. } = &body[0] else {
        panic!("harus assignment");
    };
    assert_eq!(
        assignment.target,
        design.find_variable("y").unwrap().signal_id
    );
    assert_eq!(evaluasi(&assignment.value, &mut signals), 5);
}

/// LRM §13.3: argumen `output` menulis langsung ke sinyal pemanggil.
#[test]
fn task_argumen_output_menulis_ke_sinyal_pemanggil() {
    let src = "module m(input logic [7:0] a, output logic [7:0] y);\n\
               task kuadrat(input [7:0] v, output [7:0] hasil);\n\
                 hasil = v * v;\n\
               endtask\n\
               always_comb begin kuadrat(a, y); end\n\
               endmodule\n";
    let design = build(src);
    let mut signals = vec![0u64; design.variables.len()];
    signals[design.find_variable("a").unwrap().signal_id as usize] = 7;
    let body = semua_stmt(&design);
    let Statement::Assign { assignment, .. } = &body[0] else {
        panic!("harus assignment");
    };
    assert_eq!(
        assignment.target,
        design.find_variable("y").unwrap().signal_id
    );
    assert_eq!(evaluasi(&assignment.value, &mut signals), 49);
}

/// Task tanpa argumen boleh membaca dan menulis sinyal modul.
#[test]
fn task_tanpa_argumen_mengubah_sinyal_modul() {
    let src = "module m(output logic [7:0] y);\n\
               task set_nol;\n\
                 y = 8'd0;\n\
               endtask\n\
               always_comb begin set_nol; end\n\
               endmodule\n";
    let design = build(src);
    let body = semua_stmt(&design);
    let Statement::Assign { assignment, .. } = &body[0] else {
        panic!("harus assignment");
    };
    assert_eq!(
        assignment.target,
        design.find_variable("y").unwrap().signal_id
    );
}

/// Task boleh memuat lebih dari satu statement.
#[test]
fn task_multi_statement_semua_ter_inline() {
    let src = "module m(input logic [7:0] a, output logic [7:0] y, output logic [7:0] z);\n\
               task duo(input [7:0] v, output [7:0] satu, output [7:0] dua);\n\
                 satu = v;\n\
                 dua = v + 1;\n\
               endtask\n\
               always_comb begin duo(a, y, z); end\n\
               endmodule\n";
    let design = build(src);
    let mut signals = vec![0u64; design.variables.len()];
    signals[design.find_variable("a").unwrap().signal_id as usize] = 4;
    let body = semua_stmt(&design);
    assert_eq!(body.len(), 2, "dua statement task harus ikut inline");
    let Statement::Assign {
        assignment: first, ..
    } = &body[0]
    else {
        panic!("harus assignment");
    };
    assert_eq!(first.target, design.find_variable("y").unwrap().signal_id);
    assert_eq!(evaluasi(&first.value, &mut signals), 4);
    let Statement::Assign {
        assignment: second, ..
    } = &body[1]
    else {
        panic!("harus assignment");
    };
    assert_eq!(second.target, design.find_variable("z").unwrap().signal_id);
    assert_eq!(evaluasi(&second.value, &mut signals), 5);
}

/// Task boleh memanggil function di dalam badannya.
#[test]
fn task_memanggil_function() {
    let src = "module m(input logic [7:0] a, output logic [7:0] y);\n\
               function [7:0] kali(input [7:0] v);\n\
                 kali = v * 2;\n\
               endfunction\n\
               task terapkan(input [7:0] v, output [7:0] hasil);\n\
                 hasil = kali(v);\n\
               endtask\n\
               always_comb begin terapkan(a, y); end\n\
               endmodule\n";
    let design = build(src);
    let mut signals = vec![0u64; design.variables.len()];
    signals[design.find_variable("a").unwrap().signal_id as usize] = 6;
    let body = semua_stmt(&design);
    let Statement::Assign { assignment, .. } = &body[0] else {
        panic!("harus assignment");
    };
    assert_eq!(evaluasi(&assignment.value, &mut signals), 12);
}

/// Task yang dipanggil dua kali menghasilkan dua salinan statement.
#[test]
fn task_dipanggil_dua_kali_ter_inline_dua_kali() {
    let src = "module m(input logic [7:0] a, output logic [7:0] y, output logic [7:0] z);\n\
               task salin(input [7:0] v, output [7:0] hasil);\n\
                 hasil = v;\n\
               endtask\n\
               always_comb begin salin(a, y); salin(a, z); end\n\
               endmodule\n";
    let design = build(src);
    assert_eq!(semua_stmt(&design).len(), 2);
}

/// Subrutin yang dipakai di dalam region generate ikut ter-expand.
#[test]
fn function_dipakai_di_generate() {
    let src = "module m(input logic [1:0] s, output logic [7:0] y);\n\
               function [7:0] tabel(input [1:0] i);\n\
                 tabel = i + 1;\n\
               endfunction\n\
               generate\n\
                 assign y = tabel(s);\n\
               endgenerate\n\
               endmodule\n";
    let design = build(src);
    let mut signals = vec![0u64; design.variables.len()];
    let s = design.find_variable("s").unwrap().signal_id as usize;
    signals[s] = 3;
    assert_eq!(evaluasi(&nilai_rhs(&design), &mut signals), 4);
}

/// Subrutin milik modul anak ikut ter-expand saat instansiasi.
#[test]
fn function_modul_anak_ter_expand() {
    let src = "module anak(input logic [7:0] a, output logic [7:0] y);\n\
               function [7:0] twice(input [7:0] v);\n\
                 twice = v + v;\n\
               endfunction\n\
               always_comb y = twice(a);\n\
               endmodule\n\
               module top(input logic [7:0] a, output logic [7:0] y);\n\
               anak u0(.a(a), .y(y));\n\
               endmodule\n";
    let design = build_top(src, "top");
    let mut signals = vec![0u64; design.variables.len()];
    signals[design.find_variable("a").unwrap().signal_id as usize] = 9;
    let proses = design
        .processes
        .iter()
        .find(|p| p.name == "always_comb")
        .expect("proses always_comb anak ada");
    let Statement::Assign { assignment, .. } = &proses.body[0] else {
        panic!("harus assignment");
    };
    assert_eq!(evaluasi(&assignment.value, &mut signals), 18);
}

/// Keyword `automatic` diterima tanpa mengubah hasil (LRM §13.3).
#[test]
fn automatic_diterima_tanpa_mengubah_hasil() {
    let src = "module m(input logic [7:0] a, output logic [7:0] y);\n\
               function automatic [7:0] Automatic_f(input [7:0] v);\n\
                 Automatic_f = v;\n\
               endfunction\n\
               always_comb y = Automatic_f(a);\n\
               endmodule\n";
    let design = build(src);
    let mut signals = vec![0u64; design.variables.len()];
    signals[design.find_variable("a").unwrap().signal_id as usize] = 3;
    assert_eq!(evaluasi(&nilai_rhs(&design), &mut signals), 3);
}

// --- Validasi ---

#[test]
fn jumlah_argumen_salah_ditolak() {
    let e = err("module m(input logic [7:0] a, output logic [7:0] y);\n\
                 function [7:0] jumlah(input [7:0] x, input [7:0] w);\n\
                   jumlah = x + w;\n\
                 endfunction\n\
                 always_comb y = jumlah(a);\n\
                 endmodule\n");
    assert!(
        e.message.contains("mengharapkan 2 argumen"),
        "pesan: {}",
        e.message
    );
}

#[test]
fn task_dipakai_dalam_ekspresi_ditolak() {
    // Task tidak punya nilai balik, jadi tidak boleh dipanggil dari ekspresi.
    let e = err("module m(input logic [7:0] a, output logic [7:0] y);\n\
                 task isi(input [7:0] v, output [7:0] hasil);\n\
                   hasil = v;\n\
                 endtask\n\
                 always_comb y = isi(a);\n\
                 endmodule\n");
    assert!(
        e.message.contains("adalah task, bukan function"),
        "pesan: {}",
        e.message
    );
}

#[test]
fn function_dipakai_sebagai_task_ditolak() {
    let e = err("module m(input logic [7:0] a, output logic [7:0] y);\n\
                 function [7:0] f(input [7:0] v);\n\
                   f = v;\n\
                 endfunction\n\
                 always_comb begin f(a, y); end\n\
                 endmodule\n");
    assert!(
        e.message.contains("adalah function, bukan task"),
        "pesan: {}",
        e.message
    );
}

#[test]
fn subroutine_tidak_didefinisikan_ditolak() {
    let e = err("module m(input logic [7:0] a, output logic [7:0] y);\n\
                 always_comb y = tak_ada(a);\n\
                 endmodule\n");
    assert!(
        e.message.contains("undefined function or task 'tak_ada'"),
        "pesan: {}",
        e.message
    );
}

#[test]
fn argumen_output_yang_ekspresi_ditolak() {
    let e = err("module m(input logic [7:0] a, output logic [7:0] y);\n\
                 task isi(input [7:0] v, output [7:0] hasil);\n\
                   hasil = v;\n\
                 endtask\n\
                 always_comb begin isi(a, a + y); end\n\
                 endmodule\n");
    assert!(
        e.message.contains("harus berupa nama sinyal"),
        "pesan: {}",
        e.message
    );
}

#[test]
fn badan_function_majemuk_ditolak_dengan_pesan_jelas() {
    let e = err("module m(input logic [7:0] a, output logic [7:0] y);\n\
                 function [7:0] f(input [7:0] v);\n\
                   if (v > 0) y = 1;\n\
                   y = v;\n\
                 endfunction\n\
                 always_comb y = f(a);\n\
                 endmodule\n");
    assert!(
        e.message.contains("harus satu assignment ke namanya"),
        "pesan: {}",
        e.message
    );
}

#[test]
fn nama_routine_ganda_ditolak() {
    let e = err("module m(output logic [7:0] y);\n\
                 function [7:0] f;\n\
                   f = 1;\n\
                 endfunction\n\
                 task f;\n\
                 endtask\n\
                 always_comb y = f();\n\
                 endmodule\n");
    assert!(
        e.message.contains("duplicate routine 'f'"),
        "pesan: {}",
        e.message
    );
}

/// Rekursi tak berujung harus ditolak, bukan membuat elaborator hang.
#[test]
fn rekursi_fungsi_ditolak_dengan_batas_kedalaman() {
    let e = err("module m(input logic [7:0] a, output logic [7:0] y);\n\
                 function [7:0] f(input [7:0] v);\n\
                   f = f(v);\n\
                 endfunction\n\
                 always_comb y = f(a);\n\
                 endmodule\n");
    assert!(e.message.contains("melebihi batas"), "pesan: {}", e.message);
}

/// Span error menunjuk ke nama subroutine yang tidak dikenal, bukan dummy.
#[test]
fn span_error_menunjuk_ke_nama_subroutine() {
    let e = err("module m(input logic [7:0] a, output logic [7:0] y);\n\
                 always_comb y = tak_ada(a);\n\
                 endmodule\n");
    assert_eq!(e.span.line, 2, "span: {:?}", e.span);
    // `tak_ada` dimulai tepat setelah `y = ` pada baris kedua.
    assert_eq!(e.span.col, 17, "span: {:?}", e.span);
}

// --- Integrasi dengan pipeline lain ---

/// Subrutin tidak boleh mengubah lebar sinyal yang dideklarasinya.
#[test]
fn lebar_hasil_function_mengikuti_ekspresi_badan() {
    let src = "module m(input logic [3:0] a, output logic [7:0] y);\n\
               function [7:0] lebarkan(input [3:0] v);\n\
                 lebarkan = v;\n\
               endfunction\n\
               always_comb y = lebarkan(a);\n\
               endmodule\n";
    let design = build(src);
    // Badan function di-inline jadi signal `a` selebar 4 bit, tapi batas
    // assignment adalah context-determined (LRM §11.6.1 Tabel 11-21): ekspresi
    // kanan diskalakan ke lebar target. Nilai akhirnya tetap sama — yang
    // berubah hanya lebar yang diklaim IR.
    assert_eq!(nilai_rhs(&design).data_type().width, 8);
    let mut signals = vec![0u64; design.variables.len()];
    signals[design.find_variable("a").unwrap().signal_id as usize] = 0xF;
    assert_eq!(
        evaluasi(&nilai_rhs(&design), &mut signals) & lebar_mask(8),
        0xF
    );
}

#[test]
fn blocking_pertahankan_style_assignment() {
    let src = "module m(input logic [7:0] a, output logic [7:0] y);\n\
               function [7:0] f(input [7:0] v);\n\
                 f = v;\n\
               endfunction\n\
               always_comb y = f(a);\n\
               endmodule\n";
    let design = build(src);
    let Statement::Assign { assignment, .. } = stmt_pertama(&design) else {
        panic!("harus assignment");
    };
    assert_eq!(assignment.style, AssignStyle::Blocking);
    assert!(assignment.slice.is_none());
}
