// Tanggung jawab: operator logika bitwise untuk Bits.
use crate::bits::Bits;
use std::ops::{BitAnd, BitOr, BitXor, Not};

impl<const N: usize> BitAnd for &Bits<N> {
    type Output = Bits<N>;
    fn bitand(self, rhs: Self) -> Self::Output {
        let mut result = Bits::zero();
        for i in 0..N {
            result.set(i, self.get(i).and(rhs.get(i)));
        }
        result
    }
}

impl<const N: usize> BitOr for &Bits<N> {
    type Output = Bits<N>;
    fn bitor(self, rhs: Self) -> Self::Output {
        let mut result = Bits::zero();
        for i in 0..N {
            result.set(i, self.get(i).or(rhs.get(i)));
        }
        result
    }
}

impl<const N: usize> BitXor for &Bits<N> {
    type Output = Bits<N>;
    fn bitxor(self, rhs: Self) -> Self::Output {
        let mut result = Bits::zero();
        for i in 0..N {
            result.set(i, self.get(i).xor(rhs.get(i)));
        }
        result
    }
}

impl<const N: usize> Not for &Bits<N> {
    type Output = Bits<N>;
    fn not(self) -> Self::Output {
        let mut result = Bits::zero();
        for i in 0..N {
            result.set(i, !self.get(i));
        }
        result
    }
}

impl<const N: usize> BitAnd for Bits<N> {
    type Output = Bits<N>;
    fn bitand(self, rhs: Self) -> Self::Output {
        &self & &rhs
    }
}

impl<const N: usize> BitOr for Bits<N> {
    type Output = Bits<N>;
    fn bitor(self, rhs: Self) -> Self::Output {
        &self | &rhs
    }
}

impl<const N: usize> BitXor for Bits<N> {
    type Output = Bits<N>;
    fn bitxor(self, rhs: Self) -> Self::Output {
        &self ^ &rhs
    }
}

impl<const N: usize> Not for Bits<N> {
    type Output = Bits<N>;
    fn not(self) -> Self::Output {
        !&self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logic::Logic;

    #[test]
    fn bitand_dan_bitor() {
        let a = Bits::<8>::from_u64(0b1100);
        let b = Bits::<8>::from_u64(0b1010);
        assert_eq!((&a & &b).to_u64(), 0b1000);
        assert_eq!((&a | &b).to_u64(), 0b1110);
        assert_eq!((&a ^ &b).to_u64(), 0b0110);
    }

    #[test]
    fn negasi_membalik() {
        let a = Bits::<4>::from_u64(0b1010);
        let negated = !&a;
        assert_eq!(negated.to_u64(), 0b0101);
    }

    #[test]
    fn x_dan_nol_adalah_nol() {
        let a = Bits::<1>::from_logic(Logic::X);
        let b = Bits::<1>::from_logic(Logic::Zero);
        assert_eq!((&a & &b).get(0), Logic::Zero);
    }

    #[test]
    fn x_dan_satu_adalah_x() {
        let a = Bits::<1>::from_logic(Logic::X);
        let b = Bits::<1>::from_logic(Logic::One);
        assert_eq!((&a & &b).get(0), Logic::X);
    }
}
