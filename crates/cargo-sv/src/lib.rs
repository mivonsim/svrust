// Tanggung jawab: pub API crate cargo-sv.
pub mod cli;
pub mod commands;
pub mod driver_gen;
pub mod pipeline;
pub mod vcd_timescale;
pub mod workspace;

pub fn run(args: Vec<String>) {
    use crate::cli::{parse_args, print_usage};
    match parse_args(args) {
        Ok(opts) => {
            if let Err(e) = crate::commands::dispatch(opts) {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            print_usage();
            std::process::exit(1);
        }
    }
}
