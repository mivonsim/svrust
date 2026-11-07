// Tanggung jawab: tipe data HDL beserta lebar bit dan signedness.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueKind {
    /// 2-state: hanya 0 dan 1.
    Bit,
    /// 4-state: 0, 1, X, Z.
    Logic,
    /// Floating point 64-bit IEEE 754 (LRM §5.5 `real`).
    Real,
}

/// Resolusi net multi-driver (LRM §6.6).
///
/// Ketika beberapa `assign` atau proses menulis ke net yang sama, nilai
/// akhir diformulasikan sesuai tipe net:
/// - `None`: tidak ada resolusi (last-wins, perilaku default untuk wire/reg)
/// - `WiredAnd`: semua nilai di-AND bitwise (wand, triand)
/// - `WiredOr`: semua nilai di-OR bitwise (wor, wor, tri, trior)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NetResolution {
    #[default]
    None,
    WiredAnd,
    WiredOr,
}

impl ValueKind {
    pub fn is_four_state(self) -> bool {
        matches!(self, ValueKind::Logic)
    }

    /// True bila tipe adalah floating point `real` (64-bit IEEE 754).
    pub fn is_real(self) -> bool {
        matches!(self, ValueKind::Real)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DataType {
    pub kind: ValueKind,
    pub width: u32,
    pub signed: bool,
    /// Resolusi net multi-driver (LRM §6.6). `None` untuk variabel dan wire
    /// biasa (last-wins). `WiredAnd`/`WiredOr` untuk net wand/wor/tri.
    pub net_resolution: NetResolution,
}

impl DataType {
    pub fn bit() -> Self {
        Self {
            kind: ValueKind::Bit,
            width: 1,
            signed: false,
            net_resolution: NetResolution::None,
        }
    }

    pub fn logic(width: u32) -> Self {
        Self {
            kind: ValueKind::Logic,
            width,
            signed: false,
            net_resolution: NetResolution::None,
        }
    }

    pub fn signed(width: u32) -> Self {
        Self {
            kind: ValueKind::Logic,
            width,
            signed: true,
            net_resolution: NetResolution::None,
        }
    }

    pub fn real() -> Self {
        Self {
            kind: ValueKind::Real,
            width: 64,
            signed: false,
            net_resolution: NetResolution::None,
        }
    }

    /// Tipe net konstan supply0 (selalu baca 0, LRM §6.6).
    pub fn supply0() -> Self {
        Self {
            kind: ValueKind::Logic,
            width: 1,
            signed: false,
            net_resolution: NetResolution::None, // supply0/supply1 are constant, not multi-driver
        }
    }

    /// Tipe net konstan supply1 (selalu baca 1, LRM §6.6).
    pub fn supply1() -> Self {
        Self {
            kind: ValueKind::Logic,
            width: 1,
            signed: false,
            net_resolution: NetResolution::None,
        }
    }

    /// Tipe net wired-AND (wand/triand).
    pub fn wand(width: u32) -> Self {
        Self {
            kind: ValueKind::Logic,
            width,
            signed: false,
            net_resolution: NetResolution::WiredAnd,
        }
    }

    /// Tipe net wired-OR (wor/wor/tri/trior).
    pub fn wor(width: u32) -> Self {
        Self {
            kind: ValueKind::Logic,
            width,
            signed: false,
            net_resolution: NetResolution::WiredOr,
        }
    }

    pub fn with_width(self, width: u32) -> Self {
        Self { width, ..self }
    }

    pub fn with_signed(self, signed: bool) -> Self {
        Self { signed, ..self }
    }

    pub fn with_net_resolution(self, resolution: NetResolution) -> Self {
        Self {
            net_resolution: resolution,
            ..self
        }
    }

    pub fn type_name(self) -> String {
        match self.kind {
            ValueKind::Real => return "Real".to_string(),
            _ => {}
        }
        let base = match self.kind {
            ValueKind::Bit => "Bit",
            ValueKind::Logic => "Logic",
            ValueKind::Real => "Logic",
        };
        let net = match self.net_resolution {
            NetResolution::WiredAnd => "Wand",
            NetResolution::WiredOr => "Wor",
            NetResolution::None => base,
        };
        if self.signed {
            format!("Signed<{}>", self.width)
        } else {
            format!("{}<{}>", net, self.width)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_bit_type() {
        assert_eq!(DataType::bit().width, 1);
        assert!(!DataType::bit().kind.is_four_state());
    }

    #[test]
    fn logic_is_four_state() {
        assert!(DataType::logic(32).kind.is_four_state());
    }

    #[test]
    fn type_name_includes_width() {
        assert_eq!(DataType::logic(32).type_name(), "Logic<32>");
        assert_eq!(DataType::signed(8).type_name(), "Signed<8>");
    }

    #[test]
    fn real_type_name() {
        assert_eq!(DataType::real().type_name(), "Real");
        assert!(DataType::real().kind.is_real());
        assert!(!DataType::real().kind.is_four_state());
    }
}
