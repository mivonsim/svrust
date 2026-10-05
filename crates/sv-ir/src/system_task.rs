// Tanggung jawab: system task pada IR (versi bebas-AST untuk codegen).
use crate::expr::Expr;
use crate::process::Span;

/// Argumen system task: string format atau ekspresi.
#[derive(Debug, Clone, PartialEq)]
pub enum SystemArg {
    /// String format seperti `"nilai = %d\n"`.
    Format(String),
    /// Ekspresi yang nilainya ditampilkan.
    Value(Expr),
}

/// System task yang didukung (LRM §20).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemTaskKind {
    /// `$display(...)` — cetak ke stdout.
    Display,
    /// `$finish` — hentikan simulasi.
    Finish,
    /// `$monitor` — cetak ulang saat nilai sinyal berubah (LRM §20.2).
    Monitor,
    /// `$monitoron` — nyalakan `$monitor` (LRM §20.2).
    MonitorOn,
    /// `$monitoroff` — matikan `$monitor` (LRM §20.2).
    MonitorOff,
    /// `$strobe` — cetak di akhir timestep berjalan (LRM §20.3).
    Strobe,
    /// `$dumpfile` — tentukan nama berkas VCD (LRM §23.2).
    DumpFile,
    /// `$dumpvars` — aktifkan pencatatan waveform (LRM §23.2).
    DumpVars,
}

/// Panggilan system task beserta argumennya.
#[derive(Debug, Clone, PartialEq)]
pub struct SystemTask {
    pub kind: SystemTaskKind,
    pub args: Vec<SystemArg>,
    /// Syarat pada `$monitor if (kondisi)`; `None` berarti tanpa syarat.
    pub condition: Option<Expr>,
    /// `timescale` modul pemanggil; `%t` menampilkannya dalam satuan
    /// `timeprecision` modul itu (LRM §20.4 + §21.8), bukan modul top.
    pub time_scale: crate::time_scale::TimeScale,
    pub span: Span,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::datatype::DataType;

    #[test]
    fn argumen_format_dan_nilai_bisa_bercampuran() {
        let args = [
            SystemArg::Format("x = %d".to_string()),
            SystemArg::Value(Expr::constant(7, DataType::logic(8))),
        ];
        assert_eq!(args.len(), 2);
        assert!(matches!(args[0], SystemArg::Format(_)));
        assert!(matches!(args[1], SystemArg::Value(_)));
    }

    #[test]
    fn argumen_format_tidak_membawa_data_type() {
        let arg = SystemArg::Format("%h".to_string());
        assert_eq!(arg, SystemArg::Format("%h".to_string()));
    }

    #[test]
    fn kind_bisa_disalin() {
        let kind = SystemTaskKind::Finish;
        let salinan = kind;
        assert_eq!(kind, salinan);
    }

    #[test]
    fn nilai_argumen_membawa_lebar_logis() {
        let expr = Expr::constant(0xFF, DataType::logic(8));
        let SystemArg::Value(isi) = SystemArg::Value(expr) else {
            panic!("harus nilai");
        };
        assert_eq!(isi.data_type().width, 8);
    }

    #[test]
    fn dump_bisa_disalin_sebagai_nilai_kecil() {
        // `SystemTaskKind`Copy sehingga design bisa membandingkan tanpa pinjam.
        let salinan = SystemTaskKind::DumpFile;
        assert_eq!(salinan, SystemTaskKind::DumpFile);
        assert_eq!(SystemTaskKind::DumpVars, SystemTaskKind::DumpVars);
    }
}
