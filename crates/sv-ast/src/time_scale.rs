// Tanggung jawab: `timescale` unit/precision (LRM §21.8).
use crate::time_unit::TimeUnit;

/// Skala waktu modul: satuan `#delay` tanpa satuan dan satuan pembulatan.
///
/// `timescale 1us/1ns` berarti `#1` = 1 microsecond, sedangkan presisi 1ns
/// menentukan satuan yang dipakai `$time` dan `%t` menampilkan waktu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeScale {
    /// Satuan `timeunit`: dipakai `#n` yang ditulis tanpa satuan.
    pub unit: TimeUnit,
    /// Satuan `timeprecision`: satuan `$time`/`%t`.
    pub precision: TimeUnit,
}

impl Default for TimeScale {
    /// Tanpa `timescale`, LRM §21.8 membiarkan alat memilih; simulator pada
    /// umumnya memakai nanosecond, jadi itu bawaan di sini.
    fn default() -> Self {
        Self {
            unit: TimeUnit::NanoSeconds,
            precision: TimeUnit::NanoSeconds,
        }
    }
}

impl TimeScale {
    /// Parse teks direktif seperti `1us/1ns`.
    ///
    /// Pengali di depan satuan (`10ns`) **ditolak**, bukan diabaikan: mengabaikannya
    /// berarti `#1` jadi 1 ns, bukan 10 ns, dan itu nilai salah tanpa pesan. Batas
    /// ini dicatat di `LRM_COMPLIANCE.md`.
    ///
    /// BUG: `timeprecision` yang lebih kasar dari `timeunit` juga ditolak di sini.
    /// LRM §21.8 mensyaratkan `timeunit >= timeprecision`, dan `iverilog`/
    /// `verilator` menolaknya. Diterima begitu saja, `1us/1ms` membuat
    /// `unit_femtos_dibulatkan` mengembalikan 1 femtosecond sehingga `$time`
    /// melaporkan 1000000000 untuk `#1` — angka yang tidak mungkin ditafsirkan
    /// sebagai waktu, dan tidak ada pesan error yang menyinggung.
    pub fn parse(teks: &str) -> Option<Self> {
        let teks = teks.trim();
        let (unit, precision) = teks.split_once('/')?;
        let unit = parse_satuan(unit)?;
        let precision = parse_satuan(precision)?;
        if precision.femtos() > unit.femtos() {
            return None;
        }
        Some(Self { unit, precision })
    }

    /// Jumlah femtosecond dalam satu `timeunit` modul ini.
    pub fn unit_femtos(self) -> u64 {
        self.unit.femtos()
    }
}

/// Parse satu bagian `1ns`; faktor pengali harus `1`.
fn parse_satuan(teks: &str) -> Option<TimeUnit> {
    let teks = teks.trim();
    let teks = teks.strip_prefix('1')?;
    TimeUnit::from_name(teks)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timescale_lengkap_diparse() {
        let ts = TimeScale::parse("1us/1ps").expect("parse");
        assert_eq!(ts.unit, TimeUnit::MicroSeconds);
        assert_eq!(ts.precision, TimeUnit::PicoSeconds);
    }

    #[test]
    fn pengali_bukan_satu_ditolak() {
        // `10ns/1ns` berarti `#1` = 10 ns. Mengabaikannya menghasilkan delay
        // sepuluh kali terlalu kecil tanpa pesan apa pun.
        assert!(TimeScale::parse("10ns/1ns").is_none());
    }

    #[test]
    fn teks_bukan_timescale_ditolak() {
        assert!(TimeScale::parse("1ns").is_none());
        assert!(TimeScale::parse("1x/1ns").is_none());
        assert!(TimeScale::parse("ns/1ns").is_none());
    }

    #[test]
    fn bawaan_adalah_nanosecond() {
        let ts = TimeScale::default();
        assert_eq!(ts.unit, TimeUnit::NanoSeconds);
        assert_eq!(ts.precision, TimeUnit::NanoSeconds);
    }

    /// BUG: `timeprecision` yang lebih kasar dari `timeunit` diterima diam-diam.
    /// LRM §21.8 mewajibkan `timeunit >= timeprecision`, dan `iverilog`/
    /// `verilator` menolaknya. Diterima begitu saja, `1us/1ms` membuat
    /// `unit_femtos_dibulatkan` jadi 1 femtosecond, sehingga `$time` melaporkan
    /// `1000000000` untuk `#1` — angka yang mustahil ditafsirkan sebagai waktu,
    /// tanpa pesan error apa pun.
    #[test]
    fn bug_presisi_kasar_dari_unit_ditolak() {
        assert!(TimeScale::parse("1us/1ms").is_none());
        assert!(TimeScale::parse("1ns/1us").is_none());
        assert!(TimeScale::parse("1ps/1ns").is_none());
        // Kebalikannya tetap sah: `1ms/1us` boleh.
        assert!(TimeScale::parse("1ms/1us").is_some());
    }
}
