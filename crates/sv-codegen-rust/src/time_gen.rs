// Tanggung jawab: konversi satuan `timescale` modul menjadi konstanta runtime
// untuk `%t` (LRM §20.4 + §21.8).
use sv_ir::TimeScale;

/// Ekspresi Rust `sv_runtime::TimeScale { .. }` untuk satu `timescale`.
///
/// Dipakai per call-site system task, bukan per design: nilainya milik modul
/// yang memuat format string, bukan modul top. Design yang sudah di-flatten
/// tidak lagi tahu asal tiap argumen, jadi skala ikut dibawa di node
/// `SystemTask`.
pub fn literal_time_scale_dari(scale: TimeScale) -> String {
    format!(
        "sv_runtime::TimeScale {{ unit_femtos: {}, precision_femtos: {} }}",
        unit_femtos_dibulatkan(scale),
        scale.precision.femtos().max(1),
    )
}

/// Jumlah femtosecond dalam satu `timeunit`, setelah pembulatan ke presisi.
///
/// Pembulatan ke `timeprecision` membuat `%t` selalu dapat diekspresikan:
/// pada `timescale 1us/10ps`, satu unit adalah 1.000.000.000 fs yang sudah
/// habis dibagi 10.000 fs, jadi tidak ada sisa yang hilang.
fn unit_femtos_dibulatkan(scale: TimeScale) -> u64 {
    let presisi = scale.precision.femtos().max(1);
    let bulat = scale.unit.femtos() / presisi * presisi;
    // LRM §21.8 mensyaratkan `timeunit` >= `timeprecision`, dan
    // `sv_ast::time_scale::TimeScale::parse` sudah menolak yang melanggar.
    // Penjaga di sini hanya supaya perubahan berikutnya tidak membuat satu unit 0.
    bulat.max(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sv_ir::TimeUnit;

    fn skala(unit: TimeUnit, precision: TimeUnit) -> TimeScale {
        TimeScale::new(unit, precision)
    }

    #[test]
    fn satuan_kecil_dipakai_sesuai_timescale() {
        let kode = literal_time_scale_dari(skala(TimeUnit::PicoSeconds, TimeUnit::PicoSeconds));
        assert_eq!(
            kode,
            "sv_runtime::TimeScale { unit_femtos: 1000, precision_femtos: 1000 }"
        );
    }

    #[test]
    fn presisi_kasar_tidak_mengubah_satuan() {
        let kode = literal_time_scale_dari(skala(TimeUnit::MicroSeconds, TimeUnit::NanoSeconds));
        assert_eq!(
            kode,
            "sv_runtime::TimeScale { unit_femtos: 1000000000, precision_femtos: 1000000 }"
        );
    }

    #[test]
    fn presisi_lebih_besar_dari_unit_tidak_membekukan_waktu() {
        // Menolak `timeprecision` kasar adalah tugas parser; penjaga di sini
        // hanya memastikan skala tak pernah jadi nol.
        let kode = literal_time_scale_dari(skala(TimeUnit::PicoSeconds, TimeUnit::NanoSeconds));
        assert_eq!(
            kode,
            "sv_runtime::TimeScale { unit_femtos: 1, precision_femtos: 1000000 }"
        );
    }
}
