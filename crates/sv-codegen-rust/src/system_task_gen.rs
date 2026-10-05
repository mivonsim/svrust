// Tanggung jawab: generate kode Rust untuk system task testbench.
use crate::expr_gen::write_expr;
use crate::indent::Indent;
use sv_ir::system_task::{SystemArg, SystemTaskKind};
use sv_ir::Design;

/// Tulis statement `$display(...)` atau `$finish` sebagai Rust.
///
/// `$display` diterjemahkan menjadi pemanggilan helper runtime dengan
/// spesifikasi gaya printf yang sudah dipisah dari literalnya, sehingga
/// kode yang dihasilkan tidak perlu `format!` dinamis.
pub fn write_system_task(
    kind: SystemTaskKind,
    args: &[SystemArg],
    scale: sv_ir::TimeScale,
    out: &mut String,
    indent: &Indent,
) {
    match kind {
        SystemTaskKind::Display => write_display(args, scale, out, indent),
        // LRM §20.2: `$monitor` mendaftarkan format + argumen; pencetakan
        // terjadi nanti di driver saat ada sinyal yang berubah.
        SystemTaskKind::Monitor => write_monitor(out, indent),
        // LRM §20.2: `$monitoron`/`$monitoroff` hanya menyalakan/mematikan.
        SystemTaskKind::MonitorOn => {
            indent.push(out);
            out.push_str("self.monitor_active = true;\n");
        }
        SystemTaskKind::MonitorOff => {
            indent.push(out);
            out.push_str("self.monitor_active = false;\n");
        }
        // LRM §20.3: argumen `$strobe` dievaluasi SAAT dipanggil, lalu
        // dicetak di akhir timestep. Jadi nilai ikut disimpan saat ini.
        SystemTaskKind::Strobe => write_strobe(args, scale, out, indent),
        // LRM §20.3: `$finish` menghentikan simulasi. Selain menandai flag,
        // statement setelahnya pada proses yang sama tidak boleh jalan lagi,
        // jadi proses langsung keluar lewat `return`.
        SystemTaskKind::Finish => {
            indent.push(out);
            out.push_str("self.finished = true;\n");
            indent.push(out);
            out.push_str("return;\n");
        }
        // LRM §23.2: `$dumpfile` hanya mencatat nama berkas; rekam baru aktif
        // saat `$dumpvars` dipanggil.
        SystemTaskKind::DumpFile => write_dumpfile(args, out, indent),
        // LRM §23.2: `$dumpvars` menyalakan pencatatan waveform.
        SystemTaskKind::DumpVars => {
            indent.push(out);
            out.push_str("self.dump_on = true;\n");
        }
    }
}

/// Tulis `$dumpfile("nama.vcd")`; argumen harus berupa string literal.
///
/// Nama berkas disimpan sebagai `&'static str` supaya driver bisa membacanya
/// tanpa alokasi ulang (LRM §23.2).
fn write_dumpfile(args: &[SystemArg], out: &mut String, indent: &Indent) {
    let path = args.iter().find_map(|arg| match arg {
        SystemArg::Format(teks) => Some(teks.clone()),
        SystemArg::Value(_) => None,
    });
    indent.push(out);
    match path {
        Some(nama) => {
            out.push_str(&format!(
                "self.dump_path = \"{}\";\n",
                escape_rust_string(&nama)
            ));
        }
        // Tanpa argumen LRM tidak mendefinisikan nama; pakai bawaan.
        None => out.push_str("self.dump_path = \"\";\n"),
    }
}

/// Antrekan satu panggilan `$strobe` beserta argumen yang sudah dievaluasi.
fn write_strobe(args: &[SystemArg], scale: sv_ir::TimeScale, out: &mut String, indent: &Indent) {
    indent.push(out);
    // Skala `%t` ikut disimpan karena milik modul pemanggil, bukan modul top
    // (LRM §21.8) — design yang sudah di-flatten tidak lagi tahu asal masing-masing
    // argumen.
    out.push_str(&format!(
        "self.strobe_pending.push(({:?}, {}, vec![\n",
        format_monitor(args),
        crate::time_gen::literal_time_scale_dari(scale)
    ));
    let mut ada = false;
    for arg in args {
        if let SystemArg::Value(expr) = arg {
            let tipe = expr.data_type();
            if ada {
                out.push_str(",\n");
            }
            ada = true;
            let inner = indent.child();
            inner.push(out);
            out.push_str("sv_runtime::FormatArg { value: ");
            write_expr(expr, out, &inner);
            out.push_str(&format!(
                ", width: {}, signed: {} }}",
                tipe.width, tipe.signed
            ));
        }
    }
    out.push_str("]));\n");
}

/// Tulis statement `$monitor`; hanya mengaktifkan pencetakan.
///
/// Nilai argumen sengaja TIDAK di sini: `run_monitor` mengevaluasi ulang
/// ekspresi argumen setiap kali memancarkan, supaya nilai yang tampil
/// selalu yang terbaru (LRM §20.2).
fn write_monitor(out: &mut String, indent: &Indent) {
    indent.push(out);
    out.push_str("self.monitor_active = true;\n");
}

/// Tulis argumen `$monitor` ke writer sebagai daftar `FormatArg`.
pub fn write_monitor_args(args: &[SystemArg], out: &mut String, indent: &Indent) {
    let mut ada = false;
    for arg in args {
        if let SystemArg::Value(expr) = arg {
            let tipe = expr.data_type();
            if ada {
                out.push_str(",\n");
            }
            ada = true;
            let inner = indent.child();
            inner.push(out);
            out.push_str("sv_runtime::FormatArg { value: ");
            write_expr(expr, out, &inner);
            out.push_str(&format!(
                ", width: {}, signed: {} }}",
                tipe.width, tipe.signed
            ));
        }
    }
    out.push_str("];\n");
}

/// Format string gabungan dari argumen `$monitor`.
pub fn format_monitor(args: &[SystemArg]) -> String {
    let mut format = String::new();
    for arg in args {
        if let SystemArg::Format(teks) = arg {
            format.push_str(teks);
        }
    }
    format
}

/// True bila design memanggil `$monitor` di salah satu prosesnya.
pub fn has_monitor(design: &Design) -> bool {
    find_monitor(design).is_some()
}

/// True bila design memakai flag `monitor_active` (monitor/monitoron/off).
pub fn uses_monitor_flag(design: &Design) -> bool {
    has_monitor(design)
        || find_task_pub(design, SystemTaskKind::MonitorOn).is_some()
        || find_task_pub(design, SystemTaskKind::MonitorOff).is_some()
}

/// True bila design memanggil `$strobe` di salah satu prosesnya.
pub fn has_strobe(design: &Design) -> bool {
    find_strobe(design).is_some()
}

/// True bila design memanggil `$finish` di salah satu prosesnya.
///
/// Driver memakai ini untuk deciding apakah perlu mengecek flag `finished`
/// tiap timestep (LRM §20.3). Tanpa ini, `$finish` di dalam `always_ff` pada
/// design tanpa `initial` tidak akan menghentikan simulasi.
pub fn has_finish(design: &Design) -> bool {
    find_task(design, SystemTaskKind::Finish).is_some()
}

/// True bila design memanggil `$dumpvars`; tanpa ini tidak ada VCD (LRM §23.2).
pub fn has_dumpvars(design: &Design) -> bool {
    find_task(design, SystemTaskKind::DumpVars).is_some()
}

/// Nama berkas dari `$dumpfile("...")`, bila ada (LRM §23.2).
pub fn dumpfile_path(design: &Design) -> Option<String> {
    find_task(design, SystemTaskKind::DumpFile).and_then(|task| {
        task.args.into_iter().find_map(|arg| match arg {
            SystemArg::Format(teks) => Some(teks),
            SystemArg::Value(_) => None,
        })
    })
}

fn find_task_pub(design: &Design, jenis: SystemTaskKind) -> Option<sv_ir::system_task::SystemTask> {
    find_task(design, jenis)
}

/// Task `$monitor` terakhir pada design; yang terakhir menang (LRM §20.2).
pub fn find_monitor(design: &Design) -> Option<sv_ir::system_task::SystemTask> {
    find_task(design, SystemTaskKind::Monitor)
}

/// Task `$strobe` terakhir pada design; yang terakhir menang (LRM §20.3).
pub fn find_strobe(design: &Design) -> Option<sv_ir::system_task::SystemTask> {
    find_task(design, SystemTaskKind::Strobe)
}

/// Syarat `$monitor if (kondisi)` pada design, bila ada (LRM §20.2).
pub fn monitor_condition(design: &Design) -> Option<sv_ir::expr::Expr> {
    find_monitor(design).and_then(|task| task.condition)
}

/// Task system task terakhir dengan `jenis` tertentu pada design.
fn find_task(design: &Design, jenis: SystemTaskKind) -> Option<sv_ir::system_task::SystemTask> {
    let mut ditemukan: Option<sv_ir::system_task::SystemTask> = None;
    for process in &design.processes {
        crate::time_scan::kumpulkan_task(&process.body, jenis, &mut ditemukan);
    }
    ditemukan
}

/// Gabungkan seluruh argumen format menjadi satu string lalu tulis printf call.
fn write_display(args: &[SystemArg], scale: sv_ir::TimeScale, out: &mut String, indent: &Indent) {
    let mut format = String::new();
    for arg in args {
        if let SystemArg::Format(teks) = arg {
            format.push_str(teks);
        }
    }

    indent.push(out);
    out.push_str("print!(\"{}\", sv_runtime::sv_format_args(\"");
    out.push_str(&escape_rust_string(&format));
    out.push('"');
    if args.iter().any(|a| matches!(a, SystemArg::Value(_))) {
        out.push_str(", &[\n");
        let mut pertama = true;
        for arg in args {
            if let SystemArg::Value(expr) = arg {
                let tipe = expr.data_type();
                if !pertama {
                    out.push_str(",\n");
                }
                pertama = false;
                let inner = indent.child();
                inner.push(out);
                out.push_str("sv_runtime::FormatArg { value: ");
                write_expr(expr, out, &inner);
                out.push_str(&format!(
                    ", width: {}, signed: {} }}",
                    tipe.width, tipe.signed
                ));
            }
        }
        out.push('\n');
        indent.push(out);
        out.push(']');
    } else {
        // Tanpa argumen nilai, const generic M masih harus tegas.
        out.push_str(", &[] as &[sv_runtime::FormatArg<MAX_WIDTH>]");
    }
    // LRM §21.8 + §20.4: `%t` memakai `timeunit`/`timeprecision` modul yang
    // memuat format string ini, bukan modul top.
    out.push_str(&format!(
        ", {}));\n",
        crate::time_gen::literal_time_scale_dari(scale)
    ));
}

/// Escape string sebagai literal Rust; newline dari lexer dikembalikan
/// menjadi escape `\n` agar source yang dihasilkan tetap valid.
fn escape_rust_string(teks: &str) -> String {
    let mut out = String::with_capacity(teks.len() + 2);
    for ch in teks.chars() {
        match ch {
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            lain => out.push(lain),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use sv_ir::{DataType, Expr};

    fn render(kind: SystemTaskKind, args: &[SystemArg]) -> String {
        let mut out = String::new();
        write_system_task(
            kind,
            args,
            sv_ir::TimeScale::default(),
            &mut out,
            &Indent::new(),
        );
        out
    }

    fn sinyal() -> Expr {
        Expr::SignalRef {
            signal: 1,
            data_type: DataType::logic(8),
        }
    }

    #[test]
    fn display_tanpa_argumen_nilai_menghasilkan_string_kosong() {
        let kode = render(
            SystemTaskKind::Display,
            &[SystemArg::Format("halo".to_string())],
        );
        assert_eq!(
            kode,
            "print!(\"{}\", sv_runtime::sv_format_args(\"halo\", &[] as &[sv_runtime::FormatArg<MAX_WIDTH>], \
            sv_runtime::TimeScale { unit_femtos: 1000000, precision_femtos: 1000000 }));\n"
        );
    }

    #[test]
    fn display_dengan_nilai_membangun_daftar_format_arg() {
        let kode = render(
            SystemTaskKind::Display,
            &[
                SystemArg::Format("n=%d".to_string()),
                SystemArg::Value(sinyal()),
            ],
        );
        assert!(kode.contains("sv_runtime::FormatArg {"), "kode: {}", kode);
        assert!(kode.contains("width: 8"), "kode: {}", kode);
        assert!(kode.contains("signed: false"), "kode: {}", kode);
    }

    #[test]
    fn finish_mengeset_flag_dan_keluar_dari_proses() {
        // LRM §20.3: statement setelah `$finish` tidak dijalankan lagi.
        let kode = render(SystemTaskKind::Finish, &[]);
        assert_eq!(kode, "self.finished = true;\nreturn;\n");
    }

    #[test]
    fn newline_diformat_ulang_sebagai_escape() {
        let kode = render(
            SystemTaskKind::Display,
            &[SystemArg::Format("a\nb".to_string())],
        );
        assert!(kode.contains("\\n"), "kode: {}", kode);
        assert!(!kode.contains("a\nb"), "kode: {}", kode);
    }

    #[test]
    fn dua_format_berurutan_digabung() {
        let kode = render(
            SystemTaskKind::Display,
            &[
                SystemArg::Format("a=".to_string()),
                SystemArg::Format("b".to_string()),
            ],
        );
        assert!(kode.contains("\"a=b\""), "kode: {}", kode);
    }

    #[test]
    fn dumpfile_menyimpan_nama_berkas() {
        let kode = render(
            SystemTaskKind::DumpFile,
            &[SystemArg::Format("sim.vcd".to_string())],
        );
        assert_eq!(kode, "self.dump_path = \"sim.vcd\";\n");
    }

    #[test]
    fn dumpvars_menyalakan_flag_rekam() {
        let kode = render(SystemTaskKind::DumpVars, &[]);
        assert_eq!(kode, "self.dump_on = true;\n");
    }
}
