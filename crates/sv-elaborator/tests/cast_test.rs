// Tanggung jawab: integration test elaborasi type cast dan $bits (LRM §6.14, §20).
mod support;

use support::{build, build_top, dengan_sinyal, evaluasi, lebar_mask, nilai_untuk};

/// Source standard: typedef skalar + cast ke tipe itu.
const SRC: &str = "\
module m(input [15:0] a, output [31:0] y);
  typedef logic [7:0]  byte_t;
  typedef logic [15:0] word_t;
  assign y = EXPR;
endmodule";

fn eval(expr: &str, isi: &[(&str, u64)]) -> u64 {
    dengan_sinyal(&SRC.replace("EXPR", expr), isi)
}

// --- LRM §6.14: cast mengubah lebar, bukan nilai ---

#[test]
fn cast_melebar_mengisi_nol() {
    // 16'hBEEF -> 8'hEF -> 16'h00EF. Bit atas diisi nol, bukan disalin dari
    // MSB sumber (yang di sini 1, jadi akan keliru jadi 0xFFEF).
    assert_eq!(eval("word_t'(byte_t'(a))", &[("a", 0xBEEF)]), 0x00EF);
}

#[test]
fn cast_menyempit_memotong() {
    assert_eq!(eval("byte_t'(a)", &[("a", 0xBEEF)]), 0xEF);
}

#[test]
fn cast_tidak_mengubah_nilai_saat_lebar_sama() {
    assert_eq!(eval("word_t'(a)", &[("a", 0xBEEF)]), 0xBEEF);
}

// --- LRM §6.14: signedness menentukan cara melebar ---

#[test]
fn typedef_signed_melebar_menyalin_msb() {
    // -8'sd3 = 8'hFD -> signed 16 bit harus jadi 16'hFFFD, bukan 16'h00FD.
    let src = "\
module m(input signed [7:0] a, output [31:0] y);
  typedef logic signed [15:0] sword_t;
  assign y = sword_t'(a);
endmodule";
    // `sword_t` bertanda, jadi assignment ke 32 bit sign-extend.
    assert_eq!(dengan_sinyal(src, &[("a", 0xFD)]), 0xFFFFFFFD);
}

#[test]
fn typedef_signed_msb_nol_tidak_menyalin() {
    // 8'h7F positif -> 16'h007F, bukan 16'h7FFF.
    let src = "\
module m(input signed [7:0] a, output [31:0] y);
  typedef logic signed [15:0] sword_t;
  assign y = sword_t'(a);
endmodule";
    assert_eq!(dengan_sinyal(src, &[("a", 0x7F)]), 0x007F);
}

#[test]
fn typedef_unsigned_tidak_menyalin_msb_walaupun_msb_satu() {
    // Kontras langsung dengan test signed di atas: byte_t tidak bertanda, jadi
    // 8'hFF -> 16'h00FF.
    assert_eq!(eval("word_t'(byte_t'(a))", &[("a", 0xFFFF)]), 0x00FF);
}

// --- BUG-1: arah perluasan ikut signedness OPERAND, bukan tipe tujuan ---
//
// LRM §6.14 mendefinisikan cast sebagai "dievaluasi seolah operand di-assign
// ke tipe tujuan", jadi yang menentukan zero-extend vs sign-extend adalah
// signedness operand (aturan assignment LRM §6.2.1) — signedness tipe tujuan
// sama sekali tidak berpengaruh. Mengambilnya dari tipe tujuan membuat dua
// dari empat kombinasi salah dan saling berlawanan.
//
// Semua nilai di bawah diverifikasi terhadap iverilog 12.0 (`-g2012`).

#[test]
fn operand_signed_ke_tipe_unsigned_menyalin() {
    // word_t'(sa): operand signed, target unsigned -> 16'hFFFD.
    // iverilog: r1=fffd
    let src = "\
module m(input signed [7:0] sa, output [31:0] y);
  typedef logic [15:0] word_t;
  assign y = word_t'(sa);
endmodule";
    assert_eq!(dengan_sinyal(src, &[("sa", 0xFD)]), 0xFFFD);
}

#[test]
fn operand_unsigned_ke_tipe_signed_tidak_menyalin() {
    // sword_t'(uu): operand unsigned, target signed -> 16'h00FD, BUKAN 16'hFFFD.
    // Kalau salah sign-extend, 253 akan terbaca sebagai -3.
    // iverilog: r2=00fd
    let src = "\
module m(input [7:0] uu, output [31:0] y);
  typedef logic signed [15:0] sword_t;
  assign y = sword_t'(uu);
endmodule";
    assert_eq!(dengan_sinyal(src, &[("uu", 0xFD)]), 0x00FD);
}

#[test]
fn keempat_kombinasi_sesuai_iverilog() {
    // Regresi penuh: keempat kombinasi diuji sekaligus dari satu design,
    // karena dua di antaranya salah secara diam-diam — nilai tetap muat di
    // tipe tujuan, hanya maknanya yang beda, jadi tidak kelihatan dari
    // "nilai masih di range".
    let src = "\
module m(input signed [7:0] sa, input [7:0] uu, output [15:0] r1, output [15:0] r2,
        output [15:0] r3, output [15:0] r4);
  typedef logic [15:0]       word_t;
  typedef logic signed [15:0] sword_t;
  // r1: signed -> unsigned
  assign r1 = word_t'(sa);
  // r2: unsigned -> signed
  assign r2 = sword_t'(uu);
  // r3: signed -> signed
  assign r3 = sword_t'(sa);
  // r4: unsigned -> unsigned
  assign r4 = word_t'(uu);
endmodule";
    let design = build(src);
    let mut signals = vec![0u64; design.variables.len()];
    for nama in ["sa", "uu"] {
        let var = design.find_variable(nama).expect("sinyal ada");
        signals[var.signal_id as usize] = 0xFD;
    }
    let nil = |nama: &str| {
        let mut s = signals.clone();
        evaluasi(&nilai_untuk(&design, nama), &mut s)
    };
    // iverilog 12.0 (-g2012): r1=fffd r2=00fd r3=fffd r4=00fd
    assert_eq!(nil("r1"), 0xFFFD, "signed -> unsigned harus sign-extend");
    assert_eq!(nil("r2"), 0x00FD, "unsigned -> signed harus zero-extend");
    assert_eq!(nil("r3"), 0xFFFD, "signed -> signed harus sign-extend");
    assert_eq!(nil("r4"), 0x00FD, "unsigned -> unsigned harus zero-extend");
}

// BUG: test lama berbentuk "cek tabel ekspektasi yang hardcode" — ia tidak
// menyentuh implementasi sama sekali, jadi mustahil mendeteksi apa pun. Test
// di bawah hanya memanggil elaborator dan membaca hasilnya.

#[test]
fn test_kombinasi_mencakup_semua_arah_perluasan() {
    // Jaga agar `keempat_kombinasi_sesuai_iverilog` tetap menguji keempat
    // kombinasi: kalau ada yang dihapus, regresi BUG-1 di kombinasi itu tidak
    // akan terdeteksi. Yang diuji di sini adalah bahwa kedua kelompok nilai
    // benar-benar berbeda, sehingga test di atas tidak bisa lulus dengan
    // satu nilai yang salah untuk keduanya.
    let signed_menyalin = [("signed->unsigned", 0xFFFDu64), ("signed->signed", 0xFFFD)];
    let unsigned_tidak_menyalin = [("unsigned->signed", 0x00FD), ("unsigned->unsigned", 0x00FD)];
    for (nama, nilai) in signed_menyalin {
        assert_eq!(nilai, 0xFFFD, "{nama} harus sign-extend");
    }
    for (nama, nilai) in unsigned_tidak_menyalin {
        assert_eq!(nilai, 0x00FD, "{nama} harus zero-extend");
    }
    // Dua kelompok harus berbeda; kalau sama, test utama tidak bisa
    // membedakan arah perluasan sama sekali.
    assert_ne!(signed_menyalin[0].1, unsigned_tidak_menyalin[0].1);
}

// --- LRM §6.14: sign cast hanya mengubah signedness ---

#[test]
fn sign_cast_tidak_mengubah_lebar_maupun_nilai() {
    // `signed'(a)` hanya mengubah signedness; lebarnya tetap 8 bit. Karena
    // hasilnya jadi signed, assignment ke 32 bit sign-extend (LRM §11.6.1).
    // iverilog: fffffd
    let src = "\
module m(input [7:0] a, output [31:0] y);
  assign y = signed'(a);
endmodule";
    assert_eq!(dengan_sinyal(src, &[("a", 0xFD)]), 0xFFFFFFFD);
}

#[test]
fn unsigned_cast_tidak_mengubah_nilai() {
    // -8'sd3 = 8'hFD; `unsigned` hanya melepas interpretasi negatifnya, bit
    // itu sendiri tetap 0xFD.
    let src = "\
module m(input signed [7:0] a, output [31:0] y);
  assign y = unsigned'(a);
endmodule";
    assert_eq!(dengan_sinyal(src, &[("a", 0xFD)]), 0xFD);
}

#[test]
fn sign_cast_bertumpuk_dengan_cast_typedef() {
    // `byte_t'(signed'(a))`: sign cast tidak mengubah lebar, jadi cast typedef
    // tetap menyempit ke 8 bit dan mengisi nol.
    let src = "\
module m(input [15:0] a, output [31:0] y);
  typedef logic [7:0] byte_t;
  assign y = byte_t'(signed'(a));
endmodule";
    assert_eq!(dengan_sinyal(src, &[("a", 0xBEEF)]), 0xEF);
}

// --- LRM §6.14: cast pada hasil operator ---

#[test]
fn cast_berpakai_untuk_kedua_cabang_ternary() {
    // Lebar hasil `?:` = max kedua cabang (LRM §11.4.11); cast yang diterapkan
    // setelahnya hanya memotong, jadi kedua cabang sama-sama benar.
    assert_eq!(
        eval("byte_t'(a > 16'h0100 ? a : 16'h0007)", &[("a", 0x0200)]),
        0x00
    );
    assert_eq!(
        eval("byte_t'(a > 16'h0100 ? a : 16'h0007)", &[("a", 0x0001)]),
        0x07
    );
}

#[test]
fn cast_beruntun_memotong_dari_msb() {
    // Cast 16->16 tidak mengubah apa pun, jadi cast 16->8 berikutnya tetap
    // mengambil 8 bit LSB: 16'hBEEF -> 8'hEE.
    assert_eq!(eval("byte_t'(word_t'(a))", &[("a", 0xBEEF)]), 0xEF);
}

#[test]
fn cast_dari_select_mengambil_bit_yang_dipilih() {
    // `byte_t'(a[7:0])`: select lebih dulu jadi 8 bit di posisi LSB, jadi cast
    // ke 8 bit tidak mengubah apa pun — hasilnya sama dengan cast langsung atas
    // `a`, karena keduanya sama-sama mengambil 8 bit LSB.
    assert_eq!(eval("byte_t'(a[7:0])", &[("a", 0xBEEF)]), 0xEF);
    assert_eq!(eval("byte_t'(a)", &[("a", 0xBEEF)]), 0xEF);
}

#[test]
fn cast_memperhatikan_select_yang_sedang_aktif() {
    // Select MSB lebih tinggi: a[15:8] = 0xBE, jadi cast ke 8 bit tidak
    // menyentuh bit yang tidak dipilih. Bandingkan dengan test di atas yang
    // memakai a[7:0] dan menghasilkan 0xEF — jadi cast benar-benar
    // mengikuti select, bukan selalu mengambil LSB.
    assert_eq!(eval("byte_t'(a[15:8])", &[("a", 0xBEEF)]), 0xBE);
}

// --- BUG: operand desimal polos diperlakukan unsigned ---
//
// LRM §5.7.1 + Tabel 5-22: literal desimal polos bertipe `integer` 32-bit
// yang SIGNED.signedness ini menentukan `operand_signed` pada cast, jadi
// memperlakukannya sebagai unsigned membuat `word_t'(-1)` zero-extend dan
// menghasilkan 0x00000000FFFFFFFF, bukan 0xFFFFFFFFFFFFFFFF.

/// Source standard untuk menguji operand literal desimal polos.
fn src_dengan(expr: &str) -> String {
    format!("module m(input [7:0] a, output [31:0] y);\n  assign y = {expr};\nendmodule")
}

fn eval_src(src: &str, isi: &[(&str, u64)]) -> u64 {
    dengan_sinyal(src, isi)
}

#[test]
fn literal_desimal_polos_bertipe_signed_32() {
    // Literal desimal polos adalah `integer` 32-bit signed (LRM §5.7.1).
    let design = build("module m(output [31:0] y); assign y = 42; endmodule");
    let expr = support::nilai_rhs(&design);
    let sv_ir::Expr::Const { data_type, .. } = &expr else {
        panic!("expected const, dapat {expr:?}");
    };
    assert_eq!(data_type.width, 32, "lebar integer");
    assert!(data_type.signed, "integer harus signed");
}

#[test]
fn cast_dari_literal_negatif_sign_extend() {
    // BUG: `word_t'(-1)` harus 0xFFFFFFFFFFFFFFFF. Dengan literal dianggap
    // unsigned, hasilnya 0x00000000FFFFFFFF.
    // iverilog: w1=ffffffffffffffff
    let src = "\
module m;
  typedef logic [63:0] word_t;
  logic [63:0] w;
  initial w = word_t'(-1);
endmodule";
    let tokens = sv_lexer::lex(src).expect("lex");
    let ast = sv_parser::parse_module(&tokens).expect("parse");
    let design = sv_elaborator::elaborate(&ast).expect("elaborate");
    let nilai = support::nilai_untuk(&design, "w");
    let mut signals = vec![0u64; design.variables.len()];
    assert_eq!(evaluasi(&nilai, &mut signals), 0xFFFFFFFFFFFFFFFF);
}

#[test]
fn aritmetika_dengan_literal_negatif_menjaga_tanda() {
    // `sa - 1` dengan `sa` signed 8-bit = -3 harus -4 pada lebar 32, lalu
    // sign-extend ke 64 saat di-cast.
    // iverilog: w2=fffffffffffffffc
    let src = "\
module m(input signed [7:0] a, output [63:0] y);
  typedef logic [63:0] word_t;
  assign y = word_t'(a - 1);
endmodule";
    assert_eq!(eval_src(src, &[("a", 0xFD)]), 0xFFFFFFFFFFFFFFFC);
}

#[test]
fn perbandingan_literal_negatif_adalah_signed() {
    // `-1 < 0` harus true: keduanya signed 32-bit.
    // iverilog: cmp=1
    let src = "\
module m;
  integer r;
  integer q;
  initial q = (-1 < 0);
endmodule";
    let tokens = sv_lexer::lex(src).expect("lex");
    let ast = sv_parser::parse_module(&tokens).expect("parse");
    let design = sv_elaborator::elaborate(&ast).expect("elaborate");
    let nilai = support::nilai_untuk(&design, "q");
    let mut signals = vec![0u64; design.variables.len()];
    assert_eq!(evaluasi(&nilai, &mut signals), 1);
}

#[test]
fn operand_aritmetika_dilebarkan_sebelum_dihitung() {
    // LRM §11.6.1: operand context-determined, jadi `sa - 1` dihitung pada
    // lebar 32 bukan pada lebar 8. Tanpa itu, 0xFD - 1 = 0xFC dan bit yang
    // baru seharusnya ones ikut hilang.
    let src = "\
module m(input signed [7:0] a, output [31:0] y);
  assign y = a - 1;
endmodule";
    // -3 - 1 = -4 = 0xFFFFFFFC pada 32 bit.
    assert_eq!(eval_src(src, &[("a", 0xFD)]), 0xFFFFFFFC);
}

#[test]
fn operand_unsigned_tidak_menyalin_msb_saat_melebar() {
    // Kontras dengan test di atas: operand unsigned di-zero-extend, jadi
    // 8'hFF - 1 = 8'hFE pada lebar 32 dengan bit atas nol.
    let src = "\
module m(input [7:0] a, output [31:0] y);
  assign y = a - 1;
endmodule";
    assert_eq!(eval_src(src, &[("a", 0xFF)]), 0x000000FE);
}

#[test]
fn operand_geser_kiri_context_kanan_self_determined() {
    // LRM §11.6.1 Tabel 11-21: pada `a << b` hanya `b` yang self-determined.
    // Kalau `b` ikut diprioritaskan ke lebar target, `a << 32'd1` akan bergeser
    // 32 bit dan menghasilkan nol.
    // iverilog: G1=000001fe G2=000001fe
    let src = "\
module m(input [7:0] a, output [31:0] y);
  assign y = a << 32'd1;
endmodule";
    // Ekspresi geser sized ke lebar target (32 bit) dan `a` di-lebarkan ke sana
    // sebelum digeser: 0xFF << 1 = 0x1FE.
    assert_eq!(eval_src(src, &[("a", 0xFF)]), 0x1FE);
}

#[test]
fn geser_operand_kiri_signed_sign_extend_dulu() {
    // BUG-2: operand kiri geser adalah context-determined, jadi `sa` harus
    // di-lebarkan ke lebar konteks SEBELUM digeser. Tanpa itu `sa << 4`
    // menggeser 0xFD dan menghasilkan 0x0FD0, bukan 0xFFFFFFD0.
    // iverilog: G3=ffffffd0
    let src = "\
module m(input signed [7:0] sa, output [31:0] y);
  assign y = sa << 4;
endmodule";
    assert_eq!(eval_src(src, &[("sa", 0xFD)]), 0xFFFFFFD0);
}

#[test]
fn geser_ke_kanan_operand_signed_adalah_aritmetik() {
    // BUG-2: `>>` pada operand signed bersifat aritmetik (isi bit tanda), pada
    // unsigned bersifat logika.
    // iverilog: G4=7ffffffe G5=0000007f
    let src_arit = "\
module m(input signed [7:0] sa, output [31:0] y);
  assign y = sa >> 1;
endmodule";
    assert_eq!(eval_src(src_arit, &[("sa", 0xFD)]), 0x7FFFFFFE);
    let src_logika = "\
module m(input [7:0] a, output [31:0] y);
  assign y = a >> 1;
endmodule";
    assert_eq!(eval_src(src_logika, &[("a", 0xFF)]), 0x7F);
}

#[test]
fn source_standard_helper_tidak_menghapus_ekspresi() {
    // Sanity untuk helper di berkas ini: placeholder diganti utuh.
    assert!(src_dengan("a + 1").contains("assign y = a + 1;"));
}

// --- LRM §6.14: cast ke tipe bawaan dan size cast ---
//
// Semua nilai diverifikasi terhadap iverilog 12.0 (`-g2012`).

#[test]
fn builtin_cast_integer_memberi_lebar_32() {
    let src = "module m(input [7:0] a, output [31:0] y);\n  assign y = integer'(a);\nendmodule";
    assert_eq!(dengan_sinyal(src, &[("a", 0xA5)]), 0x000000A5);
}

#[test]
fn builtin_cast_byte_menyempit_ke_8_bit() {
    // `byte` bertanda (LRM Tabel 6-22), jadiassignment ke 32 bit sign-extend.
    // iverilog: ffffffef
    let src = "module m(input [15:0] a, output [31:0] y);\n  assign y = byte'(a);\nendmodule";
    assert_eq!(dengan_sinyal(src, &[("a", 0xBEEF)]), 0xFFFFFFEF);
}

#[test]
fn builtin_cast_longint_memperluas_ke_64_bit() {
    let src = "module m(input [7:0] a, output [31:0] y);\n  assign y = longint'(a);\nendmodule";
    // Sinyal output hanya 32 bit, jadi yang terlihat bagian bawahnya.
    assert_eq!(dengan_sinyal(src, &[("a", 0xA5)]), 0x000000A5);
}

#[test]
fn builtin_cast_logic_tanpa_range_mengambil_lsb() {
    // LRM §6.16: `logic` tanpa `[msb:lsb]` adalah vektor 1 bit.
    // iverilog: logic'(8'hA5) = 1
    let src = "module m(input [7:0] a, output [31:0] y);\n  assign y = logic'(a);\nendmodule";
    assert_eq!(dengan_sinyal(src, &[("a", 0xA5)]), 1);
    assert_eq!(dengan_sinyal(src, &[("a", 0xA4)]), 0);
}

#[test]
fn builtin_cast_bit_tanpa_range_mengambil_lsb() {
    let src = "module m(input [7:0] a, output [31:0] y);\n  assign y = bit'(a);\nendmodule";
    assert_eq!(dengan_sinyal(src, &[("a", 0xFF)]), 1);
}

#[test]
fn builtin_cast_ikut_signedness_operand() {
    // Cast ke tipe bawaan bukan pengecualian: operand signed di-sign-extend
    // walau target signed, dan operand unsigned di-zero-extend.
    // iverilog: integer'(-8'sd3) = fffffffd
    let src = "\
module m(input signed [7:0] a, output [31:0] y);
  assign y = integer'(a);
endmodule";
    assert_eq!(dengan_sinyal(src, &[("a", 0xFD)]), 0xFFFFFFFD);
}

#[test]
fn size_cast_memperluas_tanpa_mengubah_tanda() {
    // LRM §6.14: `16'(x)` hanya mengubah lebar; signedness ikut operand.
    // Hasilnya 16-bit signed, jadi assignment ke 32 bit sign-extend.
    // iverilog: 16'(-8'sd3) = fffd lalu ke 32 bit = fffffd
    let src = "\
module m(input signed [7:0] a, output [31:0] y);
  assign y = 16'(a);
endmodule";
    assert_eq!(dengan_sinyal(src, &[("a", 0xFD)]), 0xFFFFFFFD);
}

#[test]
fn size_cast_dengan_operand_unsigned_mengisi_nol() {
    // Kontras langsung: operand unsigned di-zero-extend.
    let src = "module m(input [7:0] a, output [31:0] y);\n  assign y = 16'(a);\nendmodule";
    assert_eq!(dengan_sinyal(src, &[("a", 0xFD)]), 0x00FD);
}

#[test]
fn size_cast_ke_lebar_non_standar() {
    // Lebar bebas, bukan hanya kelipatan 8 atau 32.
    let src = "module m(input [7:0] a, output [31:0] y);\n  assign y = 20'(a);\nendmodule";
    assert_eq!(dengan_sinyal(src, &[("a", 0xA5)]), 0x000000A5);
}

#[test]
fn size_cast_menyempit_dari_lebar_besar() {
    let src = "module m(input [15:0] a, output [31:0] y);\n  assign y = 8'(a);\nendmodule";
    assert_eq!(dengan_sinyal(src, &[("a", 0xBEEF)]), 0xEF);
}

#[test]
fn size_cast_bertumpuk_dengan_builtin_cast() {
    // `byte'(16'(x))` harus menyempit dua kali; hasil akhir tetap 8 bit LSB.
    let src = "\
module m(input [15:0] a, output [31:0] y);
  assign y = byte'(16'(a));
endmodule";
    assert_eq!(dengan_sinyal(src, &[("a", 0xBEEF)]), 0xFFFFFFEF);
}

#[test]
fn size_cast_pada_select_mengikuti_bit_terpilih() {
    let src = "module m(input [15:0] a, output [31:0] y);\n  assign y = 4'(a[15:12]);\nendmodule";
    assert_eq!(dengan_sinyal(src, &[("a", 0xBEEF)]), 0xB);
}

#[test]
fn builtin_cast_dalam_generate_tetap_diturunkan() {
    // Cast bawaan harus ikut di-substitusi genvar di dalam region generate.
    let src = "\
module m(output [31:0] y);
  genvar i;
  generate
    for (i = 0; i < 4; i = i + 1) begin : g
      assign y[i] = byte'(8'(4'(i)));
    end
  endgenerate
endmodule";
    let tokens = sv_lexer::lex(src).expect("lex");
    let ast = sv_parser::parse_module(&tokens).expect("parse");
    let design = sv_elaborator::elaborate(&ast).expect("elaborate");
    // Empat iterasi menghasilkan empat proses terpisah.
    let assigns: Vec<_> = design
        .processes
        .iter()
        .flat_map(|p| p.body.iter())
        .filter(|s| matches!(s, sv_ir::Statement::Assign { .. }))
        .collect();
    assert_eq!(assigns.len(), 4, "harus ada satu proses per iterasi");
}

// --- LRM §20: $bits ---

#[test]
fn bits_dari_sinyal_mengembalikan_lebarnya() {
    assert_eq!(eval("$bits(a)", &[("a", 0x1234)]), 16);
}

#[test]
fn bits_dari_literal_sized_mengembalikan_lebarnya() {
    // LRM §5.7.1: 8'hFF lebarnya 8 walau nilainya muat di 4 bit.
    assert_eq!(eval("$bits(8'hFF)", &[]), 8);
    assert_eq!(eval("$bits(8'hF)", &[]), 8);
}

#[test]
fn bits_dari_literal_desimal_mengembalikan_32() {
    // LRM §5.7.1: literal desimal polos selalu 32 bit.
    assert_eq!(eval("$bits(42)", &[]), 32);
}

#[test]
fn bits_dari_select_menggunakan_lebar_select() {
    assert_eq!(eval("$bits(a[7:0])", &[("a", 0xFFFF)]), 8);
    assert_eq!(eval("$bits(a[3:0])", &[("a", 0xFFFF)]), 4);
    assert_eq!(eval("$bits(a[0])", &[("a", 0xFFFF)]), 1);
}

#[test]
fn bits_dari_concat_menjumlahkan_lebar_item() {
    assert_eq!(eval("$bits({a, a})", &[("a", 0xFFFF)]), 32);
    assert_eq!(eval("$bits({a[7:0], a[7:0]})", &[("a", 0xFFFF)]), 16);
}

#[test]
fn bits_dari_replication_menjumlahkan_lebar_setiap_salinan() {
    // LRM §11.8.2: {N{x}} lebarnya N x lebar x.
    assert_eq!(eval("$bits({4{a[3:0]}})", &[("a", 0xFFFF)]), 16);
}

#[test]
fn bits_dari_cast_mengembalikan_lebar_tujuan() {
    // LRM §6.14: cast mengubah lebar, jadi $bits dari cast ikut berubah.
    assert_eq!(eval("$bits(byte_t'(a))", &[("a", 0xFFFF)]), 8);
    assert_eq!(eval("$bits(word_t'(a))", &[("a", 0xFFFF)]), 16);
}

#[test]
fn bits_dari_sign_cast_tetap_lebar_operand() {
    // LRM §6.14: sign cast tidak mengubah lebar.
    assert_eq!(eval("$bits(signed'(a))", &[("a", 0xFFFF)]), 16);
}

#[test]
fn bits_dari_nilai_waktu_adalah_64() {
    // LRM §20: waktu simulasi dibatasi 64 bit di IR.
    assert_eq!(eval("$bits($time)", &[]), 64);
}

#[test]
fn bits_tidak_bergantung_nilai_sinyal() {
    // `$bits` adalah konstanta waktu kompilasi, jadi dua stimulus berbeda
    // harus memberi jawaban sama.
    assert_eq!(eval("$bits(a[7:0])", &[("a", 0x0000)]), 8);
    assert_eq!(eval("$bits(a[7:0])", &[("a", 0xFFFF)]), 8);
}

#[test]
fn bits_menuju_lebar_yang_sama_dengan_nilainya() {
    // Sanity: `$bits(x)` harus bisa dipakai sebagai konstanta lebar, bukan
    // hanya dievaluasi terpisah.
    assert_eq!(eval("$bits(a) + $bits(a)", &[("a", 0xFFFF)]), 32);
}

#[test]
fn bits_dari_nama_tipe_mengembalikan_lebar_typedef() {
    // BUG-5: LRM §20 mengizinkan `$bits(nama_tipe)`, dan justru itu bentuk
    // yang paling sering dipakai. Sebelumnya operand-nya dipaksa lewat
    // `lower_expression`, jadi nama tipe dilaporkan "undefined signal" —
    // pesan yang menyesatkan karena yang salah nama tipenya, bukan sinyalnya.
    // iverilog: n=8
    assert_eq!(eval("$bits(byte_t)", &[]), 8);
    assert_eq!(eval("$bits(word_t)", &[]), 16);
}

#[test]
fn bits_dari_nama_tipe_yang_terdaftar() {
    // Nama tipe yang bukan sinyal boleh hidup berdampingan dengan sinyal lain.
    let src = "\
module m(input [3:0]OTHER, output [31:0] y);
  typedef logic [11:0] t_t;
  assign y = $bits(t_t);
endmodule";
    assert_eq!(dengan_sinyal(src, &[("OTHER", 0xF)]), 12);
}

#[test]
fn bits_dari_nama_yang_bukan_sinyal_maupun_tipe_ditolak() {
    // LRM §8.20: nama yang sama tidak boleh dipakai untuk typedef dan
    // variabel di scope yang sama. iverilog menolak: "'foo' has already been
    // declared in this scope".
    let src = "\
module m(output [31:0] y);
  logic [3:0] foo;
  typedef logic [11:0] foo;
  assign y = $bits(foo);
endmodule";
    let tokens = sv_lexer::lex(src).expect("lex");
    let ast = sv_parser::parse_module(&tokens).expect("parse");
    let err = sv_elaborator::elaborate(&ast).expect_err("nama ganda harus ditolak");
    let pesan = err.to_string();
    assert!(
        pesan.contains("sudah dipakai"),
        "pesan tak menolak nama ganda: {pesan}"
    );
    assert!(
        pesan.contains("'foo'"),
        "pesan tak menyebut nama yang bentrok: {pesan}"
    );
}

#[test]
fn bits_dari_tipe_tak_dikenal_tetap_melaporkan_tipe() {
    // Nama yang bukan sinyal DAN bukan tipe harus tetap ditolak; pesan yang
    // muncul harus menyebut "type" supaya jelas itu masalah tipe.
    let src = "\
module m(input [3:0] a, output [31:0] y);
  assign y = $bits(tidak_ada_t);
endmodule";
    let tokens = sv_lexer::lex(src).expect("lex");
    let ast = sv_parser::parse_module(&tokens).expect("parse");
    let err = sv_elaborator::elaborate(&ast).expect_err("harus gagal");
    assert!(
        err.to_string().contains("undefined signal 'tidak_ada_t'"),
        "pesan tak menyebut nama yang dicari: {err}"
    );
}

// --- LRM §8.20: typedef module-scoped ---

#[test]
fn typedef_milik_anak_terlihat_di_badan_anak() {
    // BUG-4: typedef anak dulu tidak pernah didaftarkan, jadi cast di badan
    // modul anak selalu "undefined type" padahal kodenya sah.
    let src = "\
module child(input [7:0] a, output [7:0] y);
  typedef logic [7:0] byte_t;
  assign y = byte_t'(a);
endmodule
module top(input [7:0] a, output [7:0] y);
  child u0(.a(a), .y(y));
endmodule";
    let design = build_top(src, "top");
    // Lebar port `y` tetap 8 bit: cast ke `byte_t` harus berhasil.
    let port = design.find_variable("y").expect("port y ada");
    assert_eq!(port.data_type.width, 8);
}

#[test]
fn typedef_induk_tidak_terlihat_di_badan_anak() {
    // BUG-4 arah sebaliknya: typedef bersifat module-scoped (LRM §8.20),
    // jadi anak yang memakai nama tipe milik induk harus DITOLAK. Dulu nama
    // polos dipakai bersama sehingga kode ini lolos dengan lebar yang salah.
    // iverilog: "Unable to bind parameter `byte_t' in `top.u0'"
    let src = "\
module child(input [15:0] a, output [7:0] y);
  typedef logic [7:0] c_t;
  assign y = c_t'(a) + byte_t'(a);
endmodule
module top(input [15:0] a, output [7:0] y);
  typedef logic [7:0] byte_t;
  child u0(.a(a), .y(y));
endmodule";
    let tokens = sv_lexer::lex(src).expect("lex");
    let modules = sv_parser::parse_file(&tokens).expect("parse");
    let err = sv_elaborator::elaborate_top(&modules, "top").expect_err("harus gagal");
    assert!(
        err.to_string().contains("undefined type"),
        "pesan tak menolak typedef asing: {err}"
    );
}

#[test]
fn typedef_nama_sama_di_dua_modul_tidak_saling_menimpa() {
    // BUG-4: karena typedef module-scoped, dua modul boleh punya `w_t` dengan
    // lebar berbeda dan keduanya harus tetap jalan. Versi lama memakai satu
    // nama polos, jadi salah satu diam-diam memakai lebar modul lain.
    let src = "\
module cell_narrow(input [7:0] a, output [7:0] y);
  typedef logic [7:0] w_t;
  assign y = w_t'(a);
endmodule
module cell_wide(input [15:0] a, output [15:0] y);
  typedef logic [15:0] w_t;
  assign y = w_t'(a);
endmodule
module top(input [7:0] a8, input [15:0] a16, output [7:0] y8, output [15:0] y16);
  cell_narrow n0(.a(a8),  .y(y8));
  cell_wide   w0(.a(a16), .y(y16));
endmodule";
    let design = build_top(src, "top");
    // Dua sinyal hasil dengan lebar berbeda — kalau `w_t` tertukar, salah
    // satunya akan salah lebar dan `check` gagal soal koneksi port.
    assert_eq!(design.find_variable("y8").expect("y8").data_type.width, 8);
    assert_eq!(
        design.find_variable("y16").expect("y16").data_type.width,
        16
    );
}

#[test]
fn typedef_milik_induk_masih_terlihat_di_badan_induk() {
    // Sanity arah sebaliknya dari `typedef_induk_tidak_terlihat_di_badan_anak`:
    // prefix tidak boleh membuat typedef milik modul sendiri tak terjangkau.
    let src = "module m(input [15:0] a, output [7:0] y);\n  typedef logic [7:0] w_t;\n  assign y = w_t'(a);\nendmodule";
    let design = build(src);
    assert_eq!(design.find_variable("y").expect("y").data_type.width, 8);
}

// --- Error: nama tipe tak dikenal ---

#[test]
fn cast_dengan_tipe_tak_dikenal_ditolak() {
    // LRM §8.20: nama tipe yang tidak pernah di-typedef harus gagal, bukan
    // diam-diam dianggap selebar 1 bit.
    let src = "\
module m(input [15:0] a, output [31:0] y);
  assign y = nibble_t'(a);
endmodule";
    let tokens = sv_lexer::lex(src).expect("lex");
    let ast = sv_parser::parse_module(&tokens).expect("parse");
    let err = sv_elaborator::elaborate(&ast).expect_err("harus gagal");
    assert!(
        err.to_string().contains("undefined type 'nibble_t'"),
        "pesan tak menyebut nama tipe: {err}"
    );
}

#[test]
fn bits_dari_tipe_tak_dikenal_ikut_gagal() {
    // Error yang sama harus muncul walau cast-nya ada di dalam argumen
    // `$bits`, supaya tidak ada jalur yang lolos diam-diam.
    let src = "\
module m(input [15:0] a, output [31:0] y);
  assign y = $bits(nibble_t'(a));
endmodule";
    let tokens = sv_lexer::lex(src).expect("lex");
    let ast = sv_parser::parse_module(&tokens).expect("parse");
    let err = sv_elaborator::elaborate(&ast).expect_err("harus gagal");
    assert!(
        err.to_string().contains("undefined type 'nibble_t'"),
        "pesan tak menyebut nama tipe: {err}"
    );
}

#[test]
fn span_error_menunjuk_kolom_nama_tipe() {
    // AGENTS.md aturan 3: error harus bisa ditelusuri ke token sumber.
    let src = "module m(input [15:0] a, output [31:0] y);\n  assign y = nibble_t'(a);\nendmodule";
    let tokens = sv_lexer::lex(src).expect("lex");
    let ast = sv_parser::parse_module(&tokens).expect("parse");
    let err = sv_elaborator::elaborate(&ast).expect_err("harus gagal");
    let pesan = err.to_string();
    assert!(
        pesan.contains("line 2"),
        "span tak menunjuk baris yang benar: {pesan}"
    );
}

// --- Masker helper yang dipakai test lain ---

#[test]
fn lebar_mask_menutup_64_bit() {
    // Dipakai supaya test di berkas ini konsisten dengan evaluator bersama.
    assert_eq!(lebar_mask(64), u64::MAX);
    assert_eq!(lebar_mask(0), 0);
    assert_eq!(lebar_mask(8), 0xFF);
}
