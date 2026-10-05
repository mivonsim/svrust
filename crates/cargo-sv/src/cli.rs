// Tanggung jawab: parsing argumen baris perintah cargo-sv.
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Check,
    Build,
    Run,
    Inspect,
    Codegen,
    Elaborate,
    Clean,
    Help,
    Version,
}

#[derive(Debug, Clone)]
pub struct Options {
    pub command: Command,
    pub rtl: Vec<PathBuf>,
    pub top: Option<String>,
    pub release: bool,
    pub steps: usize,
    pub vcd: Option<PathBuf>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            command: Command::Help,
            rtl: Vec::new(),
            top: None,
            release: false,
            steps: 64,
            vcd: None,
        }
    }
}

pub fn parse_args(args: Vec<String>) -> Result<Options, String> {
    let mut opts = Options::default();
    let mut i = 1; // skip program name

    // support "cargo sv" -> args[1] == "sv"
    if i < args.len() && args[i] == "sv" {
        i += 1;
    }

    if i >= args.len() {
        return Err(
            "tidak ada subcommand. gunakan: check, build, run, inspect, codegen, elaborate, clean"
                .to_string(),
        );
    }

    opts.command = match args[i].as_str() {
        "check" => Command::Check,
        "build" => Command::Build,
        "run" => Command::Run,
        "inspect" => Command::Inspect,
        "codegen" => Command::Codegen,
        "elaborate" => Command::Elaborate,
        "clean" => Command::Clean,
        "help" => Command::Help,
        "--help" | "-h" => Command::Help,
        "version" | "--version" | "-V" => Command::Version,
        other => return Err(format!("subcommand tidak dikenal: {}", other)),
    };
    i += 1;

    while i < args.len() {
        let arg = &args[i];
        match arg.as_str() {
            "--rtl" | "-r" => {
                i += 1;
                if i >= args.len() {
                    return Err("--rtl memerlukan path".to_string());
                }
                opts.rtl.push(PathBuf::from(&args[i]));
            }
            "--top" | "-t" => {
                i += 1;
                if i >= args.len() {
                    return Err("--top memerlukan nama".to_string());
                }
                opts.top = Some(args[i].clone());
            }
            "--release" => opts.release = true,
            "--steps" => {
                i += 1;
                if i >= args.len() {
                    return Err("--steps memerlukan angka".to_string());
                }
                opts.steps = args[i]
                    .parse()
                    .map_err(|_| "--steps bukan angka".to_string())?;
            }
            "--vcd" => {
                i += 1;
                if i >= args.len() {
                    return Err("--vcd memerlukan path".to_string());
                }
                opts.vcd = Some(PathBuf::from(&args[i]));
            }
            "--help" | "-h" => opts.command = Command::Help,
            other if other.starts_with('-') => {
                return Err(format!("opsi tidak dikenal: {}", other))
            }
            other => opts.rtl.push(PathBuf::from(other)),
        }
        i += 1;
    }

    if opts.command != Command::Clean
        && opts.command != Command::Help
        && opts.command != Command::Version
        && opts.rtl.is_empty()
    {
        return Err("tidak ada file RTL. gunakan --rtl <file>".to_string());
    }

    Ok(opts)
}

pub fn print_usage() {
    println!(
        r#"
cargo-sv - SystemVerilog ke Rust simulator

Usage:
  cargo sv <COMMAND> [OPTIONS]

Commands:
  check       Parse dan elaborate tanpa build
  build       Generate Rust dan build executable
  run         Build dan jalankan simulasi
  inspect     Tampilkan statistik design
  codegen     Hanya generate kode Rust
  elaborate   Hanya elaborate ke IR
  clean       Hapus target/sv/
  help        Tampilkan bantuan ini
  version     Tampilkan versi

Options:
  --rtl, -r <FILE>     File RTL SystemVerilog (bisa berulang)
  --top, -t <NAME>     Batasi modul top (default: semua modul root)
  --release            Build mode release
  --steps <N>          Maksimum langkah simulasi (default: 64)
  --vcd <FILE>         Output VCD waveform
  --help, -h           Tampilkan bantuan
"#
    );
}

pub fn print_version() {
    println!("cargo-sv {}", env!("CARGO_PKG_VERSION"));
}
