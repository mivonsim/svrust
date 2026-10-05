// Tanggung jawab: satuan waktu penulisan pada AST delay SystemVerilog.
/// Satuan waktu pada `#n` (LRM §3.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeUnit {
    /// `#n` ditulis tanpa satuan: satuan ditentukan `timescale` modul
    /// (LRM §21.8). Eligator yang menyelesaikannya, karena parser tidak
    /// tahu `timescale` modul mana yang berlaku.
    Bawaan,
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

    /// `#n` ditulis tanpa satuan: satuan ditentukan `timeunit` modul
    /// (LRM §21.8), jadi parser menandai `Bawaan` dan eligator yang
    /// menyelesaikannya memakai [`crate::time_scale::TimeScale`] modul.
    pub fn default_unit() -> Self {
        TimeUnit::Bawaan
    }

    /// Jumlah femtosecond dalam satu satuan.
    ///
    /// Dipakai [`crate::time_scale::TimeScale::parse`] untuk memeriksa syarat
    /// LRM §21.8 bahwa `timeunit >= timeprecision`.
    pub fn femtos(self) -> u64 {
        match self {
            TimeUnit::Seconds => 1_000_000_000_000_000,
            TimeUnit::MilliSeconds => 1_000_000_000_000,
            TimeUnit::MicroSeconds => 1_000_000_000,
            TimeUnit::NanoSeconds => 1_000_000,
            TimeUnit::PicoSeconds => 1_000,
            TimeUnit::FectoSeconds => 1,
            // `Bawaan` belum diselesaikan; nol membuat perbandingan selalu
            // menolak, jadi pesannya tetap pazu.
            TimeUnit::Bawaan => 0,
        }
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
    fn satuan_bawaan_ditandai_bawaan() {
        // LRM §21.8: `#n` tanpa satuan mengikuti `timeunit` modul, jadi
        // parser tidak boleh langsung memakai nanosecond.
        assert_eq!(TimeUnit::default_unit(), TimeUnit::Bawaan);
    }

    #[test]
    fn satuan_bawaan_bukan_satuan_waktu() {
        assert_eq!(TimeUnit::from_name("bawaan"), None);
    }
}
