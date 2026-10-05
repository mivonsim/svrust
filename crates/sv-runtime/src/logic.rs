// Tanggung jawab: representasi nilai 4-state (0, 1, X, Z) pada lebar bit sembarang.
use std::fmt;

/// Satu digit logika 4-state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Logic {
    Zero,
    One,
    X,
    Z,
}

impl Logic {
    pub fn from_bool(value: bool) -> Self {
        if value {
            Logic::One
        } else {
            Logic::Zero
        }
    }

    /// Logika AND 4-state dengan propagasi X/Z.
    pub fn and(self, other: Logic) -> Logic {
        use Logic::*;
        match (self, other) {
            (Zero, _) | (_, Zero) => Zero,
            (One, One) => One,
            _ => X,
        }
    }

    pub fn or(self, other: Logic) -> Logic {
        use Logic::*;
        match (self, other) {
            (One, _) | (_, One) => One,
            (Zero, Zero) => Zero,
            _ => X,
        }
    }

    pub fn xor(self, other: Logic) -> Logic {
        use Logic::*;
        match (self, other) {
            (Zero, Zero) | (One, One) => Zero,
            (Zero, One) | (One, Zero) => One,
            _ => X,
        }
    }

    pub fn is_definite(self) -> bool {
        matches!(self, Logic::Zero | Logic::One)
    }

    pub fn to_bool(self) -> Option<bool> {
        match self {
            Logic::Zero => Some(false),
            Logic::One => Some(true),
            Logic::X | Logic::Z => None,
        }
    }

    pub fn vcd_char(self) -> char {
        match self {
            Logic::Zero => '0',
            Logic::One => '1',
            Logic::X => 'x',
            Logic::Z => 'z',
        }
    }

    pub fn as_char(self) -> char {
        self.vcd_char()
    }
}

impl fmt::Display for Logic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.vcd_char())
    }
}

/// Kebalikan logika: `0 → 1`, `1 → 0`, `x`/`z` → `x` (LRM §6.12).
///
/// Ditulis sebagai trait `Not` supaya idiom `!nilai` bisa dipakai, sama
/// seperti operator unary pada tipe lain.
impl std::ops::Not for Logic {
    type Output = Logic;

    fn not(self) -> Logic {
        use Logic::*;
        match self {
            Zero => One,
            One => Zero,
            X | Z => X,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Logic::*;

    #[test]
    fn and_table() {
        assert_eq!(Zero.and(Zero), Zero);
        assert_eq!(Zero.and(One), Zero);
        assert_eq!(One.and(One), One);
        assert_eq!(One.and(X), X);
        assert_eq!(X.and(Z), X);
    }

    #[test]
    fn or_table() {
        assert_eq!(One.or(Zero), One);
        assert_eq!(Zero.or(Zero), Zero);
        assert_eq!(X.or(Zero), X);
        assert_eq!(X.or(One), One);
    }

    #[test]
    fn not_table() {
        assert_eq!(!Zero, One);
        assert_eq!(!One, Zero);
        assert_eq!(!X, X);
        assert_eq!(!Z, X);
    }

    #[test]
    fn to_bool_known_only() {
        assert_eq!(One.to_bool(), Some(true));
        assert_eq!(Zero.to_bool(), Some(false));
        assert_eq!(X.to_bool(), None);
    }
}
