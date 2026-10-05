// Tanggung jawab: implementasi operator pembagian dan modulo SystemVerilog di runtime.
use crate::bits::Bits;

/// Masker seluruh bit set pada lebar `n`.
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

/// `a / b` — pembagian integer unsigned pada lebar logis N (LRM §11.4.5).
///
/// Pembagian dengan pembagi nol menghasilkan 0 (engine 2-state; pada 4-state
/// SV hasilnya `x`). Bit di atas lebar logis N diabaikan.
#[inline]
pub fn bit_div<const N: usize, const M: usize>(lhs: Bits<M>, rhs: Bits<M>) -> Bits<M> {
    let a = lhs.to_u64() & mask(N);
    let b = rhs.to_u64() & mask(N);
    Bits::from_u64(a.checked_div(b).unwrap_or(0))
}

/// `a % b` — sisa pembagian integer unsigned pada lebar logis N (LRM §11.4.6).
///
/// Sisa pembagian dengan pembagi nol menghasilkan 0 (engine 2-state; pada 4-state
/// SV hasilnya `x`). Bit di atas lebar logis N diabaikan.
#[inline]
pub fn bit_mod<const N: usize, const M: usize>(lhs: Bits<M>, rhs: Bits<M>) -> Bits<M> {
    let a = lhs.to_u64() & mask(N);
    let b = rhs.to_u64() & mask(N);
    Bits::from_u64(a.checked_rem(b).unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bagi_pembagi_normal() {
        assert_eq!(
            bit_div::<8, 8>(Bits::from_u64(100), Bits::from_u64(7)).to_u64(),
            14
        );
    }

    #[test]
    fn bagi_pembagi_nol_menghasilkan_nol() {
        assert_eq!(
            bit_div::<8, 8>(Bits::from_u64(100), Bits::from_u64(0)).to_u64(),
            0
        );
    }

    #[test]
    fn sisa_pembagi_normal() {
        assert_eq!(
            bit_mod::<8, 8>(Bits::from_u64(100), Bits::from_u64(7)).to_u64(),
            2
        );
    }

    #[test]
    fn sisa_pembagi_nol_menghasilkan_nol() {
        assert_eq!(
            bit_mod::<8, 8>(Bits::from_u64(100), Bits::from_u64(0)).to_u64(),
            0
        );
    }

    #[test]
    fn bagi_mengabaikan_bit_di_atas_lebar_logis() {
        // Lebar logis 4, penyimpanan 8: hanya 4 bit bawah yang dihitung,
        // jadi 12 / 3 = 4 (bukan 0b11111100 / 0b11110011 yang tak bermakna).
        assert_eq!(
            bit_div::<4, 8>(Bits::from_u64(12), Bits::from_u64(3)).to_u64(),
            4
        );
    }

    #[test]
    fn sisa_mengabaikan_bit_di_atas_lebar_logis() {
        // Lebar logis 4: pembagi 0b11110011 (243) terpotong jadi 3,
        // sehingga 10 % 3 = 1 (bukan 10 % 243 = 10).
        assert_eq!(
            bit_mod::<4, 8>(Bits::from_u64(10), Bits::from_u64(0b1111_0011)).to_u64(),
            1
        );
    }

    #[test]
    fn pembagi_nol_terpotong_lebar_logis_nihilkan() {
        // Pembagi 8'hF3 pada lebar logis 4 jadi 4'h3, bukan nol =>
        // pembagian tetap terjadi. Bila tidak dimask, hasilnya nol.
        assert_eq!(
            bit_div::<4, 8>(Bits::from_u64(12), Bits::from_u64(0b1111_0011)).to_u64(),
            4
        );
    }
}
