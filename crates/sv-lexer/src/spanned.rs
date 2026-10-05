// Tanggung jawab: pasangan token + lokasi sumber.
use crate::span::Span;
use crate::token::Token;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpannedToken {
    pub token: Token,
    pub span: Span,
}

impl SpannedToken {
    pub fn new(token: Token, span: Span) -> Self {
        Self { token, span }
    }
}
