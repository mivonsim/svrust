// Tanggung jawab: node AST untuk expression.
use sv_lexer::span::Span;

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    /// Identifier; `span` dipakai agar error elaborasi bisa menunjuk ke token
    /// aslinya, bukan ke posisi dummy (AGENTS.md aturan 3).
    Ident {
        name: String,
        span: Span,
    },
    Number(u64),
    /// Based literal sized, misal `8'hFF` (LRM §5.7.1).
    Sized {
        value: u64,
        width: u32,
        signed: bool,
        /// Bit yang ditulis `x`/`z`/`?`; dipakai `casez`/`casex` sebagai wildcard.
        unknown_mask: u64,
        /// Bit yang ditulis `z` (dan `?`); sisanya di `unknown_mask` adalah `x`.
        zmask: u64,
    },
    Binary {
        op: BinaryOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
        span: Span,
    },
    /// Unary dan reduction operator (LRM §11.4/§11.5).
    Unary {
        op: UnaryOp,
        operand: Box<Expr>,
        span: Span,
    },
    /// Operator kondisional `? :` (LRM §11.4.11).
    Ternary {
        condition: Box<Expr>,
        when_true: Box<Expr>,
        when_false: Box<Expr>,
        span: Span,
    },
    /// Bit-select `a[i]` bila `msb == lsb`, part-select `a[msb:lsb]` bila tidak.
    Select {
        base: Box<Expr>,
        msb: u32,
        lsb: u32,
        span: Span,
    },
    /// Indeks dinamis `a[i]` dengan `i` bukan konstanta (LRM §7.8).
    ///
    /// Bentuk konstanta `a[3]` tetap jadi `Select`; yang ini perlu helper
    /// runtime karena posisi elemennya baru diketahui saat simulasi berjalan.
    /// Hanya untuk baca — penulisan dengan indeks dinamis ditolak parser.
    IndexDynamic {
        base: Box<Expr>,
        index: Box<Expr>,
        span: Span,
    },
    /// Concatenation `{a, b, c}` — lebar = jumlah lebar seluruh item (LRM §11.8.1).
    Concat {
        items: Vec<Expr>,
        span: Span,
    },
    /// Replication `{N{a}}` — ulangi `value` sebanyak `count` kali (LRM §11.8.2).
    ///
    /// `count` adalah ekspresi, bukan literal: `{WIDTH{1'b1}}` dengan `WIDTH`
    /// parameter modul adalah idiom RTL paling umum, dan standarnya hanya
    /// mensyaratkan konstanta. Nilainya di-resolve elaborator lewat tabel
    /// parameter, bukan oleh parser.
    Replicate {
        count: Box<Expr>,
        value: Box<Expr>,
        span: Span,
    },
    /// System function `$time` — waktu simulasi berjalan (LRM §20).
    SystemTime {
        /// Satuan `timeunit` modul tempat `$time` ditulis (LRM §21.8).
        ///
        /// Nilai `$time` selalu dalam satuan `timeunit` modul yang memuatnya,
        /// bukan modul top. Parser menandai `Bawaan` karena `timescale` baru
        /// diketahui setelah modul diparse;
        /// [`crate::delay_unit::terapkan_module`] yang menyelesaikannya —
        /// mekanismenya sama dengan `#delay` tanpa satuan.
        unit: crate::time_unit::TimeUnit,
        span: Span,
    },
    /// Panggilan function `f(a, b)` (LRM §13.4).
    ///
    /// Node ini hanya bertahan sampai elaborator meng-inline badannya menjadi
    /// ekspresi biasa, sehingga tidak pernah mencapai IR.
    FunctionCall {
        name: String,
        args: Vec<Expr>,
        span: Span,
    },
    /// Type cast ke tipe bertipe `tipe_nama'(ekspresi)` (LRM §6.14).
    ///
    /// Lebar dan signedness diambil dari typedef `tipe_nama` saat elaborasi.
    Cast {
        type_name: String,
        operand: Box<Expr>,
        span: Span,
    },
    /// Type cast ke tipe bawaan: `integer'(x)`, `byte'(x)`, `logic'(x)` (LRM §6.14).
    ///
    /// Berbeda dari `Cast`, lebar dan signedness sudah diketahui dari nama
    /// built-in sehingga tidak perlu lookup typedef.
    BuiltinCast {
        /// Nama tipe bawaan seperti `integer`/`byte`/`logic`.
        type_name: String,
        /// Lebar logis tipe bawaan (Tabel 6-22).
        width: u32,
        /// Signedness tipe bawaan; `integer`/`int`/`longint` bertanda.
        signed: bool,
        operand: Box<Expr>,
        span: Span,
    },
    /// Size cast `16'(x)` (LRM §6.14).
    ///
    /// Hanya mengubah lebar; signedness mengikuti operand, jadi harus dibaca
    /// setelah operand dielaborasi dan bukan dari token.
    SizeCast {
        width: u32,
        operand: Box<Expr>,
        span: Span,
    },
    /// Sign cast `signed'(ekspresi)` atau `unsigned'(ekspresi)` (LRM §6.14).
    ///
    /// Lebar ekspresi tidak berubah; hanya interpretasi_signedness-nya yang
    /// berubah, jadi ini berbeda dari `Cast` yang mengubah lebar eksplisit.
    SignCast {
        signed: bool,
        operand: Box<Expr>,
        span: Span,
    },
    /// System function `$bits(ekspresi)` (LRM §20).
    ///
    /// Menghasilkan konstanta lebar bit ekspresi, bukan nilai runtime.
    Bits {
        operand: Box<Expr>,
        span: Span,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    And,
    Or,
    Xor,
    Add,
    Sub,
    Mul,
    /// Pembagian bulat unsigned; pembagi nol menghasilkan 0 (engine 2-state).
    Div,
    /// Sisa pembagian bulat unsigned; pembagi nol menghasilkan 0.
    Mod,
    Shl,
    Shr,
    /// `>>>` — geser kanan aritmetik; isi dengan bit sign bila tipe hasil
    /// signed (LRM §11.4.10).
    Sar,
    Eq,
    NotEq,
    Lt,
    Gt,
    Le,
    Ge,
    LogAnd,
    LogOr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    /// `~` — negasi bitwise.
    BitNot,
    /// `!` — negasi logika.
    LogNot,
    /// `-` satu operand — negasi aritmetika.
    BitNeg,
    /// `&` — reduksi AND.
    RedAnd,
    /// `~&` — reduksi NAND.
    RedNand,
    /// `|` — reduksi OR.
    RedOr,
    /// `~|` — reduksi NOR.
    RedNor,
    /// `^` — reduksi XOR.
    RedXor,
    /// `~^` atau `^~` — reduksi XNOR.
    RedXnor,
}

impl Expr {
    /// Bentukkan identifier. Dipakai parser (dengan span token) dan elaborator
    /// (dengan span asal, supaya pesan error tetap menunjuk ke sumber).
    pub fn ident(name: impl Into<String>, span: Span) -> Self {
        Expr::Ident {
            name: name.into(),
            span,
        }
    }
}

impl UnaryOp {
    /// Operator reduksi selalu menghasilkan 1 bit (LRM §11.4.9).
    pub fn is_reduction(self) -> bool {
        matches!(
            self,
            UnaryOp::RedAnd
                | UnaryOp::RedNand
                | UnaryOp::RedOr
                | UnaryOp::RedNor
                | UnaryOp::RedXor
                | UnaryOp::RedXnor
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reduction_operator_ditandai() {
        assert!(UnaryOp::RedAnd.is_reduction());
        assert!(UnaryOp::RedXnor.is_reduction());
        assert!(!UnaryOp::BitNot.is_reduction());
        assert!(!UnaryOp::LogNot.is_reduction());
    }

    #[test]
    fn function_call_menyimpan_nama_dan_argumen() {
        let expr = Expr::FunctionCall {
            name: "paritas".to_string(),
            args: vec![Expr::Number(3)],
            span: Span::dummy(),
        };
        match expr {
            Expr::FunctionCall { name, args, .. } => {
                assert_eq!(name, "paritas");
                assert_eq!(args.len(), 1);
            }
            other => panic!("harus function call, dapat {other:?}"),
        }
    }

    #[test]
    fn cast_menyimpan_nama_tipe_dan_operand() {
        let expr = Expr::Cast {
            type_name: "byte_t".to_string(),
            operand: Box::new(Expr::Number(5)),
            span: Span::dummy(),
        };
        match expr {
            Expr::Cast {
                type_name, operand, ..
            } => {
                assert_eq!(type_name, "byte_t");
                assert_eq!(*operand, Expr::Number(5));
            }
            other => panic!("harus cast, dapat {other:?}"),
        }
    }

    #[test]
    fn sign_cast_menyimpan_arah_tanda() {
        let expr = Expr::SignCast {
            signed: true,
            operand: Box::new(Expr::Number(5)),
            span: Span::dummy(),
        };
        match expr {
            Expr::SignCast { signed, .. } => assert!(signed),
            other => panic!("harus sign cast, dapat {other:?}"),
        }
    }

    #[test]
    fn bits_menyimpan_operandnya() {
        let expr = Expr::Bits {
            operand: Box::new(Expr::ident("a", Span::dummy())),
            span: Span::dummy(),
        };
        match expr {
            Expr::Bits { operand, .. } => {
                assert_eq!(*operand, Expr::ident("a", Span::dummy()));
            }
            other => panic!("harus bits, dapat {other:?}"),
        }
    }

    #[test]
    fn sized_literal_menyimpan_lebar() {
        let expr = Expr::Sized {
            value: 0xFF,
            width: 8,
            signed: false,
            unknown_mask: 0,
            zmask: 0,
        };
        assert_eq!(
            expr,
            Expr::Sized {
                value: 255,
                width: 8,
                signed: false,
                unknown_mask: 0,
                zmask: 0,
            }
        );
    }
}
