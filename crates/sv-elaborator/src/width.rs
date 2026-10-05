// Tanggung jawab: aturan inference lebar bit dan signedness SystemVerilog.
use sv_ir::{BinOp, DataType};

/// Tipe hasil operasi biner dengan operand self-determined.
pub fn infer_binary(op: BinOp, lhs: DataType, rhs: DataType) -> DataType {
    match op {
        // Bitwise: lebar = max kedua operand, signed hanya bila keduanya signed.
        BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor => {
            let width = lhs.width.max(rhs.width);
            let signed = lhs.signed && rhs.signed;
            lhs.with_width(width).with_signed(signed)
        }
        // Aritmetika: lebar = max kedua operand.
        BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod => {
            let width = lhs.width.max(rhs.width);
            let signed = lhs.signed && rhs.signed;
            lhs.with_width(width).with_signed(signed)
        }
        // Shift: lebar dan signedness hasil mengikuti operand kiri. Untuk `>>>` ini
        // penting karena LRM §11.4.10 menentukan bit pengisi dari signedness
        // tipe HASIL, bukan dari penulisan operatornya.
        BinOp::Shl | BinOp::Shr | BinOp::Sar => lhs,
        // Perbandingan: hasil 1 bit, selalu unsigned.
        BinOp::Eq | BinOp::NotEq | BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge => DataType::bit(),
        // Logika: hasil 1 bit.
        BinOp::LogAnd | BinOp::LogOr => DataType::bit(),
    }
}

/// Tipe hasil operator kondisional `? :` (LRM §11.4.11).
/// Lebar = max kedua cabang; signed bila keduanya signed.
pub fn infer_ternary(when_true: DataType, when_false: DataType) -> DataType {
    let width = when_true.width.max(when_false.width);
    let signed = when_true.signed && when_false.signed;
    when_true.with_width(width).with_signed(signed)
}

/// Tipe konteks untuk operasi context-determined (assignment, argumen).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Context {
    pub width: u32,
    pub signed: bool,
}

impl Context {
    pub fn of(data_type: DataType) -> Self {
        Self {
            width: data_type.width,
            signed: data_type.signed,
        }
    }

    pub fn as_data_type(self, base: DataType) -> DataType {
        base.with_width(self.width).with_signed(self.signed)
    }
}

/// Terapkan konteks operand: lebarkan atau persempit sesuai aturan SV.
pub fn apply_context(operand: DataType, context: Context) -> DataType {
    operand
        .with_width(context.width)
        .with_signed(context.signed)
}

/// Aksi yang perlu dilakukan untuk menyesuaikan operand ke lebar target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResizeAction {
    None,
    ZeroExtend,
    SignExtend,
    Truncate,
}

/// Hitung aksi resize dari tipe operand menuju tipe target.
pub fn resize_action(operand: DataType, target: DataType) -> ResizeAction {
    match (operand.width, target.width) {
        (from, to) if from == to => ResizeAction::None,
        (_, to) if operand.width < to => {
            if operand.signed {
                ResizeAction::SignExtend
            } else {
                ResizeAction::ZeroExtend
            }
        }
        _ => ResizeAction::Truncate,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bitwise_uses_widest_operand() {
        let result = infer_binary(BinOp::BitAnd, DataType::logic(8), DataType::logic(32));
        assert_eq!(result.width, 32);
        assert!(!result.signed);
    }

    #[test]
    fn mixed_signedness_yields_unsigned() {
        let result = infer_binary(BinOp::Add, DataType::signed(8), DataType::logic(8));
        assert_eq!(result.width, 8);
        assert!(!result.signed);
    }

    #[test]
    fn comparison_yields_single_bit() {
        let result = infer_binary(BinOp::Eq, DataType::logic(32), DataType::logic(32));
        assert_eq!(result.width, 1);
    }

    #[test]
    fn shift_keeps_left_width() {
        let result = infer_binary(BinOp::Shl, DataType::logic(16), DataType::logic(4));
        assert_eq!(result.width, 16);
    }

    #[test]
    fn widen_unsigned_is_zero_extend() {
        let action = resize_action(DataType::logic(8), DataType::logic(32));
        assert_eq!(action, ResizeAction::ZeroExtend);
    }

    #[test]
    fn widen_signed_is_sign_extend() {
        let action = resize_action(DataType::signed(8), DataType::signed(32));
        assert_eq!(action, ResizeAction::SignExtend);
    }

    #[test]
    fn shrink_is_truncate() {
        let action = resize_action(DataType::logic(32), DataType::logic(8));
        assert_eq!(action, ResizeAction::Truncate);
    }

    #[test]
    fn same_width_needs_no_resize() {
        let action = resize_action(DataType::logic(16), DataType::logic(16));
        assert_eq!(action, ResizeAction::None);
    }

    #[test]
    fn ternary_uses_widest_branch() {
        let result = infer_ternary(DataType::logic(8), DataType::logic(32));
        assert_eq!(result.width, 32);
        assert!(!result.signed);
    }

    #[test]
    fn ternary_signed_only_if_both_signed() {
        let result = infer_ternary(DataType::signed(8), DataType::logic(8));
        assert_eq!(result.width, 8);
        assert!(!result.signed);
    }
}
