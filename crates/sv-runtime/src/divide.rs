// Tanggung jawab: implementasi operator pembagian dan modulo SystemVerilog di runtime.
use crate::bits::Bits;
use crate::logic::Logic;

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
/// SV hasilnya `x`). Bit di atas lebar logis N diabaikan. Operand yang
/// bernilai X/Z pada lebar logis membuat hasil X.
#[inline]
pub fn bit_div<const N: usize, const M: usize>(lhs: Bits<M>, rhs: Bits<M>) -> Bits<M> {
    if !lhs.is_fully_known() || !rhs.is_fully_known() {
        return Bits::<M>::all(Logic::X);
    }
    let a = lhs.to_u64() & mask(N);
    let b = rhs.to_u64() & mask(N);
    Bits::from_u64(a.checked_div(b).unwrap_or(0))
}

/// `a % b` — sisa pembagian integer unsigned pada lebar logis N (LRM §11.4.6).
///
/// Sisa pembagian dengan pembagi nol menghasilkan 0 (engine 2-state; pada 4-state
/// SV hasilnya `x`). Bit di atas lebar logis N diabaikan. Operand yang
/// bernilai X/Z pada lebar logis membuat hasil X.
#[inline]
pub fn bit_mod<const N: usize, const M: usize>(lhs: Bits<M>, rhs: Bits<M>) -> Bits<M> {
    if !lhs.is_fully_known() || !rhs.is_fully_known() {
        return Bits::<M>::all(Logic::X);
    }
    let a = lhs.to_u64() & mask(N);
    let b = rhs.to_u64() & mask(N);
    Bits::from_u64(a.checked_rem(b).unwrap_or(0))
}

/// `a ** b` — perpangkatan integer unsigned pada lebar logis N (LRM §11.4.7).
///
/// Hasilnya adalah `a` pangkat `b`, dipotong ke lebar logis N.
/// Jika operand bernilai X/Z, hasilnya X.
#[inline]
pub fn bit_pow<const N: usize, const M: usize>(lhs: Bits<M>, rhs: Bits<M>) -> Bits<M> {
    if !lhs.is_fully_known() || !rhs.is_fully_known() {
        return Bits::<M>::all(Logic::X);
    }
    let a = lhs.to_u64() & mask(N);
    let b = rhs.to_u64() & mask(N);
    // `u64::pow` mengambil eksponen `u32`; hasil dibungkus ke u64 lalu dipotong ke N bit.
    let result = a.wrapping_pow(b as u32);
    Bits::from_u64(result & mask(N))
}

/// `a inside {b, c, d}` — hasil 1 jika `a` sama dengan salah satu elemen
/// dalam concatenation `rhs`, chunk per chunk dengan lebar N (LRM §11.4.3).
#[inline]
pub fn bit_inside<const N: usize, const M: usize>(lhs: Bits<M>, rhs: Bits<M>) -> Bits<M> {
    if !lhs.is_fully_known() || !rhs.is_fully_known() {
        return Bits::<M>::all(Logic::X);
    }
    let a = lhs.to_u64() & mask(N);
    let b = rhs.to_u64();
    // Periksa setiap chunk N bit dalam rhs.
    let chunks = M.checked_div(N).unwrap_or(0);
    for i in 0..chunks {
        let shift = i * N;
        if shift >= 64 {
            break;
        }
        let elem = (b >> shift) & mask(N);
        if elem == a {
            return Bits::from_u64(1);
        }
    }
    Bits::from_u64(0)
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

    #[test]
    fn bagi_dengan_x_hasil_x() {
        let a = Bits::<8>::from_u64(100);
        let b = Bits::<8>::from_logic(Logic::X);
        let r = bit_div::<8, 8>(a, b);
        assert!(!r.is_fully_known());
        assert_eq!(r.get(0), Logic::X);
    }

    #[test]
    fn sisa_dengan_x_hasil_x() {
        let a = Bits::<8>::from_u64(100);
        let b = Bits::<8>::from_logic(Logic::Z);
        let r = bit_mod::<8, 8>(a, b);
        assert!(!r.is_fully_known());
        assert_eq!(r.get(0), Logic::X);
    }

    #[test]
    fn pangkat_normal() {
        assert_eq!(
            bit_pow::<8, 8>(Bits::from_u64(2), Bits::from_u64(3)).to_u64(),
            8
        );
    }

    #[test]
    fn pangkat_dengan_x_hasil_x() {
        let a = Bits::<8>::from_u64(2);
        let b = Bits::<8>::from_logic(Logic::X);
        let r = bit_pow::<8, 8>(a, b);
        assert!(!r.is_fully_known());
        assert_eq!(r.get(0), Logic::X);
    }

    #[test]
    fn inside_ketemu() {
        // 64-bit storage: rhs = {8'd3, 8'd5, 8'd7} = 0x070503
        let a = Bits::<64>::from_u64(5);
        let b = Bits::<64>::from_u64(0x070503);
        let r = bit_inside::<8, 64>(a, b);
        assert_eq!(r.to_u64(), 1);
    }

    #[test]
    fn inside_tidak_ketemu() {
        let a = Bits::<64>::from_u64(4);
        let b = Bits::<64>::from_u64(0x070503);
        let r = bit_inside::<8, 64>(a, b);
        assert_eq!(r.to_u64(), 0);
    }

    #[test]
    fn inside_dengan_x_hasil_x() {
        let a = Bits::<64>::from_u64(5);
        let b = Bits::<64>::from_logic(Logic::X);
        let r = bit_inside::<8, 64>(a, b);
        assert!(!r.is_fully_known());
        assert_eq!(r.get(0), Logic::X);
    }
}
