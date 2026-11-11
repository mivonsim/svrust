// Tanggung jawab: dispatch perintah ke implementasi.
use std::path::Path;

use crate::cli::{Command, Options};
use crate::pipeline::{generate_rust, compile_semua_root};
use crate::workspace::Workspace;
use sv_cache::{CacheKey, CachedCodegen};
use sv_codegen_rust::module_name::module_name;

pub fn dispatch(opts: Options) -> Result<(), crate::CliError> {
    let workspace = Workspace::detect()?;

    match opts.command {
        Command::Clean => {
            workspace.clean()?;
            println!("cleaned target/sv/");
            Ok(())
        }
        Command::Help => {
            crate::cli::print_usage();
            Ok(())
        }
        Command::Version => {
            crate::cli::print_version();
            Ok(())
        }
        Command::Inspect | Command::Elaborate => {
            // Semua modul root diperiksa, sama seperti `check`/`run`: kalau
            // hanya satu, modul root lain tidak pernah dilaporkan.
            // Inspect/Elaborate tidak pakai codegen cache karena butuh
            // Design (processes, variables) untuk ditampilkan.
            let (designs, _) =
                compile_semua_root(&opts.rtl, opts.top.as_deref(), &opts.incdir)?;
            for design in designs {
                let stats = sv_elaborator::DesignStats::of(&design);
                println!("{}", stats);
                if opts.command == Command::Elaborate {
                    println!("\nProcesses:");
                    for p in &design.processes {
                        println!(
                            "  {} ({}) - {} sensitivity",
                            p.name,
                            p.kind,
                            p.sensitivity.len()
                        );
                    }
                }
                println!();
            }
            Ok(())
        }
        Command::Codegen => {
            let (designs, sumber_hash) =
                compile_semua_root(&opts.rtl, opts.top.as_deref(), &opts.incdir)?;
            let cache_root = workspace.target_sv.join("cache");
            let mut cache = sv_cache::SvCache::baru_dengan_root(cache_root)?;
            for design in &designs {
                let kunci = CacheKey::baru(&design.name, &sumber_hash);
                let rust = if let Some(cached) = cache.cek_codegen(&kunci) {
                    cached.kode_rust.clone()
                } else {
                    let rust = generate_rust(design);
                    cache.simpan_codegen(
                        kunci,
                        CachedCodegen {
                            kode_rust: rust.clone(),
                            nama_modul_rust: module_name(&design.name),
                        },
                    );
                    rust
                };
                let path = workspace.write_generated(&design.name, &rust)?;
                println!("generated {}", path.display());
            }
            cache.simpan_ke_disk()?;
            Ok(())
        }
        Command::Check => {
            // LRM §23.1: setiap modul root dielaborasi sebagai top tersendiri,
            // jadi `check` harus memeriksa semuanya — memeriksa satu saja
            // membiarkan modul lain tidak pernah diuji.
            // `compile_semua_root` sudah pakai parse cache + elaborate cache.
            // Hasilnya cukup untuk validasi: nama-nama top yang valid.
            let (designs, _sumber_hash) =
                compile_semua_root(&opts.rtl, opts.top.as_deref(), &opts.incdir)?;
            println!(
                "check OK ({} module, top {})",
                designs.len(),
                designs
                    .iter()
                    .map(|design| design.name.as_str())
                    .collect::<Vec<_>>()
                    .join("+")
            );
            Ok(())
        }
        Command::Build | Command::Run => {
            // LRM §23.1: setiap modul yang tidak diinstansiasi modul lain
            // adalah modul root dan dijalankan sebagai top tersendiri. Versi
            // lama hanya mengelaborasi modul terakhir, jadi output modul root
            // lain hilang tanpa pesan.
            let (designs, sumber_hash) =
                compile_semua_root(&opts.rtl, opts.top.as_deref(), &opts.incdir)?;
            // Dua nama modul SV bisa menjadi identik setelah normalisasi
            // (`foo_bar` vs `FOO_BAR`), jadi nama Rust dibuat unik lebih dulu;
            // kalau tidak, kode hasil generate bentrok saat compile.
            let module_names = sv_codegen_rust::module_name::module_names_unik(
                &designs.iter().map(|d| d.name.clone()).collect::<Vec<_>>(),
            );
            let mut parts: Vec<(&str, String)> = Vec::new();
            let cache_root = workspace.target_sv.join("cache");
            let mut cache = sv_cache::SvCache::baru_dengan_root(cache_root)?;
            for (design, module) in designs.iter().zip(&module_names) {
                let kunci = CacheKey::baru(&design.name, &sumber_hash);
                let rust = if let Some(cached) = cache.cek_codegen(&kunci) {
                    // CACHE HIT: gunakan Rust source yang sudah di-generate,
                    // skip re-elaborasi + re-codegen.
                    cached.kode_rust.clone()
                } else {
                    let rust = generate_rust(design);
                    cache.simpan_codegen(
                        kunci,
                        CachedCodegen {
                            kode_rust: rust.clone(),
                            nama_modul_rust: module.to_string(),
                        },
                    );
                    rust
                };
                workspace.write_generated(&design.name, &rust)?;
                parts.push((module.as_str(), rust));
            }
            cache.simpan_ke_disk()?;

            // LRM §23.2: tiap root yang memanggil `$dumpvars` punya berkasnya
            // sendiri. Memakai path dari root pertama saja membuat `$dumpvars`
            // di root lain hilang tanpa pesan, dan dua root yang menunjuk path
            // sama akan saling menimpa.
            let paths = crate::vcd_path::vcd_paths(&designs, opts.vcd.as_deref())?;
            let pakai_vcd = paths.iter().any(|p| p.is_some());
            // Satu crate paralel per modul root (parallel.md fase 5): setiap
            // root dikompilasi dalam crate sendiri, dibangun dengan `cargo -j N`.
            let view: Vec<(&str, &sv_ir::Design, Option<&Path>)> = module_names
                .iter()
                .zip(designs.iter())
                .zip(paths.iter())
                .map(|((m, d), p)| (m.as_str(), d, p.as_deref()))
                .collect();
            workspace.write_sim_project(&view, opts.steps, pakai_vcd)?;
            workspace.build(opts.release)?;

            if opts.command == Command::Run {
                workspace.run(opts.release, &opts.plusargs, &view)?;
            } else {
                println!("build OK");
            }
            Ok(())
        }
    }
}
