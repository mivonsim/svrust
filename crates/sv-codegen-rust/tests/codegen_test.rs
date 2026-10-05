// Tanggung jawab: integration test codegen IR design menjadi Rust source.
use sv_codegen_rust::generate_module;

fn generate(source: &str) -> String {
    let tokens = sv_lexer::lex(source).expect("lex");
    let ast = sv_parser::parse_module(&tokens).expect("parse");
    let mut design = sv_elaborator::elaborate(&ast).expect("elaborate");
    // Optimisasi wajib ikut karena pipeline sungguhan memanggilnya: label
    // `casez`/`casex` berupa cast baru menjadi `Expr::Const` setelah fold.
    sv_opt::optimize(&mut design);
    generate_module(&design)
}

#[test]
fn generates_struct_named_after_module() {
    let code = generate("module and_gate(input a, input b, output y); assign y = a & b; endmodule");
    assert!(code.contains("pub struct AndGate"));
    assert!(code.contains("pub fn eval_comb"));
}

#[test]
fn generates_width_typed_accessors() {
    let code = generate("module m(input [7:0] a, output [7:0] y); assign y = a; endmodule");
    assert!(code.contains("pub fn a(&self) -> Bits<8>"));
    assert!(code.contains("pub fn set_y(&mut self, value: Bits<8>)"));
}

#[test]
fn generates_sensitivity_table() {
    let code = generate("module and_gate(input a, input b, output y); assign y = a & b; endmodule");
    assert!(code.contains("sensitivity: vec!["));
    assert!(code.contains("vec![0, 1]"));
    assert!(!code.contains("diisi oleh elaborator"));
}

#[test]
fn emits_bitwise_and_for_and_operator() {
    let code = generate("module and_gate(input a, input b, output y); assign y = a & b; endmodule");
    assert!(code.contains("self.signals[2].write("));
    assert!(code.contains('&'));
}

#[test]
fn emits_shift_helpers_dengan_lebar_logis() {
    // LRM §11.4.10: operator Rust `<<` bekerja pada vektor penuh MAX_WIDTH,
    // sedangkan LRM menentukan lebar dari operand kiri. `<<<` harus tetap
    // memakai helper yang sama dengan `<<`.
    let code = generate(
        "module m(input [7:0] a, input [3:0] n, output [7:0] y); assign y = a << n; endmodule",
    );
    assert!(code.contains("geser_kiri::<8, MAX_WIDTH>"), "dapat {code}");
    let code = generate(
        "module m(input [7:0] a, input [3:0] n, output [7:0] y); assign y = a <<< n; endmodule",
    );
    assert!(code.contains("geser_kiri::<8, MAX_WIDTH>"), "dapat {code}");
}

#[test]
fn geser_aritmetik_meneruskan_flag_signed_hasil() {
    // LRM §11.4.10: bit pengisi `>>>` ditentukan signedness tipe HASIL.
    let kode_signed = generate(
        "module m(input signed [7:0] a, input [3:0] n, output signed [7:0] y); assign y = a >>> n; endmodule",
    );
    assert!(
        kode_signed.contains("geser_kanan_aritmetik::<8, MAX_WIDTH>(")
            && kode_signed.contains(", true)"),
        "dapat {kode_signed}"
    );
    let kode_unsigned = generate(
        "module m(input [7:0] a, input [3:0] n, output [7:0] y); assign y = a >>> n; endmodule",
    );
    assert!(
        kode_unsigned.contains("geser_kanan_aritmetik::<8, MAX_WIDTH>(")
            && kode_unsigned.contains(", false)"),
        "dapat {kode_unsigned}"
    );
}

#[test]
fn sequential_assignment_uses_pending_queue() {
    let code =
        generate("module dff(input clk, input d, output q); always_ff @(posedge clk) begin q <= d; end endmodule");
    assert!(code.contains("PendingWrite::new(2,"));
    assert!(code.contains("pub fn eval_seq"));
    assert!(code.contains("pub fn commit_pending"));
}

#[test]
fn case_menghasilkan_rantai_if_else() {
    let source = "module mux(input [1:0] sel, input [3:0] a, input [3:0] b, output [3:0] y); \
                 always_comb begin \
                   case (sel) \
                     0: y = a; \
                     default: y = b; \
                   endcase \
                 end endmodule";
    let code = generate(source);
    assert!(
        code.contains("sv_runtime::case_eq::<2, MAX_WIDTH>("),
        "code:\n{code}"
    );
    assert!(code.contains("} else {"), "code:\n{code}");
    assert!(code.contains(", 0x0, 0x0, 0x0)"), "code:\n{code}");
}

#[test]
fn max_width_is_largest_signal() {
    let code = generate(
        "module m(input [3:0] a, input [15:0] b, output [31:0] y); assign y = b; endmodule",
    );
    assert!(code.contains("const MAX_WIDTH: usize = 32;"));
    assert!(code.contains("pub struct M"));
}

#[test]
fn imports_runtime_types() {
    let code = generate("module m(output y); assign y = 1; endmodule");
    assert!(code.contains("use sv_runtime::Bits;"));
    assert!(code.contains("use sv_runtime::PendingWrite;"));
    assert!(code.contains("use sv_runtime::SignalCell;"));
}

#[test]
fn generates_empty_module_without_process() {
    let code = generate("module hello; endmodule");
    assert!(code.contains("pub struct Hello"));
    assert!(code.contains("pub fn eval_comb"));
    assert!(code.contains("pub fn eval_seq"));
}

#[test]
fn bool_assign_dikonversi_ke_bits() {
    let code =
        generate("module m(input a, input b, output zero); assign zero = (a == b); endmodule");
    assert!(
        code.contains("Bits::<MAX_WIDTH>::from_u64(("),
        "code:\n{code}"
    );
    assert!(code.contains(") as u64)"), "code:\n{code}");
}

#[test]
fn if_dengan_perbandingan_emit_bool_yang_dibalut_bits() {
    let code = generate(
        "module m(input [3:0] a, input [3:0] b, output [3:0] y); \
         always_comb begin \
           if (a == b) y = a; else y = b; \
         end endmodule",
    );
    assert!(code.contains(".to_u64() =="), "code:\n{code}");
    // Di posisi syarat, perbandingan tetap `bool` supaya `if` Rust bisa
    // memakainya. Di posisi nilai (`write_expr`) barulah dibungkus `Bits`.
    assert!(code.contains("if ("), "code:\n{code}");
    assert!(
        !code.contains("if (Bits::<MAX_WIDTH>::from_u64("),
        "syarat `if` jangan dibungkus Bits:\n{code}"
    );
}

#[test]
fn perbandingan_di_posisi_nilai_dibalut_bits() {
    // BUG-11: emission `bool` di posisi nilai membuat kode gagal dikompilasi
    // begitu masuk concat/cast/`$display`. `assign y = (a == b);` harus
    // menghasilkan `Bits` supaya bisa ditulis ke sinyal.
    let code = generate(
        "module m(input [3:0] a, input [3:0] b, output [3:0] y); \
         assign y = (a == b); endmodule",
    );
    assert!(
        code.contains("Bits::<MAX_WIDTH>::from_u64(("),
        "perbandingan di posisi nilai harus dibungkus Bits:\n{code}"
    );
}

// --- BUG-12: bit/part-select pada LHS assignment ---

#[test]
fn part_select_lhs_menghasilkan_masked_write() {
    let code =
        generate("module m(input [7:0] a, output [7:0] y); assign y[7:4] = a[3:0]; endmodule");
    assert!(code.contains("write_masked("), "code:\n{code}");
    // Masker dihitung saat runtime dari (lsb, lebar) — bukan literal `u64`,
    // karena irisan di atas bit 63 tidak bisa diwakili `u64`.
    assert!(
        code.contains("sv_runtime::range_mask::<MAX_WIDTH>(4, 4)"),
        "code:\n{code}"
    );
}

#[test]
fn bit_select_lhs_menghasilkan_masker_satu_bit() {
    let code = generate("module m(input [7:0] a, output [7:0] y); assign y[0] = a[7]; endmodule");
    assert!(code.contains("write_masked("), "code:\n{code}");
    assert!(
        code.contains("sv_runtime::range_mask::<MAX_WIDTH>(0, 1)"),
        "code:\n{code}"
    );
}

#[test]
fn lvalue_penuh_tetap_memakai_write_biasa() {
    // Tidak ada alur masked pada assignment bila target seluruh sinyal.
    let code = generate("module m(input [7:0] a, output [7:0] y); assign y = a; endmodule");
    assert!(
        code.contains("self.signals[1].write(_v1);"),
        "code:\n{code}"
    );
    assert!(!code.contains("write_masked(_v1"), "code:\n{code}");
}

#[test]
fn nba_dengan_irisan_memakai_pending_masked() {
    let code = generate(
        "module m(input clk, input [3:0] d, output [3:0] q); \
         always_ff @(posedge clk) q[1:0] <= d[1:0]; endmodule",
    );
    assert!(code.contains("PendingWrite::new_masked("), "code:\n{code}");
    assert!(
        code.contains("sv_runtime::range_mask::<MAX_WIDTH>(0, 2)"),
        "code:\n{code}"
    );
}

#[test]
fn nba_tanpa_irisan_tetap_memakai_pending_biasa() {
    let code = generate(
        "module m(input clk, input d, output q); always_ff @(posedge clk) q <= d; endmodule",
    );
    assert!(code.contains("PendingWrite::new("), "code:\n{code}");
    assert!(!code.contains("new_masked("), "code:\n{code}");
}

#[test]
fn commit_pending_selalu_memakai_masker() {
    let code = generate("module m(input [7:0] a, output [7:0] y); assign y = a; endmodule");
    assert!(
        code.contains("write_masked(write.value, write.mask)"),
        "code:\n{code}"
    );
}

// --- sized literal, unary, reduction, ternary, select ---

#[test]
fn sized_literal_emit_nilai_hex_asli() {
    let code = generate("module m(output [7:0] y); assign y = 8'hA5; endmodule");
    assert!(code.contains("from_u64(165)"), "code:\n{code}");
}

#[test]
fn unary_reduction_dipanggil_lewat_helper_runtime() {
    let code = generate("module m(input [3:0] a, output y); assign y = &a; endmodule");
    assert!(
        code.contains("sv_runtime::reduce_and::<4, MAX_WIDTH>("),
        "code:\n{code}"
    );
}

#[test]
fn ternary_emit_if_else() {
    let code =
        generate("module m(input s, output [7:0] y); assign y = s ? 8'hFF : 8'h00; endmodule");
    assert!(code.contains("if "), "code:\n{code}");
    assert!(code.contains("} else {"), "code:\n{code}");
}

#[test]
fn select_lhs_dan_rhs_terdukung_bersamaan() {
    let code =
        generate("module m(input [7:0] a, output [7:0] y); assign y[7:4] = a[3:0]; endmodule");
    // RHS `a[3:0]` diambil lewat helper runtime pada lebar 4 dari bit 0.
    // Helper dipakai (bukan `>> 0 & 0xf`) supaya `X`/`Z` ikut terbawa.
    assert!(
        code.contains("sv_runtime::select_kbits::<8, MAX_WIDTH>(self.signals[0].read(), 0, 4)"),
        "code:\n{code}"
    );
    // LHS `y[7:4]` memakai masker 0xf0 dan nilai digeser ke posisi bit 4,
    // karena `write_masked` hanya menyalin bit yang sudah berada di masker.
    assert!(
        code.contains("write_masked((_v1 << 4), sv_runtime::range_mask::<MAX_WIDTH>(4, 4))"),
        "code:\n{code}"
    );
}

// BUG: nilai pada LHS teriris tidak digeser ke posisi bitnya, sehingga
// `y[7:4] = a` menulis ke bit 0..3 alih-alih 4..7.
#[test]
fn nilai_lhs_teriris_digeser_ke_posisi_irisan() {
    let code = generate("module m(input [7:0] a, output [7:0] y); assign y[7:4] = a; endmodule");
    assert!(
        code.contains("write_masked((_v1 << 4), sv_runtime::range_mask::<MAX_WIDTH>(4, 4))"),
        "code:\n{code}"
    );
}

#[test]
fn irisan_di_bit_nol_tidak_perlu_pergeseran() {
    // `y[0]` sudah berada di posisi benar, jadi tidak ada `<< 0`.
    let code = generate("module m(input a, output [7:0] y); assign y[0] = a; endmodule");
    assert!(
        code.contains("write_masked(_v1, sv_runtime::range_mask::<MAX_WIDTH>(0, 1))"),
        "code:\n{code}"
    );
    assert!(!code.contains("<< 0)"), "code:\n{code}");
}

#[test]
fn nba_teriris_juga_digeser_ke_posisi_irisan() {
    let code = generate(
        "module m(input clk, input d, output reg [7:0] q); \
         always_ff @(posedge clk) q[7:4] <= d; endmodule",
    );
    assert!(
        code.contains("<< 4), sv_runtime::range_mask::<MAX_WIDTH>(4, 4)"),
        "code:\n{code}"
    );
}

#[test]
fn port_dengan_qualifier_tipe_tetap_ter_elaborate() {
    let code =
        generate("module m(input logic [3:0] a, output reg [3:0] y); assign y = a; endmodule");
    assert!(code.contains("pub fn a(&self) -> Bits<4>"), "code:\n{code}");
}

// --- BUG-14: concatenation dan replication ---

// BUG: concat dan replicate sebelumnya dirakit lewat `to_u64()` plus
// penggeseran/pengalian integer, yang membuang digit `X`/`Z`. Sekarang keduanya
// memakai helper runtime yang bekerja pada digit, sehingga `X`/`Z` ikut
// terbawa ke posisi yang benar.

#[test]
fn concat_memakai_helper_yang_mempertahankan_xz() {
    let code = generate(
        "module m(input [3:0] a, input [3:0] b, output [7:0] y); assign y = {a, b}; endmodule",
    );
    assert!(
        code.contains("sv_runtime::concat_kbits::<MAX_WIDTH>("),
        "code:\n{code}"
    );
    // Lebar tiap item diteruskan eksplisit; helper butuh tahu posisi tiap item.
    assert!(code.contains("], &[4, 4])"), "code:\n{code}");
    assert!(!code.contains(".to_u64() << 4"), "code:\n{code}");
}

#[test]
fn concat_tiga_item_meneruskan_lebar_semua_item() {
    let code = generate(
        "module m(input [1:0] a, input [1:0] b, input [1:0] c, output [5:0] y); \
         assign y = {a, b, c}; endmodule",
    );
    assert!(code.contains("], &[2, 2, 2])"), "code:\n{code}");
}

#[test]
fn replication_mengulang_operand_sebanyak_count_kali() {
    let code = generate("module m(output [7:0] y); assign y = {4{2'b10}}; endmodule");
    // 4 salinan, masing-masing 2 bit.
    let baris = code
        .lines()
        .find(|l| l.contains("concat_kbits"))
        .expect("baris assignment");
    assert!(baris.contains("], &[2, 2, 2, 2,"), "baris:\n{baris}");
    assert_eq!(
        baris.matches("from_u64(2)").count(),
        4,
        "operand harus diulang 4 kali:\n{baris}"
    );
}

#[test]
fn replication_satu_bit_mengulang_delapan_kali() {
    let code = generate("module m(output [7:0] y); assign y = {8{1'b1}}; endmodule");
    let baris = code
        .lines()
        .find(|l| l.contains("concat_kbits"))
        .expect("baris assignment");
    assert_eq!(
        baris.matches("from_u64(1)").count(),
        8,
        "satu bit harus diulang 8 kali:\n{baris}"
    );
    assert!(
        baris.contains("], &[1, 1, 1, 1, 1, 1, 1, 1,"),
        "baris:\n{baris}"
    );
}

#[test]
fn replication_dari_signal_tetap_menggunakan_lebar_operand() {
    let code = generate("module m(input [1:0] a, output [7:0] y); assign y = {4{a}}; endmodule");
    // Operand yang sama ditulis empat kali, masing-masing pada lebar 2.
    let baris = code
        .lines()
        .find(|l| l.contains("concat_kbits"))
        .expect("baris assignment");
    assert_eq!(
        baris.matches("self.signals[0].read()").count(),
        4,
        "operand harus diulang 4 kali:\n{baris}"
    );
    assert!(baris.contains("], &[2, 2, 2, 2,"), "baris:\n{baris}");
}

#[test]
fn concat_dalam_assignment_lhs_sliced_tetap_kompilable() {
    let code = generate(
        "module m(input [3:0] a, input [3:0] b, output [7:0] y); assign y[7:0] = {a, b}; endmodule",
    );
    // Kurung harus seimbang agar Rust bisa diparse.
    let buka = code.matches('(').count();
    let tutup = code.matches(')').count();
    assert_eq!(buka, tutup, "kurung tak seimbang:\n{code}");
}

// --- BUG-13: signed ---

#[test]
fn signed_port_masih_menghasilkan_lebar_biasa() {
    let code = generate("module m(input signed [7:0] a, output [7:0] y); assign y = a; endmodule");
    assert!(code.contains("pub fn a(&self) -> Bits<8>"), "code:\n{code}");
}

#[test]
fn concat_menyusun_bit_dari_msb_ke_lsb() {
    // LRM §11.8.1: {4'hA, 4'h3} = 8'hA3, jadi item paling kiri menjadi MSB.
    // Perakitan kini lewat helper runtime, jadi yang diuji adalah urutan item
    // pada larik — helper menggeser tiap item ke posisi lebarnya.
    let code = generate("module m(output [7:0] y); assign y = {4'hA, 4'h3}; endmodule");
    let baris = code
        .lines()
        .find(|l| l.contains("concat_kbits"))
        .expect("baris assignment");
    let posisi_a = baris.find("from_u64(10)").expect("operand A ada");
    let posisi_b = baris.find("from_u64(3)").expect("operand B ada");
    assert!(
        posisi_a < posisi_b,
        "item kiri harus ditulis lebih dulu:\n{baris}"
    );
    assert!(baris.contains("], &[4, 4])"), "baris:\n{baris}");
}

#[test]
fn concat_tiga_item_urutan_kiri_ke_kanan() {
    // {a, b, c} 2-bit masing-masing: urutan di larik menentukan MSB-ke-LSB.
    let code = generate(
        "module m(input [1:0] a, input [1:0] b, input [1:0] c, output [5:0] y); \
         assign y = {a, b, c}; endmodule",
    );
    let baris = code
        .lines()
        .find(|l| l.contains("concat_kbits"))
        .expect("baris assignment");
    let pa = baris.find("signals[0]").expect("a ada");
    let pb = baris.find("signals[1]").expect("b ada");
    let pc = baris.find("signals[2]").expect("c ada");
    assert!(pa < pb && pb < pc, "urutan harus kiri ke kanan: {baris}");
    assert!(baris.contains("], &[2, 2, 2])"), "baris:\n{baris}");
}

// --- BUG-15: casez / casex dengan wildcard ---

#[test]
fn casez_menghasilkan_perbandingan_bermasker() {
    let code = generate(
        "module m(input [1:0] s, output [3:0] y); \
         always_comb casez (s) 2'b1?: y = 4'd1; default: y = 4'd0; endcase endmodule",
    );
    // Wildcard di bit 0 dicoret, jadi hanya bit 1 yang dibandingkan.
    // Perbandingan lewat helper per digit, bukan `to_u64()` — bit di atas 63
    // akan hilang kalau lewat `to_u64()`.
    assert!(
        code.contains("sv_runtime::case_eq::<2, MAX_WIDTH>("),
        "code:\n{code}"
    );
    assert!(code.contains("} else {"), "code:\n{code}");
}

// --- BUG-3/BUG-4: label `casez`/`casex` yang berupa cast dan pemisahan `x`/`z` ---

#[test]
fn casez_dengan_label_cast_tetap_dicoret() {
    // BUG-3: cast wildcard (`16'(...)`) dulu tidak terlipat ke `Expr::Const`,
    // sehingga `label_wildcard_mask` mengembalikan None dan lengannya terbuang
    // diam-diam — tanpa error apa pun, hasilnya selalu `default`.
    // Wildcard `?` ada di bit 6, jadi bit itu harus dikecoret dari perbandingan.
    let code = generate(
        "module m(input [15:0] s, output y); \
         always_comb casez (s) 16'(16'b0000_0000_1?10_0000): y = 1; \
         default: y = 0; endcase endmodule",
    );
    // Wildcard ada di bit 6: `zmask` = 0x40 (casez hanya z yang wildcard), nilai 0xa0.
    assert!(
        code.contains("case_eq::<16, MAX_WIDTH>("),
        "wildcard cast tidak dikecoret:\n{code}"
    );
    assert!(
        code.contains(", 0xa0, 0x0, 0x40)"),
        "nilai label salah:\n{code}"
    );
}

#[test]
fn casez_hanya_menjadikan_z_sebagai_wildcard() {
    // LRM §12.5: `casez` hanya `z`/`?` yang don't-care; `x` pada label tetap
    // dibandingkan. Kalau `x` ikut dicoret, `casez (16'h01F2) 16'h01x2` akan
    // cocok — padahal iverilog bilang tidak.
    //
    // Yang di-emit adalah `!mask`, jadi "tidak ada wildcard" berarti seluruh
    // bit ikut dibandingkan (`0xffff...`).
    let code = generate(
        "module m(input [15:0] s, output y); \
         always_comb casez (s) 16'h01x2: y = 1; default: y = 0; endcase endmodule",
    );
    // `casez (16'h01F2) 16'h01x2`: `x` di label TIDAK wildcard, jadi
    // `unknown_mask` harus nol (hanya `zmask` yang dipakai).
    assert!(
        code.contains(", 0x102, 0x0, 0x0)"),
        "casez salah mencoret x sebagai wildcard:\n{code}"
    );
}

#[test]
fn casex_menjadikan_x_sebagai_wildcard() {
    // Kontras langsung: `casex` menjadikan `x` juga wildcard.
    //
    // `16'h01x2` menaruh `x` pada nibble kedua dari kanan, yaitu bit 4..7 —
    // bukan bit 8. Empat bit itu dikecoret, sisanya tetap dibandingkan.
    let code = generate(
        "module m(input [15:0] s, output y); \
         always_comb casex (s) 16'h01x2: y = 1; default: y = 0; endcase endmodule",
    );
    // `casex`: `x` di bit 4..7 menjadi wildcard, jadi `unknown_mask` = 0xf0.
    assert!(
        code.contains(", 0x102, 0xf0, 0x0)"),
        "casex harus mencoret posisi x:\n{code}"
    );
}

#[test]
fn casez_dengan_label_typedef_cast_tetap_dicoret() {
    // Sama seperti test size cast, tapi lewat typedef.
    let code = generate(
        "module m(input [15:0] s, output y); \
         typedef logic [15:0] word_t; \
         always_comb casez (s) word_t'(16'b0000_0000_1?10_0000): y = 1; \
         default: y = 0; endcase endmodule",
    );
    assert!(
        code.contains(", 0xa0, 0x0, 0x40)"),
        "wildcard cast typedef tidak dikecoret:\n{code}"
    );
}

#[test]
fn casex_menghasilkan_perbandingan_bermasker() {
    let code = generate(
        "module m(input [1:0] s, output [3:0] y); \
         always_comb casex (s) 2'b1x: y = 4'd1; default: y = 4'd0; endcase endmodule",
    );
    assert!(
        code.contains("sv_runtime::case_eq::<2, MAX_WIDTH>("),
        "code:\n{code}"
    );
    assert!(code.contains("} else {"), "code:\n{code}");
}

#[test]
fn case_biasa_juga_memakai_pembanding_per_digit() {
    // BUG: `case` eksak memakai `match sel.to_u64()` yang hanya melihat 64
    // bit LSB, sehingga pada selektor lebih dari 64 bit semua bit di atas 63
    // hilang dan cabang yang salah ikut diambil.
    let code = generate(
        "module m(input [1:0] s, output [3:0] y); \
         always_comb case (s) 2'b10: y = 4'd1; default: y = 4'd0; endcase endmodule",
    );
    assert!(
        code.contains("sv_runtime::case_eq::<2, MAX_WIDTH>("),
        "code:\n{code}"
    );
    assert!(!code.contains("match _sel"), "code:\n{code}");
    assert!(code.contains(", 0x2, 0x0, 0x0)"), "code:\n{code}");
}

#[test]
fn casez_tanpa_default_tidak_menutup_blok_berlebih() {
    // Tanpa `default`, rantai hanya boleh punya satu penghubung `else`.
    let code = generate(
        "module m(input [1:0] s, output [3:0] y); \
         always_comb casez (s) 2'b1?: y = 4'd1; 2'b01: y = 4'd2; endcase endmodule",
    );
    assert_eq!(
        code.matches("} else if ").count(),
        1,
        "dua cabang => satu else if:\n{code}"
    );
    assert!(
        !code.contains("} else {\n"),
        "tanpa default tidak boleh ada else polos:\n{code}"
    );
    assert_braces_seimbang(&code);
}

#[test]
fn casez_default_di_akhir_menghasilkan_rantai_lengkap() {
    let code = generate(
        "module m(input [2:0] s, output [3:0] y); \
         always_comb casez (s) 3'b1?0: y = 4'd1; 3'b0??: y = 4'd2; default: y = 4'd3; endcase endmodule",
    );
    assert_eq!(
        code.matches("} else if ").count(),
        1,
        "satu else if:\n{code}"
    );
    assert_eq!(
        code.matches("} else {").count(),
        1,
        "satu else untuk default:\n{code}"
    );
    assert_braces_seimbang(&code);
}

/// Kurung kurawal harus seimbang agar Rust yang di-generate bisa dikompilasi.
fn assert_braces_seimbang(code: &str) {
    let buka = code.matches('{').count();
    let tutup = code.matches('}').count();
    assert_eq!(buka, tutup, "kurung kurawal tak seimbang:\n{code}");
}

#[test]
fn casez_default_di_awal_tidak_menghasilkan_blok_pengaman() {
    let code = generate(
        "module m(input [1:0] s, output [3:0] y); \
         always_comb casez (s) default: y = 4'd3; 2'b1?: y = 4'd1; endcase endmodule",
    );
    assert!(code.contains("} else if "), "code:\n{code}");
}

#[test]
fn kurung_casez_seimbang() {
    let code = generate(
        "module m(input [2:0] s, output [3:0] y); \
         always_comb casez (s) 3'b1?0: y = 1; default: y = 0; endcase endmodule",
    );
    let buka = code.matches('(').count();
    let tutup = code.matches(')').count();
    assert_eq!(buka, tutup, "kurung tak seimbang:\n{code}");
    assert_braces_seimbang(&code);
}

// --- BUG-34: satuan waktu dan lebar internal $time ---

#[test]
fn delay_menghasilkan_penambahan_waktu_simtime() {
    let code = generate("module tb; logic [7:0] a; initial begin #5 a = 8'd1; end endmodule");
    assert!(
        code.contains("self.time_now = self.time_now.saturating_add("),
        "code:\n{code}"
    );
    assert!(code.contains("from_nanos"), "code:\n{code}");
}

#[test]
fn delay_tanpa_satuan_memakai_nanosecond() {
    let code = generate("module tb; logic [7:0] a; initial begin #5 a = 8'd1; end endmodule");
    assert!(code.contains("SimTime::from_nanos"), "code:\n{code}");
}

#[test]
fn satuan_picosecond_memakai_from_picos() {
    let code = generate("module tb; logic [7:0] a; initial begin #1ps a = 8'd1; end endmodule");
    assert!(code.contains("SimTime::from_picos"), "code:\n{code}");
}

#[test]
fn satuan_mikrosecond_mengalikan_faktor_femtos() {
    let code = generate("module tb; logic [7:0] a; initial begin #1us a = 8'd1; end endmodule");
    assert!(code.contains("FS_PER_US"), "code:\n{code}");
}

#[test]
fn satuan_millisecond_memakai_from_millis() {
    let code = generate("module tb; logic [7:0] a; initial begin #1ms a = 8'd1; end endmodule");
    assert!(code.contains("SimTime::from_millis"), "code:\n{code}");
}

#[test]
fn satuan_detik_mengalikan_faktor_femtos() {
    let code = generate("module tb; logic [7:0] a; initial begin #1s a = 8'd1; end endmodule");
    assert!(code.contains("FS_PER_SEC"), "code:\n{code}");
}

#[test]
fn time_membaca_lebar_internal_64_bit() {
    // BUG-34: design tanpa sinyal lebar pun butuh MAX_WIDTH 64 karena
    // `$time` bertipe 64-bit; kalau tidak, nilainya terpangkas nol.
    let code = generate("module tb; initial begin $display(\"t=%d\", $time); end endmodule");
    assert!(
        code.contains("pub const MAX_WIDTH: usize = 64;"),
        "code:\n{code}"
    );
}

#[test]
fn design_tanpa_time_tidak_menaikkan_lebar_internal() {
    let code = generate("module m(input a, output y); assign y = a; endmodule");
    assert!(
        code.contains("pub const MAX_WIDTH: usize = 1;"),
        "code:\n{code}"
    );
}

#[test]
fn design_dengan_sinyal_lebar_lebih_besar_dari_time() {
    let code = generate(
        "module tb; logic [127:0] w; initial begin #1 $display(\"t=%d\", $time); end endmodule",
    );
    assert!(
        code.contains("pub const MAX_WIDTH: usize = 128;"),
        "code:\n{code}"
    );
}

#[test]
fn finish_menghasilkan_return_setelah_flag() {
    let code = generate("module tb; initial begin $finish; end endmodule");
    assert!(code.contains("self.finished = true;"), "code:\n{code}");
    let posisi_flag = code.find("self.finished = true;").expect("flag");
    let posisi_return = code[posisi_flag..].find("return;").expect("return");
    assert!(posisi_return > 0, "return harus menyusul flag:\n{code}");
}

// --- BUG-35: langkah waktu menyisipkan clock & combinational antar #delay ---

#[test]
fn initial_tanpa_delay_punya_satu_langkah() {
    let code = generate("module tb; logic [7:0] a; initial begin a = 8'd1; end endmodule");
    assert!(
        code.contains("pub const INITIAL_STEPS: usize = 1;"),
        "code:\n{code}"
    );
}

#[test]
fn dua_delay_membuat_tiga_langkah() {
    let code = generate(
        "module tb; logic [7:0] a; initial begin a = 1; #4 a = 2; #4 a = 3; end endmodule",
    );
    assert!(
        code.contains("pub const INITIAL_STEPS: usize = 3;"),
        "code:\n{code}"
    );
}

#[test]
fn setiap_langkah_memiliki_lengan_match() {
    let code = generate(
        "module tb; logic [7:0] a; initial begin a = 1; #4 a = 2; #4 a = 3; end endmodule",
    );
    assert!(code.contains("0 => {"), "code:\n{code}");
    assert!(code.contains("1 => {"), "code:\n{code}");
    assert!(code.contains("2 => {"), "code:\n{code}");
    assert!(code.contains("_ => {}"), "code:\n{code}");
}

#[test]
fn langkah_menerima_indeks_sebagai_parameter() {
    let code = generate("module tb; logic [7:0] a; initial begin a = 1; end endmodule");
    assert!(
        code.contains("pub fn eval_initial(&mut self, step: usize)"),
        "code:\n{code}"
    );
}

#[test]
fn delay_hanya_di_segmen_sebelumnya_tidak_double_count() {
    // Statement delay harus muncul persis sekali, di awal segmen berikutnya.
    let code = generate("module tb; logic [7:0] a; initial begin #4 a = 1; end endmodule");
    let jumlah = code.matches("saturating_add(").count();
    assert_eq!(jumlah, 1, "delay hanya boleh dihitung sekali:\n{code}");
}

#[test]
fn kurung_eval_initial_seimbang() {
    let code = generate(
        "module tb; logic [7:0] a; initial begin a = 1; #4 a = 2; #4 a = 3; end endmodule",
    );
    let buka = code.matches('{').count();
    let tutup = code.matches('}').count();
    assert_eq!(buka, tutup, "kurung tak seimbang:\n{code}");
}

#[test]
fn design_tanpa_initial_tidak_meminta_langkah() {
    let code = generate("module m(input a, output y); assign y = a; endmodule");
    assert!(!code.contains("INITIAL_STEPS"), "code:\n{code}");
}

// --- BUG-39: $monitor mengevaluasi ulang argumen ---

#[test]
fn monitor_menghasilkan_method_pemantau() {
    let code = generate(
        "module tb(input logic clk, output logic [7:0] c); \
         always_ff @(posedge clk) c <= c + 1; \
         initial $monitor(\"c=%d\", c); endmodule",
    );
    assert!(
        code.contains("pub fn run_monitor(&mut self)"),
        "code:\n{code}"
    );
    assert!(
        code.contains("fn refresh_monitor_args(&mut self)"),
        "code:\n{code}"
    );
    assert!(code.contains("monitor_active: bool"), "code:\n{code}");
}

#[test]
fn monitor_menghasilkan_snapshot_untuk_deteksi_perubahan() {
    let code =
        generate("module tb(output logic [7:0] c); initial $monitor(\"c=%d\", c); endmodule");
    assert!(
        code.contains("pub fn snapshot(&self) -> Vec<u64>"),
        "code:\n{code}"
    );
}

#[test]
fn monitor_menyalin_format_string_ke_kode() {
    let code =
        generate("module tb(output logic [7:0] c); initial $monitor(\"c=%d\", c); endmodule");
    assert!(code.contains("\"c=%d\""), "code:\n{code}");
}

#[test]
fn design_tanpa_monitor_tidak_memakai_method_pemantau() {
    let code = generate("module m(input a, output y); assign y = a; endmodule");
    assert!(!code.contains("monitor_active"), "code:\n{code}");
    assert!(!code.contains("run_monitor"), "code:\n{code}");
}

// --- BUG-41: $monitor if (kondisi) hanya mencetak saat syarat benar ---

#[test]
fn monitor_dengan_syarat_menyimpan_field_kondisi() {
    let code = generate(
        "module tb(output logic [7:0] c); \
         initial $monitor if (c > 8'd2) \"c=%d\", c; endmodule",
    );
    assert!(code.contains("monitor_cond: bool"), "code:\n{code}");
    assert!(code.contains("monitor_cond: false"), "code:\n{code}");
    assert!(code.contains("self.monitor_cond = "), "code:\n{code}");
}

#[test]
fn monitor_dengan_syarat_membungkus_cetak_di_dalam_cek() {
    let code = generate(
        "module tb(output logic [7:0] c); \
         initial $monitor if (c > 8'd2) \"c=%d\", c; endmodule",
    );
    assert!(code.contains("if self.monitor_cond {"), "code:\n{code}");
}
#[test]
fn monitor_tanpa_syarat_tidak_membuat_field_kondisi() {
    let code =
        generate("module tb(output logic [7:0] c); initial $monitor(\"c=%d\", c); endmodule");
    assert!(!code.contains("monitor_cond"), "code:\n{code}");
}

// --- BUG-42: $dumpfile/$dumpvars controlling VCD ---

#[test]
fn dumpvars_membuat_field_dan_accessor_rekam() {
    let code = generate(
        "module tb(output logic [7:0] c); \
         initial begin $dumpfile(\"sim.vcd\"); $dumpvars; end endmodule",
    );
    assert!(code.contains("dump_on: bool"), "code:\n{code}");
    assert!(code.contains("dump_path: &'static str"), "code:\n{code}");
    assert!(code.contains("dump_on: false"), "code:\n{code}");
    assert!(
        code.contains("pub fn dump_requested(&self) -> bool"),
        "code:\n{code}"
    );
    assert!(
        code.contains("pub fn dump_path(&self) -> &'static str"),
        "code:\n{code}"
    );
    assert!(code.contains("self.dump_on = true;"), "code:\n{code}");
    assert!(
        code.contains("self.dump_path = \"sim.vcd\";"),
        "code:\n{code}"
    );
}

#[test]
fn design_tanpa_dumpvars_tidak_membuat_field_rekam() {
    let code = generate("module tb(output logic [7:0] c); initial c = 8'd1; endmodule");
    assert!(!code.contains("dump_on"), "code:\n{code}");
    assert!(!code.contains("dump_path"), "code:\n{code}");
}

// --- BUG-43: @(posedge clk) di dalam blok initial ---

#[test]
fn event_control_membuat_field_penunjuk_segmen() {
    let code = generate(
        "module tb(input clk, output logic [7:0] c); \
         initial begin c = 8'd0; @(posedge clk); c = 8'd1; end endmodule",
    );
    assert!(code.contains("initial_pc: Vec<usize>"), "code:\n{code}");
    assert!(code.contains("edge_prev: Vec<u64>"), "code:\n{code}");
}

#[test]
fn event_control_menguji_posedge_antara_langkah_sebelumnya_dan_sekarang() {
    let code = generate(
        "module tb(input clk, output logic [7:0] c); \
         initial begin c = 8'd0; @(posedge clk); c = 8'd1; end endmodule",
    );
    // LRM §9.7: posedge berarti nilai sebelumnya 0 dan nilai sekarang 1.
    assert!(
        code.contains("self.edge_prev[0] & 1 == 0 && self.signals[0].read().to_u64() & 1 == 1"),
        "code:\n{code}"
    );
    assert!(
        code.contains("pub fn sample_edge(&mut self)"),
        "code:\n{code}"
    );
}

#[test]
fn event_control_hanya_maju_bila_edge_terjadi() {
    let code = generate(
        "module tb(input clk, output logic [7:0] c); \
         initial begin c = 8'd0; @(posedge clk); c = 8'd1; end endmodule",
    );
    // Pointer tidak boleh naik sebelum syarat edge terpenuhi.
    let posisi_if = code
        .find("if self.initial_pc[0] == 1 &&")
        .expect("ada cek edge");
    let posisi_naik = code[posisi_if..]
        .find("self.initial_pc[0] = 2;")
        .expect("ada kenaikan pointer");
    assert!(
        posisi_naik > 0,
        "kenaikan pointer harus di dalam blok cek edge"
    );
    let blok = &code[posisi_if..posisi_if + posisi_naik];
    // `c` adalah sinyal index 1 pada module `tb(input clk, output logic [7:0] c)`.
    assert!(
        blok.contains("self.signals[1].write"),
        "blok cek edge harus memuat statement badan:\n{blok}"
    );
}

#[test]
fn design_tanpa_event_control_tetap_pakai_langkah_berurutan() {
    let code = generate("module tb(output logic [7:0] c); initial c = 8'd1; endmodule");
    // Tanpa menunggu edge, `eval_initial` tetap memakai match langkah.
    assert!(code.contains("match step {"), "code:\n{code}");
    assert!(!code.contains("sample_edge"), "code:\n{code}");
}

// --- BUG-44: @(posedge clk or negedge rst) ---

#[test]
fn event_gabungan_menggabungkan_item_dengan_or() {
    let code = generate(
        "module tb(input clk, input rst, output logic [7:0] c); \
         initial begin c = 8'd0; @(posedge clk or negedge rst); c = 8'd1; end endmodule",
    );
    // LRM §9.7: pemicu bila salah satu edge terjadi.
    assert!(code.contains(" || "), "code:\n{code}");
    assert!(code.contains("edge_prev[0] & 1 == 0"), "code:\n{code}");
    assert!(code.contains("edge_prev[1] & 1 == 1"), "code:\n{code}");
}

#[test]
fn event_gabungan_menyimpan_semua_sinyal_yang_diawasi() {
    let code = generate(
        "module tb(input clk, input rst, output logic [7:0] c); \
         initial begin c = 8'd0; @(posedge clk or negedge rst); c = 8'd1; end endmodule",
    );
    // `sample_edge` harus mengambil sampel kedua sinyal.
    assert!(
        code.contains("self.edge_prev[0] = self.signals[0].read().to_u64() & 1;"),
        "code:\n{code}"
    );
    assert!(
        code.contains("self.edge_prev[1] = self.signals[1].read().to_u64() & 1;"),
        "code:\n{code}"
    );
}

// --- BUG-46: always_ff @(posedge clk or posedge rst) ---

#[test]
fn always_ff_event_gabungan_menguji_edge_pada_setiap_sinyal() {
    let code = generate(
        "module m(input clk, input rst, output logic [7:0] q); \
         always_ff @(posedge clk or posedge rst) \
           if (rst) q <= 8'd0; else q <= q + 8'd1; endmodule",
    );
    // Evaluasi langkah utama digabung dengan edge nyata tiap sinyal (LRM §9.7).
    assert!(code.contains("if clock == 0 || ("), "code:\n{code}");
    assert!(code.contains("self.edge_prev[1] & 1 == 0"), "code:\n{code}");
    // Tanpa blok `initial` pun `edge_prev` dan `sample_edge` disiapkan.
    assert!(
        code.contains("edge_prev: vec![0; signal_count]"),
        "code:\n{code}"
    );
    assert!(
        code.contains("pub fn sample_edge(&mut self)"),
        "code:\n{code}"
    );
    // BUG-48: `if (rst)` sekuensial menghasilkan blok `if` pada Rust dengan
    // bentuk kebenaran yang benar.
    assert!(
        code.contains("if self.signals[1].read().to_u64() != 0 {")
            || code.contains("if self.signals[0].read().to_u64() != 0 {"),
        "syarat `if` sekuensial:\n{code}"
    );
}

#[test]
fn segmen_berikutnya_tidak_berjalan_dalam_panggilan_yang_sama() {
    let code = generate(
        "module tb(input clk, output logic [7:0] c); \
         initial begin c = 8'd0; #4 c = 8'd1; @(posedge clk); c = 8'd2; end endmodule",
    );
    // Rantai `else if` wajib; dua `if` terpisah membuat segmen berikutnya ikut jalan.
    assert!(
        code.contains("} else if self.initial_pc[0] =="),
        "code:\n{code}"
    );
}

#[test]
fn statement_monitor_hanya_mengaktifkan_pencetakan() {
    // Nilai argumen harus dievaluasi ulang di `refresh_monitor_args`,
    // bukan dibekukan saat statement dijalankan.
    let code =
        generate("module tb(output logic [7:0] c); initial $monitor(\"c=%d\", c); endmodule");
    let aktif = code
        .find("self.monitor_active = true;")
        .expect("stmt monitor");
    let refresh = code.find("fn refresh_monitor_args").expect("refresh");
    assert!(
        refresh > aktif,
        "method refresh harus ada terpisah dari statement:\n{code}"
    );
}

#[test]
fn kurung_run_monitor_seimbang() {
    let code =
        generate("module tb(output logic [7:0] c); initial $monitor(\"c=%d\", c); endmodule");
    let buka = code.matches('{').count();
    let tutup = code.matches('}').count();
    assert_eq!(buka, tutup, "kurung tak seimbang:\n{code}");
}

// --- BUG-40: $monitoron/$monitoroff dan $strobe ---

#[test]
fn monitoron_menyalakan_flag_pemantau() {
    let code = generate("module tb(output logic [7:0] c); initial $monitoron; endmodule");
    assert!(
        code.contains("self.monitor_active = true;"),
        "code:\n{code}"
    );
    assert!(code.contains("monitor_active: bool"), "code:\n{code}");
}

#[test]
fn monitoroff_mematikan_flag_pemantau() {
    let code = generate("module tb(output logic [7:0] c); initial $monitoroff; endmodule");
    assert!(
        code.contains("self.monitor_active = false;"),
        "code:\n{code}"
    );
}

#[test]
fn strobe_mengantrekan_panggilan_bukan_boolean() {
    // LRM §20.3: beberapa $strobe dalam satu timestep harus semuanya
    // dicetak, jadi penandanya berupa antrean, bukan flag tunggal.
    let code = generate("module tb(output logic [7:0] c); initial $strobe(\"c=%d\", c); endmodule");
    assert!(code.contains("self.strobe_pending.push(("), "code:\n{code}");
    assert!(
        code.contains("pub fn run_strobe(&mut self)"),
        "code:\n{code}"
    );
}

#[test]
fn strobe_mengosongkan_antrean_saat_mencetak() {
    let code = generate("module tb(output logic [7:0] c); initial $strobe(\"c=%d\", c); endmodule");
    assert!(
        code.contains("std::mem::take(&mut self.strobe_pending)"),
        "code:\n{code}"
    );
}

#[test]
fn strobe_menyimpan_formatnya_sendiri() {
    // Dua $strobe berbeda tidak boleh saling menimpa format.
    let code = generate(
        "module tb(output logic [7:0] c); initial begin $strobe(\"a=%d\", c); $strobe(\"b=%d\", c); end endmodule",
    );
    assert!(code.contains("\"a=%d\""), "code:\n{code}");
    assert!(code.contains("\"b=%d\""), "code:\n{code}");
}

#[test]
fn design_tanpa_strobe_tidak_memakai_method_strobe() {
    let code = generate("module m(input a, output y); assign y = a; endmodule");
    assert!(!code.contains("strobe_pending"), "code:\n{code}");
}

#[test]
fn kurung_generated_seimbang_dengan_strobe() {
    let code = generate("module tb(output logic [7:0] c); initial $strobe(\"c=%d\", c); endmodule");
    let buka = code.matches('{').count();
    let tutup = code.matches('}').count();
    assert_eq!(buka, tutup, "kurung tak seimbang:\n{code}");
}
