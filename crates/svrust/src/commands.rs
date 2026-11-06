// Tanggung jawab: dispatch perintah ke implementasi.
use std::path::Path;

use crate::cli::{Command, Options};
use crate::pipeline::generate_rust;
use crate::workspace::Workspace;

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
            for design in
                crate::pipeline::compile_semua_root(&opts.rtl, opts.top.as_deref(), &opts.incdir)?
            {
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
            for design in
                crate::pipeline::compile_semua_root(&opts.rtl, opts.top.as_deref(), &opts.incdir)?
            {
                let rust = generate_rust(&design);
                let path = workspace.write_generated(&design.name, &rust)?;
                println!("generated {}", path.display());
            }
            Ok(())
        }
        Command::Check => {
            // LRM §23.1: setiap modul root dielaborasi sebagai top tersendiri,
            // jadi `check` harus memeriksa semuanya — memeriksa satu saja
            // membiarkan modul lain tidak pernah diuji.
            let collection =
                crate::pipeline::collect_modules(&opts.rtl, &opts.incdir, opts.top.as_deref())?;
            let tops = crate::pipeline::tops_dipakai(&collection.modules, opts.top.as_deref())
                .map_err(|error| crate::pipeline::diagnostic_top_error(error, &collection))?;
            let designs = crate::pipeline::elaborate_tops(&collection, &tops)?;
            println!(
                "check OK ({} module, top {})",
                collection.modules.len(),
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
            let designs =
                crate::pipeline::compile_semua_root(&opts.rtl, opts.top.as_deref(), &opts.incdir)?;
            // Dua nama modul SV bisa menjadi identik setelah normalisasi
            // (`foo_bar` vs `FOO_BAR`), jadi nama Rust dibuat unik lebih dulu;
            // kalau tidak, kode hasil generate bentrok saat compile.
            let module_names = sv_codegen_rust::module_name::module_names_unik(
                &designs.iter().map(|d| d.name.clone()).collect::<Vec<_>>(),
            );
            let mut parts: Vec<(&str, String)> = Vec::new();
            for (design, module) in designs.iter().zip(&module_names) {
                let rust = generate_rust(design);
                workspace.write_generated(&design.name, &rust)?;
                parts.push((module.as_str(), rust));
            }
            // LRM §23.2: tiap root yang memanggil `$dumpvars` punya berkasnya
            // sendiri. Memakai path dari root pertama saja membuat `$dumpvars`
            // di root lain hilang tanpa pesan, dan dua root yang menunjuk path
            // sama akan saling menimpa.
            let paths = crate::vcd_path::vcd_paths(&designs, opts.vcd.as_deref())?;
            let pakai_vcd = paths.iter().any(|p| p.is_some());
            let view: Vec<(&str, &sv_ir::Design, Option<&Path>)> = module_names
                .iter()
                .zip(designs.iter())
                .zip(paths.iter())
                .map(|((m, d), p)| (m.as_str(), d, p.as_deref()))
                .collect();
            let driver = crate::driver_gen::generate_multi_driver(&view, opts.steps);
            let refs: Vec<(&str, &str)> = parts
                .iter()
                .map(|(nama, kode)| (*nama, kode.as_str()))
                .collect();
            workspace.write_sim_project(&refs, &driver, pakai_vcd)?;
            workspace.build(opts.release)?;

            if opts.command == Command::Run {
                workspace.run(opts.release, &opts.plusargs)?;
            } else {
                println!("build OK");
            }
            Ok(())
        }
    }
}
