// Tanggung jawab: node AST untuk port module.
use crate::width::WidthExpr;
use sv_lexer::span::Span;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortDirection {
    Input,
    Output,
    Inout,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Port {
    pub direction: PortDirection,
    pub name: String,
    pub width: WidthExpr,
    /// True bila ditulis tipe net/variabel eksplisit (`logic`/`wire`/`reg`/`bit`/tipe atom).
    /// `output [7:0] z` tanpa tipe adalah implicit net (LRM §6.5), bukan variabel,
    /// jadi nilainya `Z` bila tidak driven — bukan `X` seperti variabel.
    pub has_explicit_type: bool,
    /// True bila ditulis `signed`; false berarti unsigned (default LRM §6.2.1).
    pub signed: bool,
    pub span: Span,
}

impl Port {
    /// Lebar dalam bit bila tidak melibatkan parameter; None bila berparameter.
    pub fn literal_width(&self) -> Option<usize> {
        match &self.width {
            WidthExpr::Literal(n) => Some(*n),
            _ => None,
        }
    }

    /// True bila port adalah implicit net tanpa tipe eksplisit (LRM §6.5).
    /// Net tanpa driver bernilai `Z`; variabel tanpa inisialisasi bernilai `X`.
    pub fn is_implicit_net(&self) -> bool {
        !self.has_explicit_type
    }
}
