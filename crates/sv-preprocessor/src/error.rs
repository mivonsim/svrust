// Tanggung jawab: error preprocessor dengan posisi sumber.
use std::fmt;
use sv_lexer::span::Span;

/// Error preprocessor; membawa file dan baris sumber asli.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreprocessError {
    pub message: String,
    pub file: String,
    pub line: usize,
    pub col: usize,
}

impl PreprocessError {
    pub fn new(
        message: impl Into<String>,
        file: impl Into<String>,
        line: usize,
        col: usize,
    ) -> Self {
        Self {
            message: message.into(),
            file: file.into(),
            line,
            col,
        }
    }

    pub fn at(message: impl Into<String>, file: &str, line: usize) -> Self {
        Self::new(message, file, line, 1)
    }
}

impl fmt::Display for PreprocessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.file.is_empty() {
            write!(
                f,
                "preprocessor error at line {}: {}",
                self.line, self.message
            )
        } else {
            write!(
                f,
                "preprocessor error at {}:{}:{}: {}",
                self.file, self.line, self.col, self.message
            )
        }
    }
}

impl std::error::Error for PreprocessError {}

/// Konversi span leksikal menjadi error preprocessor.
pub fn dari_span(message: impl Into<String>, file: &str, span: Span) -> PreprocessError {
    PreprocessError::new(message, file, span.line, span.col)
}
