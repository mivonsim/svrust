// Tanggung jawab: evaluasi `$signed` / `$unsigned` (LRM §11.4.7, §20).
//
// Bentuk system function-nya identik dengan `signed'(x)` / `unsigned'(x)`
// (LRM §6.14): hanya mengubah signedness, lebar TIDAK berubah. Parsing-nya
// diuji di `sv-parser` (`signcast_test.rs`).
//
// Semua nilai diverifikasi terhadap iverilog 12.0 (`-g2012`).
mod support;

/// Source dengan `ua` dan `sa` sudah diisi, lalu `y` diberi nilai `rhs`.
///
/// `y` selebar 32 bit, jadi assignment ikut sign-extend kalau `rhs` bertanda.
/// Itu perilaku LRM §11.6.1 yang disengaja, jadi ekspektasi test menyertakannya
/// — dengan sink 8 bit, `$signed(ua)` akan terlihat apa adanya (0xFD).
/// `lebar_y` adalah bit TERAKHIR indeks, jadi lebarnya `lebar_y + 1` —
/// dipakai agar penulisan `[7:0]` terbaca apa adanya.
fn src(rhs: &str, lebar_y: u32) -> String {
    format!(
        "module m;\n  \
         logic [7:0] ua;\n  \
         logic signed [7:0] sa;\n  \
         logic [{lebar_y}:0] y;\n  \
         initial begin ua = 8'hFD; sa = -8'sd3; #1; y = {rhs}; end\n\
         endmodule\n"
    )
}

/// Nilai `y` untuk `ua = sa = 0xFD`.
fn nilai(rhs: &str) -> u64 {
    nilai_lebar(rhs, 32)
}

fn nilai_lebar(rhs: &str, lebar_y: u32) -> u64 {
    let d = support::build(&src(rhs, lebar_y));
    let expr = support::nilai_untuk(&d, "y");
    let mut signals = vec![0u64; d.variables.len()];
    for nama in ["ua", "sa"] {
        let var = d.find_variable(nama).expect("sinyal ada");
        signals[var.signal_id as usize] = 0xFD;
    }
    let v = support::evaluasi(&expr, &mut signals);
    v & support::lebar_mask(lebar_y)
}

#[test]
fn signed_membaca_nilai_sebagai_tanda() {
    // iverilog: S1=-3 — dua's complement 8 bit dari -3 adalah 0xFD.
    assert_eq!(nilai_lebar("$signed(ua)", 8), 0xFD);
}

#[test]
fn unsigned_membaca_nilai_sebagai_penuh() {
    // iverilog: S2=253 — `$unsigned` TIDAK memperlebar ke 32 bit, jadi
    // hasilnya tetap 8 bit: 0xFD = 253.
    assert_eq!(nilai_lebar("$unsigned(sa)", 8), 253);
}

#[test]
fn signed_tidak_memperlebar_lebar() {
    // iverilog: S3=ff (bukan ffff) — hanya signedness yang berubah.
    assert_eq!(nilai_lebar("$signed(8'hFF)", 8), 0xFF);
}

#[test]
fn unsigned_tidak_memperlebar_lebar() {
    // iverilog: S4=ffff
    assert_eq!(nilai_lebar("$unsigned(16'hFFFF)", 16), 0xFFFF);
}

#[test]
fn bits_dari_signed_mengikuti_lebar_asli() {
    // iverilog: S5=8
    assert_eq!(nilai("$bits($signed(ua))"), 8);
}

#[test]
fn assignment_sign_extend_hasil_signed() {
    // Bentuk-bentuk di atas memakai sink selebar operandnya. Dengan sink lebih
    // lebar, aturan assignment LRM §11.6.1 ikut berlaku dan yang menentukan
    // arah perluasan adalah signedness EKSPRESINYA — bukan operand aslinya.
    //
    // iverilog: T1=fd, T2=fd (dicetak pada lebar ekspresinya sendiri).
    // `$signed(ua)` membuat ekspresinya signed 8-bit, jadi assignment ke 33 bit
    // sign-extend: -3 -> 0x1FFFFFFFFFD, dan setelah dimask 32 bit jadi 0xFFFFFFFD.
    assert_eq!(nilai("$signed(ua)"), 0xFFFFFFFD);
    // `$unsigned(sa)` membuat ekspresinya unsigned 8-bit, jadi zero-extend.
    assert_eq!(nilai("$unsigned(sa)"), 253);
}

#[test]
fn signed_dan_sign_cast_setara() {
    assert_eq!(nilai("$signed(ua)"), nilai("signed'(ua)"));
    assert_eq!(nilai("$unsigned(sa)"), nilai("unsigned'(sa)"));
}

#[test]
fn perbandingan_tetap_unsigned_kalau_salah_satu_operand_unsigned() {
    // LRM §11.4.5: perbandingan bertanda hanya bila KEDUA operand signed.
    // Karena `ua` unsigned, kedua perbandingan di bawah tetap unsigned:
    // 0xFD < 0xFD adalah false. iverilog: T3=0, T4=0.
    //
    // Ini yang membuat `$signed(sa)` tidak otomatis membuat lhs bertanda.
    assert_eq!(nilai("(ua < $unsigned(sa)) ? 32'd1 : 32'd0"), 0);
    assert_eq!(nilai("(ua < $signed(sa)) ? 32'd1 : 32'd0"), 0);
}

#[test]
fn perbandingan_bertanda_bila_kedua_operand_signed() {
    // LRM §11.4.5: dua operand signed -> perbandingan bertanda.
    // -3 < 1 benar. iverilogjol: 1
    let d = support::build(
        "module m;\n  \
         logic signed [7:0] sb;\n  \
         logic signed [7:0] sc;\n  \
         logic [31:0] y;\n  \
         initial begin sb = -8'sd3; sc = 8'sd1; #1; y = (sb < sc) ? 32'd1 : 32'd0; end\n\
         endmodule\n",
    );
    let expr = support::nilai_untuk(&d, "y");
    let mut signals = vec![0u64; d.variables.len()];
    for nama in ["sb", "sc"] {
        let var = d.find_variable(nama).expect("sinyal ada");
        signals[var.signal_id as usize] = if nama == "sb" { 0xFD } else { 0x01 };
    }
    assert_eq!(support::evaluasi(&expr, &mut signals), 1);
}

#[test]
fn signed_dalam_kondisi_generate_didukung() {
    // `konst` harus memahami `$signed` supaya kondisi generate tetap bisa
    // mengevaluasinya.
    let tokens = sv_lexer::lex(
        "module m #(parameter W = 4) (output logic [31:0] y);\n\
         generate\n\
           if ($signed(W) > 0) begin : g assign y = 32'hAA; end\n\
           else begin : h assign y = 32'hBB; end\n\
         endgenerate\n\
         endmodule",
    )
    .expect("lex");
    let m = sv_parser::parse_module(&tokens).expect("parse");
    let d = sv_elaborator::elaborate(&m).expect("elaborate");
    let expr = support::nilai_untuk(&d, "y");
    let mut signals = vec![0u64; d.variables.len()];
    assert_eq!(support::evaluasi(&expr, &mut signals), 0xAA);
}
