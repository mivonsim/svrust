// Tanggung jawab: satuan waktu penulisan pada AST delay SystemVerilog.
/// Satuan waktu pada `#n` (LRM §3.3).
///
/// `#n` tanpa satuan memakai nanosecond sebagai default, mengikuti
/// presisi bawaan yang dipakai sebagian besar alat simulasi.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeUnit {
    Seconds,
    MilliSeconds,
    MicroSeconds,
    NanoSeconds,
    PicoSeconds,
    FectoSeconds,
}

impl TimeUnit {
    /// Petakan identifier satuan ke enum; `None` bila bukan satuan waktu.
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "s" => Some(TimeUnit::Seconds),
            "ms" => Some(TimeUnit::MilliSeconds),
            "us" => Some(TimeUnit::MicroSeconds),
            "ns" => Some(TimeUnit::NanoSeconds),
            "ps" => Some(TimeUnit::PicoSeconds),
            "fs" => Some(TimeUnit::FectoSeconds),
            _ => None,
        }
    }

    /// Satuan yang dipakai bila `#n` ditulis tanpa satuan.
    pub fn default_unit() -> Self {
        TimeUnit::NanoSeconds
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semua_satuan_waktu_dikenali() {
        assert_eq!(TimeUnit::from_name("s"), Some(TimeUnit::Seconds));
        assert_eq!(TimeUnit::from_name("ms"), Some(TimeUnit::MilliSeconds));
        assert_eq!(TimeUnit::from_name("us"), Some(TimeUnit::MicroSeconds));
        assert_eq!(TimeUnit::from_name("ns"), Some(TimeUnit::NanoSeconds));
        assert_eq!(TimeUnit::from_name("ps"), Some(TimeUnit::PicoSeconds));
        assert_eq!(TimeUnit::from_name("fs"), Some(TimeUnit::FectoSeconds));
    }

    #[test]
    fn identifier_bukan_satuan_waktu_ditolak() {
        assert_eq!(TimeUnit::from_name("step"), None);
        assert_eq!(TimeUnit::from_name("nsx"), None);
    }

    #[test]
    fn satuan_bawaan_adalah_nanosecond() {
        assert_eq!(TimeUnit::default_unit(), TimeUnit::NanoSeconds);
    }
}
