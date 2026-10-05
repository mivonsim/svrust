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
}
