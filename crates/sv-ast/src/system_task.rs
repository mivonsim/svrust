// Tanggung jawab: node AST untuk system task testbench.
use crate::expression::Expr;
use sv_lexer::span::Span;

/// Argumen system task: string format, ekspresi, atau named association.
#[derive(Debug, Clone, PartialEq)]
pub enum SystemArg {
    /// String format seperti `"nilai = %d\n"`.
    Format(String),
    /// Ekspresi yang nilainya ditampilkan.
    Value(Expr),
}

/// System task yang didukung (LRM §20).
#[derive(Debug, Clone, PartialEq)]
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

impl SystemTaskKind {
    /// Petakan nama system task ke enum; `None` berarti belum didukung.
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "display" => Some(SystemTaskKind::Display),
            "finish" => Some(SystemTaskKind::Finish),
            "monitor" => Some(SystemTaskKind::Monitor),
            "monitoron" => Some(SystemTaskKind::MonitorOn),
            "monitoroff" => Some(SystemTaskKind::MonitorOff),
            "strobe" => Some(SystemTaskKind::Strobe),
            "dumpfile" => Some(SystemTaskKind::DumpFile),
            "dumpvars" => Some(SystemTaskKind::DumpVars),
            _ => None,
        }
    }
}

/// Panggilan system task beserta argumennya.
#[derive(Debug, Clone, PartialEq)]
pub struct SystemTask {
    pub kind: SystemTaskKind,
    pub args: Vec<SystemArg>,
    /// Syarat pada `$monitor if (kondisi)`; `None` berarti tanpa syarat.
    pub condition: Option<crate::expression::Expr>,
    /// `timescale` modul pemanggil, dipakai `%t` (LRM §20.4 + §21.8).
    ///
    /// `%t` menampilkan waktu dalam satuan `timeprecision` **modul yang
    /// memuat format string**, bukan modul top. Setelah flatten hanya skala top
    /// yang ada di `Design`, jadi skala pemanggil harus ikut dibawa di node ini.
    pub time_scale: crate::time_scale::TimeScale,
    pub span: Span,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_dikenal_dipetakan() {
        assert_eq!(
            SystemTaskKind::from_name("display"),
            Some(SystemTaskKind::Display)
        );
        assert_eq!(
            SystemTaskKind::from_name("finish"),
            Some(SystemTaskKind::Finish)
        );
    }

    #[test]
    fn monitor_dikenali() {
        assert_eq!(
            SystemTaskKind::from_name("monitor"),
            Some(SystemTaskKind::Monitor)
        );
    }

    #[test]
    fn task_tidak_dikenal_mengembalikan_none() {
        assert_eq!(SystemTaskKind::from_name("writeln"), None);
    }

    #[test]
    fn dump_dikenali() {
        assert_eq!(
            SystemTaskKind::from_name("dumpfile"),
            Some(SystemTaskKind::DumpFile)
        );
        assert_eq!(
            SystemTaskKind::from_name("dumpvars"),
            Some(SystemTaskKind::DumpVars)
        );
    }
}
