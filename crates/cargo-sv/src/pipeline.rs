// Tanggung jawab: orkestrasi SV source menjadi Design dan Rust source.
use std::path::{Path, PathBuf};
use sv_ast::module::Module;
use sv_ir::Design;

/// Compile satu berkas; top default = module terakhir seberkas.
pub fn compile(source: &str) -> Result<Design, String> {
    compile_with_top(source, None)
}

/// Compile dengan pilihan top; tanpa top pakai module terakhir.
pub fn compile_with_top(source: &str, top: Option<&str>) -> Result<Design, String> {
    let pp = sv_preprocessor::preprocess(source, None).map_err(|e| e.to_string())?;
    let tokens = sv_lexer::lex(&pp.source).map_err(|e| e.to_string())?;
    let modules = sv_parser::parse_file(&tokens)?;
    let nama_top = top
        .map(|nama| nama.to_string())
        .unwrap_or_else(|| last_module_name(&modules));
    let mut design =
        sv_elaborator::elaborate_top(&modules, &nama_top).map_err(|e| e.to_string())?;
    optimalkan(&mut design);
    Ok(design)
}

/// Jalankan pass optimisasi pada design hasil elaborasi.
///
/// Pass constant folding aman terhadap semantik: hanya ekspresi yang seluruh
/// operand-nya konstanta yang dilipat. Syarat tambahan "hasil muat dalam lebar
/// tipe" adalah pengaman; pada jalur simulasi sekarang efeknya belum terlihat
/// karena pemformat dan penulisan sinyal sudah memask.
pub fn optimalkan(design: &mut Design) -> sv_opt::Report {
    let laporan = sv_opt::optimize(design);
    eprintln!(
        "sv-opt: {} ekspresi terlipat dari {} proses",
        laporan.folded, laporan.processes
    );
    laporan
}

/// Kumpulkan dan parse semua berkas RTL menjadi daftar module.
/// Modul dengan nama sama dari berkas berbeda ditolak agar ambiguity ketahuan.
pub fn collect_modules(paths: &[PathBuf]) -> Result<Vec<Module>, String> {
    let mut modules = Vec::new();
    for path in paths {
        let source = read_preprocessed(path)?;
        let tokens = sv_lexer::lex(&source).map_err(|e| e.to_string())?;
        let parsed = sv_parser::parse_file(&tokens)?;
        for m in parsed {
            if let Some(sama) = modules.iter().find(|x: &&Module| x.name == m.name) {
                return Err(format!(
                    "module duplikat '{}' (ditemukan line {})",
                    m.name, sama.span.line
                ));
            }
            modules.push(m);
        }
    }
    if modules.is_empty() {
        return Err("tidak ada module di berkas RTL".to_string());
    }
    Ok(modules)
}

/// Compile multi-berkas; top wajib eksplisit bila lebih dari satu module.
pub fn compile_paths(paths: &[PathBuf], top: Option<&str>) -> Result<Design, String> {
    let modules = collect_modules(paths)?;
    let nama_top = match top {
        Some(nama) => nama.to_string(),
        None if modules.len() == 1 => modules[0].name.clone(),
        None => modules
            .last()
            .map(|m| m.name.clone())
            .ok_or_else(|| "tidak ada module".to_string())?,
    };
    let mut design =
        sv_elaborator::elaborate_top(&modules, &nama_top).map_err(|e| e.to_string())?;
    optimalkan(&mut design);
    Ok(design)
}

fn last_module_name(modules: &[sv_ast::module::Module]) -> String {
    modules.last().map(|m| m.name.clone()).unwrap_or_default()
}

pub fn read_source(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("gagal baca {}: {}", path.display(), e))
}

/// Baca dan preprocess satu berkas; `include relatif ke direktori berkas.
pub fn read_preprocessed(path: &Path) -> Result<String, String> {
    let source = read_source(path)?;
    let dir = path.parent().map(|p| p.to_path_buf());
    let pp = sv_preprocessor::preprocess(&source, dir.as_deref()).map_err(|e| e.to_string())?;
    Ok(pp.source)
}

/// Ubah path ke bentuk absolut relatif terhadap cwd pemanggil CLI.
/// Proses simulasi berjalan di direktori temp, sehingga path relatif wajib
/// di-resolve lebih dulu dari lokasi asal perintah dijalankan.
pub fn absolutize(path: &Path) -> Result<PathBuf, String> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()
            .map_err(|e| e.to_string())?
            .join(path))
    }
}

pub fn generate_rust(design: &Design) -> String {
    sv_codegen_rust::generate_module(design)
}

/// Nama berkas VCD bawaan bila SV memanggil `$dumpvars` tanpa `$dumpfile`.
pub const BAWAAN_VCD: &str = "dump.vcd";

/// Tentukan path VCD efektif menurut LRM §23.2.
///
/// Bila source memanggil `$dumpvars`, SV yang menentukan: path dari
/// `$dumpfile`; kalau tidak ada, `--vcd` dipakai sebagai cadangan, lalu nama
/// bawaan. Tanpa `$dumpvars`, hanya `--vcd` yang mengaktifkan rekam.
pub fn resolve_vcd(design: &Design, cli: Option<&Path>) -> Result<Option<PathBuf>, String> {
    if !sv_codegen_rust::system_task_gen::has_dumpvars(design) {
        return cli.map(absolutize).transpose();
    }
    let path = sv_codegen_rust::system_task_gen::dumpfile_path(design)
        .filter(|nama| !nama.is_empty())
        .map(PathBuf::from)
        .or_else(|| cli.map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from(BAWAAN_VCD));
    Ok(Some(absolutize(&path)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn design_dari(source: &str) -> Design {
        let tokens = sv_lexer::lex(source).expect("lex");
        let ast = sv_parser::parse_module(&tokens).expect("parse");
        sv_elaborator::elaborate(&ast).expect("elaborate")
    }

    #[test]
    fn tanpa_dumpvars_cli_tetap_menentukan() {
        let design = design_dari("module m(input a, output y); assign y = a; endmodule");
        let hasil = resolve_vcd(&design, Some(Path::new("cli.vcd"))).expect("resolve");
        assert_eq!(hasil, Some(absolutize(Path::new("cli.vcd")).expect("abs")));
    }

    #[test]
    fn tanpa_dumpvars_dan_tanpa_cli_tidak_ada_vcd() {
        let design = design_dari("module m(input a, output y); assign y = a; endmodule");
        assert_eq!(resolve_vcd(&design, None).expect("resolve"), None);
    }

    #[test]
    fn dumpfile_menang_atas_cli() {
        // LRM §23.2: SV yang menentukan berkas mana yang ditulis.
        let design = design_dari(
            "module m(output y); initial begin $dumpfile(\"dari_sv.vcd\"); $dumpvars; end endmodule",
        );
        let hasil = resolve_vcd(&design, Some(Path::new("cli.vcd"))).expect("resolve");
        assert_eq!(
            hasil,
            Some(absolutize(Path::new("dari_sv.vcd")).expect("abs"))
        );
    }

    #[test]
    fn dumpvars_tanpa_dumpfile_memakai_cli_sebagai_cadangan() {
        let design = design_dari("module m(output y); initial $dumpvars; endmodule");
        let hasil = resolve_vcd(&design, Some(Path::new("cli.vcd"))).expect("resolve");
        assert_eq!(hasil, Some(absolutize(Path::new("cli.vcd")).expect("abs")));
    }

    #[test]
    fn dumpvars_tanpa_dumpfile_dan_cli_memakai_nama_bawaan() {
        let design = design_dari("module m(output y); initial $dumpvars; endmodule");
        let hasil = resolve_vcd(&design, None).expect("resolve");
        assert_eq!(hasil, Some(absolutize(Path::new(BAWAAN_VCD)).expect("abs")));
    }
}
