// Tanggung jawab: orkestrasi SV source menjadi Design dan Rust source.
use std::collections::HashMap;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use sv_diag::{SourceId, SourceMap};

/// Ekspansi berkas `.f` (file list) menjadi daftar berkas SV konkret.
/// Baris kosong dan komentar (`//`, `#`) dilewati; path relatif terhadap
/// direktori berkas `.f`. Berkas `.f` bersarang juga diekspansi.
pub fn expand_rtl_inputs(paths: &[PathBuf]) -> Result<Vec<PathBuf>, String> {
    let mut result = Vec::new();
    let mut visited = HashSet::new();
    for p in paths {
        if p.extension().and_then(|e| e.to_str()) == Some("f") {
            expand_f_file(p, &mut result, &mut visited)?;
        } else {
            result.push(p.clone());
        }
    }
    Ok(result)
}

fn expand_f_file(
    fpath: &Path,
    result: &mut Vec<PathBuf>,
    visited: &mut HashSet<PathBuf>,
) -> Result<(), String> {
    let canonical = fpath.canonicalize().unwrap_or_else(|_| fpath.to_path_buf());
    if !visited.insert(canonical.clone()) {
        return Ok(());
    }
    let content =
        fs::read_to_string(fpath).map_err(|e| format!("gagal baca {}: {}", fpath.display(), e))?;
    let base = fpath
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    for raw in content.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with("//") || line.starts_with('#') {
            continue;
        }
        let p = if Path::new(line).is_absolute() {
            PathBuf::from(line)
        } else {
            base.join(line)
        };
        if p.extension().and_then(|e| e.to_str()) == Some("f") {
            expand_f_file(&p, result, visited)?;
        } else {
            result.push(p);
        }
    }
    Ok(())
}
use sv_ast::module::Module;
use sv_ast::typedef::TypeDefDecl;
use sv_ir::Design;

/// Hasil kumpulkan module dari semua berkas RTL, termasuk `typedef` di luar
/// module (compilation-unit scope, LRM §8.20) yang tersedia untuk semua module.
pub struct ModuleCollection {
    pub modules: Vec<Module>,
    pub typedefs_global: Vec<TypeDefDecl>,
    diagnostics: Vec<sv_diag::Diagnostic>,
    sources: SourceMap,
    source_preprocessed: HashMap<SourceId, String>,
    module_origins: HashMap<String, ModuleOrigin>,
}

#[derive(Clone, Copy)]
struct ModuleOrigin {
    source: SourceId,
    first_line: usize,
    last_line: usize,
}

/// Compile satu berkas; tanpa top, modul root tunggal yang dipakai.
pub fn compile(source: &str) -> Result<Design, String> {
    compile_with_top(source, None)
}

/// Compile dengan pilihan top; tanpa top pakai modul root tunggal.
pub fn compile_with_top(source: &str, top: Option<&str>) -> Result<Design, String> {
    let pp = sv_preprocessor::preprocess(source, None).map_err(|e| e.to_string())?;
    let tokens = sv_lexer::lex(&pp.source).map_err(|e| e.to_string())?;
    let parsed = sv_parser::parse_file(&tokens)?;
    let nama_top = pilih_top(&parsed.modules, top)?;
    let mut design =
        sv_elaborator::elaborate_top(&parsed.modules, &parsed.typedefs_global, &nama_top)
            .map_err(|e| e.to_string())?;
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
///
/// `typedef` di luar module dikumpulkan sebagai compilation-unit scope
/// (LRM §8.20) dan tersedia untuk semua module.
pub fn collect_modules(
    paths: &[PathBuf],
    incdir: &[PathBuf],
) -> Result<ModuleCollection, crate::CliError> {
    let paths = expand_rtl_inputs(paths)?;
    let mut modules = Vec::new();
    let mut typedefs_global = Vec::new();
    let mut diagnostics = Vec::new();
    let mut sources = SourceMap::new();
    let mut source_preprocessed = HashMap::new();
    let mut module_origins = HashMap::new();
    for path in paths {
        let (source_asli, source) = read_preprocessed_with_original(&path, incdir)?;
        let source_id = sources.add_file(path.display().to_string(), source_asli.clone());
        source_preprocessed.insert(source_id, source.clone());
        let tokens = match sv_lexer::lex(&source) {
            Ok(tokens) => tokens,
            Err(error) => {
                let line = baris_sumber_asli(&source_asli, &source, error.span.line)
                    .unwrap_or(error.span.line);
                let start = span_fallback(&sources, source_id, line, error.span.col)
                    .map(|span| span.start)
                    .unwrap_or(0);
                let span = sv_diag::Span::new(
                    source_id,
                    start..start + error.span.end.saturating_sub(error.span.start),
                );
                diagnostics.push(sv_diag::Diagnostic::error(
                    "E0101",
                    format!("lexical error: {}", error.message),
                    span,
                ));
                continue;
            }
        };
        let parsed = match sv_parser::parse_file(&tokens) {
            Ok(parsed) => parsed,
            Err(error) => {
                diagnostics.push(diagnostic_parser(
                    &error,
                    &tokens,
                    &source_asli,
                    &source,
                    &path,
                    source_id,
                    &sources,
                ));
                continue;
            }
        };
        for m in parsed.modules {
            if let Some(sama) = modules.iter().find(|x: &&Module| x.name == m.name) {
                let message = format!(
                    "module duplikat '{}' (ditemukan line {})",
                    m.name, sama.span.line
                );
                let span = baris_sumber_asli(&source_asli, &source, m.span.line).and_then(|line| {
                    sources.span_at(source_id, line, m.span.col).map(|span| {
                        sv_diag::Span::new(
                            source_id,
                            span.start..span.start + m.span.end.saturating_sub(m.span.start),
                        )
                    })
                });
                let span = span.unwrap_or_else(|| {
                    span_fallback(&sources, source_id, m.span.line, m.span.col)
                        .unwrap_or_else(|| sv_diag::Span::empty(source_id, 0))
                });
                diagnostics.push(sv_diag::Diagnostic::error("E0002", message, span));
                continue;
            }
            let last_line = end_module_line(&tokens, &m.name).unwrap_or(m.span.line);
            module_origins.insert(
                m.name.clone(),
                ModuleOrigin {
                    source: source_id,
                    first_line: m.span.line,
                    last_line,
                },
            );
            modules.push(m);
        }
        typedefs_global.extend(parsed.typedefs_global);
    }
    if modules.is_empty() && diagnostics.is_empty() {
        return Err("tidak ada module di berkas RTL".into());
    }
    Ok(ModuleCollection {
        modules,
        typedefs_global,
        diagnostics,
        sources,
        source_preprocessed,
        module_origins,
    })
}

fn end_module_line(tokens: &[sv_lexer::SpannedToken], module_name: &str) -> Option<usize> {
    let start = tokens.windows(2).position(|pair| {
        pair[0].token == sv_lexer::Token::Module
            && pair[1].token == sv_lexer::Token::Ident(module_name.to_string())
    })?;
    tokens[start + 2..]
        .iter()
        .find(|token| token.token == sv_lexer::Token::Endmodule)
        .map(|token| token.span.line)
}

pub(crate) fn diagnostic_elaborator(
    error: sv_elaborator::ElaborateError,
    collection: &ModuleCollection,
) -> crate::CliError {
    let pesan = error.to_string();
    let mut matching_origins: Vec<_> = collection
        .module_origins
        .values()
        .filter(|origin| {
            origin.first_line <= error.span.line && error.span.line <= origin.last_line
        })
        .copied()
        .collect();
    matching_origins.sort_by_key(|origin| origin.source.0);
    let origin = matching_origins.first().copied().or_else(|| {
        collection
            .module_origins
            .values()
            .min_by_key(|origin| origin.source.0)
            .copied()
    });
    let Some(origin) = origin else {
        return diagnostic_tanpa_lokasi("E0201", pesan);
    };

    let Some(preprocessed) = collection.source_preprocessed.get(&origin.source) else {
        return diagnostic_tanpa_lokasi("E0201", pesan);
    };
    let Some(source_file) = collection.sources.get(origin.source) else {
        return diagnostic_tanpa_lokasi("E0201", pesan);
    };
    let line = baris_sumber_asli(&source_file.text, preprocessed, error.span.line)
        .unwrap_or(error.span.line);
    let span = span_fallback(&collection.sources, origin.source, line, error.span.col)
        .unwrap_or_else(|| sv_diag::Span::empty(origin.source, source_file.text.trim_end().len()));
    let span = sv_diag::Span::new(
        origin.source,
        span.start..span.start + error.span.end.saturating_sub(error.span.start).max(1),
    );
    crate::CliError::reported(
        vec![sv_diag::Diagnostic::error("E0201", error.message, span)],
        collection.sources.clone(),
    )
}

pub(crate) fn diagnostic_tanpa_lokasi(kode: &str, pesan: impl Into<String>) -> crate::CliError {
    crate::CliError::reported(
        vec![sv_diag::Diagnostic::new(
            sv_diag::Severity::Error,
            kode,
            pesan,
        )],
        SourceMap::new(),
    )
}

pub(crate) fn diagnostic_top_error(
    pesan: String,
    collection: &ModuleCollection,
) -> crate::CliError {
    let mut diagnostics = collection.diagnostics.clone();
    let Some(module) = collection.modules.first() else {
        if !diagnostics.is_empty() {
            return crate::CliError::reported(diagnostics, collection.sources.clone());
        }
        return diagnostic_tanpa_lokasi("E0201", pesan);
    };
    let Some(origin) = collection.module_origins.get(&module.name) else {
        return diagnostic_tanpa_lokasi("E0201", pesan);
    };
    let Some(source) = collection.sources.get(origin.source) else {
        return diagnostic_tanpa_lokasi("E0201", pesan);
    };
    let span = span_fallback(
        &collection.sources,
        origin.source,
        module.span.line,
        module.span.col,
    )
    .unwrap_or_else(|| sv_diag::Span::empty(origin.source, source.text.trim_end().len()));
    diagnostics.push(sv_diag::Diagnostic::error("E0201", pesan, span));
    crate::CliError::reported(diagnostics, collection.sources.clone())
}

fn diagnostic_dengan_sumber(
    kode: &str,
    pesan: impl Into<String>,
    nama: String,
    source: String,
    line: usize,
    col: usize,
) -> crate::CliError {
    let mut sources = SourceMap::new();
    let source_id = sources.add_file(nama, source);
    let span = span_fallback(&sources, source_id, line, col)
        .unwrap_or_else(|| sv_diag::Span::empty(source_id, 0));
    crate::CliError::reported(vec![sv_diag::Diagnostic::error(kode, pesan, span)], sources)
}

fn span_fallback(
    sources: &SourceMap,
    source: SourceId,
    line: usize,
    col: usize,
) -> Option<sv_diag::Span> {
    let file = sources.get(source)?;
    let line = line.clamp(1, file.text.lines().count().max(1));
    let line_text = file.line(line)?;
    let col = col.clamp(1, line_text.chars().count() + 1);
    sources.span_at(source, line, col)
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
pub fn compile_paths(
    paths: &[PathBuf],
    top: Option<&str>,
    incdir: &[PathBuf],
) -> Result<Design, String> {
    let collection = collect_modules(paths, incdir).map_err(|error| error.to_string())?;
    let nama_top = pilih_top(&collection.modules, top)?;
    let mut design =
        sv_elaborator::elaborate_top(&collection.modules, &collection.typedefs_global, &nama_top)
            .map_err(|e| e.to_string())?;
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
    incdir: &[PathBuf],
) -> Result<Vec<sv_ir::Design>, crate::CliError> {
    let collection = collect_modules(paths, incdir)?;
    let tops = tops_dipakai(&collection.modules, top)
        .map_err(|error| diagnostic_top_error(error, &collection))?;
    elaborate_tops(&collection, &tops)
}

pub(crate) fn elaborate_tops(
    collection: &ModuleCollection,
    tops: &[String],
) -> Result<Vec<sv_ir::Design>, crate::CliError> {
    let mut designs = Vec::new();
    let mut diagnostics = collection.diagnostics.clone();
    for nama in tops {
        match sv_elaborator::elaborate_top(&collection.modules, &collection.typedefs_global, nama) {
            Ok(mut design) => {
                optimalkan(&mut design);
                designs.push(design);
            }
            Err(error) => {
                if let crate::CliError::Reported {
                    diagnostics: mut berikut,
                    ..
                } = diagnostic_elaborator(error, collection)
                {
                    diagnostics.append(&mut berikut);
                }
            }
        }
    }
    if !diagnostics.is_empty() {
        return Err(crate::CliError::reported(
            diagnostics,
            collection.sources.clone(),
        ));
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

/// Baca dan preprocess satu berkas; `include relatif ke direktori berkas
/// dan direktori pencarian yang diberikan lewat --incdir.
pub fn read_preprocessed(path: &Path, incdir: &[PathBuf]) -> Result<String, String> {
    read_preprocessed_with_original(path, incdir)
        .map(|(_, source)| source)
        .map_err(|error| error.to_string())
}

fn read_preprocessed_with_original(
    path: &Path,
    incdir: &[PathBuf],
) -> Result<(String, String), crate::CliError> {
    let source = read_source(path)?;
    let dir = path.parent().map(|p| p.to_path_buf());
    // Kumpulkan direktori pencarian: --incdir dulu, lalu direktori berkas.
    let mut all_dirs: Vec<PathBuf> = incdir.to_vec();
    if let Some(d) = dir {
        all_dirs.push(d);
    }
    let all_dirs_refs: Vec<&Path> = all_dirs.iter().map(|p| p.as_path()).collect();
    let display_path = path.display().to_string();
    let pp = sv_preprocessor::preprocess_with_dirs(&source, &all_dirs_refs, &display_path)
        .map_err(|error| {
            let (file_name, source_diagnostic, line, col) = if error.file == display_path {
                (display_path.clone(), source.clone(), error.line, error.col)
            } else {
                match fs::read_to_string(&error.file) {
                    Ok(source_include) => {
                        (error.file.clone(), source_include, error.line, error.col)
                    }
                    Err(_) => (display_path.clone(), source.clone(), 1, 1),
                }
            };
            diagnostic_dengan_sumber(
                "E0100",
                format!("preprocessor error: {}", error.message),
                file_name,
                source_diagnostic,
                line,
                col,
            )
        })?;
    Ok((source, pp.source))
}

fn baris_sumber_asli(source_asli: &str, source: &str, line: usize) -> Option<usize> {
    let baris = source.lines().nth(line.checked_sub(1)?)?;
    if source_asli.lines().nth(line - 1) == Some(baris) {
        return Some(line);
    }
    let mut cocok = source_asli
        .lines()
        .enumerate()
        .filter(|(_, kandidat)| *kandidat == baris)
        .map(|(indeks, _)| indeks + 1);
    let baris_asli = cocok.next()?;
    cocok.next().is_none().then_some(baris_asli)
}

fn diagnostic_parser(
    error: &str,
    tokens: &[sv_lexer::SpannedToken],
    source_asli: &str,
    source: &str,
    path: &Path,
    source_id: SourceId,
    sources: &SourceMap,
) -> sv_diag::Diagnostic {
    let token_text = error
        .split_once("ditemukan ")
        .map(|(_, rest)| rest.trim_end_matches(['.', ' ']).trim_matches('`'));
    let token_range = token_text
        .and_then(|target| {
            (0..tokens.len()).find_map(|start| {
                let mut actual = String::new();
                for (end, token) in tokens.iter().enumerate().skip(start) {
                    if end > start {
                        actual.push(' ');
                    }
                    actual.push_str(&format!("{:?}", token.token));
                    if actual == target {
                        return Some(start..end + 1);
                    }
                    if actual.len() > target.len() {
                        break;
                    }
                }
                None
            })
        })
        .or_else(|| {
            let token = tokens
                .iter()
                .position(|token| error.contains(&format!("{:?}", token.token)))?;
            Some(token..token + 1)
        });
    let token_range = token_range.or_else(|| {
        let nama = error.strip_prefix("module duplikat: ")?;
        let (index, _) = tokens
            .windows(2)
            .enumerate()
            .filter(|(_, pair)| {
                pair[0].token == sv_lexer::Token::Module
                    && pair[1].token == sv_lexer::Token::Ident(nama.to_string())
            })
            .nth(1)?;
        Some(index..index + 1)
    });

    let Some(token_range) = token_range else {
        if error.contains("expected 'endmodule'") || error.contains("akhir input") {
            let offset = source_asli.trim_end().len();
            return sv_diag::Diagnostic::error(
                "E0002",
                error,
                sv_diag::Span::empty(source_id, offset),
            );
        }
        let line = source_asli.lines().count().max(1);
        let col = source_asli
            .lines()
            .last()
            .map(|line| line.chars().count() + 1)
            .unwrap_or(1);
        let span = span_fallback(sources, source_id, line, col)
            .unwrap_or_else(|| sv_diag::Span::empty(source_id, source_asli.len()));
        return sv_diag::Diagnostic::error("E0002", format!("{}: {}", path.display(), error), span);
    };
    let token = &tokens[token_range.start];
    let line_asli =
        baris_sumber_asli(source_asli, source, token.span.line).unwrap_or(token.span.line);

    let start = span_fallback(sources, source_id, line_asli, token.span.col)
        .map(|span| span.start)
        .unwrap_or(0);
    let end = tokens[token_range.end - 1].span.end;
    let span = sv_diag::Span::new(
        source_id,
        start..start + end.saturating_sub(token.span.start),
    );
    sv_diag::Diagnostic::error("E0002", error, span)
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

    fn render_parser_error(error: &str, source: &str, name: &str) -> String {
        let tokens = sv_lexer::lex(source).expect("lex");
        let mut sources = SourceMap::new();
        let source_id = sources.add_file(name, source);
        let diagnostic = diagnostic_parser(
            error,
            &tokens,
            source,
            source,
            Path::new(name),
            source_id,
            &sources,
        );
        sv_diag::render_report(&[diagnostic], &sources, sv_diag::RenderOptions::default())
    }

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
    fn error_parser_dirender_dengan_kode_lokasi_dan_cuplikan() {
        let source = "module m;\n  assign y = {a b};\nendmodule\n";
        let tokens = sv_lexer::lex(source).expect("lex");
        let error = match sv_parser::parse_file(&tokens) {
            Ok(_) => panic!("concatenation yang malformed harus gagal diparse"),
            Err(error) => error,
        };
        let report = render_parser_error(&error, source, "jtagdpi.sv");

        assert!(report.contains("[E0002]"));
        assert!(report.contains("--> jtagdpi.sv:2:"));
        assert!(report.contains("2 |   assign y = {a b};"));
        assert!(report.contains("= expected koma atau penutup"));
        assert!(report.contains("ringkasan: 1 error(s), 0 warning(s)"));
    }

    #[test]
    fn error_parser_dengan_token_ditemukan_mendapat_lokasi() {
        let source = "module m;\n  assign y = {a b};\nendmodule\n";
        let report = render_parser_error(
            "expected koma atau penutup kurung kurawal, ditemukan Ident(\"b\")",
            source,
            "concat.sv",
        );

        assert!(report.contains("--> concat.sv:2:"));
        assert!(report.contains("2 |   assign y = {a b};"));
        assert!(report.contains("= expected koma atau penutup kurung kurawal"));
    }

    #[test]
    fn pemetaan_baris_parser_mengabaikan_baris_directive_yang_dihapus() {
        let sumber_asli = "`define FLAG\nmodule m;\n  assign y = a;\nendmodule\n";
        let sumber_preprocess = "module m;\n  assign y = a;\nendmodule\n";
        assert_eq!(
            baris_sumber_asli(sumber_asli, sumber_preprocess, 2),
            Some(3)
        );
    }

    #[test]
    fn pemetaan_baris_ambigu_tidak_mengarang_lokasi() {
        let sumber_asli =
            "module m;\n  assign y = b;\n  assign y = a;\n  assign y = a;\nendmodule\n";
        let sumber_preprocess = "module m;\n  assign y = a;\nendmodule\n";
        assert_eq!(baris_sumber_asli(sumber_asli, sumber_preprocess, 2), None);
    }

    #[test]
    fn parser_multi_token_menunjuk_ke_seluruh_indeks_genvar() {
        let source = "module m;\n  assign y[rxcount-1] = 1;\nendmodule\n";
        let report = render_parser_error(
            "indeks genvar harus satu token, ditemukan `Ident(\"rxcount\") Minus Number(1)`",
            source,
            "uartdpi.sv",
        );

        assert!(report.contains("--> uartdpi.sv:2:12"), "{report}");
        assert!(report.contains("^^^^^^^^^"), "{report}");
    }

    #[test]
    fn error_parser_di_eof_tidak_jatuh_ke_kode_generik() {
        let source = "module m;\n";
        let tokens = sv_lexer::lex(source).expect("lex");
        let error = match sv_parser::parse_file(&tokens) {
            Ok(_) => panic!("endmodule yang hilang harus gagal diparse"),
            Err(error) => error,
        };
        let report = render_parser_error(&error, source, "eof.sv");

        assert!(report.contains("[E0002]"));
        assert!(report.contains("--> eof.sv:1:"));
        assert!(!report.contains("[E0000]"));
    }

    #[test]
    fn error_parser_module_duplikat_menunjuk_ke_deklarasi_kedua() {
        let source = "module m; endmodule\nmodule m; endmodule\n";
        let tokens = sv_lexer::lex(source).expect("lex");
        let error = match sv_parser::parse_file(&tokens) {
            Ok(_) => panic!("module duplikat harus gagal diparse"),
            Err(error) => error,
        };
        let report = render_parser_error(&error, source, "duplikat.sv");

        assert!(report.contains("--> duplikat.sv:2:1"));
        assert!(report.contains("2 | module m; endmodule"));
    }

    #[test]
    fn error_elaborasi_menggunakan_span_ast() {
        let source = "module m;\n  assign y = a;\nendmodule\n";
        let tokens = sv_lexer::lex(source).expect("lex");
        let parsed = sv_parser::parse_file(&tokens).expect("parse");
        let mut sources = SourceMap::new();
        let source_id = sources.add_file("elab.sv", source);
        let collection = ModuleCollection {
            modules: parsed.modules,
            typedefs_global: parsed.typedefs_global,
            diagnostics: Vec::new(),
            sources,
            source_preprocessed: HashMap::from([(source_id, source.to_string())]),
            module_origins: HashMap::from([(
                "m".to_string(),
                ModuleOrigin {
                    source: source_id,
                    first_line: 1,
                    last_line: 3,
                },
            )]),
        };
        let error = sv_elaborator::elaborate_top(&collection.modules, &[], "m")
            .expect_err("sinyal y tidak dideklarasikan");
        let report = diagnostic_elaborator(error, &collection)
            .report()
            .expect("error harus dirender");

        assert!(report.contains("[E0201]"));
        assert!(report.contains("--> elab.sv:2:"));
        assert!(report.contains("2 |   assign y = a;"));
        assert!(!report.contains("[E0000]"));
    }

    #[test]
    fn elaborasi_mengumpulkan_error_dari_semua_root() {
        let source = "module a;\n  assign missing_a = a;\nendmodule\n\
                      module b;\n  assign missing_b = b;\nendmodule\n";
        let tokens = sv_lexer::lex(source).expect("lex");
        let parsed = sv_parser::parse_file(&tokens).expect("parse");
        let mut sources = SourceMap::new();
        let source_id = sources.add_file("multi.sv", source);
        let collection = ModuleCollection {
            modules: parsed.modules,
            typedefs_global: parsed.typedefs_global,
            diagnostics: Vec::new(),
            sources,
            source_preprocessed: HashMap::from([(source_id, source.to_string())]),
            module_origins: HashMap::from([
                (
                    "a".to_string(),
                    ModuleOrigin {
                        source: source_id,
                        first_line: 1,
                        last_line: 3,
                    },
                ),
                (
                    "b".to_string(),
                    ModuleOrigin {
                        source: source_id,
                        first_line: 4,
                        last_line: 6,
                    },
                ),
            ]),
        };

        let error = elaborate_tops(&collection, &["a".to_string(), "b".to_string()])
            .expect_err("kedua root mengandung sinyal target yang tidak dikenal");
        let report = error
            .report()
            .expect("error harus dirender sebagai satu report");

        assert_eq!(report.matches("error:").count(), 2, "{report}");
        assert!(report.contains("undefined signal 'missing_a'"), "{report}");
        assert!(report.contains("undefined signal 'missing_b'"), "{report}");
        assert!(report.contains("--> multi.sv:2:"), "{report}");
        assert!(report.contains("--> multi.sv:5:"), "{report}");
        assert!(
            report.contains("ringkasan: 2 error(s), 0 warning(s)"),
            "{report}"
        );
    }

    #[test]
    fn pipeline_mengumpulkan_error_parse_dan_elaborasi_lintas_berkas() {
        let directory = std::env::temp_dir().join(format!(
            "svrust-diag-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("jam sistem valid")
                .as_nanos()
        ));
        std::fs::create_dir(&directory).expect("buat direktori fixture");
        let bad = directory.join("bad.sv");
        let semantic = directory.join("semantic.sv");
        std::fs::write(&bad, "module bad; assign y = {a b}; endmodule\n")
            .expect("tulis fixture parser");
        std::fs::write(
            &semantic,
            "module semantic; assign missing = value; endmodule\n",
        )
        .expect("tulis fixture elaborasi");

        let error = compile_semua_root(&[bad.clone(), semantic.clone()], None, &[])
            .expect_err("diagnostik parser dan elaborasi dikumpulkan");
        let report = error.report().expect("satu laporan diagnostik");
        assert!(
            report.contains(&format!("--> {}:1:", bad.display())),
            "{report}"
        );
        assert!(
            report.contains(&format!("--> {}:1:", semantic.display())),
            "{report}"
        );
        assert!(
            report.contains("concatenation") || report.contains("koma"),
            "{report}"
        );
        assert!(report.contains("missing"), "{report}");
        assert!(
            report.contains("ringkasan: 2 error(s), 0 warning(s)"),
            "{report}"
        );

        std::fs::remove_file(bad).expect("hapus fixture parser");
        std::fs::remove_file(semantic).expect("hapus fixture elaborasi");
        std::fs::remove_dir(directory).expect("hapus direktori fixture");
    }

    #[test]
    fn error_tanpa_span_memakai_kode_domain() {
        let report = diagnostic_tanpa_lokasi("E0101", "lexical error")
            .report()
            .expect("error harus dirender");
        assert!(report.contains("[E0101]"));
        assert!(report.contains("ringkasan: 1 error(s), 0 warning(s)"));
        assert!(!report.contains("[E0000]"));
    }

    #[test]
    fn value_plusargs_diparse_berhasil() {
        // $value$plusargs sekarang didukung (LRM §20.9).
        let source = "module m;\n  initial $value$plusargs(\"port=%0d\", port);\nendmodule\n";
        let tokens = sv_lexer::lex(source).expect("lex");
        let ast = sv_parser::parse_file(&tokens).expect("parse harus berhasil");
        assert!(!ast.modules.is_empty());
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
        sv_parser::parse_file(&tokens).expect("parse").modules
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
