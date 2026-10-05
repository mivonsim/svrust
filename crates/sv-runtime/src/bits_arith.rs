// Tanggung jawab: aritmetika dan geser logika untuk Bits.
use crate::bits::Bits;
use crate::logic::Logic;
use std::ops::{Add, Mul, Shl, Shr, Sub};

/// Tambah modulo 2^N; X/Z pada operand membuat digit hasil X.
impl<const N: usize> Add for &Bits<N> {
    type Output = Bits<N>;
    fn add(self, rhs: Self) -> Self::Output {
        let mut out = Bits::<N>::zero();
        let mut carry = false;
        let mut carry_x = false;
        for i in 0..N {
            let a = self.get(i);
            let b = rhs.get(i);
            if matches!(a, Logic::X | Logic::Z) || matches!(b, Logic::X | Logic::Z) {
                out.set(i, Logic::X);
                carry = false;
                carry_x = true;
                continue;
            }
            let av = matches!(a, Logic::One) as u8;
            let bv = matches!(b, Logic::One) as u8;
            let cv = (carry as u8) + (carry_x as u8);
            // Carry X diperlakukan sebagai 1 yang meracuni hasil.
            let total = av + bv + cv;
            if carry_x {
                out.set(i, Logic::X);
                carry_x = total >= 2;
                carry = false;
                continue;
            }
            out.set(i, Logic::from_bool(total % 2 == 1));
            carry = total >= 2;
            carry_x = false;
        }
        out
    }
}

/// Kurang modulo 2^N; X/Z membuat digit hasil X.
impl<const N: usize> Sub for &Bits<N> {
    type Output = Bits<N>;
    fn sub(self, rhs: Self) -> Self::Output {
        let mut out = Bits::<N>::zero();
        let mut borrow = false;
        for i in 0..N {
            let a = self.get(i);
            let b = rhs.get(i);
            if matches!(a, Logic::X | Logic::Z) || matches!(b, Logic::X | Logic::Z) {
                out.set(i, Logic::X);
                borrow = false;
                continue;
            }
            let av = matches!(a, Logic::One) as i8;
            let bv = matches!(b, Logic::One) as i8;
            let mut diff = av - bv - (borrow as i8);
            if diff < 0 {
                diff += 2;
                borrow = true;
            } else {
                borrow = false;
            }
            out.set(i, Logic::from_bool(diff == 1));
        }
        out
    }
}

/// Perkalian bilangan bulat yang dipangkas ke lebar penyimpanan N.
///
/// Perkalian dihitung pada u128 supaya tidak meluap sebelum dipangkas,
/// lalu hanya N bit terbawah yang dipertahankan.
impl<const N: usize> Mul for &Bits<N> {
    type Output = Bits<N>;
    fn mul(self, rhs: Self) -> Self::Output {
        let a = self.to_u64() as u128;
        let b = rhs.to_u64() as u128;
        Bits::from_u64(((a * b) & u64::MAX as u128) as u64)
    }
}

/// Geser kiri logika; pengisi nol.
impl<const N: usize> Shl<usize> for &Bits<N> {
    type Output = Bits<N>;
    fn shl(self, rhs: usize) -> Self::Output {
        let mut out = Bits::<N>::zero();
        for i in 0..N {
            if let Some(src) = i.checked_sub(rhs) {
                out.set(i, self.get(src));
            } else {
                out.set(i, Logic::Zero);
            }
        }
        out
    }
}

/// Geser kanan logika; pengisi nol.
impl<const N: usize> Shr<usize> for &Bits<N> {
    type Output = Bits<N>;
    fn shr(self, rhs: usize) -> Self::Output {
        let mut out = Bits::<N>::zero();
        for i in 0..N {
            let src = i + rhs;
            if src < N {
                out.set(i, self.get(src));
            } else {
                out.set(i, Logic::Zero);
            }
        }
        out
    }
}

impl<const N: usize> Add for Bits<N> {
    type Output = Bits<N>;
    fn add(self, rhs: Self) -> Self::Output {
        &self + &rhs
    }
}

impl<const N: usize> Sub for Bits<N> {
    type Output = Bits<N>;
    fn sub(self, rhs: Self) -> Self::Output {
        &self - &rhs
    }
}

impl<const N: usize> Mul for Bits<N> {
    type Output = Bits<N>;
    fn mul(self, rhs: Self) -> Self::Output {
        &self * &rhs
    }
}

/// Geser dengan jumlah dari vektor Bits (LRM: jumlah diambil sebagai integer).
impl<const N: usize> Shl for &Bits<N> {
    type Output = Bits<N>;
    fn shl(self, rhs: Self) -> Self::Output {
        self << rhs.to_u64() as usize
    }
}

/// Geser kanan dengan jumlah dari vektor Bits.
impl<const N: usize> Shr for &Bits<N> {
    type Output = Bits<N>;
    fn shr(self, rhs: Self) -> Self::Output {
        self >> rhs.to_u64() as usize
    }
}

impl<const N: usize> Shl<usize> for Bits<N> {
    type Output = Bits<N>;
    fn shl(self, rhs: usize) -> Self::Output {
        &self << rhs
    }
}

impl<const N: usize> Shr<usize> for Bits<N> {
    type Output = Bits<N>;
    fn shr(self, rhs: usize) -> Self::Output {
        &self >> rhs
    }
}

impl<const N: usize> Shl for Bits<N> {
    type Output = Bits<N>;
    fn shl(self, rhs: Self) -> Self::Output {
        &self << &rhs
    }
}

impl<const N: usize> Shr for Bits<N> {
    type Output = Bits<N>;
    fn shr(self, rhs: Self) -> Self::Output {
        &self >> &rhs
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tambah_tanpa_carry() {
        let a = Bits::<8>::from_u64(3);
        let b = Bits::<8>::from_u64(4);
        assert_eq!((&a + &b).to_u64(), 7);
    }

    #[test]
    fn tambah_bungkus_modulo() {
        let a = Bits::<4>::from_u64(15);
        let b = Bits::<4>::from_u64(1);
        assert_eq!((&a + &b).to_u64(), 0);
    }

    #[test]
    fn tambah_dengan_x_meracuni() {
        let a = Bits::<4>::from_logic(Logic::X);
        let b = Bits::<4>::from_u64(1);
        assert_eq!((&a + &b).get(0), Logic::X);
    }

    #[test]
    fn kurang_dasar() {
        let a = Bits::<8>::from_u64(10);
        let b = Bits::<8>::from_u64(4);
        assert_eq!((&a - &b).to_u64(), 6);
    }

    #[test]
    fn kali_dasar() {
        // BUG-19: `*` harus punya impl di Bits, bukan hanya di parser.
        let a = Bits::<8>::from_u64(3);
        let b = Bits::<8>::from_u64(4);
        assert_eq!((&a * &b).to_u64(), 12);
        assert_eq!((a * b).to_u64(), 12);
    }

    #[test]
    fn kali_bungkus_modulo_lebar() {
        // 16 * 16 = 256, dipangkas ke 8 bit menjadi 0.
        let a = Bits::<8>::from_u64(16);
        let b = Bits::<8>::from_u64(16);
        assert_eq!((&a * &b).to_u64(), 0);
        // 200 * 2 = 400; 400 & 0xFF = 0x90 = 144.
        let c = Bits::<8>::from_u64(200);
        let d = Bits::<8>::from_u64(2);
        assert_eq!((&c * &d).to_u64(), 144);
    }

    #[test]
    fn kali_dengan_nol_hasil_nol() {
        let a = Bits::<8>::from_u64(123);
        let b = Bits::<8>::from_u64(0);
        assert_eq!((&a * &b).to_u64(), 0);
    }

    #[test]
    fn geser_kiri_isi_nol() {
        let a = Bits::<8>::from_u64(0b0000_0011);
        assert_eq!((&a << 2usize).to_u64(), 0b0000_1100);
    }

    #[test]
    fn geser_kanan_isi_nol() {
        let a = Bits::<8>::from_u64(0b0000_1100);
        assert_eq!((&a >> 2usize).to_u64(), 0b0000_0011);
    }
}
