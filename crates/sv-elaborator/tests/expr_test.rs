// Tanggung jawab: integration test semantik ekspresi hasil elaborasi ke IR.
mod support;

use support::*;
use sv_ir::{Expr, UnOp};

// --- BUG-1: sized literal ---

#[test]
fn sized_literal_heksa_memakai_lebar_eksplisit() {
    let design = build("module m(output [7:0] y); assign y = 8'hFF; endmodule");
    match nilai_rhs(&design) {
        Expr::Const {
            value,
            data_type,
            unknown_mask,
            zmask: _,
        } => {
            assert_eq!(value, 0xFF);
            assert_eq!(data_type.width, 8);
            assert_eq!(unknown_mask, 0);
        }
        other => panic!("expected const, dapat {other:?}"),
    }
}

#[test]
fn sized_literal_biner_dengan_separator() {
    let design = build("module m(output [7:0] y); assign y = 8'b1010_1010; endmodule");
    assert_eq!(evaluasi(&nilai_rhs(&design), &mut vec![0; 1]), 0xAA);
}

#[test]
fn sized_literal_desimal_berukuran() {
    let design = build("module m(output [7:0] y); assign y = 8'd200; endmodule");
    assert_eq!(evaluasi(&nilai_rhs(&design), &mut vec![0; 1]), 200);
}

#[test]
fn sized_literal_oktal_tanpa_ukuran_lebar_32() {
    let design = build("module m(output [7:0] y); assign y = 'o17; endmodule");
    match nilai_rhs_mentah(&design) {
        Expr::Const {
            value,
            data_type,
            unknown_mask,
            zmask: _,
        } => {
            assert_eq!(value, 15);
            // LRM §5.7.1: literal tanpa ukuran lebarnya 32 bit.
            assert_eq!(data_type.width, 32);
            assert_eq!(unknown_mask, 0);
        }
        other => panic!("expected const, dapat {other:?}"),
    }
}

#[test]
fn sized_literal_bertanda_menandai_tipe() {
    let design = build("module m(output [7:0] y); assign y = 8'shFF; endmodule");
    match nilai_rhs(&design) {
        Expr::Const { data_type, .. } => assert!(data_type.signed),
        other => panic!("expected const, dapat {other:?}"),
    }
}

#[test]
fn sized_literal_dipotong_ke_lebar() {
    // 4'b11111 dipotong ke 4 bit -> 0b1111.
    let design = build("module m(output [3:0] y); assign y = 4'b11111; endmodule");
    assert_eq!(evaluasi(&nilai_rhs(&design), &mut vec![0; 1]), 0b1111);
}

#[test]
fn sized_literal_dipakai_sebagai_case_label() {
    let source = "module m(input [3:0] a, output [3:0] y); \
                  always_comb case (a) 8'b0001: y = 4'd7; default: y = 4'd0; endcase endmodule";
    let design = build(source);
    match &design.processes[0].body[0] {
        sv_ir::Statement::Case { arms, .. } => {
            assert_eq!(arms.len(), 2);
            match &arms[0].labels[0] {
                Expr::Const {
                    value,
                    data_type,
                    unknown_mask,
                    zmask,
                } => {
                    assert_eq!(*value, 1);
                    assert_eq!(data_type.width, 8);
                    assert_eq!(*unknown_mask, 0, "label biasa tanpa wildcard");
                    assert_eq!(*zmask, 0, "label biasa tanpa `z`");
                }
                other => panic!("expected const label, dapat {other:?}"),
            }
        }
        other => panic!("expected case, dapat {other:?}"),
    }
}

#[test]
fn literal_desimal_polos_memakai_lebar_32() {
    // LRM §5.7.1: integer literal tak berspesifikasi berbasis desimal = 32 bit.
    //
    // `nilai_rhs_mentah` dipakai karena batas assignment akan sized ke lebar
    // target (8 bit di sini) dan membungkus literal dalam `Cast`.
    let design = build("module m(output [7:0] y); assign y = 42; endmodule");
    match nilai_rhs_mentah(&design) {
        Expr::Const { data_type, .. } => assert_eq!(data_type.width, 32),
        other => panic!("expected const, dapat {other:?}"),
    }
}

// --- BUG-3: unary operator ---

#[test]
fn bitwise_not_membalik_bit_dalam_lebar() {
    let nilai = dengan_sinyal(
        "module m(input [3:0] a, output [3:0] y); assign y = ~a; endmodule",
        &[("a", 0b1010)],
    );
    assert_eq!(nilai, 0b0101);
}

#[test]
fn bitwise_not_tidak_membocorkan_bit_atas_lebar_4() {
    // 4'b0000 -> 4'b1111. Bila bocor jadi 0xffff_fff0.
    let nilai = dengan_sinyal(
        "module m(input [3:0] a, output [3:0] y); assign y = ~a; endmodule",
        &[("a", 0)],
    );
    assert_eq!(nilai, 0b1111);
}

#[test]
fn unary_minus_menghasilkan_dua_s_complement() {
    let nilai = dengan_sinyal(
        "module m(input [3:0] a, output [3:0] y); assign y = -a; endmodule",
        &[("a", 1)],
    );
    // 4-bit: 16 - 1 = 15 = 0b1111.
    assert_eq!(nilai, 0b1111);
}

#[test]
fn unary_minus_pada_lima_menghasilkan_251() {
    let nilai = dengan_sinyal(
        "module m(input [7:0] a, output [7:0] y); assign y = -a; endmodule",
        &[("a", 5)],
    );
    assert_eq!(nilai, 251);
}

#[test]
fn logical_not_menghasilkan_bool_satu_bit() {
    let design = build("module m(input a, output y); assign y = !a; endmodule");
    match nilai_rhs(&design) {
        Expr::Un { op, data_type, .. } => {
            assert_eq!(op, UnOp::LogNot);
            assert_eq!(data_type.width, 1);
        }
        other => panic!("expected unary, dapat {other:?}"),
    }
}

// --- BUG-4: reduction operator ---

#[test]
fn reduksi_and_benar_bila_semua_bit_satu() {
    let src = "module m(input [3:0] a, output y); assign y = &a; endmodule";
    assert_eq!(dengan_sinyal(src, &[("a", 0b1111)]), 1);
    assert_eq!(dengan_sinyal(src, &[("a", 0b1110)]), 0);
}

#[test]
fn reduksi_or_benar_bila_ada_bit_satu() {
    let src = "module m(input [3:0] a, output y); assign y = |a; endmodule";
    assert_eq!(dengan_sinyal(src, &[("a", 0b0000)]), 0);
    assert_eq!(dengan_sinyal(src, &[("a", 0b1000)]), 1);
}

#[test]
fn reduksi_xor_menghitung_paritas() {
    let src = "module m(input [3:0] a, output y); assign y = ^a; endmodule";
    // Dua bit satu => genap => 0. Tiga bit satu => ganjil => 1.
    assert_eq!(dengan_sinyal(src, &[("a", 0b0011)]), 0);
    assert_eq!(dengan_sinyal(src, &[("a", 0b0111)]), 1);
}

#[test]
fn reduksi_nand_membalik_reduksi_and() {
    let src = "module m(input [3:0] a, output y); assign y = ~&a; endmodule";
    assert_eq!(dengan_sinyal(src, &[("a", 0b1111)]), 0);
    assert_eq!(dengan_sinyal(src, &[("a", 0b1010)]), 1);
}

#[test]
fn reduksi_nor_membalik_reduksi_or() {
    let src = "module m(input [3:0] a, output y); assign y = ~|a; endmodule";
    assert_eq!(dengan_sinyal(src, &[("a", 0b0000)]), 1);
    assert_eq!(dengan_sinyal(src, &[("a", 0b0001)]), 0);
}

#[test]
fn reduksi_xnor_membalik_reduksi_xor() {
    let src = "module m(input [3:0] a, output y); assign y = ~^a; endmodule";
    assert_eq!(dengan_sinyal(src, &[("a", 0b0011)]), 1);
    assert_eq!(dengan_sinyal(src, &[("a", 0b0111)]), 0);
}

#[test]
fn reduksi_selalu_hasil_satu_bit() {
    let design = build("module m(input [7:0] a, output y); assign y = &a; endmodule");
    match nilai_rhs(&design) {
        Expr::Un { op, data_type, .. } => {
            assert_eq!(op, UnOp::RedAnd);
            assert_eq!(data_type.width, 1);
        }
        other => panic!("expected unary, dapat {other:?}"),
    }
}

// --- BUG-5: ternary ---

#[test]
fn ternary_memilih_cabang_benar() {
    let src = "module m(input s, output [7:0] y); assign y = s ? 8'hFF : 8'h00; endmodule";
    assert_eq!(dengan_sinyal(src, &[("s", 1)]), 0xFF);
    assert_eq!(dengan_sinyal(src, &[("s", 0)]), 0x00);
}

#[test]
fn ternary_lebar_mengambil_cabang_terlebar() {
    let src = "module m(input s, output [31:0] y); assign y = s ? 8'hFF : 32'h1; endmodule";
    let design = build(src);
    match nilai_rhs(&design) {
        Expr::Ternary { data_type, .. } => assert_eq!(data_type.width, 32),
        other => panic!("expected ternary, dapat {other:?}"),
    }
}

#[test]
fn ternary_nested_memilih_cabang_yang_benar() {
    let src = "module m(input a, input b, output [7:0] y); \
               assign y = a ? (b ? 8'd1 : 8'd2) : 8'd3; endmodule";
    assert_eq!(dengan_sinyal(src, &[("a", 1), ("b", 1)]), 1);
    assert_eq!(dengan_sinyal(src, &[("a", 1), ("b", 0)]), 2);
    assert_eq!(dengan_sinyal(src, &[("a", 0), ("b", 1)]), 3);
}

#[test]
fn ternary_membaca_seluruh_sinyal_kedua_cabang() {
    // Kedua cabang harus ikut membentuk sensitivity, bukan hanya kondisi.
    let src = "module m(input s, input [7:0] a, input [7:0] b, output [7:0] y); \
               assign y = s ? a : b; endmodule";
    let design = build(src);
    assert_eq!(design.processes[0].sensitivity.len(), 3);
}

// --- BUG-6: select ---

#[test]
fn bit_select_mengambil_satu_bit() {
    let nilai = dengan_sinyal(
        "module m(input [7:0] a, output y); assign y = a[0]; endmodule",
        &[("a", 0b1010_1010)],
    );
    assert_eq!(nilai, 0);
    let nilai = dengan_sinyal(
        "module m(input [7:0] a, output y); assign y = a[1]; endmodule",
        &[("a", 0b1010_1010)],
    );
    assert_eq!(nilai, 1);
}

#[test]
fn part_select_menggeser_dan_meng_mask() {
    let nilai = dengan_sinyal(
        "module m(input [7:0] a, output [3:0] y); assign y = a[7:4]; endmodule",
        &[("a", 0b1010_1100)],
    );
    assert_eq!(nilai, 0b1010);
}

#[test]
fn part_select_lebar_tersimpan_benar() {
    let design = build("module m(input [7:0] a, output [3:0] y); assign y = a[7:4]; endmodule");
    match nilai_rhs(&design) {
        Expr::Select {
            msb,
            lsb,
            data_type,
            ..
        } => {
            assert_eq!((msb, lsb), (7, 4));
            assert_eq!(data_type.width, 4);
        }
        other => panic!("expected select, dapat {other:?}"),
    }
}

#[test]
fn bit_select_lebar_satu_bit() {
    let design = build("module m(input [7:0] a, output [3:0] y); assign y = a[2]; endmodule");
    match nilai_rhs_mentah(&design) {
        Expr::Select {
            msb,
            lsb,
            data_type,
            ..
        } => {
            assert_eq!((msb, lsb), (2, 2));
            assert_eq!(data_type.width, 1);
        }
        other => panic!("expected select, dapat {other:?}"),
    }
}

#[test]
fn part_select_tidak_tertutup_ditolak() {
    let tokens =
        sv_lexer::lex("module m(input [7:0] a, output [3:0] y); assign y = a[3:0; endmodule")
            .expect("lex");
    let parsed = sv_parser::parse_module(&tokens);
    assert!(parsed.is_err(), "part-select tanpa ']' harus ditolak");
}

// --- kombinasi ---

#[test]
fn kombinasi_semua_operator_bersama() {
    let src = "module m(input [3:0] a, input s, output [3:0] y, output p, output q); \
               assign y = s ? ~a : -a; \
               assign p = &a; \
               assign q = ^a; endmodule";
    let tokens = sv_lexer::lex(src).expect("lex");
    let ast = sv_parser::parse_module(&tokens).expect("parse");
    let design = sv_elaborator::elaborate(&ast).expect("elaborate");
    assert_eq!(design.processes.len(), 3);
}

#[test]
fn sensitivitas_mencakup_sinyal_dalam_select_dan_ternary() {
    let src = "module m(input s, input [7:0] a, input [7:0] b, output [7:0] y); \
               assign y = s ? a[3:0] : b[3:0]; endmodule";
    let design = build(src);
    // s, a, b semuanya terbaca.
    assert_eq!(design.processes[0].sensitivity.len(), 3);
}

// --- BUG-12: bit/part-select pada LHS assignment ---

/// Ambil assignment pertama dari proses pertama.
fn assignment_rhs(design: &sv_ir::Design) -> sv_ir::process::Assignment {
    match &design.processes[0].body[0] {
        sv_ir::Statement::Assign { assignment, .. } => assignment.clone(),
        other => panic!("expected assignment, dapat {other:?}"),
    }
}

#[test]
fn lvalue_penuh_tanpa_irisan() {
    let design = build("module m(input [7:0] a, output [7:0] y); assign y = a; endmodule");
    assert_eq!(assignment_rhs(&design).slice, None);
}

#[test]
fn part_select_lhs_membawa_irisan() {
    let design =
        build("module m(input [7:0] a, output [7:0] y); assign y[7:4] = a[3:0]; endmodule");
    let assignment = assignment_rhs(&design);
    let slice = assignment.slice.expect("harus ada irisan");
    assert_eq!((slice.msb, slice.lsb), (7, 4));
    assert_eq!(assignment.target, 1, "target adalah sinyal y");
}

#[test]
fn bit_select_lhs_membawa_irisan_satu_bit() {
    let design = build("module m(input [7:0] a, output [7:0] y); assign y[0] = a[7]; endmodule");
    let slice = assignment_rhs(&design).slice.expect("harus ada irisan");
    assert_eq!((slice.msb, slice.lsb), (0, 0));
    assert_eq!(slice.width(), 1);
}

#[test]
fn bit_select_lhs_di_always_comb() {
    let src = "module m(input [3:0] a, output [3:0] y); always_comb y[1] = a[0]; endmodule";
    let slice = assignment_rhs(&build(src)).slice.expect("harus ada irisan");
    assert_eq!((slice.msb, slice.lsb), (1, 1));
}

#[test]
fn bit_select_lhs_di_always_ff_menggunakan_nba_masked() {
    let src = "module m(input clk, input [3:0] d, output [3:0] q); \
               always_ff @(posedge clk) q[1:0] <= d[1:0]; endmodule";
    let design = build(src);
    let assignment = assignment_rhs(&design);
    assert_eq!(assignment.style, sv_ir::AssignStyle::NonBlocking);
    let slice = assignment.slice.expect("harus ada irisan");
    assert_eq!(slice.mask_range(), (0, 2));
}

#[test]
fn irisan_melebihi_lebar_sinyal_ditolak() {
    let tokens =
        sv_lexer::lex("module m(input [3:0] a, output [3:0] y); assign y[7:4] = a; endmodule")
            .expect("lex");
    let ast = sv_parser::parse_module(&tokens).expect("parse");
    let result = sv_elaborator::elaborate(&ast);
    assert!(result.is_err(), "irisan di luar lebar sinyal harus ditolak");
    assert!(result.err().unwrap().message.contains("lebar"));
}

#[test]
fn nama_process_menggunakan_nama_sinyal_bukan_lvalue() {
    let design =
        build("module m(input [7:0] a, output [7:0] y); assign y[7:4] = a[3:0]; endmodule");
    assert_eq!(design.processes[0].name, "assign_y");
}

#[test]
fn irisan_lhs_tidak_masuk_sensitivitas() {
    // Target bukan sumber, jadi sensitivity hanya berisi sinyal yang dibaca.
    let src = "module m(input [7:0] a, output [7:0] y); assign y[3:0] = a[3:0]; endmodule";
    let design = build(src);
    assert_eq!(design.processes[0].sensitivity.len(), 1);
}

// --- BUG-13: qualifier signed pada port dan deklarasi ---

#[test]
fn port_signed_ditandai_di_ir() {
    let design = build("module m(input signed [3:0] a, output [3:0] y); assign y = a; endmodule");
    let a = design.find_variable("a").expect("a");
    assert!(
        a.data_type.signed,
        "port `signed` harus menghasilkan tipe bertanda"
    );
    assert_eq!(a.data_type.width, 4);
}

#[test]
fn port_tanpa_signed_menghasilkan_unsigned() {
    // Default LRM §6.2.1 adalah unsigned.
    let design = build("module m(input [3:0] a, output [3:0] y); assign y = a; endmodule");
    assert!(!design.find_variable("a").unwrap().data_type.signed);
}

#[test]
fn port_unsigned_eksplisit_tetap_unsigned() {
    let design = build("module m(input unsigned [7:0] a, output [7:0] y); assign y = a; endmodule");
    assert!(!design.find_variable("a").unwrap().data_type.signed);
}

#[test]
fn deklarasi_signed_ditandai_di_ir() {
    let design = build("module m(output [3:0] y); logic signed [7:0] t; assign y = t; endmodule");
    let t = design.find_variable("t").expect("t");
    assert!(t.data_type.signed);
    assert_eq!(t.data_type.width, 8);
}

#[test]
fn signed_dan_lebar_bersamaan_terjaga() {
    let design = build("module m(output [3:0] y); logic signed [15:0] t; assign y = t; endmodule");
    let t = design.find_variable("t").unwrap();
    assert!(t.data_type.signed, "signedness harus bertahan");
    assert_eq!(t.data_type.width, 16, "lebar harus 16, bukan ikut jadi 4");
}

#[test]
fn signed_pada_port_instansi_kid() {
    let design = build_top(
        "module sub #(parameter W = 8) (input signed [W-1:0] a, output [W-1:0] y); assign y = a; endmodule \
         module top(input signed [7:0] x, output [7:0] y); \
         sub u0 (.a(x), .y(y)); \
         endmodule",
        "top",
    );
    let x = design.find_variable("x").expect("x");
    assert!(x.data_type.signed);
    assert_eq!(x.data_type.width, 8);
}

#[test]
fn signed_memperluas_lebar_saat_infer_binary() {
    // signed + unsigned = unsigned, lebar tetap max kedua operand.
    let design = build(
        "module m(input signed [7:0] a, input [7:0] b, output [7:0] y); assign y = a + b; endmodule",
    );
    match nilai_rhs(&design) {
        Expr::Bin { data_type, .. } => {
            assert_eq!(data_type.width, 8);
            assert!(!data_type.signed, "campuran signed/unsigned jadi unsigned");
        }
        other => panic!("expected binary, dapat {other:?}"),
    }
}

// --- BUG-14: concatenation dan replication ---

#[test]
fn concat_menjumlahkan_lebar_seluruh_item() {
    // LRM §11.8.1: 4 bit + 4 bit = 8 bit.
    let design = build(
        "module m(input [3:0] a, input [3:0] b, output [7:0] y); assign y = {a, b}; endmodule",
    );
    match nilai_rhs(&design) {
        Expr::Concat { items, data_type } => {
            assert_eq!(items.len(), 2);
            assert_eq!(data_type.width, 8);
        }
        other => panic!("expected concat, dapat {other:?}"),
    }
}

#[test]
fn concat_menempatkan_item_pertama_di_msb() {
    // {4'hA, 4'h3} = 8'hA3, bukan 8'h3A.
    let nilai = dengan_sinyal(
        "module m(output [7:0] y); assign y = {4'hA, 4'h3}; endmodule",
        &[],
    );
    assert_eq!(nilai, 0xA3);
}

#[test]
fn concat_tiga_item_berurutan_dari_msb() {
    // {2'h1, 2'h2, 2'h3} = 6'b01_10_11.
    let nilai = dengan_sinyal(
        "module m(output [5:0] y); assign y = {2'h1, 2'h2, 2'h3}; endmodule",
        &[],
    );
    assert_eq!(nilai, 0b01_10_11);
}

#[test]
fn concat_dari_sinyal_mengambil_msb_kiri() {
    let nilai = dengan_sinyal(
        "module m(input [3:0] a, input [3:0] b, output [7:0] y); assign y = {a, b}; endmodule",
        &[("a", 0xA), ("b", 0x3)],
    );
    assert_eq!(nilai, 0xA3);
}

#[test]
fn concat_tidak_membocorkan_bit_lebar_item() {
    // Operan hanya 4 bit; bit di atasnya tidak boleh masuk ke hasil.
    let nilai = dengan_sinyal(
        "module m(input [7:0] b, output [3:0] y); assign y = {2'h0, b[1:0]}; endmodule",
        &[("b", 0xFF)],
    );
    assert_eq!(nilai, 0b11);
}

#[test]
fn concat_satu_item_menjadi_signal_biasa() {
    // `{a}` tidak mengubah lebar maupun nilai.
    let design = build("module m(input [7:0] a, output [7:0] y); assign y = {a}; endmodule");
    match nilai_rhs(&design) {
        Expr::SignalRef { .. } => {}
        other => panic!("expected signal ref, dapat {other:?}"),
    }
}

#[test]
fn concat_membaca_seluruh_sinyal_untuk_sensitivitas() {
    let src = "module m(input [3:0] a, input [3:0] b, output [7:0] y); \
               assign y = {a, b}; endmodule";
    assert_eq!(build(src).processes[0].sensitivity.len(), 2);
}

#[test]
fn replication_mengulang_operand() {
    // LRM §11.8.2: {4{2'b10}} = 8'b1010_1010 = 0xAA.
    let nilai = dengan_sinyal(
        "module m(output [7:0] y); assign y = {4{2'b10}}; endmodule",
        &[],
    );
    assert_eq!(nilai, 0xAA);
}

#[test]
fn replication_lebar_hasil_kali_operand() {
    // 4 x 2 bit = 8 bit.
    let design = build("module m(output [7:0] y); assign y = {4{2'b01}}; endmodule");
    match nilai_rhs(&design) {
        Expr::Replicate {
            count, data_type, ..
        } => {
            assert_eq!(count, 4);
            assert_eq!(data_type.width, 8);
        }
        other => panic!("expected replicate, dapat {other:?}"),
    }
}

#[test]
fn replication_dari_sinyal() {
    let nilai = dengan_sinyal(
        "module m(input [1:0] a, output [7:0] y); assign y = {4{a}}; endmodule",
        &[("a", 0b10)],
    );
    assert_eq!(nilai, 0b10_10_10_10);
}

#[test]
fn replication_lapis_tidak_diterima() {
    // LRM §11.8.2 tidak mengizinkan replication di dalam replication;
    // iverilog juga menolak dengan `syntax error`.
    let tokens =
        sv_lexer::lex("module m(input [1:0] a, output [7:0] y); assign y = {2{2{a}}}; endmodule")
            .expect("lex");
    let parsed = sv_parser::parse_module(&tokens);
    assert!(
        parsed.is_err(),
        "replication di dalam replication harus ditolak"
    );
}

#[test]
fn concat_bersamaan_dengan_select_pada_lhs() {
    let src = "module m(input [3:0] a, input [3:0] b, output [7:0] y); \
               assign y[7:4] = a; assign y[3:0] = b; endmodule";
    let design = build(src);
    assert_eq!(design.processes.len(), 2);
}
