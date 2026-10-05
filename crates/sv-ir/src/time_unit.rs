// Tanggung jawab: satuan waktu simulasi pada IR.
/// Satuan waktu yang dipakai statement delay (LRM §3.3).
///
/// Nilai disalin dari AST agar elaborator tidak perlu menyimpan bentuk
/// penulisan asli; konversi ke satuan internal dilakukan saat codegen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeUnit {
    Seconds,
    MilliSeconds,
    MicroSeconds,
    NanoSeconds,
    PicoSeconds,
    FectoSeconds,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn satuan_bisa_disalin() {
        let unit = TimeUnit::PicoSeconds;
        let salinan = unit;
        assert_eq!(unit, salinan);
    }

    #[test]
    fn satuan_picosecond_beda_dari_nanosecond() {
        assert_ne!(TimeUnit::PicoSeconds, TimeUnit::NanoSeconds);
    }

    #[test]
    fn keenam_satuan_terdaftar() {
        let semua = [
            TimeUnit::Seconds,
            TimeUnit::MilliSeconds,
            TimeUnit::MicroSeconds,
            TimeUnit::NanoSeconds,
            TimeUnit::PicoSeconds,
            TimeUnit::FectoSeconds,
        ];
        assert_eq!(semua.len(), 6);
    }
}
