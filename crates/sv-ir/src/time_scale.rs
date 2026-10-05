// Tanggung jawab: konstanta femtosecond per satuan waktu dan skala `timescale`
// pada IR design (LRM §21.8).
//
// Konstanta ini hidup di IR, bukan di `sv-runtime`, supaya elaborator bisa
// mengonversi `#n` dan `$time` sesuai `timescale` tanpa bergantung pada crate
// runtime.
use crate::time_unit::TimeUnit;

pub const FS_PER_FS: u64 = 1;
pub const FS_PER_PS: u64 = 1_000;
pub const FS_PER_NS: u64 = 1_000_000;
pub const FS_PER_US: u64 = 1_000_000_000;
pub const FS_PER_MS: u64 = 1_000_000_000_000;
pub const FS_PER_SEC: u64 = 1_000_000_000_000_000;

/// Skala waktu design: satuan `timeunit` dan `timeprecision`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeScale {
    pub unit: TimeUnit,
    pub precision: TimeUnit,
}

impl TimeScale {
    /// Skala bawaan saat tidak ada `timescale`: nanosecond.
    pub const BAVAAN: TimeScale = TimeScale {
        unit: TimeUnit::NanoSeconds,
        precision: TimeUnit::NanoSeconds,
    };

    pub fn new(unit: TimeUnit, precision: TimeUnit) -> Self {
        Self { unit, precision }
    }

    /// Bulatkan nilai femtosecond ke presisi modul (LRM §21.8).
    ///
    /// LRM §21.8 menyatakan waktu dibulatkan ke `timeprecision`; pemotongan ke
    /// bawah dipakai karena waktu yang lebih kecil dari presisi tidak dapat
    /// diwakili, dan `iverilog` juga memotong ke presisi.
    pub fn bulatkan(&self, femtos: u64) -> u64 {
        let presisi = self.precision.femtos().max(1);
        femtos / presisi * presisi
    }
}

impl Default for TimeScale {
    fn default() -> Self {
        Self::BAVAAN
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setiap_satuan_sepuluh_dari_berikutnya() {
        assert_eq!(FS_PER_PS, FS_PER_FS * 1_000);
        assert_eq!(FS_PER_NS, FS_PER_PS * 1_000);
        assert_eq!(FS_PER_US, FS_PER_NS * 1_000);
        assert_eq!(FS_PER_MS, FS_PER_US * 1_000);
        assert_eq!(FS_PER_SEC, FS_PER_MS * 1_000);
    }

    #[test]
    fn bulatkan_memotong_ke_presisi() {
        let ts = TimeScale::new(TimeUnit::NanoSeconds, TimeUnit::NanoSeconds);
        // 1.5 ns -> 1 ns, bukan 2 ns.
        assert_eq!(ts.bulatkan(1_500_000), 1_000_000);
        assert_eq!(ts.bulatkan(1_000_000), 1_000_000);
    }

    #[test]
    fn bawaan_nanosecond() {
        let ts = TimeScale::default();
        assert_eq!(ts.unit, TimeUnit::NanoSeconds);
        assert_eq!(ts.precision, TimeUnit::NanoSeconds);
    }
}
