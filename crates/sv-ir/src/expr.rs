// Tanggung jawab: ekspresi IR yang sudah di-resolve ke simbol konkret.
use crate::datatype::DataType;
pub use crate::system_task::{SystemArg, SystemTaskKind};

pub type SignalId = u32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    BitAnd,
    BitOr,
    BitXor,
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
pub enum UnOp {
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
    /// `~^` — reduksi XNOR.
    RedXnor,
}

impl UnOp {
    /// Reduksi selalu menghasilkan 1 bit (LRM §11.4.9).
    pub fn is_reduction(self) -> bool {
        matches!(
            self,
            UnOp::RedAnd
                | UnOp::RedNand
                | UnOp::RedOr
                | UnOp::RedNor
                | UnOp::RedXor
                | UnOp::RedXnor
        )
    }
}

impl BinOp {
    /// True bila kedua operand adalah context-determined, jadi harus
    /// diprioritaskan ke lebar ekspresi sebelum operasi (LRM §11.6.1).
    ///
    /// Bergeser hanya punya operand kiri yang menentukan lebar; operand kanan
    /// bersifat self-determined, jadi tidak boleh ikut diprioritaskan — kalau
    /// ikut, `a << b` dengan `a` 8-bit dan `b` 32-bit akan melebar jadi 32 bit
    /// dan menghasilkan nilai yang salah.
    pub fn is_context_determined(self) -> bool {
        !self.is_shift()
    }

    /// True bila operator geser (`<<`, `>>`, `>>>`).
    ///
    /// Lebar hasilnya adalah lebar operand kiri saja, dan operand kanan
    /// self-determined — keduanya tidak ikut konteks luar (LRM §11.6.1).
    pub fn is_shift(self) -> bool {
        matches!(self, BinOp::Shl | BinOp::Shr | BinOp::Sar)
    }

    /// True bila operator menghasilkan nilai kebenaran (LRM §11.4.5-8).
    ///
    /// Hasilnya 1 bit, jadi lebar konteks operand bukan lebar hasil — kalau
    /// tidak, `-1 < 0` akan memotong kedua operand ke 1 bit.
    pub fn is_relational(self) -> bool {
        matches!(
            self,
            BinOp::Eq | BinOp::NotEq | BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge
        )
    }

    /// True bila perbandingannya harus signed.
    ///
    /// LRM §11.4.5: perbandingan bertanda hanya bila kedua operand signed;
    /// begitu ada satu operand unsigned, perbandingan jadi unsigned.
    pub fn relational_is_signed(self, lhs: DataType, rhs: DataType) -> bool {
        self.is_relational() && lhs.signed && rhs.signed
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Const {
        value: u64,
        data_type: DataType,
        /// Bit yang ditulis `x`/`z`/`?`; 0 untuk konstanta biasa.
        unknown_mask: u64,
        /// Bit yang ditulis `z` (dan `?`); sisanya di `unknown_mask` adalah `x`.
        zmask: u64,
    },
    SignalRef {
        signal: SignalId,
        data_type: DataType,
    },
    Bin {
        op: BinOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
        data_type: DataType,
    },
    Un {
        op: UnOp,
        operand: Box<Expr>,
        data_type: DataType,
    },
    Select {
        base: Box<Expr>,
        msb: u32,
        lsb: u32,
        data_type: DataType,
    },
    /// Operator kondisional `cond ? a : b` (LRM §11.4.11).
    Ternary {
        condition: Box<Expr>,
        when_true: Box<Expr>,
        when_false: Box<Expr>,
        data_type: DataType,
    },
    /// Concatenation `{a, b, c}` — item diurut dari MSB ke LSB (LRM §11.8.1).
    Concat {
        items: Vec<Expr>,
        data_type: DataType,
    },
    /// Replication `{N{a}}` (LRM §11.8.2).
    Replicate {
        count: u32,
        value: Box<Expr>,
        data_type: DataType,
    },
    /// Waktu simulasi saat ini dari `$time` (LRM §20).
    ///
    /// Lebarnya 64 bit karena waktu simulasi tidak dibatasi lebar sinyal.
    SimTime { data_type: DataType },
    /// Indeks dinamis `a[i]` (LRM §7.8).
    ///
    /// Berbeda dari `Select` yang msb/lsb-nya konstan, posisi elemen di sini
    /// baru diketahui saat simulasi berjalan, jadi codegen memanggil helper
    /// runtime. `data_type` adalah lebar ELEMEN.
    ///
    /// `flat_element` membedakan dua semantik yang lebarnya bisa sama:
    ///
    /// - `false` — bit-select pada sinyal **packed** (`logic [7:0] a; a[i]`).
    ///   Elemennya satu bit, posisinya `i`.
    /// - `true` — elemen ke-`i` pada **array unpacked** (`mem[i]`, disimpan
    ///   rata). Elemennya `data_type.width` bit, posisinya `i * lebar`.
    ///
    /// Tanpa penanda ini, `a[idx]` pada sinyal 8 bit terbaca sebagai elemen
    /// array dan menggeser `idx * 8` bit — hasilnya selalu di luar jangkauan
    /// (nol) untuk setiap indeks.
    Index {
        base: Box<Expr>,
        index: Box<Expr>,
        data_type: DataType,
        flat_element: bool,
    },
    /// Type cast `tipe'(ekspresi)` (LRM §6.14).
    ///
    /// Lebar hasil mengikuti `data_type`, tapi arah perluasan saat melebar
    /// mengikuti `operand_signed` — bukan `data_type.signed`.
    ///
    /// LRM §6.14 mendefinisikan cast sebagai "dievaluasi seolah operand
    /// di-assign ke tipe tujuan", sehingga yang menentukan adalah aturan
    /// assignment: operand signed di-sign-extend, operand unsigned di
    /// zero-extend. Signedness tipe tujuan tidak berpengaruh apa pun. Kalau
    /// `data_type.signed` yang dipakai, `word_t'(a)` dengan `a` signed akan
    /// salah zero-extend, dan `sword_t'(u)` dengan `u` unsigned akan salah
    /// sign-extend.
    Cast {
        operand: Box<Expr>,
        data_type: DataType,
        /// Signedness operand pada saat cast dibuat.
        operand_signed: bool,
    },
}

impl Expr {
    pub fn data_type(&self) -> DataType {
        match self {
            Expr::Const { data_type, .. }
            | Expr::SignalRef { data_type, .. }
            | Expr::Bin { data_type, .. }
            | Expr::Un { data_type, .. }
            | Expr::Select { data_type, .. }
            | Expr::Ternary { data_type, .. }
            | Expr::Concat { data_type, .. }
            | Expr::Replicate { data_type, .. }
            | Expr::Cast { data_type, .. }
            | Expr::Index { data_type, .. }
            | Expr::SimTime { data_type } => *data_type,
        }
    }

    pub fn signal_ref(signal: SignalId, data_type: DataType) -> Self {
        Expr::SignalRef { signal, data_type }
    }

    pub fn constant(value: u64, data_type: DataType) -> Self {
        Expr::Const {
            value,
            data_type,
            unknown_mask: 0,
            zmask: 0,
        }
    }

    /// Konstanta dengan bit wildcard `x`/`z`/`?` untuk `casez`/`casex`.
    pub fn constant_unknown(value: u64, data_type: DataType, unknown_mask: u64) -> Self {
        Expr::Const {
            value,
            data_type,
            unknown_mask,
            zmask: 0,
        }
    }

    /// Konstanta dengan digit tak diketahui, `x` dan `z` dipisahkan.
    ///
    /// LRM §5.7.1: `z` adalah impedansi tinggi dan `x` nilai tak diketahui;
    /// keduanya tampil berbeda pada `$display`, jadi tidak boleh dilebur jadi
    /// satu masker.
    pub fn constant_four_state(
        value: u64,
        data_type: DataType,
        unknown_mask: u64,
        zmask: u64,
    ) -> Self {
        Expr::Const {
            value,
            data_type,
            unknown_mask,
            zmask,
        }
    }
}
