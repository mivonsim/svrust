// Tanggung jawab: teks `$timescale` untuk header VCD dari `timescale` modul
// (LRM §21.8).
use sv_ir::{Design, TimeScale, TimeUnit};

/// Teks `$timescale` untuk VCD, misalnya `"1ps"`.
pub fn vcd_timescale(design: &Design) -> String {
    let scale = design.time_scale.unwrap_or(TimeScale::BAVAAN);
    format!("1{}", suffix(scale.precision))
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
}
