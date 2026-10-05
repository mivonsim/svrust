// Tanggung jawab: tipe data HDL beserta lebar bit dan signedness.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueKind {
    /// 2-state: hanya 0 dan 1.
    Bit,
    /// 4-state: 0, 1, X, Z.
    Logic,
}

impl ValueKind {
    pub fn is_four_state(self) -> bool {
        matches!(self, ValueKind::Logic)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DataType {
    pub kind: ValueKind,
    pub width: u32,
    pub signed: bool,
}

impl DataType {
    pub fn bit() -> Self {
        Self {
            kind: ValueKind::Bit,
            width: 1,
            signed: false,
        }
    }

    pub fn logic(width: u32) -> Self {
        Self {
            kind: ValueKind::Logic,
            width,
            signed: false,
        }
    }

    pub fn signed(width: u32) -> Self {
        Self {
            kind: ValueKind::Logic,
            width,
            signed: true,
        }
    }

    pub fn with_width(self, width: u32) -> Self {
        Self { width, ..self }
    }

    pub fn with_signed(self, signed: bool) -> Self {
        Self { signed, ..self }
    }

    pub fn type_name(self) -> String {
        let base = match self.kind {
            ValueKind::Bit => "Bit",
            ValueKind::Logic => "Logic",
        };
        if self.signed {
            format!("Signed<{}>", self.width)
        } else {
            format!("{}<{}>", base, self.width)
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
}
