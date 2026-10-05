// Tanggung jawab: uji operator geser pada jumlah geser >= lebar logis.
mod support;

use support::{build, evaluasi, nilai_untuk};
use sv_ir::Expr;

/// Cari node `BinOp::Sar` pertama dan kembalikan flag signed tipe HASILNYA.
///
/// Justru tipe hasil yang dicari, bukan signedness operand kiri: LRM §11.4.10
/// menetapkan bit pengisi `>>>` dari signedness tipe hasil, dan §11.6.1 langkah 3
/// membuat operand kiri yang context-determined mengikuti tipe hasil itu.
fn cari_sar_signed(expr: &Expr) -> Option<bool> {
    match expr {
        Expr::Bin {
            op: sv_ir::BinOp::Sar,
            data_type,
            ..
        } => Some(data_type.signed),
        Expr::Bin { lhs, rhs, .. } => cari_sar_signed(lhs).or_else(|| cari_sar_signed(rhs)),
        Expr::Cast { operand, .. } => cari_sar_signed(operand),
        Expr::Ternary {
            condition,
            when_true,
            when_false,
            ..
        } => cari_sar_signed(condition)
            .or_else(|| cari_sar_signed(when_true))
            .or_else(|| cari_sar_signed(when_false)),
        Expr::Un { operand, .. } => cari_sar_signed(operand),
        _ => None,
    }
}

/// BUG: bit pengisi `>>>` mengabaikan signedness KONTEKS luar.
///
/// `c ? (a >>> 1) : 8'h00` — cabang `true` bertipe signed, cabang `false`
/// unsigned, jadi tipe hasil ternary unsigned (LRM §11.6.1). Karena operand
/// kiri geser context-determined, ia harus ikut jadi unsigned dan `>>>` mengisi
/// nol. svrust memakai `tipe_kiri.signed` dan mengisi sign: iverilog 12.0 dan
/// verilator 5.020 sama-sama memberi `0x78`, svrust memberi `0xf8`.
#[test]
fn geser_kanan_aritmetik_ikuti_signedness_konteks_ternary() {
    let design = build(
        "module m(input signed [7:0] a, input c, output [7:0] y);
         assign y = c ? (a >>> 1) : 8'h00; endmodule",
    );
    let nilai = nilai_untuk(&design, "y");
    let signed =
        cari_sar_signed(&nilai).unwrap_or_else(|| panic!("node Sar tidak ditemukan: {nilai:?}"));
    assert!(
        !signed,
        "konteks ternary unsigned harus membuat pengisi `>>>` nol: {nilai:?}"
    );
}

/// Regresi: konteks signed (kedua cabang ternary signed) tetap mengisi sign.
#[test]
fn geser_kanan_aritmetik_konteks_signed_masih_mengisi_sign() {
    let design = build(
        "module m(input signed [7:0] a, input c, output signed [7:0] y);
         assign y = c ? (a >>> 1) : 8'sh00; endmodule",
    );
    let nilai = nilai_untuk(&design, "y");
    let signed =
        cari_sar_signed(&nilai).unwrap_or_else(|| panic!("node Sar tidak ditemukan: {nilai:?}"));
    assert!(
        signed,
        "dua cabang signed harus membuat pengisi `>>>` tetap sign: {nilai:?}"
    );
}

/// `a >> 64` dan `a << 64` harus bernilai 0, bukan panic.
///
/// Jalur `Shl`/`Shr` di `support::evaluasi` masih memakai operator Rust
/// mentah (`a << b` / `a >> b`). Operator itu PANIC di build debug saat
/// `b >= 64` ("attempt to shift right with overflow"), padahal
/// `sv_runtime::geser_kiri`/`geser_kanan_logis` mengembalikan 0 untuk jumlah
/// yang melebihi lebar logis. Akibatnya integration test sv-elaborator tidak
/// bisa menulis regresi untuk `>>`/`<<` pada jumlah geser >= 64 sama sekali —
/// persis kasus yang LRM §11.4.10 tetapkan dan yang runtime sudah dukung.
#[test]
fn geser_jumlah_ge_64_pada_lebar_kecil_tidak_panik() {
    let mut sinyal = vec![0u64; 1];
    sinyal[0] = 0xFF;

    let kanan = build("module m(input [7:0] a, output [7:0] y); assign y = a >> 64; endmodule");
    assert_eq!(
        evaluasi(&nilai_untuk(&kanan, "y"), &mut sinyal),
        0x00,
        "8'hFF >> 64 harus 0"
    );

    let kiri = build("module m(input [7:0] a, output [7:0] y); assign y = a << 64; endmodule");
    assert_eq!(
        evaluasi(&nilai_untuk(&kiri, "y"), &mut sinyal),
        0x00,
        "8'hFF << 64 harus 0"
    );

    // `>>>` pada tipe hasil unsigned sama dengan `>>` (LRM §11.4.10).
    let aritmetik =
        build("module m(input [7:0] a, output [7:0] y); assign y = a >>> 64; endmodule");
    assert_eq!(
        evaluasi(&nilai_untuk(&aritmetik, "y"), &mut sinyal),
        0x00,
        "8'hFF >>> 64 harus 0"
    );
}

/// `64'h8000_0000_0000_0000 >>> 64` harus 0 pada tipe hasil unsigned.
///
/// Helper `geser_kanan_aritmetik` di `support` meng-clamp jumlah geser ke 63
/// lewat `jumlah.min(63)`, jadi pada lebar 64 bit 63 tetap tertinggal dan
/// hasilnya 0x1, bukan 0. Jalur ini dipakai `evaluasi` sebagai oracle bagi
/// seluruh integration test sv-elaborator, jadi helper yang menyimpang
/// membuat test yang meng-assert nilai salah tetap hijau.
#[test]
fn geser_kanan_aritmetik_jumlah_64_pada_lebar_64_mengosongkan_semua_bit() {
    let unsigned =
        build("module m(output [63:0] y); assign y = 64'h8000_0000_0000_0000 >>> 64; endmodule");
    assert_eq!(
        evaluasi(&nilai_untuk(&unsigned, "y"), &mut vec![0u64; 1]),
        0x0,
        "64'h8000_0000_0000_0000 >>> 64 harus 0"
    );

    // Tipe hasil signed: seluruh bit terisi sign (iverilog: ffffffffffffffff).
    let signed = build(
        "module m(output signed [63:0] y); assign y = 64'sh8000_0000_0000_0000 >>> 64; endmodule",
    );
    assert_eq!(
        evaluasi(&nilai_untuk(&signed, "y"), &mut vec![0u64; 1]),
        u64::MAX,
        "64'sh8000_0000_0000_0000 >>> 64 harus semua bit sign"
    );

    // Jumlah jauh di atas 64 juga harus kosong.
    let jauh =
        build("module m(output [63:0] y); assign y = 64'hFFFF_FFFF_FFFF_FFFF >>> 100; endmodule");
    assert_eq!(
        evaluasi(&nilai_untuk(&jauh, "y"), &mut vec![0u64; 1]),
        0x0,
        "64'hFFFF_FFFF_FFFF_FFFF >>> 100 harus 0"
    );
}
