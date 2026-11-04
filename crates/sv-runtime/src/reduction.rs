// Tanggung jawab: implementasi unary dan reduction operator SystemVerilog di runtime.
/// Nilai selalu dimask ke lebar `N` agar bit di luar lebar operand tidak bocor.
use crate::bits::Bits;
use crate::logic::Logic;

/// Masker seluruh bit set pada lebar `N`.
#[inline]
fn mask(n: usize) -> u64 {
    if n == 0 {
        0
    } else if n >= 64 {
        u64::MAX
    } else {
        (1u64 << n) - 1
    }
}

/// `~a` — nolkan tiap bit pada lebar logis N (LRM §11.4.3).
///
/// Lebar logis `N` boleh lebih kecil dari lebar penyimpanan `M`; bit di atas
/// `N` diabaikan agar tepat `N` bit yang dibalik, sesuai semantik SV. Digit
/// X/Z pada input menjadi X pada output.
#[inline]
pub fn bit_not<const N: usize, const M: usize>(value: Bits<M>) -> Bits<M> {
    if !value.is_fully_known() {
        // Hanya periksa bit di dalam lebar logis N; X di luar lebar logis
        // tidak relevan.
        let mut ada_tidak_diketahui = false;
        for i in 0..N.min(M) {
            if matches!(value.get(i), Logic::X | Logic::Z) {
                ada_tidak_diketahui = true;
                break;
            }
        }
        if ada_tidak_diketahui {
            return Bits::<M>::all(Logic::X);
        }
    }
    Bits::from_u64(!value.to_u64() & mask(N))
}

/// `-a` — negasi dua's complement pada lebar logis N (LRM §11.4.7).
///
/// Dua's complement adalah `2^N - x`, jadi basisnya `2^N` (bukan `2^N - 1`
/// seperti `mask`). `wrapping_neg` pada lebar u64 diikuti mask menghasilkan
/// hasil yang benar untuk semua N <= 64. Operand X/Z membuat hasil X.
#[inline]
pub fn bit_neg<const N: usize, const M: usize>(value: Bits<M>) -> Bits<M> {
    if !value.is_fully_known() {
        let mut ada_tidak_diketahui = false;
        for i in 0..N.min(M) {
            if matches!(value.get(i), Logic::X | Logic::Z) {
                ada_tidak_diketahui = true;
                break;
            }
        }
        if ada_tidak_diketahui {
            return Bits::<M>::all(Logic::X);
        }
    }
    Bits::from_u64((value.to_u64() & mask(N)).wrapping_neg() & mask(N))
}

/// `&a` — reduksi AND: 1 bila semua dari N bit bernilai 1 (LRM §11.4.9).
///
/// Digit X/Z membuat hasil X (AND dengan X adalah X).
#[inline]
pub fn reduce_and<const N: usize, const M: usize>(value: Bits<M>) -> Bits<1> {
    let mut ada_x = false;
    let mut semua_satu = true;
    for i in 0..N.min(M) {
        match value.get(i) {
            Logic::One => {}
            Logic::Zero => semua_satu = false,
            Logic::X | Logic::Z => ada_x = true,
        }
    }
    if ada_x {
        Bits::from_logic(Logic::X)
    } else {
        Bits::from_u64(u64::from(semua_satu))
    }
}

/// `~&a` — reduksi NAND.
#[inline]
pub fn reduce_nand<const N: usize, const M: usize>(value: Bits<M>) -> Bits<1> {
    let mut ada_x = false;
    let mut semua_satu = true;
    for i in 0..N.min(M) {
        match value.get(i) {
            Logic::One => {}
            Logic::Zero => semua_satu = false,
            Logic::X | Logic::Z => ada_x = true,
        }
    }
    if ada_x {
        Bits::from_logic(Logic::X)
    } else {
        Bits::from_u64(u64::from(!semua_satu))
    }
}

/// `|a` — reduksi OR: 1 bila ada bit 1 pada N bit pertama.
///
/// Digit X/Z membuat hasil X bila tidak ada bit 1 yang pasti.
#[inline]
pub fn reduce_or<const N: usize, const M: usize>(value: Bits<M>) -> Bits<1> {
    let mut ada_x = false;
    let mut ada_satu = false;
    for i in 0..N.min(M) {
        match value.get(i) {
            Logic::One => ada_satu = true,
            Logic::Zero => {}
            Logic::X | Logic::Z => ada_x = true,
        }
    }
    if ada_x && !ada_satu {
        Bits::from_logic(Logic::X)
    } else {
        Bits::from_u64(u64::from(ada_satu))
    }
}

/// `~|a` — reduksi NOR.
#[inline]
pub fn reduce_nor<const N: usize, const M: usize>(value: Bits<M>) -> Bits<1> {
    let mut ada_x = false;
    let mut ada_satu = false;
    for i in 0..N.min(M) {
        match value.get(i) {
            Logic::One => ada_satu = true,
            Logic::Zero => {}
            Logic::X | Logic::Z => ada_x = true,
        }
    }
    if ada_x && !ada_satu {
        Bits::from_logic(Logic::X)
    } else {
        Bits::from_u64(u64::from(!ada_satu))
    }
}

/// `^a` — reduksi XOR: paritas N bit pertama.
///
/// Digit X/Z membuat hasil X.
#[inline]
pub fn reduce_xor<const N: usize, const M: usize>(value: Bits<M>) -> Bits<1> {
    let mut ada_x = false;
    let mut paritas = 0u32;
    for i in 0..N.min(M) {
        match value.get(i) {
            Logic::One => paritas += 1,
            Logic::Zero => {}
            Logic::X | Logic::Z => ada_x = true,
        }
    }
    if ada_x {
        Bits::from_logic(Logic::X)
    } else {
        Bits::from_u64(u64::from(paritas % 2))
    }
}

/// `~^a` — reduksi XNOR: kebalikan paritas.
///
/// Digit X/Z membuat hasil X.
#[inline]
pub fn reduce_xnor<const N: usize, const M: usize>(value: Bits<M>) -> Bits<1> {
    let mut ada_x = false;
    let mut paritas = 0u32;
    for i in 0..N.min(M) {
        match value.get(i) {
            Logic::One => paritas += 1,
            Logic::Zero => {}
            Logic::X | Logic::Z => ada_x = true,
        }
    }
    if ada_x {
        Bits::from_logic(Logic::X)
    } else {
        Bits::from_u64(u64::from(paritas % 2 == 0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bit_not_membalik_semua_bit_dalam_lebar() {
        // 4'b1010 -> 4'b0101
        assert_eq!(bit_not::<4, 4>(Bits::from_u64(0b1010)).to_u64(), 0b0101);
    }

    #[test]
    fn bit_not_tidak_membocorkan_bit_atas() {
        // 4'b0000 -> 4'b1111, bukan 0xffff_fff0.
        assert_eq!(bit_not::<4, 4>(Bits::from_u64(0)).to_u64(), 0b1111);
    }

    #[test]
    fn bit_not_satu_bit() {
        assert_eq!(bit_not::<1, 1>(Bits::from_u64(1)).to_u64(), 0);
        assert_eq!(bit_not::<1, 1>(Bits::from_u64(0)).to_u64(), 1);
    }

    #[test]
    fn bit_neg_menghasilkan_dua_s_complement() {
        assert_eq!(bit_neg::<8, 8>(Bits::from_u64(5)).to_u64(), 251);
        assert_eq!(bit_neg::<8, 8>(Bits::from_u64(0)).to_u64(), 0);
        assert_eq!(bit_neg::<4, 4>(Bits::from_u64(1)).to_u64(), 0b1111);
    }

    #[test]
    fn bit_neg_basis_dua_pangkat_n_bukan_minus_satu() {
        // 4'b0001 -> 4'b1111. Basis harus 2^4 = 16; bila salah jadi 15
        // maka hasilnya 4'b1110 (14) dan test ini gagal.
        assert_eq!(bit_neg::<4, 4>(Bits::from_u64(1)).to_u64(), 15);
        // 8'd2 -> 8'd254 (256 - 2).
        assert_eq!(bit_neg::<8, 8>(Bits::from_u64(2)).to_u64(), 254);
    }

    #[test]
    fn bit_neg_nilai_maksimum_menjadi_satu() {
        // 8'hFF adalah -1, jadi negasinya 1.
        assert_eq!(bit_neg::<8, 8>(Bits::from_u64(255)).to_u64(), 1);
    }

    #[test]
    fn bit_neg_lebar_64_membungkus_dengan_benar() {
        assert_eq!(bit_neg::<64, 64>(Bits::from_u64(5)).to_u64(), u64::MAX - 4);
    }

    #[test]
    fn reduce_and_true_hanya_semua_bit_satu() {
        assert_eq!(reduce_and::<4, 4>(Bits::from_u64(0b1111)).to_u64(), 1);
        assert_eq!(reduce_and::<4, 4>(Bits::from_u64(0b1110)).to_u64(), 0);
        assert_eq!(reduce_and::<4, 4>(Bits::from_u64(0b0000)).to_u64(), 0);
    }

    #[test]
    fn reduce_nand_kebalikan_and() {
        assert_eq!(reduce_nand::<4, 4>(Bits::from_u64(0b1111)).to_u64(), 0);
        assert_eq!(reduce_nand::<4, 4>(Bits::from_u64(0b1010)).to_u64(), 1);
    }

    #[test]
    fn reduce_or_true_bila_ada_bit_satu() {
        assert_eq!(reduce_or::<4, 4>(Bits::from_u64(0b0000)).to_u64(), 0);
        assert_eq!(reduce_or::<4, 4>(Bits::from_u64(0b1000)).to_u64(), 1);
        assert_eq!(reduce_or::<4, 4>(Bits::from_u64(0b1111)).to_u64(), 1);
    }

    #[test]
    fn reduce_nor_kebalikan_or() {
        assert_eq!(reduce_nor::<4, 4>(Bits::from_u64(0b0000)).to_u64(), 1);
        assert_eq!(reduce_nor::<4, 4>(Bits::from_u64(0b0001)).to_u64(), 0);
    }

    #[test]
    fn reduce_xor_menghitung_paritas() {
        // Dua bit satu => genap => 0.
        assert_eq!(reduce_xor::<4, 4>(Bits::from_u64(0b0011)).to_u64(), 0);
        // Tiga bit satu => ganjil => 1.
        assert_eq!(reduce_xor::<4, 4>(Bits::from_u64(0b0111)).to_u64(), 1);
        assert_eq!(reduce_xor::<4, 4>(Bits::from_u64(0b0000)).to_u64(), 0);
    }

    #[test]
    fn reduce_xnor_kebalikan_xor() {
        assert_eq!(reduce_xnor::<4, 4>(Bits::from_u64(0b0011)).to_u64(), 1);
        assert_eq!(reduce_xnor::<4, 4>(Bits::from_u64(0b0111)).to_u64(), 0);
    }

    #[test]
    fn reduce_mengabaikan_bit_di_atas_lebar() {
        // Selector 4-bit, bit atas harus diabaikan walau Bits menyimpan lebih.
        assert_eq!(reduce_and::<4, 8>(Bits::from_u64(0b1111_1111)).to_u64(), 1);
    }

    #[test]
    fn reduce_xor_lebar_64_menghitung_paritas_penuh() {
        let nilai = u64::MAX; // 64 bit satu => genap
        assert_eq!(reduce_xor::<64, 64>(Bits::from_u64(nilai)).to_u64(), 0);
        assert_eq!(reduce_xor::<63, 64>(Bits::from_u64(nilai)).to_u64(), 1);
    }

    #[test]
    fn helper_menerima_lebar_simpan_lebih_besar() {
        // N=4 logis tapi M=8 penyimpanan: hanya 4 bit bawah yang dihitung.
        // Bit di posisi 4..7 harus diabaikan walau tersimpan penuh.
        assert_eq!(reduce_or::<4, 8>(Bits::from_u64(0b1111_0000)).to_u64(), 0);
        assert_eq!(reduce_or::<4, 8>(Bits::from_u64(0b0001_0000)).to_u64(), 0);
        assert_eq!(reduce_or::<4, 8>(Bits::from_u64(0b0000_0001)).to_u64(), 1);
    }

    #[test]
    fn bit_not_dengan_lebar_simpan_lebih_besar() {
        // N=4 logis: ~0000_0000 pada 4 bit = 0000_1111, bukan 1111_1111.
        assert_eq!(bit_not::<4, 8>(Bits::from_u64(0)).to_u64(), 0b0000_1111);
    }

    #[test]
    fn bit_not_dengan_x_hasil_x() {
        let mut v = Bits::<8>::zero();
        v.set(2, Logic::X);
        let r = bit_not::<8, 8>(v);
        assert!(!r.is_fully_known());
        assert_eq!(r.get(2), Logic::X);
    }

    #[test]
    fn bit_neg_dengan_x_hasil_x() {
        let mut v = Bits::<8>::zero();
        v.set(3, Logic::Z);
        let r = bit_neg::<8, 8>(v);
        assert!(!r.is_fully_known());
        assert_eq!(r.get(0), Logic::X);
    }

    #[test]
    fn reduce_and_dengan_x_hasil_x() {
        let mut v = Bits::<8>::zero();
        v.set(0, Logic::One);
        v.set(1, Logic::X);
        assert_eq!(reduce_and::<8, 8>(v).get(0), Logic::X);
    }

    #[test]
    fn reduce_or_dengan_x_tanpa_satu_hasil_x() {
        let mut v = Bits::<8>::zero();
        v.set(0, Logic::X);
        assert_eq!(reduce_or::<8, 8>(v).get(0), Logic::X);
    }

    #[test]
    fn reduce_or_dengan_x_dan_satu_hasil_satu() {
        let mut v = Bits::<8>::zero();
        v.set(0, Logic::X);
        v.set(1, Logic::One);
        assert_eq!(reduce_or::<8, 8>(v).get(0), Logic::One);
    }

    #[test]
    fn reduce_xor_dengan_x_hasil_x() {
        let mut v = Bits::<8>::zero();
        v.set(0, Logic::One);
        v.set(1, Logic::X);
        assert_eq!(reduce_xor::<8, 8>(v).get(0), Logic::X);
    }

    #[test]
    fn reduce_xnor_dengan_x_hasil_x() {
        let mut v = Bits::<8>::zero();
        v.set(0, Logic::Zero);
        v.set(1, Logic::X);
        assert_eq!(reduce_xnor::<8, 8>(v).get(0), Logic::X);
    }
}
