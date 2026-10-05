// Tanggung jawab: error elaborasi dengan posisi sumber.
use std::fmt;
use sv_lexer::span::Span;

#[derive(Debug, Clone, PartialEq)]
pub struct ElaborateError {
    pub message: String,
    pub span: Span,
}

impl ElaborateError {
    pub fn new(message: impl Into<String>, span: Span) -> Self {
        Self {
            message: message.into(),
            span,
        }
    }

    pub fn undefined_signal(name: &str, span: Span) -> Self {
        Self::new(format!("undefined signal '{}'", name), span)
    }

    pub fn duplicate_signal(name: &str, span: Span) -> Self {
        Self::new(format!("duplicate signal '{}'", name), span)
    }

    pub fn invalid_port(name: &str, span: Span) -> Self {
        Self::new(format!("signal '{}' is not a module port", name), span)
    }

    pub fn undefined_param(name: &str, span: Span) -> Self {
        Self::new(format!("undefined parameter '{}'", name), span)
    }

    pub fn duplicate_param(name: &str, span: Span) -> Self {
        Self::new(format!("duplicate parameter '{}'", name), span)
    }

    pub fn invalid_width(message: impl Into<String>, span: Span) -> Self {
        Self::new(message, span)
    }
}

impl fmt::Display for ElaborateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "elaboration error at line {} col {}: {}",
            self.span.line, self.span.col, self.message
        )
    }
}

impl std::error::Error for ElaborateError {}
