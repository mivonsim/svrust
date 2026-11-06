// Tanggung jawab: pub API crate cargo-sv.
pub mod cli;
mod cli_error;
pub mod commands;
pub mod driver_gen;
pub mod pipeline;
pub mod vcd_path;
pub mod vcd_timescale;
pub mod workspace;

pub use cli_error::CliError;

pub fn run(args: Vec<String>) {
    use crate::cli::{parse_args, print_usage};
    match parse_args(args) {
        Ok(opts) => {
            if let Err(e) = crate::commands::dispatch(opts) {
                if let Some(report) = e.report() {
                    eprintln!("{report}");
                } else {
                    eprintln!("{}", render_error(&e.to_string()));
                }
                std::process::exit(1);
            }
        }
        Err(e) => {
            eprintln!("{}", render_error(&e));
            print_usage();
            std::process::exit(1);
        }
    }
}

fn render_error(message: &str) -> String {
    let diagnostic = sv_diag::Diagnostic::new(sv_diag::Severity::Error, "E0000", message);
    sv_diag::render_report(
        &[diagnostic],
        &sv_diag::SourceMap::new(),
        sv_diag::RenderOptions::default(),
    )
}

#[cfg(test)]
mod tests {
    use super::render_error;

    #[test]
    fn cli_errors_use_diagnostic_format() {
        assert_eq!(
            render_error("tidak ada file RTL"),
            "error: tidak ada file RTL [E0000]\n\n\
             ringkasan: 1 error(s), 0 warning(s)\n  [E0000] x1"
        );
    }
}
