// Tanggung jawab: orkestrasi SV source menjadi Design dan Rust source.
use std::path::{Path, PathBuf};
use sv_ast::module::Module;
use sv_ir::Design;

/// Compile satu berkas; tanpa top, modul root tunggal yang dipakai.
pub fn compile(source: &str) -> Result<Design, String> {
    compile_with_top(source, None)
}

/// Compile dengan pilihan top; tanpa top pakai modul root tunggal.
pub fn compile_with_top(source: &str, top: Option<&str>) -> Result<Design, String> {
    let pp = sv_preprocessor::preprocess(source, None).map_err(|e| e.to_string())?;
    let tokens = sv_lexer::lex(&pp.source).map_err(|e| e.to_string())?;
    let modules = sv_parser::parse_file(&tokens)?;
    let nama_top = pilih_top(&modules, top)?;
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

/// Module yang tidak diinstansiasi modul lain, jadi kandidat top.
///
/// LRM §23.1: modul yang tidak diinstansiasi modul mana pun menjadi *root
/// module* dan dielaborasi sebagai top tersendiri. Kalau ada lebih dari satu,
/// `iverilog` dan `verilator` menjalankan semuanya.
pub fn root_modules(modules: &[Module]) -> Vec<String> {
    let mut terinstansi: Vec<String> = Vec::new();
    for m in modules {
        for inst in &m.instances {
            terinstansi.push(inst.module_name.clone());
        }
        for region in &m.generates {
            kumpulkan_instansiasi_generate(region, &mut terinstansi);
        }
    }
    modules
        .iter()
        .map(|m| m.name.clone())
        .filter(|nama| !terinstansi.iter().any(|i| i == nama))
        .collect()
}

/// Kumpulkan nama modul yang diinstansiasi di dalam region generate.
///
/// Instansiasi di body modul masuk `Module::instances`, sedangkan yang di
/// dalam `generate ... endgenerate` masuk ke `Module::generates` —oterapia
/// kalau keduanya tidak dipindai, modul anak_generate ikut dianggap root lalu
/// dijalankan dua kali (sekali standalone, sekali lewat induknya).
fn kumpulkan_instansiasi_generate(
    region: &sv_ast::generate::GenerateRegion,
    hasil: &mut Vec<String>,
) {
    fn item(item: &sv_ast::generate::GenerateItem, hasil: &mut Vec<String>) {
        use sv_ast::generate::GenerateItem::*;
        match item {
            Instance(inst) => hasil.push(inst.module_name.clone()),
            For(g) => region_items(&g.body, hasil),
            If(g) => {
                region_items(&g.then_branch, hasil);
                if let Some(branch) = &g.else_branch {
                    region_items(branch, hasil);
                }
            }
            Case(g) => {
                for arm in &g.arms {
                    region_items(&arm.body, hasil);
                }
            }
            Decl(_) | Assign { .. } | Process(_) => {}
        }
    }

    fn region_items(items: &[sv_ast::generate::GenerateItem], hasil: &mut Vec<String>) {
        for i in items {
            item(i, hasil);
        }
    }

    region_items(&region.items, hasil);
}

/// Compile multi-berkas.
///
/// Tanpa `--top`, modul yang tidak diinstansiasi modul lain menjadi top. Kalau
/// ada lebih dari satu, error dengan nama kandidatnya — bukan diam-diam
/// menjalankan modul terakhir saja, yang membuat output modul lain hilang
/// tanpa pesan.
pub fn compile_paths(paths: &[PathBuf], top: Option<&str>) -> Result<Design, String> {
    let modules = collect_modules(paths)?;
    let nama_top = pilih_top(&modules, top)?;
    let mut design =
        sv_elaborator::elaborate_top(&modules, &nama_top).map_err(|e| e.to_string())?;
    optimalkan(&mut design);
    Ok(design)
}

/// Daftar modul top yang akan dielaborasi: `--top` kalau ada, else semua
/// modul root.
///
/// `iverilog`/`verilator` menjalankan setiap modul root sebagai top tersendiri,
/// jadi pemeriksaan dan simulasi harus sama — memeriksa satu modul root
/// berarti modul lain tidak pernah diuji sama sekali.
pub fn tops_dipakai(modules: &[Module], top: Option<&str>) -> Result<Vec<String>, String> {
    match top {
        Some(nama) => Ok(vec![nama.to_string()]),
        None => {
            let roots = root_modules(modules);
            if roots.is_empty() {
                Err("tidak ada modul root: setiap modul diinstansiasi modul lain".to_string())
            } else {
                Ok(roots)
            }
        }
    }
}

/// Elaborasi SETIAP modul root sebagai design sendiri.
///
/// LRM §23.1: modul yang tidak diinstansiasi modul mana pun adalah modul root,
/// dan `iverilog`/`verilator` menjalankan semuanya. Versi lama mengambil modul
/// terakhir saja, jadi `$display` modul root lain tidak pernah tercetak —
/// testbench yang salah tetap terlihat "lulus".
pub fn compile_semua_root(
    paths: &[PathBuf],
    top: Option<&str>,
) -> Result<Vec<sv_ir::Design>, String> {
    let modules = collect_modules(paths)?;
    let tops = tops_dipakai(&modules, top)?;
    let mut designs = Vec::new();
    for nama in tops {
        let mut design =
            sv_elaborator::elaborate_top(&modules, &nama).map_err(|e| e.to_string())?;
        optimalkan(&mut design);
        designs.push(design);
    }
    Ok(designs)
}

/// Pilih nama modul top dari daftar module.
///
/// Satu modul root → dia. Beberapa modul root → error yang menyebut semuanya,
/// karena menjalankan hanya satu berarti modul lain tidak pernah dielaborasi
/// dan pengujiannya diam-diam lolos.
pub fn pilih_top(modules: &[Module], top: Option<&str>) -> Result<String, String> {
    if let Some(nama) = top {
        return Ok(nama.to_string());
    }
    let roots = root_modules(modules);
    match roots.len() {
        0 => Err("tidak ada modul root: setiap modul diinstansiasi modul lain".to_string()),
        1 => Ok(roots.into_iter().next().expect("panjang sudah dicek")),
        _ => Err(format!(
            "ada {} modul root ({}); pilih salah satu dengan --top",
            roots.len(),
            roots.join(", ")
        )),
    }
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

    fn modules_dari(source: &str) -> Vec<Module> {
        let tokens = sv_lexer::lex(source).expect("lex");
        sv_parser::parse_file(&tokens).expect("parse")
    }

    /// BUG: tanpa `--top` SVRust memakai modul TERAKHIR di berkas. Dua modul
    /// root (`a` dan `b`) karena keduanya tidak diinstansiasi, jadi hanya `b`
    /// yang dijalankan dan seluruh output `a` hilang tanpa pesan — testbench
    /// yang salah bisa tetap "lulus". Sekarang keduanya dielaborasi sebagai
    /// top tersendiri (LRM §23.1).
    #[test]
    fn bug_semua_modul_root_dielaborasi_bukan_satu() {
        let modules = modules_dari(
            "module a; initial $display(\"from a\"); endmodule \
             module b; initial $display(\"from b\"); endmodule",
        );
        assert_eq!(
            tops_dipakai(&modules, None).expect("tops"),
            vec!["a".to_string(), "b".to_string()]
        );
        // `pilih_top` masih dipakai jalur single-design (inspect/codegen),
        // jadi di sana ambiguitas harus disadari, bukan dijalankan diam-diam.
        let galat = pilih_top(&modules, None).expect_err("dua modul root harus error");
        assert!(galat.contains("--top"), "pesan: {galat}");
        assert!(galat.contains('a') && galat.contains('b'), "pesan: {galat}");
    }

    /// Modul yang diinstansiasi modul lain bukan root, jadi top tunggalnya
    /// modul yang menginstansiasi.
    #[test]
    fn top_tunggal_dipilih_dari_modul_bukan_instansi() {
        let modules = modules_dari(
            "module anak; endmodule \
             module induk; anak u(); initial $display(\"hi\"); endmodule",
        );
        assert_eq!(root_modules(&modules), vec!["induk".to_string()]);
        assert_eq!(pilih_top(&modules, None).expect("top"), "induk");
    }

    /// BUG: `root_modules` hanya membaca `Module::instances`, sedangkan
    /// instansiasi di dalam `generate ... endgenerate` masuk ke
    /// `Module::generates`. Akibatnya modul anak_generate ikut dianggap root
    /// lalu dijalankan DUA kali: sekali standalone, sekali lewat induknya —
    ///menghasilkan keluaran 3x untuk 2 instansi.
    #[test]
    fn bug_instansiasi_di_generate_bukan_modul_root() {
        let modules = modules_dari(
            "module anak; initial $display(\"ANAK\"); endmodule \
             module induk; \
               genvar i; \
               generate for (i = 0; i < 2; i = i + 1) begin : g anak u(); end endgenerate \
               initial $display(\"INDUK\"); \
             endmodule",
        );
        assert_eq!(root_modules(&modules), vec!["induk".to_string()]);
    }

    /// Instansiasi di lengan `if`/`case` generate juga harus dihitung.
    #[test]
    fn bug_instansiasi_di_lengan_generate_bukan_modul_root() {
        let modules = modules_dari(
            "module a; endmodule \
             module b; endmodule \
             module induk; \
               generate \
                 if (1) begin : x a u(); end else begin : y b u(); end \
               endgenerate \
             endmodule",
        );
        assert_eq!(root_modules(&modules), vec!["induk".to_string()]);
    }
}
