// Tanggung jawab: definisi jenis token SystemVerilog subset.
/// Klasifikasi tipe atom untuk deklarasi (LRM §6.16, Tabel 6-22).
///
/// Ditaruh di sini, bukan diduplikasi di setiap parser, karena daftar ini
/// pervasive per-token: `integer`/`int` dipakai di `declaration.rs`,
/// `is_net_type` di `module.rs`, lalu `routine.rs`, `typedef.rs`,
/// `generate.rs`, `always_ff.rs`, dan `always_comb.rs` semuanya butuh jawaban
/// yang sama. Salin-tempel di enam tempat pasti berbeda satu kali.
pub mod tipe {
    use super::Token;

    /// Lebar dan signedness tetap untuk tipe integer atom.
    ///
    /// `None` berarti token bukan tipe atom — `logic` dan `bit` memerlukan
    /// `[msb:lsb]`, jadi `logic` polos tetap 1 bit.
    pub fn atom(token: &Token) -> Option<(u32, bool)> {
        Some(match token {
            Token::Integer => (32, true),
            Token::Int => (32, true),
            Token::Byte => (8, true),
            Token::ShortInt => (16, true),
            Token::LongInt => (64, true),
            Token::Time => (64, false),
            _ => return None,
        })
    }

    /// True bila token memulai deklarasi net/variabel bertipe atom.
    ///
    /// `logic`, `wire`, `reg`, dan `bit` ikut karena lebarnya boleh ditulis
    /// eksplisit setelahnya, sedangkan tipe atom tidak.
    pub fn bisa_mulai_deklarasi(token: &Token) -> bool {
        matches!(
            token,
            Token::Logic
                | Token::Wire
                | Token::Reg
                | Token::Bit
                | Token::Integer
                | Token::Int
                | Token::Byte
                | Token::ShortInt
                | Token::LongInt
                | Token::Time
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    Module,
    Endmodule,
    Logic,
    Wire,
    Reg,
    Bit,
    /// `integer` — integer 32-bit signed (LRM §6.2.2).
    Integer,
    /// `int` — integer 32-bit signed (LRM §6.2.2).
    Int,
    /// `byte` — integer 8-bit unsigned (LRM §6.16, Tabel 6-22).
    Byte,
    /// `shortint` — integer 16-bit unsigned (LRM §6.16, Tabel 6-22).
    ShortInt,
    /// `longint` — integer 64-bit signed (LRM §6.16, Tabel 6-22).
    LongInt,
    /// `time` — waktu simulasi 64-bit unsigned (LRM §6.16, Tabel 6-22).
    Time,
    Signed,
    Unsigned,
    Input,
    Output,
    Inout,
    Parameter,
    /// `localparam` — konstanta lokal modul yang tidak bisa di-override
    /// (LRM §6.20).
    LocalParam,
    Assign,
    Always,
    AlwaysComb,
    AlwaysFf,
    AlwaysLatch,
    Posedge,
    Negedge,
    Edge,
    If,
    Else,
    For,
    While,
    Repeat,
    Initial,
    /// `generate` — pembuka region generate (LRM §27.1).
    Generate,
    /// `endgenerate` — penutup region generate.
    Endgenerate,
    /// `genvar` — deklarasi variabel loop generate (LRM §27.3).
    Genvar,
    /// `task` — pembuka deklarasi task (LRM §13.3).
    Task,
    /// `endtask` — penutup deklarasi task.
    Endtask,
    /// `function` — pembuka deklarasi function (LRM §13.4).
    Function,
    /// `endfunction` — penutup deklarasi function.
    Endfunction,
    /// `automatic` — qualifier subroutine: penyimpanan lokal per panggilan
    /// (LRM §13.3). Accepted but affects nothing: bodies are inlined.
    Automatic,
    /// `return` — keluar lebih awal dari function/task (LRM §13.4).
    Return,
    /// `typedef` — deklarasi tipe baru dari tipe yang sudah ada (LRM §8.20).
    Typedef,
    /// `enum` — daftar enumerasi di dalam `typedef` (LRM §6.7).
    Enum,
    /// `'` pada type cast seperti `byte_t'(x)` atau `signed'(x)` (LRM §6.14).
    ///
    /// Berbeda dari `'` pada based literal `8'hFF`, yang sudah jadi bagian
    /// token angka.
    Cast,
    /// Size cast `16'(x)` — lebar eksplisit tanpa nama tipe (LRM §6.14).
    ///
    /// Signedness-nya ikut operand, bukan unsigned seperti `TypedefCast` ke
    /// tipe non-signed: iverilog memberi `16'(-8'sd3)` = 16'hFFFD.
    SizeCast(u32),
    Case,
    Casez,
    Casex,
    Endcase,
    Default,
    Begin,
    End,
    At,
    Star,
    Slash,
    Percent,
    LParen,
    RParen,
    LBracket,
    RBracket,
    LBrace,
    RBrace,
    Semi,
    Colon,
    Comma,
    Hash,
    Dot,
    Eq,
    NonBlocking,
    Lt,
    Gt,
    Ge,
    Shl,
    Shr,
    /// `>>>` — geser kanan aritmetik (LRM §11.4.10).
    ///
    /// Dipisah dari `Shr` karena `>>` selalu mengisi nol, sedangkan `>>>`
    /// mengisi bit sign bila tipe hasil signed.
    Sar,
    And,
    AndAnd,
    Or,
    OrOr,
    Xor,
    Tilde,
    Bang,
    EqEq,
    BangEq,
    Plus,
    Minus,
    /// `++` — increment (LRM §12.5).
    Incr,
    /// `--` — decrement (LRM §12.5).
    Decr,
    Question,
    Ident(String),
    Number(u64),
    /// Based literal sized: nilai, lebar eksplisit, flag signed, dan bit `x`/`z`/`?` (LRM §5.7.1).
    BasedNumber {
        value: u64,
        width: u32,
        signed: bool,
        /// Bit yang ditulis sebagai `x`/`z`/`?`; engine 2-state menganggapnya 0.
        unknown_mask: u64,
        /// Bit yang ditulis `z` (dan `?`); sisanya di `unknown_mask` adalah `x`.
        ///
        /// Dipisah dari `unknown_mask` karena LRM §5.7.1 membedakan keduanya:
        /// `z` adalah impedansi tinggi, `x` adalah nilai tak diketahui, dan
        /// keduanya tampil berbeda pada `$display`.
        zmask: u64,
    },
    /// Compound assignment seperti `+=` (LRM §11.3).
    CompoundAssign(CompoundOp),
    /// String literal `"teks"` (LRM §5.7.3).
    StringLiteral(String),
    /// System task seperti `$display` (LRM §20).
    SystemTask(String),
    Directive(String),
}

/// Operasi dasar pada compound assignment (LRM §11.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompoundOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
    /// `>>>=` — compound assignment untuk geser kanan aritmetik (LRM §11.4.10).
    Sar,
}
