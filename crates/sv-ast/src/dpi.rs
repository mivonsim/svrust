// Tanggung jawab: node AST untuk deklarasi DPI-C (import/export) SystemVerilog (LRM §35).
use sv_lexer::span::Span;

/// Jenis deklarasi DPI-C (LRM §35.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DpiKind {
    /// `import "DPI-C" function ...;` — pemanggilan fungsi C dari SystemVerilog.
    Import,
    /// `export "DPI-C" function ... = ...;` — fungsi SystemVerilog dipanggil dari C.
    Export,
}

/// Satu deklarasi DPI-C di body modul (LRM §35).
#[derive(Debug, Clone, PartialEq)]
pub struct DpiDecl {
    pub kind: DpiKind,
    /// Nama fungsi liaran: nama fungsi C yang dipanggil (import) atau nama
    /// fungsi SystemVerilog yang diexport (export).
    pub name: String,
    /// Nama simbol C yang dipanggil / diexport; `None` bila sama dengan `name`.
    pub c_name: Option<String>,
    /// Tipe balik fungsi, misal `int`, `void`, `bit`. `None` berarti `void`.
    pub ret_type: Option<String>,
    /// Daftar argumen: `(nama, tipe)`.
    pub args: Vec<(String, String)>,
    pub span: Span,
}

/// Satu pemanggilan fungsi C lewat DPI-C import dalam statement (LRM §35).
///
/// Dihasilkan oleh elaborator saat statement memanggil fungsi yang
/// dideklarasi dengan `import "DPI-C" function ...;`.
#[derive(Debug, Clone, PartialEq)]
pub struct DpiCall {
    /// Nama fungsi SV yang diimpor, cocok dengan `DpiDecl::name`.
    pub name: String,
    /// Nama simbol C yang dipanggil (`DpiDecl::c_name` atau `name`).
    pub c_name: String,
    /// Argumen panggilan, masih dalam bentuk ekspresi AST.
    pub args: Vec<crate::expression::Expr>,
    /// Target menyimpan nilai balik (LHS assignment), `None` bila fungsi void.
    pub ret_target: Option<crate::lvalue::Lvalue>,
    pub span: Span,
}
