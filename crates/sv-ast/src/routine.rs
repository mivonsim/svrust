// Tanggung jawab: node AST untuk deklarasi task/function SystemVerilog.
use crate::combinational::CombinationalStatement;
use crate::expression::Expr;
use crate::width::WidthExpr;
use sv_lexer::span::Span;

/// Arah argumen task/function (LRM §13.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArgDirection {
    Input,
    Output,
    Inout,
}

/// Satu argumen pada daftar port task/function (LRM §13.3).
#[derive(Debug, Clone, PartialEq)]
pub struct RoutineArg {
    pub name: String,
    pub direction: ArgDirection,
    pub width: WidthExpr,
    pub signed: bool,
    pub span: Span,
}

/// Bentuk subroutine yang dideklarasikan di dalam module (LRM §13.3/§13.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoutineKind {
    /// `task ... endtask` — tidak mengembalikan nilai.
    Task,
    /// `function ... endfunction` — mengembalikan nilai lewat assignment
    /// ke nama fungsi.
    Function,
}

/// Deklarasi task atau function di body modul.
#[derive(Debug, Clone, PartialEq)]
pub struct RoutineDecl {
    pub kind: RoutineKind,
    pub name: String,
    /// Lebar nilai balik; `None` untuk task dan untuk function 1-bit polos.
    pub return_width: Option<WidthExpr>,
    pub return_signed: bool,
    pub args: Vec<RoutineArg>,
    pub body: Vec<CombinationalStatement>,
    /// Keyword `automatic` diizinkan tapi tidak mengubah apa pun, karena
    /// badan subrutin di-inline pada titik panggilan.
    pub automatic: bool,
    pub span: Span,
}

/// Panggilan task pada posisi statement (LRM §13.3).
#[derive(Debug, Clone, PartialEq)]
pub struct TaskCall {
    pub name: String,
    pub args: Vec<Expr>,
    pub span: Span,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_tidak_punya_lebar_kembalikan() {
        let decl = RoutineDecl {
            kind: RoutineKind::Task,
            name: "t".to_string(),
            return_width: None,
            return_signed: false,
            args: Vec::new(),
            body: Vec::new(),
            automatic: false,
            span: Span::dummy(),
        };
        assert_eq!(decl.kind, RoutineKind::Task);
        assert!(decl.return_width.is_none());
    }

    #[test]
    fn function_boleh_punya_lebar_kembalikan() {
        let decl = RoutineDecl {
            kind: RoutineKind::Function,
            name: "f".to_string(),
            return_width: Some(WidthExpr::Literal(8)),
            return_signed: false,
            args: Vec::new(),
            body: Vec::new(),
            automatic: false,
            span: Span::dummy(),
        };
        assert_eq!(decl.return_width, Some(WidthExpr::Literal(8)));
    }
}
