// Tanggung jawab: semantic coverage bitmap dan sidik jari bug.
//
// LLVM edge coverage menjawab "jalur kode mana yang sudah dieksekusi". Itu
// tidak menjawab "bagian semantik SystemVerilog mana yang sudah diuji" —
// keduanya hanya berkorelasi lemah.
//
// Contoh nyata: program bisa menghasilkan edge coverage tinggi sambil
// `casez` + `X`, NBA masked pada elemen array, dan selektor `case` 128-bit
// tidak pernah muncul. Karena itu corpus dipertahankan berdasarkan **novel
// semantic coverage**, bukan hanya edge coverage.
//
// Modul ini juga menyediakan **interaction coverage**: bug hampir selalu lahir
// dari interaksi fitur, bukan dari satu fitur tunggal.
use std::collections::BTreeSet;

/// Kumpulan fitur semantik sebagai bitset 128-bit.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Fitur {
    bits: u128,
}

impl std::fmt::Debug for Fitur {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Fitur[{}]", self.ringkas().join("|"))
    }
}

/// Konstanta fitur dan tabel nama dibangkitkan dari **makro yang sama**, jadi
/// nomor bit dan nama tidak pernah bisa terpisah.
///
/// Nomor bit ditulis eksplisit, bukan dicari lewat perbandingan `&str` saat
/// compile-time: `PartialEq` untuk `&str` belum stabil di const context, dan
/// makro yang mencari nama akan gagal diam-diam kalau ada salah ketik.
macro_rules! fitur {
    ($($nama:ident => $bit:expr),* $(,)?) => {
        $(
            #[allow(non_upper_case_globals)]
            pub const $nama: Fitur = Fitur { bits: 1u128 << $bit };
        )*
        #[rustfmt::skip]
        const TABEL: &[(&str, u128)] = &[
            $( (stringify!($nama), 1u128 << $bit), )*
        ];
    };
}

fitur! {
    // kelompok lebar — batas implementasi ada di sini
    W0 => 0,
    W1 => 1,
    W8 => 2,
    W32 => 3,
    W64 => 4,
    W65 => 5,
    W128 => 6,
    W129 => 7,
    W_GT_64 => 8,
    W_BOUNDARY => 9,
    // kelompok tipe dan state
    SIGNED => 10,
    UNSIGNED => 11,
    FOUR_STATE_X => 12,
    FOUR_STATE_Z => 13,
    FOUR_STATE_MIXED => 14,
    UNPACKED => 15,
    CONSTANT => 16,
    PARAMETER => 17,
    LOCALPARAM => 18,
    GENVAR => 19,
    RUNTIME_VALUE => 20,
    UNKNOWN_INDEX => 21,
    // kelompok seleksi
    SELECT_BIT => 22,
    SELECT_PART => 23,
    SELECT_INDEXED => 24,
    SELECT_DYNAMIC => 25,
    SELECT_NESTED => 26,
    SELECT_ARRAY => 27,
    SELECT_OOB => 28,
    SELECT_ON_LHS => 29,
    ARRAY_INDEX => 30,
    // kelompok statement dan scheduler
    NBA => 31,
    NBA_MASKED => 32,
    BLOCKING => 33,
    LOOP_FOR => 34,
    LOOP_REPEAT => 35,
    LOOP_WHILE => 36,
    CASE => 37,
    CASEZ => 38,
    CASEX => 39,
    EVENT_POSEDGE => 40,
    EVENT_NEDGE => 41,
    DELAY_ZERO => 42,
    DELAY_NONZERO => 43,
    TIMESCALE => 44,
    GENERATE_FOR => 45,
    GENERATE_IF => 46,
    TASK_CALL => 47,
    FUNCTION_CALL => 48,
    CONCAT => 49,
    REPLICA => 50,
    TERNARY => 51,
    CAST_TYPEDEF => 52,
    CAST_BUILTIN => 53,
    CAST_SIGN => 54,
    REDUCTION => 55,
    DIVISION => 56,
    SHIFT => 57,
    MULTILIER => 58,
    // kelompok interaksi — bug hidup di sini
    LEBAR_MISMATCH => 59,
    SIGNEDNESS_MISMATCH => 60,
    KONTEKS_LEBIH_LEBAR => 61,
    INDEKS_OOB => 62,
}

impl Fitur {
    pub const Kosong: Fitur = Fitur { bits: 0 };

    fn dari(bit: usize) -> Fitur {
        Fitur { bits: 1u128 << bit }
    }

    pub fn isi(self) -> bool {
        self.bits != 0
    }

    pub fn contains(self, lain: Fitur) -> bool {
        lain.bits != 0 && self.bits & lain.bits == lain.bits
    }

    pub fn union(self, lain: Fitur) -> Fitur {
        Fitur {
            bits: self.bits | lain.bits,
        }
    }

    /// Pasangan fitur yang sama-sama hadir — satuan interaction coverage.
    pub fn pasangan(self) -> BTreeSet<InteraksiFitur> {
        let mut out = BTreeSet::new();
        for i in 0..TABEL.len() {
            let a = Fitur::dari(i);
            if !self.contains(a) {
                continue;
            }
            for j in (i + 1)..TABEL.len() {
                let b = Fitur::dari(j);
                if self.contains(b) {
                    out.insert(InteraksiFitur { a: i, b: j });
                }
            }
        }
        out
    }

    /// Nama fitur yang hadir, urut index bit.
    pub fn ringkas(self) -> Vec<&'static str> {
        TABEL
            .iter()
            .filter(|(_, bit)| self.bits & bit != 0)
            .map(|(n, _)| *n)
            .collect()
    }

    /// Petakan kelas lebar ke bit fitur. Lebar tepat di batas implementasi
    /// dicatat terpisah supaya bisa dibedakan dari lebar besar biasa.
    pub fn dari_kelas_lebar(k: crate::boundary::KelasLebar) -> Fitur {
        use crate::boundary::KelasLebar as K;
        let n = match k {
            K::Batas(_) => return Fitur::dari(9),
            K::Interior(v) => u32::from(v).clamp(1, 512),
        };
        let bit = match n {
            0 => 0,
            1 => 1,
            2..=15 => 2,
            16..=63 => 3,
            64 => 4,
            65..=127 => 5,
            128 => 6,
            _ => 7,
        };
        let mut f = Fitur::dari(bit);
        if n > 64 {
            f = f.union(Fitur::dari(8));
        }
        f
    }
}

impl std::ops::BitOr for Fitur {
    type Output = Fitur;
    fn bitor(self, rhs: Fitur) -> Fitur {
        self.union(rhs)
    }
}

impl std::ops::BitOrAssign for Fitur {
    fn bitor_assign(&mut self, rhs: Fitur) {
        *self = self.union(rhs);
    }
}

/// Pasangan fitur — satuan interaction coverage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct InteraksiFitur {
    a: usize,
    b: usize,
}

impl InteraksiFitur {
    pub fn nama(&self) -> String {
        format!("{}x{}", TABEL[self.a].0, TABEL[self.b].0)
    }
}

/// Kumpulan coverage semantik: set fitur dan set interaksi.
#[derive(Debug, Clone, Default)]
pub struct Coverage {
    pub fitur: BTreeSet<&'static str>,
    pub interaksi: BTreeSet<String>,
}

impl Coverage {
    pub fn baru() -> Self {
        Self::default()
    }

    /// Tambahkan fitur; `true` bila ada fitur baru.
    pub fn catat(&mut self, f: Fitur) -> bool {
        let mut baru = false;
        for n in f.ringkas() {
            baru |= self.fitur.insert(n);
        }
        baru
    }

    /// Tambahkan interaksi; `true` bila ada pasangan baru.
    pub fn catat_interaksi(&mut self, f: Fitur) -> bool {
        let mut baru = false;
        for p in f.pasangan() {
            baru |= self.interaksi.insert(p.nama());
        }
        baru
    }

    /// Gabungkan coverage lain (dipakai saat menggabungkan corpus antar-worker).
    pub fn gabung(&mut self, lain: &Coverage) {
        self.fitur.extend(lain.fitur.iter().copied());
        self.interaksi.extend(lain.interaksi.iter().cloned());
    }

    pub fn jumlah_fitur(&self) -> usize {
        self.fitur.len()
    }

    pub fn jumlah_interaksi(&self) -> usize {
        self.interaksi.len()
    }

    /// Skor penjadwalan seed: makin banyak novelty, makin tinggi nilainya.
    /// Interaksi berbobot lebih tinggi karena lebih informatif.
    pub fn skor_novelty(&self, lain: &Coverage) -> usize {
        let fitur_baru = lain.fitur.difference(&self.fitur).count();
        let interaksi_baru = lain.interaksi.difference(&self.interaksi).count();
        fitur_baru + interaksi_baru * 3
    }

    pub fn ringkas(&self) -> String {
        format!(
            "fitur={} interaksi={}",
            self.fitur.len(),
            self.interaksi.len()
        )
    }
}

/// Sidik jari bug untuk deduplikasi. Satu bug yang ditemukan 100.000 kali tetap
/// satu bug.
pub fn sidik_jari(target: &str, pesan: &str, sumber: &str) -> String {
    let kategori = pesan
        .lines()
        .next()
        .unwrap_or("")
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
        .take(48)
        .collect::<String>();
    format!(
        "{target}|{kategori}|baris={}|len={}",
        sumber.lines().count(),
        sumber.len()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setiap_konstanta_fitur_punya_nama_yang_benar() {
        // Kalau nama di TABEL dan konstanta meleset, coverage akan melapor nama
        // yang salah tanpa error kompilasi.
        assert_eq!(W64.ringkas(), vec!["W64"]);
        assert_eq!(SIGNED.ringkas(), vec!["SIGNED"]);
        assert_eq!(NBA_MASKED.ringkas(), vec!["NBA_MASKED"]);
        assert_eq!(LEBAR_MISMATCH.ringkas(), vec!["LEBAR_MISMATCH"]);
    }

    #[test]
    fn nomor_bit_unik_dan_kontinu() {
        let mut seen = std::collections::BTreeSet::new();
        for (_, bit) in TABEL {
            assert!(seen.insert(bit), "nomor bit ganda: {bit}");
        }
        // Sekuens dari 0..TABEL.len()-1 supaya `pasangan()` benar.
        let expected: Vec<u128> = (0..TABEL.len() as u128).map(|i| 1u128 << i).collect();
        let got: Vec<u128> = TABEL.iter().map(|(_, b)| *b).collect();
        assert_eq!(got, expected, "bit harus berurutan tanpa celah");
    }

    #[test]
    fn fitur_union_dan_contains_bekerja() {
        let f = W64 | W_GT_64 | SIGNED;
        assert!(f.contains(W64));
        assert!(f.contains(W_GT_64));
        assert!(!f.contains(UNPACKED));
        assert!(f.contains(W64 | SIGNED));
    }

    #[test]
    fn coverage_mencatat_novelty() {
        let mut c = Coverage::baru();
        assert!(c.catat(W64 | SIGNED));
        assert!(!c.catat(SIGNED), "fitur yang sama bukan novelty");
        assert!(c.catat(W128));
    }

    #[test]
    fn interaction_coverage_menangkap_pasangan() {
        let f = SIGNED | W_GT_64 | SELECT_PART | NBA_MASKED;
        let nama: Vec<String> = f.pasangan().iter().map(|p| p.nama()).collect();
        assert!(
            nama.iter().any(|n| n == "W_GT_64xSIGNED"),
            "pasangan SIGNEDxW>64 harus ada: {nama:?}"
        );
        assert!(
            nama.iter().any(|n| n == "SELECT_PARTxNBA_MASKED"),
            "pasangan inti select-part x NBA masked harus ada: {nama:?}"
        );
    }

    #[test]
    fn skor_novelty_mengutamakan_interaksi() {
        let kosong = Coverage::baru();
        let mut c = Coverage::baru();
        c.catat(SIGNED | W_GT_64 | SELECT_PART);
        c.catat_interaksi(SIGNED | W_GT_64 | SELECT_PART);
        // 3 fitur baru, 3 interaksi baru -> 3 + 3*3 = 12
        assert_eq!(kosong.skor_novelty(&c), 12);
    }

    #[test]
    fn kelas_lebar_mencatat_batas_dan_besar() {
        use crate::boundary::{Batas, KelasLebar};
        assert!(Fitur::dari_kelas_lebar(KelasLebar::Batas(Batas::U64)).contains(W_BOUNDARY));
        assert!(Fitur::dari_kelas_lebar(KelasLebar::Interior(200)).contains(W_GT_64));
        assert!(Fitur::dari_kelas_lebar(KelasLebar::Interior(8)).contains(W8));
    }

    #[test]
    fn sidik_jari_stabil_untuk_bug_yang_sama() {
        let a = sidik_jari("elaborate", "PANIC: index out of bounds\n baris 3", "a\nb\nc");
        let b = sidik_jari("elaborate", "PANIC: index out of bounds\n baris 9", "a\nb\nc");
        assert_eq!(a, b);
        let c = sidik_jari("elaborate", "PANIC: divide by zero\n baris 3", "a\nb\nc");
        assert_ne!(a, c);
    }
}