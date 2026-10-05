// Tanggung jawab: dispatch perintah ke implementasi.
use crate::cli::{Command, Options};
use crate::driver_gen::generate_driver;
use crate::pipeline::{compile_paths, generate_rust};
use crate::workspace::Workspace;

pub fn dispatch(opts: Options) -> Result<(), String> {
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
        Command::Inspect => {
            let design = compile_paths(&opts.rtl, opts.top.as_deref())?;
            let stats = sv_elaborator::DesignStats::of(&design);
            println!("{}", stats);
            Ok(())
        }
        Command::Codegen => {
            let design = compile_paths(&opts.rtl, opts.top.as_deref())?;
            let rust = generate_rust(&design);
            let path = workspace.write_generated(&design.name, &rust)?;
            println!("generated {}", path.display());
            Ok(())
        }
        Command::Elaborate => {
            let design = compile_paths(&opts.rtl, opts.top.as_deref())?;
            let stats = sv_elaborator::DesignStats::of(&design);
            println!("{}", stats);
            println!("\nProcesses:");
            for p in &design.processes {
                println!(
                    "  {} ({}) - {} sensitivity",
                    p.name,
                    p.kind,
                    p.sensitivity.len()
                );
            }
            Ok(())
        }
        Command::Check => {
            let modules = crate::pipeline::collect_modules(&opts.rtl)?;
            let top = opts
                .top
                .clone()
                .unwrap_or_else(|| modules.last().map(|m| m.name.clone()).unwrap_or_default());
            sv_elaborator::elaborate_top(&modules, &top).map_err(|e| e.to_string())?;
            println!("check OK ({} module, top {})", modules.len(), top);
            Ok(())
        }
        Command::Build | Command::Run => {
            let design = compile_paths(&opts.rtl, opts.top.as_deref())?;
            let rust = generate_rust(&design);
            workspace.write_generated(&design.name, &rust)?;
            let design_refs = vec![design.name.as_str()];
            let vcd_abs = crate::pipeline::resolve_vcd(&design, opts.vcd.as_deref())?;
            let driver = generate_driver(&design, opts.steps, vcd_abs.as_deref());
            workspace.write_sim_project(&design_refs, &driver, &rust, vcd_abs.is_some())?;
            workspace.build(opts.release)?;

            if opts.command == Command::Run {
                workspace.run(opts.release)?;
            } else {
                println!("build OK");
            }
            Ok(())
        }
    }
}
