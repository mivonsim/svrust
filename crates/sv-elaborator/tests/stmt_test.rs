// Tanggung jawab: integration test statement prosedural hasil elaborasi ke IR.
mod support;

use support::*;
use sv_ir::Expr;

// --- BUG-15: casez / casex dengan wildcard ---

#[test]
fn case_biasa_memakai_jenis_exact() {
    let src = "module m(input [1:0] s, output [3:0] y); \
               always_comb case (s) 0: y = 1; default: y = 0; endcase endmodule";
    match &build(src).processes[0].body[0] {
        sv_ir::Statement::Case { kind, .. } => assert_eq!(*kind, sv_ir::CaseKind::Exact),
        other => panic!("expected case, dapat {other:?}"),
    }
}

#[test]
fn casez_memakai_jenis_casez() {
    let src = "module m(input [1:0] s, output [3:0] y); \
               always_comb casez (s) 2'b1?: y = 1; default: y = 0; endcase endmodule";
    match &build(src).processes[0].body[0] {
        sv_ir::Statement::Case { kind, .. } => {
            assert_eq!(*kind, sv_ir::CaseKind::Casez);
            assert!(kind.is_wildcard());
        }
        other => panic!("expected case, dapat {other:?}"),
    }
}

#[test]
fn casex_memakai_jenis_casex() {
    let src = "module m(input [1:0] s, output [3:0] y); \
               always_comb casex (s) 2'b1x: y = 1; default: y = 0; endcase endmodule";
    match &build(src).processes[0].body[0] {
        sv_ir::Statement::Case { kind, .. } => {
            assert_eq!(*kind, sv_ir::CaseKind::Casex);
            assert!(kind.is_wildcard());
        }
        other => panic!("expected case, dapat {other:?}"),
    }
}

#[test]
fn label_wildcard_membawa_unknown_mask() {
    let src = "module m(input [1:0] s, output [3:0] y); \
               always_comb casez (s) 2'b1?: y = 1; default: y = 0; endcase endmodule";
    match &build(src).processes[0].body[0] {
        sv_ir::Statement::Case { arms, .. } => {
            // `2'b1?` => bit 0 adalah wildcard.
            match &arms[0].labels[0] {
                Expr::Const {
                    value,
                    unknown_mask,
                    zmask,
                    data_type,
                } => {
                    assert_eq!(*value, 0b10);
                    assert_eq!(*unknown_mask, 0b01);
                    // `?` berarti `z` (LRM §5.7.1), jadi bukan `x`.
                    assert_eq!(*zmask, 0b01);
                    assert_eq!(data_type.width, 2);
                }
                other => panic!("expected const label, dapat {other:?}"),
            }
        }
        other => panic!("expected case, dapat {other:?}"),
    }
}

#[test]
fn case_eksak_label_biasa_tanpa_unknown() {
    let src = "module m(input [1:0] s, output [3:0] y); \
               always_comb case (s) 2'b10: y = 1; default: y = 0; endcase endmodule";
    match &build(src).processes[0].body[0] {
        sv_ir::Statement::Case { arms, .. } => match &arms[0].labels[0] {
            Expr::Const {
                value,
                unknown_mask,
                ..
            } => {
                assert_eq!(*value, 0b10);
                assert_eq!(*unknown_mask, 0, "case eksak tidak punya wildcard");
            }
            other => panic!("expected const label, dapat {other:?}"),
        },
        other => panic!("expected case, dapat {other:?}"),
    }
}

/// True bila label casez cocok dengan nilai selektor setelah wildcard dicoret.
fn label_casez_cocok(sel: u64, label: u64, unknown_mask: u64) -> bool {
    (sel & !unknown_mask) == (label & !unknown_mask)
}

#[test]
fn wildcard_hanya_mengecocokkan_bit_tertentu() {
    // 2'b1? cocok untuk 0b10 dan 0b11, bukan 0b00 atau 0b01.
    let label = 0b10u64;
    let unknown = 0b01u64;
    assert!(label_casez_cocok(0b10, label, unknown));
    assert!(label_casez_cocok(0b11, label, unknown));
    assert!(!label_casez_cocok(0b00, label, unknown));
    assert!(!label_casez_cocok(0b01, label, unknown));
}

#[test]
fn wildcard_penuh_cocok_semua_nilai() {
    // 2'b?? tidak membatasi apa pun.
    for nilai in 0..4u64 {
        assert!(label_casez_cocok(nilai, 0, 0b11), "nilai {nilai}");
    }
}

#[test]
fn case_eksak_membandingkan_seluruh_bit() {
    // Tanpa wildcard semua bit harus sama persis.
    let label = 0b10u64;
    for nilai in 0..4u64 {
        assert_eq!(
            label_casez_cocok(nilai, label, 0),
            nilai == label,
            "nilai {nilai}"
        );
    }
}

#[test]
fn casez_tidak_mempengaruhi_sensitivitas() {
    let src = "module m(input [1:0] s, output [3:0] y); \
               always_comb casez (s) 2'b1?: y = 1; default: y = 0; endcase endmodule";
    assert_eq!(build(src).processes[0].sensitivity.len(), 1);
}

/// True bila ada operasi biner `op` di seluruh body proses design.
fn memakai_binop(design: &sv_ir::Design, op: sv_ir::BinOp) -> bool {
    use sv_ir::Statement;
    design.processes.iter().any(|p| {
        p.body.iter().any(|stmt| match stmt {
            Statement::Assign { assignment, .. } => expr_memakai_binop(&assignment.value, op),
            Statement::Block { body, .. } => body.iter().any(|s| {
                matches!(s, Statement::Assign { assignment, .. }
                    if expr_memakai_binop(&assignment.value, op))
            }),
            _ => false,
        })
    })
}

/// True bila ekspresi (dan seluruh sub-ekspresinya) memakai `op`.
fn expr_memakai_binop(expr: &Expr, op: sv_ir::BinOp) -> bool {
    match expr {
        Expr::Bin {
            op: o, lhs, rhs, ..
        } => *o == op || expr_memakai_binop(lhs, op) || expr_memakai_binop(rhs, op),
        Expr::Un { operand, .. } => expr_memakai_binop(operand, op),
        Expr::Select { base, .. } => expr_memakai_binop(base, op),
        Expr::Index { base, index, .. } => {
            expr_memakai_binop(base, op) || expr_memakai_binop(index, op)
        }
        Expr::Ternary {
            condition,
            when_true,
            when_false,
            ..
        } => {
            expr_memakai_binop(condition, op)
                || expr_memakai_binop(when_true, op)
                || expr_memakai_binop(when_false, op)
        }
        Expr::Concat { items, .. } => items.iter().any(|i| expr_memakai_binop(i, op)),
        Expr::Replicate { value, .. } => expr_memakai_binop(value, op),
        Expr::Cast { operand, .. } => expr_memakai_binop(operand, op),
        Expr::Const { .. } | Expr::SignalRef { .. } | Expr::SimTime { .. } => false,
    }
}

// --- BUG-16: operator pembagian dan modulo ---

#[test]
fn pembagian_bulat_mengambil_bagian_bulat() {
    // LRM §11.4.5: 100 / 7 = 14 (bagian bulat, sisa dipotong).
    let src = "module m(input [7:0] a, input [7:0] b, output [7:0] y); assign y = a / b; endmodule";
    assert_eq!(dengan_sinyal(src, &[("a", 100), ("b", 7)]), 14);
}

#[test]
fn modulo_mengambil_sisa_pembagian() {
    // LRM §11.4.6: 100 % 7 = 2.
    let src = "module m(input [7:0] a, input [7:0] b, output [7:0] y); assign y = a % b; endmodule";
    assert_eq!(dengan_sinyal(src, &[("a", 100), ("b", 7)]), 2);
}

#[test]
fn pembagian_dengan_pembagi_nol_menghasilkan_nol() {
    // Engine 2-state tidak punya `x`, jadi pembagi nol menghasilkan 0
    // (bukan panic seperti pembagian integer Rust).
    let src = "module m(input [7:0] a, input [7:0] b, output [7:0] y); assign y = a / b; endmodule";
    assert_eq!(dengan_sinyal(src, &[("a", 100), ("b", 0)]), 0);
}

#[test]
fn modulo_dengan_pembagi_nol_menghasilkan_nol() {
    let src = "module m(input [7:0] a, input [7:0] b, output [7:0] y); assign y = a % b; endmodule";
    assert_eq!(dengan_sinyal(src, &[("a", 100), ("b", 0)]), 0);
}

#[test]
fn pembagian_lebih_kecil_dari_pembagi_nol() {
    // 20 / 30 = 0 karena pembagi lebih besar dari pembilang.
    let bagi =
        "module m(input [7:0] a, input [7:0] b, output [7:0] y); assign y = a / b; endmodule";
    assert_eq!(dengan_sinyal(bagi, &[("a", 20), ("b", 30)]), 0);
}

#[test]
fn modulo_nilai_tetap_saat_pembagi_lebih_besar() {
    // 20 % 30 = 20 karena pembagi lebih besar dari pembilang.
    let sisa =
        "module m(input [7:0] a, input [7:0] b, output [7:0] y); assign y = a % b; endmodule";
    assert_eq!(dengan_sinyal(sisa, &[("a", 20), ("b", 30)]), 20);
}

#[test]
fn pembagian_berurutan_mengikuti_presedence_kali() {
    // LRM §11.4: `*`, `/`, `%` punya presedence sama dan asosiatif kiri,
    // jadi `100 / 5 * 2` = (100 / 5) * 2 = 40, bukan 100 / (5 * 2) = 10.
    let src = "module m(output [7:0] y); assign y = 100 / 5 * 2; endmodule";
    assert_eq!(dengan_sinyal(src, &[]), 40);
}

#[test]
fn pembagian_dipotong_ke_lebar_tujuan() {
    // Hasil pembagian tidak boleh bocor di atas lebar logis.
    let src = "module m(output [3:0] y); assign y = 8'hFF / 8'h01; endmodule";
    assert_eq!(dengan_sinyal(src, &[]) & 0x0F, 0x0F);
}

// --- BUG-17: compound assignment `/=` dan `%=` ---

#[test]
fn compound_bagi_menghasilkan_operator_div() {
    // `a /= b` dielaborasi jadi assignment dengan operasi biner Div.
    let src = "module m(input [7:0] a, input [7:0] b, output [7:0] y); \
               always_comb begin a = 8'd100; a /= b; y = a; end endmodule";
    assert!(memakai_binop(&build(src), sv_ir::BinOp::Div));
}

#[test]
fn compound_sisa_menghasilkan_operator_mod() {
    // `a %= b` dielaborasi jadi assignment dengan operasi biner Mod.
    let src = "module m(input [7:0] a, input [7:0] b, output [7:0] y); \
               always_comb begin a = 8'd100; a %= b; y = a; end endmodule";
    assert!(memakai_binop(&build(src), sv_ir::BinOp::Mod));
}

// --- BUG-18: daftar deklarator ---

#[test]
fn daftar_deklarator_membuat_seuruh_signal() {
    // Ketiga signal harus terdaftar dan bisa dipakai bersama.
    let src = "module m(output [7:0] y); logic [7:0] a, b, c; \
               assign y = a | b | c; endmodule";
    let design = build(src);
    for nama in ["a", "b", "c"] {
        assert!(
            design.find_variable(nama).is_some(),
            "sinyal {nama} harus ada"
        );
    }
    assert_eq!(dengan_sinyal(src, &[("a", 1), ("b", 2), ("c", 4)]), 7);
}

#[test]
fn deklarasi_tunggal_tetap_berfungsi() {
    // Regresi: satu deklarator harus tetap menghasilkan satu signal.
    let src = "module m(output [7:0] y); logic [7:0] a; \
               always_comb begin a = 8'd5; y = a; end endmodule";
    assert_eq!(dengan_sinyal(src, &[]), 5);
}

/// Cari variable design yang berasal dari deklarasi lokal bernama `asli`.
///
/// Nama lokal diberi akhiran unik per scope blok, jadi tidak bisa dicari
/// langsung memakai nama sumber.
fn ada_lokal(design: &sv_ir::Design, asli: &str) -> bool {
    let awalan = format!("{}__lokal", asli);
    design.variables.iter().any(|v| v.name.starts_with(&awalan))
}

// --- BUG-22: deklarasi variabel lokal di dalam blok ---

#[test]
fn deklarasi_lokal_terdaftar_sebagai_variable() {
    let src = "module m(output [7:0] y); always_comb begin \
               logic [7:0] t; t = 8'd5; y = t; end endmodule";
    assert!(
        ada_lokal(&build(src), "t"),
        "variabel lokal harus terdaftar di design"
    );
}

#[test]
fn deklarasi_lokal_majemuk_terdaftar_semua() {
    let src = "module m(output [7:0] y); always_comb begin \
               logic [3:0] a, b; a = 4'd2; b = 4'd3; y = {a, b}; end endmodule";
    let design = build(src);
    for nama in ["a", "b"] {
        assert!(ada_lokal(&design, nama), "variabel {nama} harus terdaftar");
    }
}

#[test]
fn deklarasi_lokal_di_dalam_if_terdaftar() {
    // Deklarasi di cabang if/else tetap harus ditemukan.
    let src = "module m(output [7:0] y); always_comb begin \
               logic [7:0] t; if (1'b1) t = 8'd1; else t = 8'd2; y = t; end endmodule";
    assert!(ada_lokal(&build(src), "t"));
}

#[test]
fn deklarasi_lokal_di_dalam_for_terdaftar() {
    // Deklarasi di dalam blok for harus ditemukan oleh collector rekursif.
    let src = "module m(output [7:0] y); logic [3:0] k; always_comb begin \
               logic [7:0] s; s = 0; for (k = 0; k < 4; k++) s = s + k; y = s; end endmodule";
    let design = build(src);
    assert!(ada_lokal(&design, "s"), "variabel s harus terdaftar");
    assert!(
        design.find_variable("k").is_some(),
        "loop variable k ada di level modul"
    );
}

#[test]
fn deklarasi_lokal_tidak_menghasilkan_statement_runtime() {
    // Deklarasi hanya mendaftarkan variable, tidak menambah statement.
    let src = "module m(output [7:0] y); always_comb begin \
               logic [7:0] t; y = t; end endmodule";
    let with_decl = build(src);
    let tanpa_decl = build("module m(output [7:0] y); logic [7:0] t; always_comb y = t; endmodule");
    assert_eq!(
        with_decl.processes.len(),
        tanpa_decl.processes.len(),
        "jumlah proses harus sama"
    );
}

#[test]
fn deklarasi_lokal_di_always_ff_terdaftar() {
    let src = "module m(input clk, input d, output reg q); \
               always_ff @(posedge clk) begin logic t; t <= d; q <= t; end endmodule";
    assert!(ada_lokal(&build(src), "t"));
}

// --- BUG-23: blocking assignment di dalam always_ff ---

#[test]
fn blocking_assign_di_always_ff_diterima() {
    // LRM §10.3: `=` sah di blok sekuensial, hanya berbeda gaya assignment.
    let src = "module m(input clk, input d, output reg q); \
               always_ff @(posedge clk) begin q = d; end endmodule";
    let design = build(src);
    let uses_blocking = design.processes.iter().any(|p| {
        p.body.iter().any(|stmt| match stmt {
            sv_ir::Statement::Assign { assignment, .. } => {
                assignment.style == sv_ir::process::AssignStyle::Blocking
            }
            _ => false,
        })
    });
    assert!(uses_blocking, "harus ada assignment bergaya blocking");
}

#[test]
fn nonblocking_assign_di_always_ff_tetap_nonblocking() {
    // Regresi: `<=` harus tetap nonblocking.
    let src = "module m(input clk, input d, output reg q); \
               always_ff @(posedge clk) begin q <= d; end endmodule";
    let design = build(src);
    let uses_nonblocking = design.processes.iter().any(|p| {
        p.body.iter().any(|stmt| match stmt {
            sv_ir::Statement::Assign { assignment, .. } => {
                assignment.style == sv_ir::process::AssignStyle::NonBlocking
            }
            _ => false,
        })
    });
    assert!(uses_nonblocking, "harus ada assignment bergaya nonblocking");
}

#[test]
fn dua_blok_bisa_pakai_nama_lokal_sama() {
    // BUG-22: dua blok yang sama-sama mendeklarasikan `t` tidak boleh bentrok
    // pada satu namespace datar, jadi tiap blok mendapat nama uniknya.
    let src = "module m(output [7:0] y, output [7:0] z); \
               always_comb begin logic [7:0] t; t = 8'd1; y = t; end \
               always_comb begin logic [7:0] t; t = 8'd2; z = t; end endmodule";
    let design = build(src);
    let lokal_t: Vec<&str> = design
        .variables
        .iter()
        .filter(|v| v.name.starts_with("t__lokal"))
        .map(|v| v.name.as_str())
        .collect();
    assert_eq!(
        lokal_t.len(),
        2,
        "kedua blok harus punya nama lokal berbeda: {:?}",
        lokal_t
    );
}

#[test]
fn deklarasi_lokal_di_blok_bawah_terdaftar() {
    // Deklarasi di blok dalam tidak boleh bocor ke blok luar.
    let src = "module m(output [7:0] y); always_comb begin \
               begin logic [7:0] dalam; dalam = 8'd7; end \
               y = 8'd0; end endmodule";
    assert!(ada_lokal(&build(src), "dalam"));
}

#[test]
fn deklarasi_lokal_di_module_instans_terdaftar() {
    // BUG-22: variabel lokal di module anak ikut terdaftar setelah di-prefix.
    let src = "module anak(input [7:0] a, output [7:0] y); \
               always_comb begin logic [7:0] t; t = a + 8'd1; y = t; end endmodule \
               module top(input [7:0] a, output [7:0] z); \
               anak u(.a(a), .y(z)); endmodule";
    let design = build_top(src, "top");
    assert!(
        design
            .variables
            .iter()
            .any(|v| v.name.contains("u__") && v.name.contains("__lokal")),
        "variabel lokal anak harus terdaftar dengan prefix instansi: {:?}",
        design
            .variables
            .iter()
            .map(|v| v.name.as_str())
            .collect::<Vec<_>>()
    );
}

#[test]
fn dua_instans_lokal_sama_tidak_bentrok() {
    // Dua instans module yang sama tetap punya variabel lokal terpisah.
    let src = "module anak(input [7:0] a, output [7:0] y); \
               always_comb begin logic [7:0] t; t = a + 8'd1; y = t; end endmodule \
               module top(input [7:0] a, input [7:0] b, output [7:0] z, output [7:0] w); \
               anak u(.a(a), .y(z)); anak v(.a(b), .y(w)); endmodule";
    let design = build_top(src, "top");
    let lokal: Vec<&str> = design
        .variables
        .iter()
        .filter(|v| v.name.contains("__lokal"))
        .map(|v| v.name.as_str())
        .collect();
    assert_eq!(lokal.len(), 2, "harus ada dua variabel lokal: {:?}", lokal);
}

// --- BUG-24: keyword `integer` dan `int` ---

#[test]
fn integer_berlebar_32_bit_signed() {
    // LRM §6.2.2: `integer` selalu 32-bit signed tanpa perlu dimension.
    let src = "module m(output [31:0] y); integer s; assign y = s; endmodule";
    let design = build(src);
    let var = design.find_variable("s").expect("sinyal ada");
    assert_eq!(var.data_type.width, 32);
    assert!(var.data_type.signed, "integer harus signed");
}

#[test]
fn int_berlebar_32_bit_signed() {
    let src = "module m(output [31:0] y); int s; assign y = s; endmodule";
    let design = build(src);
    let var = design.find_variable("s").expect("sinyal ada");
    assert_eq!(var.data_type.width, 32);
    assert!(var.data_type.signed);
}

#[test]
fn integer_lokal_terdaftar() {
    let src = "module m(output [7:0] y); always_comb begin \
               integer s; s = 5; y = s[7:0]; end endmodule";
    assert!(ada_lokal(&build(src), "s"));
}

#[test]
fn integer_menghitung_pada_lebar_32_bit() {
    // Nilai 32-bit tidak boleh terpotong ke 8-bit.
    let src = "module m(output [31:0] y); integer s; assign y = s + 32'd200; endmodule";
    assert_eq!(dengan_sinyal(src, &[("s", 100)]), 300);
}

// --- BUG-25: inisialisasi deklarasi ---

#[test]
fn variabel_dengan_nilai_awal_mendapat_initial() {
    // LRM §6.2.2: `logic [7:0] s = 8'd42;` berarti nilai awal waktu nol.
    let src = "module m(output [7:0] y); logic [7:0] s = 8'd42; assign y = s; endmodule";
    let design = build(src);
    let var = design.find_variable("s").expect("sinyal ada");
    assert!(var.initial.is_some(), "nilai awal harus tersimpan di IR");
}

#[test]
fn deklarasi_tanpa_inisialisasi_tidak_ada_initial() {
    // Regresi: deklarasi biasa tidak boleh punya nilai awal.
    let src = "module m(output [7:0] y); logic [7:0] s; assign y = s; endmodule";
    let design = build(src);
    let var = design.find_variable("s").expect("sinyal ada");
    assert!(var.initial.is_none());
}

#[test]
fn inisialisasi_per_deklarator_independen() {
    // `logic [7:0] a = 1, b = 2;` memberi nilai awal berbeda per deklarator.
    let src = "module m(output [7:0] y); logic [7:0] a = 8'd1, b = 8'd2; \
               assign y = a | b; endmodule";
    let design = build(src);
    assert!(design.find_variable("a").expect("a ada").initial.is_some());
    assert!(design.find_variable("b").expect("b ada").initial.is_some());
}

#[test]
fn net_declaration_assign_menjadi_driver_kontinu() {
    // LRM §6.2.2: `wire y = a;` adalah driver kontinu, bukan nilai awal.
    let src = "module m(input [7:0] a, output [7:0] y); \
               wire [7:0] w = a; assign y = w; endmodule";
    let design = build(src);
    assert!(
        design.processes.len() >= 2,
        "net declaration assign harus jadi proses kontinu terpisah, dapat {}",
        design.processes.len()
    );
}

// --- BUG-26: loop `while` ---

#[test]
fn while_menjadi_statement_ir() {
    // LRM §12.7.2: `while (cond) body` jadi loop dengan kondisi.
    let src = "module m(output [7:0] y); logic [3:0] i; always_comb begin \
               i = 4'd0; while (i < 4) i = i + 1; y = 8'd1; end endmodule";
    let design = build(src);
    let has_while = design.processes.iter().any(|p| {
        p.body
            .iter()
            .any(|s| matches!(s, sv_ir::Statement::While { .. }))
    });
    assert!(has_while, "harus ada statement While");
}

#[test]
fn while_kondisi_ikut_sensitivitas() {
    // Sinyal yang hanya dibaca pada kondisi while tetap jadi sumber trigger.
    let src = "module m(input [3:0] batas, output [7:0] y); always_comb begin \
               logic [3:0] i; i = 0; while (i < batas) i = i + 1; y = i; end endmodule";
    let design = build(src);
    let batas_id = design.find_variable("batas").expect("sinyal ada").signal_id;
    assert!(
        design.processes[0]
            .sensitivity
            .iter()
            .any(|item| item.signal == batas_id),
        "sinyal pada kondisi while harus masuk sensitivitas"
    );
}

#[test]
fn while_body_kosong_diterima() {
    let src = "module m(output [7:0] y); logic [3:0] i; always_comb begin \
               while (i < 4); y = 8'd1; end endmodule";
    let design = build(src);
    assert!(
        design.processes.iter().any(|p| p
            .body
            .iter()
            .any(|s| matches!(s, sv_ir::Statement::While { .. }))),
        "while kosong harus tetap diurai"
    );
}

#[test]
fn while_dalam_instans_ter_elaborasi() {
    let src = "module anak(input [3:0] n, output [3:0] y); always_comb begin \
               logic [3:0] i; i = 0; while (i < n) i = i + 1; y = i; end endmodule \
               module top(input [3:0] n, output [3:0] z); \
               anak u(.n(n), .y(z)); endmodule";
    let design = build_top(src, "top");
    assert!(
        design.processes.iter().any(|p| p
            .body
            .iter()
            .any(|s| matches!(s, sv_ir::Statement::While { .. }))),
        "while di module anak harus tetap lowering"
    );
}

// --- BUG-27: `a++` dan `a--` sebagai statement ---

#[test]
fn increment_statement_menjadi_compound_assign() {
    // `a++;` diurai menjadi `a = a + 1` karena nilai hasilnya dibuang.
    let src = "module m(output [7:0] y); logic [3:0] a; always_comb begin \
               a = 4'd1; a++; y = a; end endmodule";
    let design = build(src);
    let ada_tambah = design.processes.iter().any(|p| {
        p.body.iter().any(|s| match s {
            sv_ir::Statement::Assign { assignment, .. } => {
                matches!(
                    assignment.value,
                    sv_ir::Expr::Bin {
                        op: sv_ir::BinOp::Add,
                        ..
                    }
                )
            }
            _ => false,
        })
    });
    assert!(
        ada_tambah,
        "increment harus jadi assignment dengan operasi Add"
    );
}

#[test]
fn decrement_statement_menjadi_compound_kurang() {
    let src = "module m(output [7:0] y); logic [3:0] a; always_comb begin \
               a = 4'd5; a--; y = a; end endmodule";
    let design = build(src);
    let ada_kurang = design.processes.iter().any(|p| {
        p.body.iter().any(|s| match s {
            sv_ir::Statement::Assign { assignment, .. } => {
                matches!(
                    assignment.value,
                    sv_ir::Expr::Bin {
                        op: sv_ir::BinOp::Sub,
                        ..
                    }
                )
            }
            _ => false,
        })
    });
    assert!(
        ada_kurang,
        "decrement harus jadi assignment dengan operasi Sub"
    );
}

#[test]
fn increment_bertumpuk_dalam_blok() {
    // Tiga `i++;` berturut-turut harus terdaftar sebagai tiga assignment.
    let src = "module m(output [7:0] y); logic [3:0] i; always_comb begin \
               i = 4'd0; i++; i++; i++; y = i; end endmodule";
    let design = build(src);
    let jumlah = design.processes[0]
        .body
        .iter()
        .filter(|s| {
            matches!(s, sv_ir::Statement::Assign { assignment, .. }
                if matches!(assignment.value, sv_ir::Expr::Bin { op: sv_ir::BinOp::Add, .. }))
        })
        .count();
    assert_eq!(jumlah, 3, "harus ada tiga assignment increment");
}

#[test]
fn increment_di_always_ff_diterima() {
    let src = "module m(input clk, output reg [3:0] c); \
               always_ff @(posedge clk) begin c++; end endmodule";
    let design = build(src);
    assert_eq!(design.processes.len(), 1);
}

// --- BUG-28: loop `repeat` ---

#[test]
fn repeat_menjadi_statement_ir() {
    // LRM §12.8.1: `repeat (n) body` jadi loop dengan hitungan.
    let src = "module m(output [7:0] y); integer n; always_comb begin \
               n = 3; repeat (n) y = y + 1; end endmodule";
    let design = build(src);
    let ada = design.processes.iter().any(|p| {
        p.body
            .iter()
            .any(|s| matches!(s, sv_ir::Statement::Repeat { .. }))
    });
    assert!(ada, "harus ada statement Repeat");
}

#[test]
fn repeat_hitungannya_ikut_sensitivitas() {
    // Sinyal yang hanya dipakai sebagai hitungan tetap jadi sumber trigger.
    let src = "module m(input integer n, output [7:0] y); always_comb begin \
               y = 0; repeat (n) y = y + 1; end endmodule";
    let design = build(src);
    let n_id = design.find_variable("n").expect("sinyal ada").signal_id;
    assert!(
        design.processes[0]
            .sensitivity
            .iter()
            .any(|item| item.signal == n_id),
        "hitungan repeat harus masuk sensitivitas"
    );
}

#[test]
fn repeat_body_kosong_diterima() {
    let src = "module m(output [7:0] y); always_comb begin repeat (2); y = 8'd1; end endmodule";
    let design = build(src);
    assert!(design.processes.iter().any(|p| p
        .body
        .iter()
        .any(|s| matches!(s, sv_ir::Statement::Repeat { .. }))));
}

// --- BUG-29: pre-increment `++a;` sebagai statement ---

#[test]
fn pre_increment_statement_didukung() {
    let src = "module m(output [7:0] y); logic [3:0] a; always_comb begin \
               a = 4'd1; ++a; y = a; end endmodule";
    let design = build(src);
    let ada_tambah = design.processes.iter().any(|p| {
        p.body.iter().any(|s| match s {
            sv_ir::Statement::Assign { assignment, .. } => {
                matches!(
                    assignment.value,
                    sv_ir::Expr::Bin {
                        op: sv_ir::BinOp::Add,
                        ..
                    }
                )
            }
            _ => false,
        })
    });
    assert!(ada_tambah, "++a; harus jadi assignment Add");
}

#[test]
fn pre_decrement_statement_didukung() {
    let src = "module m(output [7:0] y); logic [3:0] a; always_comb begin \
               a = 4'd5; --a; y = a; end endmodule";
    let design = build(src);
    let ada_kurang = design.processes.iter().any(|p| {
        p.body.iter().any(|s| match s {
            sv_ir::Statement::Assign { assignment, .. } => {
                matches!(
                    assignment.value,
                    sv_ir::Expr::Bin {
                        op: sv_ir::BinOp::Sub,
                        ..
                    }
                )
            }
            _ => false,
        })
    });
    assert!(ada_kurang, "--a; harus jadi assignment Sub");
}

#[test]
fn pre_increment_di_always_ff_didukung() {
    let src = "module m(input clk, output reg [3:0] c); \
               always_ff @(posedge clk) begin ++c; end endmodule";
    assert_eq!(build(src).processes.len(), 1);
}

/// BUG: `case`/`casez`/`casex` membandingkan selektor lewat `to_u64()`, yang
/// hanya melihat 64 bit LSB. Pada selektor lebih dari 64 bit semua bit di atas
/// 63 hilang dari perbandingan sehingga cabang yang salah ikut diambil
/// (`casez (sel) 128'b0` cocok padahal bit 64 = 1). Sekarang ditolak saat
/// elaborasi dengan pesan yang menyebut lebarnya.
#[test]
fn selektor_case_lebar_di_atas_64_bit_ditolak() {
    let tokens = sv_lexer::lex(
        "module m; logic [127:0] sel; integer r; \
         initial begin sel = 0; casez (sel) 128'b0: r = 1; default: r = 2; endcase end endmodule",
    )
    .expect("lex");
    let ast = sv_parser::parse_module(&tokens).expect("parse");
    let e = sv_elaborator::elaborate(&ast).expect_err("lebar > 64 harus ditolak");
    assert!(
        e.message.contains("128 bit") && e.message.contains("64"),
        "pesan harus menyebut batas lebar: {}",
        e.message
    );
}

/// BUG: label `case` non-konstan (`case (sel) sig: ...`) tidak bisa
/// dibandingkan saat codegen. Dulu `label_value` mengembalikan `0` sehingga
/// label itu dianggap `0` (untuk `case`) atau lengannya dibuang sama sekali
/// (untuk `casez`) — cabang yang salah dipilih tanpa pesan.
#[test]
fn label_case_non_konstan_ditolak() {
    for (src, nama) in [
        (
            "module m; logic [7:0] a, s; integer r; \
             initial begin a = 0; s = 0; case (s) a: r = 1; default: r = 2; endcase end endmodule",
            "case",
        ),
        (
            "module m; logic [7:0] a, s; integer r; \
             initial begin a = 0; s = 0; casez (s) a: r = 1; default: r = 2; endcase end endmodule",
            "casez",
        ),
        (
            "module m; logic [7:0] a, s; integer r; \
             initial begin a = 0; s = 0; casex (s) a: r = 1; default: r = 2; endcase end endmodule",
            "casex",
        ),
    ] {
        let tokens = sv_lexer::lex(src).expect("lex");
        let ast = sv_parser::parse_module(&tokens).expect("parse");
        let e = sv_elaborator::elaborate(&ast).expect_err("label non-konstan harus ditolak");
        assert!(
            e.message.contains("harus konstanta"),
            "{nama}: pesan: {}",
            e.message
        );
    }
}

/// Cast konstan pada label tetap sah setelah penjagaan label non-konstan
/// ditambahkan.
#[test]
fn label_case_dengan_cast_konstan_tetap_diterima() {
    let tokens = sv_lexer::lex(
        "module m; logic [15:0] s; logic y; \
         always_comb casez (s) 16'(16'b0000_0000_1?10_0000): y = 1; default: y = 0; endcase endmodule",
    )
    .expect("lex");
    let ast = sv_parser::parse_module(&tokens).expect("parse");
    sv_elaborator::elaborate(&ast).expect("cast konstan pada label harus sah");
}
