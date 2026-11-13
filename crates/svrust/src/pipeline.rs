// Tanggung jawab: orkestrasi SV source menjadi Design dan Rust source.
use rayon::prelude::*;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use sv_ast::module::Module;
use sv_ast::package::PackageDecl;
use sv_ast::typedef::TypeDefDecl;
use sv_cache::SvCache;
use sv_diag::{SourceId, SourceMap};
use sv_ir::Design;
use sv_lexer::SpannedToken;

use crate::workspace::Workspace;

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

/// Hasil kumpulkan module dari semua berkas RTL, termasuk `typedef` di luar
/// module (compilation-unit scope, LRM §8.20) dan parameter paket yang tersedia
/// untuk semua module.
pub struct ModuleCollection {
    pub modules: Vec<Module>,
    pub typedefs_global: Vec<TypeDefDecl>,
    pub packages: Vec<PackageDecl>,
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
    let mut design = sv_elaborator::elaborate_top(
        &parsed.modules,
        &parsed.typedefs_global,
        &parsed.packages,
        &nama_top,
    )
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
    laporan
}

/// Hasil parsing satu file (diproses secara independen dalam parallel).
struct ParsedFileResult {
    path: PathBuf,
    source_asli: String,
    source: String,
    tokens: Vec<SpannedToken>,
    modules: Vec<Module>,
    typedefs_global: Vec<TypeDefDecl>,
    packages: Vec<PackageDecl>,
    /// Error dari preprocessor/parser, jika ada. Dibuat jadi Diagnostic
    /// dengan SourceId yang benar selama merge.
    parse_error: Option<(String, String)>, // (message, path_display)
}

/// Hitung kunci cache untuk satu berkas: hash dari konten asli + path.
fn cache_key_file(path: &Path, source_asli: &str) -> sv_cache::CacheKey {
    let mut hasher = sha2::Sha256::new();
    hasher.update(path.to_string_lossy().as_bytes());
    hasher.update(b"|");
    hasher.update(source_asli.as_bytes());
    sv_cache::CacheKey(format!("{:x}", hasher.finalize()))
}

/// Hasil pemrosesan satu file beserta data yang ingin disimpan ke cache.
struct FileProcessResult {
    result: ParsedFileResult,
    /// Data untuk disimpan ke parse cache (None jika sudah ada di cache atau error).
    cache_data: Option<(sv_cache::CacheKey, sv_cache::CachedParse)>,
}

/// Proses satu berkas SV: preprocess → lex → parse.
/// Dapat dijalankan secara paralel karena setiap berkas independen.
/// Jika `cache` ada dan berkas belum berubah, gunakan hasil cache untuk
/// skip preprocessing, lexing, dan parsing.
/// Penulisan ke cache dikembalikan sebagai `cache_data` untuk dilakukan
/// secara sekuensial setelah parallel loop (karena `simpan_parse` butuh
/// `&mut SvCache`).
/// Parse source preprocessed yang di-cache (lex + parse saja, tanpa
/// preprocessing). Digunakan saat cache hit untuk mendapatkan Module struct.
fn parse_cached_source(
    _path: &Path,
    source_preprocessed: &str,
) -> Result<sv_parser::ParseResult, String> {
    let tokens =
        sv_lexer::lex(source_preprocessed).map_err(|e| format!("lexical error: {}", e.message))?;
    sv_parser::parse_file(&tokens).map_err(|e| e.to_string())
}

fn proses_satu_file(
    path: &Path,
    incdir: &[PathBuf],
    cache: Option<&sv_cache::SvCache>,
) -> FileProcessResult {
    let source_asli = match read_source(path) {
        Ok(s) => s,
        Err(e) => {
            return FileProcessResult {
                result: ParsedFileResult {
                    path: path.to_path_buf(),
                    source_asli: String::new(),
                    source: String::new(),
                    tokens: vec![],
                    modules: vec![],
                    typedefs_global: vec![],
                    packages: vec![],
                    parse_error: Some((
                        format!("gagal baca {}: {}", path.display(), e),
                        path.display().to_string(),
                    )),
                },
                cache_data: None,
            };
        }
    };

    // Cek cache sebelum preprocess/lex/parse.
    if let Some(c) = cache {
        let kunci = cache_key_file(path, &source_asli);
        if let Some(cached) = c.cek_parse(&kunci) {
            if cached.sukses {
                // Cache hit: re-parse source preprocessed (lex+parse cepat).
                match parse_cached_source(path, &cached.source_preprocessed) {
                    Ok(parsed) => {
                        return FileProcessResult {
                            result: ParsedFileResult {
                                path: path.to_path_buf(),
                                source_asli: cached.source_preprocessed.clone(),
                                source: cached.source_preprocessed.clone(),
                                tokens: vec![],
                                modules: parsed.modules,
                                typedefs_global: parsed.typedefs_global,
                                packages: parsed.packages,
                                parse_error: None,
                            },
                            cache_data: None, // Sudah ada di cache
                        };
                    }
                    Err(_) => {
                        // Re-parse gagal (source berubah?); fallthrough ke
                        // preprocess ulang.
                    }
                }
            } else {
                // Parse error yang di-cache — kembalikan sebagai error.
                return FileProcessResult {
                    result: ParsedFileResult {
                        path: path.to_path_buf(),
                        source_asli: cached.source_preprocessed.clone(),
                        source: cached.source_preprocessed.clone(),
                        tokens: vec![],
                        modules: vec![],
                        typedefs_global: vec![],
                        packages: vec![],
                        parse_error: Some((
                            format!("parse error (cached) for {}", path.display()),
                            path.display().to_string(),
                        )),
                    },
                    cache_data: None,
                };
            }
        } else {
        }
    }

    let (source_asli, source) = match read_preprocessed_with_original(path, incdir) {
        Ok((s, p)) => (s, p),
        Err(e) => {
            return FileProcessResult {
                result: ParsedFileResult {
                    path: path.to_path_buf(),
                    source_asli: source_asli.clone(),
                    source: String::new(),
                    tokens: vec![],
                    modules: vec![],
                    typedefs_global: vec![],
                    packages: vec![],
                    parse_error: Some((
                        format!("preprocessor error: {}", e),
                        path.display().to_string(),
                    )),
                },
                cache_data: None,
            };
        }
    };
    let tokens = match sv_lexer::lex(&source) {
        Ok(tokens) => tokens,
        Err(error) => {
            return FileProcessResult {
                result: ParsedFileResult {
                    path: path.to_path_buf(),
                    source_asli,
                    source,
                    tokens: vec![],
                    modules: vec![],
                    typedefs_global: vec![],
                    packages: vec![],
                    parse_error: Some((
                        format!("lexical error: {}", error.message),
                        path.display().to_string(),
                    )),
                },
                cache_data: None,
            };
        }
    };
    let parsed = match sv_parser::parse_file(&tokens) {
        Ok(parsed) => parsed,
        Err(error) => {
            return FileProcessResult {
                result: ParsedFileResult {
                    path: path.to_path_buf(),
                    source_asli,
                    source,
                    tokens,
                    modules: vec![],
                    typedefs_global: vec![],
                    packages: vec![],
                    parse_error: Some((error, path.display().to_string())),
                },
                cache_data: None,
            };
        }
    };

    // Kumpulkan nama module untuk cache.
    let module_names: Vec<String> = parsed.modules.iter().map(|m| m.name.clone()).collect();

    let kunci = cache_key_file(path, &source_asli);
    let cache_data = Some((
        kunci,
        sv_cache::CachedParse {
            diagnistik: vec![],
            sukses: true,
            source_preprocessed: source.clone(),
            module_names,
        },
    ));

    FileProcessResult {
        result: ParsedFileResult {
            path: path.to_path_buf(),
            source_asli,
            source,
            tokens,
            modules: parsed.modules,
            typedefs_global: parsed.typedefs_global,
            packages: parsed.packages,
            parse_error: None,
        },
        cache_data,
    }
}

/// Kumpulkan dan parse semua berkas RTL menjadi daftar module.
/// Modul dengan nama sama dari berkas berbeda ditolak agar ambiguity ketahuan.
///
/// `typedef` di luar module dikumpulkan sebagai compilation-unit scope
/// (LRM §8.20) dan tersedia untuk semua module.
///
/// PARALEL: preprocessing, lexing, dan parsing dijalankan secara paralel
/// per berkas menggunakan rayon. Hasil digabung secara sekuensial untuk
/// menentukan SourceId dan module_origins.
pub fn collect_modules(
    paths: &[PathBuf],
    incdir: &[PathBuf],
    _top: Option<&str>,
    mut cache: Option<&mut sv_cache::SvCache>,
) -> Result<ModuleCollection, crate::CliError> {
    let paths = expand_rtl_inputs(paths)?;

    // PARALEL: proses setiap berkas secara independen (baca cache hanya).
    let cache_ref = cache.as_deref();
    let hasil_per_file: Vec<FileProcessResult> = paths
        .par_iter()
        .map(|path| proses_satu_file(path, incdir, cache_ref))
        .collect();

    // SEKUENSIAL: simpan data baru ke cache (butuh &mut SvCache).
    if let Some(c) = cache.as_deref_mut() {
        for hasil in &hasil_per_file {
            if let Some((kunci, data)) = &hasil.cache_data {
                c.simpan_parse(kunci.clone(), data.clone());
            }
        }
    }

    // Pisahkan hasil sukses dan error parsing
    let mut all_diagnostics = Vec::new();
    let mut file_results = Vec::new();
    for hasil in hasil_per_file {
        file_results.push(hasil.result);
    }
    let mut modules = Vec::new();
    let mut typedefs_global = Vec::new();
    let mut packages = Vec::new();
    let mut sources = SourceMap::new();
    let mut source_preprocessed = HashMap::new();
    let mut module_origins = HashMap::new();

    if file_results.is_empty() && all_diagnostics.is_empty() {
        return Err("tidak ada module di berkas RTL".into());
    }

    for result in file_results {
        let source_id = sources.add_file(
            result.path.display().to_string(),
            result.source_asli.clone(),
        );
        source_preprocessed.insert(source_id, result.source.clone());

        // Tambah diagnostik parse error untuk file ini dengan SourceId yang benar
        if let Some((msg, _)) = &result.parse_error {
            let diag = diagnostic_parser(
                msg,
                &result.tokens,
                &result.source_asli,
                &result.source,
                Path::new(&result.path.display().to_string()),
                source_id,
                &sources,
            );
            all_diagnostics.push(diag);
            continue;
        }

        for m in &result.modules {
            if modules.iter().any(|x: &Module| x.name == m.name) {
                // Duplikasi module diam-diam dilewati
                continue;
            }
            let last_line = end_module_line(&result.tokens, &m.name).unwrap_or(m.span.line);
            module_origins.insert(
                m.name.clone(),
                ModuleOrigin {
                    source: source_id,
                    first_line: m.span.line,
                    last_line,
                },
            );
        }
        modules.extend(result.modules);
        typedefs_global.extend(result.typedefs_global);
        packages.extend(result.packages);
    }

    // Dengan `--top`, hanya pertahankan modul yang tercapai dari top.
    let modules_kept = if let Some(nama_top) = _top {
        let _total_modules = modules.len();
        let mut reachable = HashSet::new();
        let mut stack = vec![nama_top.to_string()];
        let module_by_name: HashMap<&str, &Module> =
            modules.iter().map(|m| (m.name.as_str(), m)).collect();
        while let Some(nama) = stack.pop() {
            if !reachable.insert(nama.clone()) {
                continue;
            }
            if let Some(modul) = module_by_name.get(nama.as_str()) {
                for inst in &modul.instances {
                    stack.push(inst.module_name.clone());
                }
            }
        }
        let kept: Vec<_> = modules
            .into_iter()
            .filter(|m| reachable.contains(&m.name))
            .collect();
        kept
    } else {
        modules
    };

    // Filter diagnostics: hanya pertahankan dari sumber yang tercapai top.
    let diagnostics_filtered = if _top.is_some() {
        let reachable_sources: HashSet<SourceId> = modules_kept
            .iter()
            .filter_map(|m| module_origins.get(&m.name))
            .map(|o| o.source)
            .collect();
        all_diagnostics
            .into_iter()
            .filter(|d| {
                d.primary
                    .as_ref()
                    .is_some_and(|label| reachable_sources.contains(&label.span.source))
            })
            .collect()
    } else {
        all_diagnostics
    };

    Ok(ModuleCollection {
        modules: modules_kept,
        typedefs_global,
        packages,
        diagnostics: diagnostics_filtered,
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
    // Hanya pertahankan origin untuk module yang berada dalam collection
    // (sudah difilter oleh reachability). Hindari memetakan error ke berkas
    // DV/lain yang tidak relevan hanya karena nomor baris sama.
    let kept_names: std::collections::HashSet<&str> =
        collection.modules.iter().map(|m| m.name.as_str()).collect();
    let mut matching_origins: Vec<_> = collection
        .module_origins
        .iter()
        .filter(|(nama, _)| kept_names.contains(nama.as_str()))
        .filter(|(_, origin)| {
            origin.first_line <= error.span.line && error.span.line <= origin.last_line
        })
        .map(|(_, origin)| *origin)
        .collect();
    matching_origins.sort_by_key(|origin| origin.source.0);
    let origin = matching_origins.first().copied().or_else(|| {
        collection
            .module_origins
            .iter()
            .filter(|(nama, _)| kept_names.contains(nama.as_str()))
            .map(|(_, origin)| *origin)
            .min_by_key(|origin| origin.source.0)
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
    let workspace = Workspace::detect()?;
    let cache_root = workspace.target_sv.join("cache");
    let mut cache = SvCache::baru_dengan_root(cache_root).map_err(|e| e.to_string())?;
    let collection =
        collect_modules(paths, incdir, top, Some(&mut cache)).map_err(|error| error.to_string())?;
    let nama_top = pilih_top(&collection.modules, top)?;
    let mut design = sv_elaborator::elaborate_top(
        &collection.modules,
        &collection.typedefs_global,
        &collection.packages,
        &nama_top,
    )
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
) -> Result<(Vec<sv_ir::Design>, String), crate::CliError> {
    let workspace = Workspace::detect()?;
    let cache_root = workspace.target_sv.join("cache");
    let mut cache = SvCache::baru_dengan_root(cache_root)?;
    let collection = collect_modules(paths, incdir, top, Some(&mut cache))?;
    let tops = tops_dipakai(&collection.modules, top)
        .map_err(|error| diagnostic_top_error(error, &collection))?;

    // Hash gabungan seluruh sumber RTL sebagai kunci cache: perubahan satu file
    // merubah hash → seluruh cache elaborate invalid (aman, meski konsentrat).
    let sumber_hash = hash_sumber(&collection);

    let designs = elaborate_tops(&collection, &tops, &sumber_hash, &mut cache)?;
    Ok((designs, sumber_hash))
}

/// Hitung hash SHA256 atas seluruh sumber RTL (preprocessed + asli) untuk
/// kunci cache elaborate. Perubahan satu file merubah hash → cache invalid.
pub fn hash_sumber(collection: &ModuleCollection) -> String {
    let mut hasher = Sha256::new();
    for sumber in collection.source_preprocessed.values() {
        hasher.update(sumber.as_bytes());
    }
    format!("{:x}", hasher.finalize())
}

#[allow(clippy::result_large_err)]
pub(crate) fn elaborate_tops(
    collection: &ModuleCollection,
    tops: &[String],
    dependensi_hash: &str,
    cache: &mut SvCache,
) -> Result<Vec<sv_ir::Design>, crate::CliError> {
    let mut diagnostics = collection.diagnostics.clone();

    // Langkah 1 (sequential): cek cache, kumpulkan kunci untuk root yang belum di-cache.
    let mut kunci_untuk_top: Vec<(String, sv_cache::CacheKey)> = Vec::new();
    let mut cached_designs: Vec<sv_ir::Design> = Vec::new();
    for nama in tops {
        let kunci = sv_cache::CacheKey::baru(
            nama,
            cache.ambil_dependensi_hash(nama).unwrap_or(dependensi_hash),
        );
        if let Some(cached) = cache.cek_elaborate(&kunci) {
            if cached.sukses {
                cached_designs.push(sv_ir::Design::new(&cached.nama_modul));
            }
            diagnostics.extend(cached.diagnistik.clone());
            continue;
        }
        kunci_untuk_top.push((nama.clone(), kunci));
    }

    // Langkah 2 (parallel): elaborate root yang belum di-cache secara independen dengan rayon.
    let hasil_elaborasi: Vec<Result<(String, sv_ir::Design), sv_diag::Diagnostic>> =
        kunci_untuk_top
            .into_par_iter()
            .map(|(nama, _kunci)| {
                match sv_elaborator::elaborate_top(
                    &collection.modules,
                    &collection.typedefs_global,
                    &collection.packages,
                    &nama,
                ) {
                    Ok(mut design) => {
                        optimalkan(&mut design);
                        Ok((nama.clone(), design))
                    }
                    Err(error) => {
                        if let crate::CliError::Reported {
                            diagnostics: mut berikut,
                            ..
                        } = diagnostic_elaborator(error, collection)
                        {
                            if let Some(d) = berikut.pop() {
                                Err(d)
                            } else {
                                Err(sv_diag::Diagnostic::error(
                                    "E0201",
                                    format!("elaborasi gagal untuk {}", nama),
                                    sv_diag::Span::empty(SourceId(0), 0),
                                ))
                            }
                        } else {
                            Err(sv_diag::Diagnostic::error(
                                "E0201",
                                format!("elaborasi gagal untuk {}", nama),
                                sv_diag::Span::empty(SourceId(0), 0),
                            ))
                        }
                    }
                }
            })
            .collect();

    // Langkah 3 (sequential): simpan hasil ke cache dan kumpulkan diagnostik.
    let mut designs = cached_designs;
    for hasil in hasil_elaborasi {
        match hasil {
            Ok((nama, design)) => {
                let kunci = sv_cache::CacheKey::baru(
                    &nama,
                    cache
                        .ambil_dependensi_hash(&nama)
                        .unwrap_or(dependensi_hash),
                );
                cache.simpan_elaborate(
                    kunci,
                    sv_cache::CachedElaborate {
                        nama_modul: nama.clone(),
                        diagnistik: vec![],
                        sukses: true,
                    },
                );
                designs.push(design);
            }
            Err(d) => diagnostics.push(d),
        }
    }

    // Simpan cache ke disk untuk panggilan CLI berikutnya.
    cache.simpan_ke_disk()?;

    if !diagnostics.is_empty() {
        // Deduplikasi: error sama dari banyak top module hanya ditampilkan sekali.
        let mut seen = std::collections::HashSet::new();
        diagnostics.retain(|d| {
            let kunci = (
                d.code.clone(),
                d.message.clone(),
                d.primary
                    .as_ref()
                    .map(|p| (p.span.source.0, p.span.start, p.span.end)),
            );
            seen.insert(kunci.clone())
        });
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

/// Pisahkan penanda ` [pos=baris:kolom]` dari pesan parser.
fn pisahkan_penanda(error: &str) -> (&str, Option<(usize, usize)>) {
    let Some((pesan, ekor)) = error.rsplit_once(sv_parser::pesan::PENANDA) else {
        return (error, None);
    };
    let Some((baris, kolom)) = ekor.strip_suffix(']').and_then(|isi| isi.split_once(':')) else {
        return (error, None);
    };
    match (baris.parse(), kolom.parse()) {
        (Ok(baris), Ok(kolom)) => (pesan, Some((baris, kolom))),
        _ => (error, None),
    }
}

/// Lokasi fallback saat parser error tidak punya penanda `[pos=baris:kolom]`
/// dan tidak cocok dengan token mana pun.
///
/// Strategi (urutan prioritas):
/// 1. Kalau pesan menyebut "ditemukan <token>", cari token itu di SELURUH
///    aliran token — penting untuk file besar seperti OpenTitan di mana token
///    target bisa berada jauh dari awal.
/// 2. Kalau tidak, cari kata kunci dari pesan di source_asli.
/// 3. Kalau masih tidak, gunakan baris 1 kolom 1 — bukan baris terakhir
///    (`endmodule`) agar tidak menyesalat pembaca.
fn cari_lokasi_fallback(error: &str, tokens: &[SpannedToken], source_asli: &str) -> (usize, usize) {
    // 1. Coba cari token yang disebut setelah "ditemukan ".
    if let Some(target) = error
        .split_once("ditemukan ")
        .map(|(_, rest)| rest.trim_end_matches(['.', ' ']).trim_matches('`'))
    {
        // Target bisa berupa:
        // - debug representation: `Ident("y")`, `Directive("NUM_REQUESTS")`, `Semi`
        // - format bahasa: `identifier 's'`, `nama sinyal`, `LParen`
        for token in tokens.iter() {
            let debug = format!("{:?}", token.token);
            if debug.contains(target) || target.contains(&debug) {
                return (token.span.line, token.span.col);
            }
        }
        // Ekstrak nama ident dari format "identifier 'X'" atau "nama sinyal".
        if let Some(nama) = target.split_whitespace().last().and_then(|w| {
            let s = w.trim_matches(['\'', '"']).to_string();
            if s.len() > 1 {
                Some(s)
            } else {
                None
            }
        }) {
            for token in tokens.iter() {
                if let sv_lexer::token::Token::Ident(ident) = &token.token {
                    if ident == &nama {
                        return (token.span.line, token.span.col);
                    }
                }
            }
        }
    }
    // 2. Coba cari kata kunci dari pesan di source_asli.
    for kata in error.split_whitespace() {
        let kata = kata.trim_matches(['(', ')', '[', ']', '{', '}', '"', '\'']);
        if kata.len() < 3 {
            continue;
        }
        if let Some((baris, _)) = source_asli
            .lines()
            .enumerate()
            .find(|(_, line)| line.contains(kata))
            .map(|(i, _)| (i + 1, 1))
        {
            return (baris, 1);
        }
    }
    // 3. Kalau error soal ekspresi kosong / input habis, cari titik koma,
    //    kurung kurawal, atau `endmodule` terdekat di source_asli — bukan
    //    paksa ke baris 1 atau baris terakhir.
    if error.contains("empty expression") || error.contains("input habis") {
        for (baris, line) in source_asli.lines().enumerate() {
            if line.contains(';')
                || line.contains('{')
                || line.contains('}')
                || line.contains("endmodule")
            {
                return (baris + 1, 1);
            }
        }
    }
    // 4. Kalau error soal seleksi / indeks (`[...]`), cari baris yang
    //    mengandung `[` di source_asli — itu lebih dekat ke masalah asli
    //    dibandingkan baris 1.
    if error.contains("seleksi") || error.contains("indeks") || error.contains("part-select") {
        for (baris, line) in source_asli.lines().enumerate() {
            if line.contains('[') {
                return (baris + 1, 1);
            }
        }
    }
    // 5. Awal file — bukan baris terakhir (`endmodule`).
    (1, 1)
}

/// Cari baris di `source_asli` yang paling relevan dengan pesan error.
///
/// Dipakai saat token error berasal dari file include (baris preprocessed
/// melebihi baris source asli), sehingga `baris_sumber_asli` tidak bisa
/// memetakan kembali. Strategi:
/// 1. Kalau error menyebut "ditemukan <ident>", cari ident itu di source_asli.
/// 2. Kalau tidak, cari kata kunci pertama dari pesan di source_asli.
fn cari_baris_dari_error(error: &str, source_asli: &str) -> Option<usize> {
    // 1. Coba cari ident yang disebut setelah "ditemukan ".
    if let Some(target) = error
        .split_once("ditemukan ")
        .map(|(_, rest)| rest.trim_end_matches(['.', ' ']).trim_matches('`'))
    {
        // Ekstrak nama ident dari format "identifier 'X'".
        let nama = target.split_whitespace().last().and_then(|w| {
            let s = w.trim_matches(['\'', '"']).to_string();
            if s.len() > 1 {
                Some(s)
            } else {
                None
            }
        });
        if let Some(nama) = nama {
            if let Some((baris, _)) = source_asli
                .lines()
                .enumerate()
                .find(|(_, line)| line.contains(&nama))
                .map(|(i, _)| (i + 1, 1))
            {
                return Some(baris);
            }
        }
        // Coba target langsung (tanpa splitting whitespace).
        if let Some((baris, _)) = source_asli
            .lines()
            .enumerate()
            .find(|(_, line)| line.contains(target))
            .map(|(i, _)| (i + 1, 1))
        {
            return Some(baris);
        }
    }
    // 2. Cari kata kunci dari pesan di source_asli.
    for kata in error.split_whitespace() {
        let kata = kata.trim_matches(['(', ')', '[', ']', '{', '}', '"', '\'']);
        if kata.len() < 3 {
            continue;
        }
        if let Some((baris, _)) = source_asli
            .lines()
            .enumerate()
            .find(|(_, line)| line.contains(kata))
            .map(|(i, _)| (i + 1, 1))
        {
            return Some(baris);
        }
    }
    None
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
    // Pesan parser membawa penanda ` [pos=baris:kolom]` dari token yang gagal
    // di-parse. Penanda itu dipakai lebih dulu supaya lokasi diagnostic tidak
    // jatuh ke `endmodule` atau ke token pertama yang kebetulan sama teksnya.
    let (error, posisi) = pisahkan_penanda(error);
    let token_text = error
        .split_once("ditemukan ")
        .map(|(_, rest)| rest.trim_end_matches(['.', ' ']).trim_matches('`'));
    let token_dari_penanda = posisi.and_then(|(baris, kolom)| {
        // Jika baris dari penanda melebihi baris source asli, berarti token
        // berasal dari file include. Jangan gunakan posisinya — jatuhkan ke
        // fallback yang mencari di source_asli.
        if baris > source_asli.lines().count() {
            return None;
        }
        tokens
            .iter()
            .position(|token| token.span.line == baris && token.span.col == kolom)
            .map(|indeks| indeks..indeks + 1)
    });
    let token_range = token_dari_penanda.or_else(|| {
        token_text.and_then(|target| {
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
    });
    let token_range = token_range.or_else(|| {
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
        // Error tanpa penanda `[pos=baris:kolom]` dan tanpa token yang
        // cocok. Jangan jatuhkan ke baris terakhir (`endmodule`) — itu
        // menyesatkan. Coba cari token yang disebut dalam pesan error,
        // kalau tidak ada gunakan baris pertama file (lokasi paling awal
        // yang relevan, bukan baris terakhir).
        let (fallback_line, fallback_col) = cari_lokasi_fallback(error, tokens, source_asli);
        let span = span_fallback(sources, source_id, fallback_line, fallback_col)
            .unwrap_or_else(|| sv_diag::Span::empty(source_id, source_asli.len()));
        return sv_diag::Diagnostic::error("E0002", format!("{}: {}", path.display(), error), span);
    };
    let token = &tokens[token_range.start];
    let line_asli = baris_sumber_asli(source_asli, source, token.span.line).unwrap_or_else(|| {
        // Token berasal dari file include (baris > source asli). Cari baris
        // di source_asli berdasarkan kata kunci dari pesan error.
        cari_baris_dari_error(error, source_asli).unwrap_or(token.span.line)
    });

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

    // Regresi: pesan "expected '='" tidak menyebut token, jadi dulu jatuh ke
    // baris terakhir berkas (`endmodule`). Penanda posisi dari parser membuat
    // diagnostic menunjuk token penyebab sebenarnya.
    #[test]
    fn error_parser_tanpa_token_tidak_lagi_menunjuk_endmodule() {
        let source = "module m;\n  assign a b;\nendmodule\n";
        let tokens = sv_lexer::lex(source).expect("lex");
        let error = match sv_parser::parse_file(&tokens) {
            Ok(_) => panic!("assign tanpa '=' harus gagal diparse"),
            Err(error) => error,
        };
        assert!(error.contains("[pos="), "{error}");

        let report = render_parser_error(&error, source, "sesat.sv");
        assert!(report.contains("--> sesat.sv:2:12"), "{report}");
        assert!(report.contains("2 |   assign a b;"), "{report}");
        assert!(!report.contains("endmodule"), "{report}");
        assert!(!report.contains("[pos="), "penanda bocor: {report}");
    }

    // Regresi: error `expected module name` pada `module ; endmodule` tidak
    // boleh jatuh ke baris `endmodule`, harus menunjuk ke `;` pada baris 1.
    #[test]
    fn error_module_tanpa_nama_tidak_jatuh_ke_endmodule() {
        let source = "module ;\n  logic a;\nendmodule\n";
        let tokens = sv_lexer::lex(source).expect("lex");
        let error = match sv_parser::parse_module(&tokens) {
            Ok(_) => panic!("module tanpa nama harus gagal diparse"),
            Err(error) => error,
        };
        assert!(error.contains("[pos="), "{error}");

        let report = render_parser_error(&error, source, "nonama.sv");
        assert!(report.contains("--> nonama.sv:1:8"), "{report}");
        assert!(report.contains("1 | module ;"), "{report}");
        assert!(!report.contains("endmodule"), "{report}");
        assert!(!report.contains("[pos="), "penanda bocor: {report}");
    }

    // Regresi: port list yang tidak tertutup (`input a; endmodule`) tidak
    // boleh melaporkan error di baris `endmodule`.
    #[test]
    fn error_port_list_tidak_tertutup_tidak_jatuh_ke_endmodule() {
        let source = "module m(input a; endmodule\n";
        let tokens = sv_lexer::lex(source).expect("lex");
        let error = match sv_parser::parse_module(&tokens) {
            Ok(_) => panic!("port list tidak tertutup harus gagal diparse"),
            Err(error) => error,
        };
        assert!(error.contains("[pos="), "{error}");

        let report = render_parser_error(&error, source, "port.sv");
        assert!(report.contains("--> port.sv:1:"), "{report}");
        assert!(
            report.contains("1 | module m(input a; endmodule"),
            "{report}"
        );
        assert!(!report.contains("[pos="), "penanda bocor: {report}");
    }

    // Regresi: `module ` (input habis sebelum nama) harus menunjuk ke awal
    // berkas, bukan ke baris `endmodule`.
    #[test]
    fn error_module_input_habis_tidak_jatuh_ke_endmodule() {
        let source = "module \nendmodule\n";
        let tokens = sv_lexer::lex(source).expect("lex");
        let error = match sv_parser::parse_module(&tokens) {
            Ok(_) => panic!("module tanpa nama harus gagal diparse"),
            Err(error) => error,
        };
        let report = render_parser_error(&error, source, "habis.sv");
        assert!(!report.contains("endmodule"), "{report}");
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
            packages: Vec::new(),
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
        let error =
            sv_elaborator::elaborate_top(&collection.modules, &[], &collection.packages, "m")
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
            packages: Vec::new(),
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

        let mut cache = SvCache::baru();
        let error = elaborate_tops(
            &collection,
            &["a".to_string(), "b".to_string()],
            "",
            &mut cache,
        )
        .expect_err("kedua root mengandung sinyal target yang tidak dikenal");
        let report = error
            .report()
            .expect("error harus dirender sebagai satu report");

        assert!(report.contains("undefined signal 'a'"), "{report}");
        assert!(report.contains("undefined signal 'b'"), "{report}");
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

    /// LRM §23.1: `elaborate_tops` harus menghasilkan SATU design per modul root,
    /// bukan hanya satu design untuk semua root.
    #[test]
    fn elaborate_tops_hasil_banyak_design_untuk_multi_root() {
        let modules = modules_dari(
            "module a; initial $display(\"from a\"); endmodule \
             module b; initial $display(\"from b\"); endmodule",
        );
        let tops = tops_dipakai(&modules, None).expect("tops");
        assert_eq!(tops, vec!["a".to_string(), "b".to_string()]);
        let sources = SourceMap::new();
        let collection = ModuleCollection {
            modules,
            typedefs_global: Vec::new(),
            packages: Vec::new(),
            diagnostics: Vec::new(),
            sources,
            source_preprocessed: HashMap::new(),
            module_origins: HashMap::new(),
        };
        let mut cache = sv_cache::SvCache::baru();
        let designs =
            elaborate_tops(&collection, &tops, "", &mut cache).expect("elaborate semua root");
        assert_eq!(designs.len(), 2, "harus ada 2 design untuk 2 root");
        let nama: Vec<_> = designs.iter().map(|d| d.name.clone()).collect();
        assert!(
            nama.contains(&"a".to_string()),
            "root a harus ada: {nama:?}"
        );
        assert!(
            nama.contains(&"b".to_string()),
            "root b harus ada: {nama:?}"
        );
    }
}
