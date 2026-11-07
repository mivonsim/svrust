// Tanggung jawab: pub API crate sv-diag.
mod diagnostic;
mod render;
mod source;

pub use diagnostic::{Diagnostic, Label, Severity, SourceId, Span};
pub use render::{render, render_report, RenderOptions};
pub use source::{Location, SourceFile, SourceMap};
