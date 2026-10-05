// Tanggung jawab: teks `$timescale` untuk header VCD dari `timescale` modul
// (LRM §21.8).
use sv_ir::{Design, TimeScale, TimeUnit};

/// Teks `$timescale` untuk VCD, misalnya `"1ps"`.
pub fn vcd_timescale(design: &Design) -> String {
    let scale = design.time_scale.unwrap_or(TimeScale::BAVAAN);
    format!("1{}", suffix(scale.precision))
}

/// Jumlah femtosecond per satuan presisi modul.
///
/// Timestamp VCD ditulis dalam satuan ini (LRM §21.8), jadi nilainya harus
/// sama dengan pembagi yang dipakai header `$timescale`.
pub fn precision_femtos(design: &Design) -> u64 {
    let scale = design.time_scale.unwrap_or(TimeScale::BAVAAN);
    scale.precision.femtos().max(1)
}

/// Sufiks satuan waktu untuk teks `timescale`/VCD.
pub fn suffix(unit: TimeUnit) -> &'static str {
    match unit {
        TimeUnit::Seconds => "s",
        TimeUnit::MilliSeconds => "ms",
        TimeUnit::MicroSeconds => "us",
        TimeUnit::NanoSeconds => "ns",
        TimeUnit::PicoSeconds => "ps",
        TimeUnit::FectoSeconds => "fs",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn design_kosong_pakai_nanosecond() {
        assert_eq!(vcd_timescale(&Design::new("m")), "1ns");
    }

    #[test]
    fn presisi_modul_dipakai() {
        let design = Design {
            time_scale: Some(TimeScale::new(TimeUnit::NanoSeconds, TimeUnit::PicoSeconds)),
            ..Design::new("m")
        };
        assert_eq!(vcd_timescale(&design), "1ps");
    }

    /// BUG: timestamp VCD memakai nomor langkah driver (`t += 1`) bukan
    /// waktu simulasi, padahal header menyatakan satuan `timeprecision`.
    /// Pembaca VCD lalu menyimpulkan edge pertama terjadi pada 1 unit
    /// padahal desainnya berjalan jauh lebih lama.
    #[test]
    fn bug_pembagi_timestamp_sama_dengan_satuan_header() {
        let design = Design {
            time_scale: Some(TimeScale::new(
                TimeUnit::MicroSeconds,
                TimeUnit::PicoSeconds,
            )),
            ..Design::new("m")
        };
        // Header `1ps` -> timestamp dalam satuan femtosecond/1000.
        assert_eq!(vcd_timescale(&design), "1ps");
        assert_eq!(precision_femtos(&design), 1_000);
    }
}
