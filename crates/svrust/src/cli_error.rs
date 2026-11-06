// Tanggung jawab: membawa error CLI biasa atau hasil render diagnostik.
use std::fmt;

#[derive(Debug)]
pub enum CliError {
    Message(String),
    Reported {
        diagnostics: Vec<sv_diag::Diagnostic>,
        sources: sv_diag::SourceMap,
    },
}

impl CliError {
    pub fn reported(diagnostics: Vec<sv_diag::Diagnostic>, sources: sv_diag::SourceMap) -> Self {
        Self::Reported {
            diagnostics,
            sources,
        }
    }

    pub fn report(&self) -> Option<String> {
        match self {
            Self::Message(_) => None,
            Self::Reported {
                diagnostics,
                sources,
            } => Some(sv_diag::render_report(
                diagnostics,
                sources,
                sv_diag::RenderOptions::default(),
            )),
        }
    }
}

impl From<String> for CliError {
    fn from(message: String) -> Self {
        Self::Message(message)
    }
}

impl From<&str> for CliError {
    fn from(message: &str) -> Self {
        Self::Message(message.to_string())
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Message(message) => f.write_str(message),
            Self::Reported {
                diagnostics,
                sources,
            } => f.write_str(&sv_diag::render_report(
                diagnostics,
                sources,
                sv_diag::RenderOptions::default(),
            )),
        }
    }
}

impl std::error::Error for CliError {}
