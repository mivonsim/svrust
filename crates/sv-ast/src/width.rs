// Tanggung jawab: ekspresi lebar vektor (literal atau parameter).
use sv_lexer::span::Span;

/// Lebar port/deklarasi sebelum elaborasi.
///
/// LRM §27: lebar boleh melibatkan parameter. Pola yang didukung: `[N]`,
/// `[MSB:LSB]` literal, `[P]`, `[P-1:0]`, dan bentuk umum dengan ekspresi
/// konstanta (`[W+1:0]`, `[W*2-1:0]`, `[7:2]`).
#[derive(Debug, Clone, PartialEq)]
pub enum WidthExpr {
    /// Lebar literal, misal `[8]` atau `[31:0]` menjadi 8 dan 32.
    Literal(usize),
    /// `[P]` — lebar sama dengan nilai parameter `P`.
    Param(String),
    /// `[P-1:0]` — lebar sama dengan nilai parameter `P`.
    ParamMinusOne(String),
    /// Batas berupa ekspresi konstanta apa pun (LRM §7.3).
    ///
    /// Dipakai untuk bentuk seperti `[W+1:0]`, `[W*2-1:0]`, dan
    /// `[ADDR_W-1:0]` yang sangat umum di RTL. Nilai belum bisa di-resolve
    /// saat parse karena parameter modul baru diketahui saat elaborasi, jadi
    /// ekspresinya disimpan utuh.
    Expr {
        /// Batas atas; untuk `[N]` tanpa titik dua, ini lebarnya langsung.
        msb: Box<crate::expression::Expr>,
        /// Batas bawah; `None` untuk `[N]` yang lebarnya `N`.
        lsb: Option<Box<crate::expression::Expr>>,
    },
}

/// Deklarasi satu parameter module, misal `parameter WIDTH = 8`.
/// Deklarasi `localparam` (LRM §6.20).
///
/// Nilai disimpan sebagai ekspresi, bukan `u64`, karena `localparam` boleh
/// merujuk `localparam` yang dideklarasikan sebelumnya dan `parameter` modul —
/// keduanya belum diketahui pada saat parse.
#[derive(Debug, Clone, PartialEq)]
pub struct LocalParamDecl {
    pub name: String,
    pub value: crate::expression::Expr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParamDecl {
    pub name: String,
    pub default: u64,
    pub span: Span,
}
