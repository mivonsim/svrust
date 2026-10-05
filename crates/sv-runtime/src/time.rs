// Tanggung jawab: waktu simulasi presisi dengan satuan & pembulatan.
use std::fmt;
use std::time::Duration;

/// Skala waktu: jumlah unit per detik.
pub type TimePrecision = u32;

/// Waktu simulasi dalam femtosecond (satuan internal terkecil).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Hash)]
pub struct SimTime {
    pub femtos: u64,
}

pub const FS_PER_PS: u64 = 1_000;
pub const FS_PER_NS: u64 = 1_000_000;
pub const FS_PER_US: u64 = 1_000_000_000;
pub const FS_PER_MS: u64 = 1_000_000_000_000;
pub const FS_PER_SEC: u64 = 1_000_000_000_000_000;

impl SimTime {
    pub const ZERO: SimTime = SimTime { femtos: 0 };

    pub fn from_femtos(femtos: u64) -> Self {
        Self { femtos }
    }

    pub fn from_picos(picos: u64) -> Self {
        Self {
            femtos: picos.saturating_mul(FS_PER_PS),
        }
    }

    pub fn from_nanos(nanos: u64) -> Self {
        Self {
            femtos: nanos.saturating_mul(FS_PER_NS),
        }
    }

    pub fn from_millis(millis: u64) -> Self {
        Self {
            femtos: millis.saturating_mul(FS_PER_MS),
        }
    }

    pub fn femtos(self) -> u64 {
        self.femtos
    }

    pub fn picos(self) -> u64 {
        self.femtos / FS_PER_PS
    }

    pub fn nanos(self) -> u64 {
        self.femtos / FS_PER_NS
    }

    /// Nilai dalam satuan tertentu (jumlah femtosecond per unit).
    ///
    /// LRM §21.8: `$time` mengembalikan waktu dalam satuan `timeunit` modul,
    /// dan nilainya sudah dibulatkan ke `timeprecision` sebelum pemanggilan ini
    /// karena pembulatan tidak dapat dilakukan pada satuan yang lebih kecil.
    pub fn in_units(self, femtos_per_unit: u64) -> u64 {
        let per = femtos_per_unit.max(1);
        // LRM §21.8: nilai `$time` dibulatkan ke `timeunit`, bukan dipotong.
        // Pemotongan membuat `#500ns` setelah `#1us` pada modul `1us/1ns`
        // melaporkan t=1 sementara iverilog (dan LRM) memberi t=2.
        //
        // BUG: `self.femtos + per / 2` meluap pada `timescale 1s/1s` dengan
        // `#1000000000`: `SimTime` sudah jenuh di `u64::MAX` femtosecond, jadi
        // penjumlahannya panic di build debug dan membungkus diam-diam di build
        // release — keduanya membuat `$time` salah. Pembulatan ke atas dibatasi
        // `saturating_add` supaya batas waktu yang jenuh tetap melaporkan nilai
        // terbesar yang bisa diekspresikan.
        self.femtos.saturating_add(per / 2) / per
    }

    pub fn checked_add(self, other: SimTime) -> Option<SimTime> {
        Some(SimTime {
            femtos: self.femtos.checked_add(other.femtos)?,
        })
    }

    pub fn saturating_add(self, other: SimTime) -> SimTime {
        SimTime {
            femtos: self.femtos.saturating_add(other.femtos),
        }
    }

    pub fn offset(self, delta: SimTime) -> SimTime {
        self.saturating_add(delta)
    }

    /// Format sebagai `100ns`, `1.5us`, dsb.
    pub fn format(self) -> String {
        let fs = self.femtos;
        if fs == 0 {
            return "0".to_string();
        }
        // Tabel diurut dari satuan terbesar; dipakai `% == 0` (bukan
        // `is_multiple_of`) karena MSRV 1.75 terkunci di `clippy.toml`.
        for (pembagi, suffix) in SATUAN_TERBALIK {
            if fs % pembagi == 0 {
                return format!("{}{}", fs / pembagi, suffix);
            }
        }
        format!("{}fs", fs)
    }
}

/// Satuan dari terbesar ke terkecil, dipakai `SimTime::format`.
const SATUAN_TERBALIK: [(u64, &str); 5] = [
    (FS_PER_SEC, "s"),
    (FS_PER_MS, "ms"),
    (FS_PER_US, "us"),
    (FS_PER_NS, "ns"),
    (FS_PER_PS, "ps"),
];

impl fmt::Display for SimTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.format())
    }
}

/// Ubah `Duration` wall-clock ke waktu simulasi pada presisi tertentu.
pub fn duration_to_simtime(duration: Duration, precision_fs: u64) -> SimTime {
    let nanos = duration.as_nanos() as u64;
    SimTime::from_femtos(nanos.saturating_mul(1_000).saturating_div(precision_fs))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_nanos_converts_to_femtos() {
        let t = SimTime::from_nanos(100);
        assert_eq!(t.femtos(), 100 * FS_PER_NS);
        assert_eq!(t.nanos(), 100);
    }

    #[test]
    fn format_nanos() {
        assert_eq!(SimTime::from_nanos(100).format(), "100ns");
    }

    #[test]
    fn format_sub_nanosecond() {
        let t = SimTime::from_femtos(1500);
        assert_eq!(t.format(), "1500fs");
    }

    #[test]
    fn ordering_is_numeric() {
        let a = SimTime::from_nanos(1);
        let b = SimTime::from_nanos(2);
        assert!(a < b);
    }

    #[test]
    fn zero_default() {
        assert_eq!(SimTime::default(), SimTime::ZERO);
    }

    #[test]
    fn format_mikrosecond() {
        // 2000 ns = 2 us; tanpa cabang FS_PER_US, format selalu jatuh ke "ns".
        assert_eq!(SimTime::from_nanos(2000).format(), "2us");
    }

    #[test]
    fn format_mikrosecond_bulat_tetap_dipilih() {
        assert_eq!(SimTime::from_nanos(1_500_000).format(), "1500us");
    }

    #[test]
    fn format_nanosecond_tidak_naik_ke_mikrosecond() {
        assert_eq!(SimTime::from_nanos(1500).format(), "1500ns");
    }

    /// BUG: `timescale 1s/1s` dengan `#1000000000` membuat `SimTime` jenuh di
    /// `u64::MAX` femtosecond. `in_units` lama menulis
    /// `self.femtos + per / 2`, yang meluap: panic di build debug dan membungkus
    /// diam-diam di build release. Kedua hasil membuat `$time` salah, dan
    /// reference tool (`iverilog`) masih melaporkan `1000000000`.
    #[test]
    fn bug_in_units_tidak_meluap_saat_waktu_jenuh() {
        let jenuh = SimTime::from_femtos(u64::MAX);
        // `1s/1s`: satu unit = 1e15 femtosecond.
        let hasil = jenuh.in_units(FS_PER_SEC);
        // Nilai terbesar yang masih terekspresikan, bukan hasil pembungkusan.
        assert_eq!(hasil, (u64::MAX - FS_PER_SEC / 2) / FS_PER_SEC);
        // Nilai paranoidanya 18446, bukan hasil `u64::MAX + per/2` yang
        // membungkus dan menghasilkan 0.
        assert_eq!(hasil, 18_446);
    }

    /// BUG: `per = 1` (satuan `1fs`) membuat `per / 2 == 0`, jadi pembulatan
    /// tidak boleh mengubah apa pun — `in_units(1)` harus identitas.
    #[test]
    fn bug_in_units_satuan_femtos_kecil_adalah_identitas() {
        for f in [0u64, 1, 499, 500, 1_500, u64::MAX] {
            assert_eq!(SimTime::from_femtos(f).in_units(1), f, "femtos={f}");
        }
    }
}
