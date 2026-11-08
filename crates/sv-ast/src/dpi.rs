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
