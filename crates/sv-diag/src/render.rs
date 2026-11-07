// Tanggung jawab: merender diagnostik tunggal dan ringkasan kumpulan diagnostik.
use crate::{Diagnostic, Label, Severity, SourceMap};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RenderOptions {
    pub max_diagnostics: usize,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            max_diagnostics: 5000,
        }
    }
}

pub fn render(diagnostic: &Diagnostic, sources: &SourceMap) -> String {
    let mut output = String::new();
    render_one(&mut output, diagnostic, sources);
    output
}

pub fn render_report(
    diagnostics: &[Diagnostic],
    sources: &SourceMap,
    options: RenderOptions,
) -> String {
    let mut output = String::new();
    for (index, diagnostic) in diagnostics.iter().take(options.max_diagnostics).enumerate() {
        if index > 0 {
            output.push('\n');
        }
        render_one(&mut output, diagnostic, sources);
    }

    let suppressed = diagnostics.len().saturating_sub(options.max_diagnostics);
    if suppressed > 0 {
        if !output.is_empty() {
            output.push('\n');
        }
        output.push_str(&format!(
            "note: {suppressed} further diagnostic(s) suppressed (showing first {} per run)\n",
            options.max_diagnostics
        ));
    }

    let errors = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity == Severity::Error)
        .count();
    let warnings = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity == Severity::Warning)
        .count();
    if !output.is_empty() {
        output.push('\n');
    }
    output.push_str(&format!(
        "ringkasan: {errors} error(s), {warnings} warning(s)"
    ));

    let mut counts = BTreeMap::<String, usize>::new();
    for diagnostic in diagnostics {
        *counts.entry(diagnostic.code.clone()).or_default() += 1;
    }
    for (code, count) in counts {
        output.push_str(&format!("\n  [{code}] x{count}"));
    }
    output
}

fn render_one(output: &mut String, diagnostic: &Diagnostic, sources: &SourceMap) {
    output.push_str(&format!(
        "{}: {} [{}]\n",
        diagnostic.severity.as_str(),
        diagnostic.message,
        diagnostic.code
    ));

    if let Some(primary) = &diagnostic.primary {
        render_label(output, primary, sources, true);
    }
    for label in &diagnostic.labels {
        render_label(output, label, sources, false);
    }
    for note in &diagnostic.notes {
        output.push_str(&format!("note: {note}\n"));
    }
}

fn render_label(output: &mut String, label: &Label, sources: &SourceMap, primary: bool) {
    let Some(file) = sources.get(label.span.source) else {
        return;
    };
    let location = file.location(label.span.start);
    output.push_str(&format!(
        "  --> {}:{}:{}\n",
        file.name, location.line, location.column
    ));
    let Some(line) = file.line(location.line) else {
        return;
    };
    let line_number_width = location.line.to_string().len();
    let line_number = location.line;
    output.push_str(&format!("{:>line_number_width$} | {line}\n", line_number));
    let start = label.span.start.min(file.text.len());
    let line_start = file.line_start(location.line).unwrap_or(start);
    let start_column = file.text[line_start..file.floor_char_boundary(start)]
        .chars()
        .count();
    let end = label.span.end.min(file.text.len()).max(start);
    let line_end = line_start + line.len();
    let end = file.floor_char_boundary(end.min(line_end));
    let width = file.text[start..end].chars().count().max(1);
    output.push_str(&format!(
        "{:>line_number_width$} | {}{}",
        "",
        " ".repeat(start_column),
        "^".repeat(width)
    ));
    output.push('\n');
    if primary && !label.message.is_empty() {
        output.push_str(&format!(
            "{}= {}\n",
            " ".repeat(line_number_width + 5),
            label.message
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Span;

    #[test]
    fn renders_source_location_label_note_and_code() {
        let mut sources = SourceMap::new();
        let source = sources.add_file("prelude.mvp", "let len = __load64(v);\n");
        let span = Span::new(source, 10..21);
        let diagnostic = Diagnostic::error("E0201", "call to unknown function `__load64`", span);

        assert_eq!(
            render(&diagnostic, &sources),
            concat!(
                "error: call to unknown function `__load64` [E0201]\n",
                "  --> prelude.mvp:1:11\n",
                "1 | let len = __load64(v);\n",
                "  |           ^^^^^^^^^^^\n",
                "      = call to unknown function `__load64`\n"
            )
        );
    }

    #[test]
    fn reports_all_counts_even_when_diagnostics_are_suppressed() {
        let diagnostics = (0..3)
            .map(|_| Diagnostic::new(Severity::Error, "E0201", "unknown function"))
            .chain(std::iter::once(Diagnostic::new(
                Severity::Warning,
                "W0001",
                "unused value",
            )))
            .collect::<Vec<_>>();
        let report = render_report(
            &diagnostics,
            &SourceMap::new(),
            RenderOptions { max_diagnostics: 2 },
        );

        assert!(report.contains("note: 2 further diagnostic(s) suppressed"));
        assert!(report.contains("ringkasan: 3 error(s), 1 warning(s)"));
        assert!(report.contains("[E0201] x3"));
        assert!(report.contains("[W0001] x1"));
    }
}
