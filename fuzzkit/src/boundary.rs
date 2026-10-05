// Tanggung jawab: kamus nilai batas dan knob semantik untuk generator.
//
// Koreksi atas pendekatan "tidak boleh ada konstanta di generator": nihil
// konstanta itu **salah**. Yang dilarang adalah *template program* — program
// tetap yang hanya divariasikan. Yang justru wajib ada adalah *dictionary nilai
// batas*, karena bug di SVRust hampir selalu muncul tepat di batas implementasi:
//
//   `1u64 << 64` (masker part-select)   `to_u64()` truncation   `MAX_WIDTH < lebar operand`
//
// Kalau lebar dipilih seragam dari 1..256, peluang mendarat tepat di 63/64/65,
// 127/128/129 mendekati nol dan kelas bug itu tidak akan pernah tersentuh.
// Karena itu setiap kelas batas diberi **bobot tinggi**, dan `LebarPool` di
// sini menyimpan dua hal sekaligus: nilai batas tetap yang dipilih dengan bobot
// tinggi, dan nilai acak untuk menjelajah interior.
use arbitrary::{Arbitrary, Unstructured};

/// Nilai batas implementasi. Nama-nama ini adalah tempat bug berada.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Batas {
    /// Nol — memicu penjaga `width == 0`, part-select dengan lebar 0.
    Nol,
    /// Satu — pemisah sah/tidak sah.
    Satu,
    /// Batas bawah indeks yang sah.
    IndeksMin,
    /// Batas bawah indeks, naik satu — memicu penjaga indeks negatif/0.
    IndeksMinPlus1,
    /// Batas atas indeks yang sah (lebar - 1).
    IndeksMaks,
    /// Tepat satu di atas batas sah — memicu penjaga OOB.
    IndeksDiAtasMaks,
    /// Dua di atas batas sah.
    IndeksDiAtasMaks2,
    /// Batas 8-bit.
    Delapan,
    /// Tepat di bawah batas `u64`.
    U64Minus1,
    /// Tepat batas `u64`.
    U64,
    /// Tepat di atas batas `u64`.
    U64Plus1,
    /// Batas halaman 32-bit.
    U32,
    /// Batas bawah vektor yang diuji sebagai 128-bit.
    Bit128,
    /// Di atas 128 — menguji kapasitas `MAX_WIDTH` yang mungkin lebih kecil.
    Bit128Plus1,
    /// Batas maksimum vektor runtime yang lebih besar.
    Bit256,
    /// Shift sebesar batas `u64` — memicu `shift left with overflow`.
    ShiftU64,
    /// Kapasitas maksimum vektor runtime.
    MaxWidth,
}

impl Batas {
    /// Nilai numeriknya sebagai lebar bit (konteks pemakaiannya selalu lebar).
    pub fn sebagai_lebar(self) -> u32 {
        match self {
            Batas::Nol => 0,
            Batas::Satu => 1,
            Batas::IndeksMin => 1,
            Batas::IndeksMinPlus1 => 2,
            Batas::IndeksMaks => 7,
            Batas::IndeksDiAtasMaks => 8,
            Batas::IndeksDiAtasMaks2 => 9,
            Batas::Delapan => 8,
            Batas::U64Minus1 => 63,
            Batas::U64 => 64,
            Batas::U64Plus1 => 65,
            Batas::U32 => 32,
            Batas::Bit128 => 128,
            Batas::Bit128Plus1 => 129,
            Batas::Bit256 => 256,
            Batas::ShiftU64 => 64,
            Batas::MaxWidth => 1024,
        }
    }

    /// Daftar lengkap; dipakai untuk menjaga kamus tetap terpakai.
    pub fn semua() -> &'static [Batas] {
        use Batas::*;
        &[
            Nol,
            Satu,
            IndeksMin,
            IndeksMinPlus1,
            IndeksMaks,
            IndeksDiAtasMaks,
            IndeksDiAtasMaks2,
            Delapan,
            U32,
            U64Minus1,
            U64,
            U64Plus1,
            Bit128,
            Bit128Plus1,
            Bit256,
            ShiftU64,
            MaxWidth,
        ]
    }

    /// Bobot 상대. Batas `u64` mendapat bobot paling tinggi karena hampir
    /// semua kelas bug truncation/masking terjadi tepat di sana.
    pub fn bobot(self) -> u32 {
        use Batas::*;
        match self {
            U64Minus1 | U64 | U64Plus1 => 12,
            U32 | ShiftU64 => 10,
            Nol | Satu => 8,
            IndeksMaks | IndeksDiAtasMaks | IndeksDiAtasMaks2 => 8,
            Bit128 | Bit128Plus1 => 9,
            Delapan | IndeksMin | IndeksMinPlus1 => 6,
            Bit256 => 5,
            MaxWidth => 3,
        }
    }
}

/// Kelas lebar untuk deklarasi sinyal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KelasLebar {
    /// Nilai batas dari kamus.
    Batas(Batas),
    /// Lebar non-batas yang dipilih dari data — menjelajah interior supaya
    /// kelas batas tidak monoton.
    Interior(u8),
}

impl<'a> Arbitrary<'a> for KelasLebar {
    /// Draw kelas lebar: separuh dari kelas batas (dengan bobot), separuh
    /// interior acak. `LebarPool::ambil` adalah jalur utama; `Arbitrary`
    /// dipakai supaya knob ini bisa disisipkan ke struktur data turunan
    /// tanpa plumbing manual.
    fn arbitrary(u: &mut Unstructured<'a>) -> arbitrary::Result<Self> {
        let batas = bool::arbitrary(u)?;
        if batas {
            Ok(KelasLebar::Batas(*u.choose(Batas::semua())?))
        } else {
            Ok(KelasLebar::Interior(u.int_in_range(1u8..=192)?))
        }
    }
}

impl KelasLebar {
    pub fn nilai(self) -> u32 {
        match self {
            KelasLebar::Batas(b) => b.sebagai_lebar().max(1),
            KelasLebar::Interior(v) => u32::from(v).clamp(1, 512),
        }
    }

    /// `true` bila kelas lebar ini menyentuh batas implementasi.
    pub fn adalah_batas(self) -> bool {
        matches!(self, KelasLebar::Batas(_))
    }
}

/// Pengambil kelas lebar dengan bobot.
///
/// **Sampling dengan replacement**, bukan tanpa: memakai setiap kelas batas
/// tepat sekali membuat semua draw berikutnya jatuh ke nilai interior, jadi
/// batas `u64` (tempat hampir semua bug truncation) hanya muncul satu kali per
/// program. Yang dijaga hanyalah pengulangan beruntun yang identik.
#[derive(Debug, Clone, Default)]
pub struct LebarPool {
    terakhir: Option<Batas>,
}

impl LebarPool {
    pub fn baru() -> Self {
        Self::default()
    }

    /// Ambil kelas lebar berikutnya.
    pub fn ambil(&mut self, data: &[u8]) -> KelasLebar {
        let semua = Batas::semua();
        // Byte pertama menentukan kelas (batas atau interior), byte kedua
        // menentukan mana di dalam kelas itu.
        let seed = data.first().copied().unwrap_or(1).max(1);
        let kedua = data.get(1).copied().unwrap_or(seed).max(1);
        let mau_batas = seed % 2 == 0;
        if !mau_batas {
            let v = kedua;
            let k = KelasLebar::Interior(v);
            self.terakhir = None;
            return k;
        }
        // Total bobot harus dihitung TANPA kelas yang dilewati, kalau tidak
        // `pick` bisa jatuh di luar jangkauan dan terpilih diam-diam jadi
        // `semua[0]` — yang membuat kelas `Nol` paling sering terpilih.
        let total: u32 = semua
            .iter()
            .filter(|b| Some(**b) != self.terakhir)
            .map(|b| b.bobot())
            .sum();
        let mut pick = u32::from(kedua) % total.max(1);
        let mut terpilih = *semua
            .iter()
            .find(|b| Some(**b) != self.terakhir)
            .unwrap_or(&semua[0]);
        for b in semua {
            if Some(*b) == self.terakhir {
                continue;
            }
            let w = b.bobot();
            if pick < w {
                terpilih = *b;
                break;
            }
            pick -= w;
        }
        self.terakhir = Some(terpilih);
        KelasLebar::Batas(terpilih)
    }
}

/// Status penentu konstanta — knob penting karena jalur `constant`, `parameter`,
/// `genvar`, dan `runtime`each punya bug sendiri.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Arbitrary)]
pub enum Constness {
    /// Literal dalam ekspresi.
    Constant,
    /// `parameter` modul.
    Parameter,
    /// `localparam`.
    Localparam,
    /// `genvar` — hanya sah di dalam generate.
    Genvar,
    /// Nilai runtime (sinyal).
    Runtime,
    /// Indeks yang bertanda `x`/`z` — tidak diketahui sampai simulasi.
    UnknownIndex,
}

impl Constness {
    pub fn nama(self) -> &'static str {
        match self {
            Constness::Constant => "konstanta",
            Constness::Parameter => "parameter",
            Constness::Localparam => "localparam",
            Constness::Genvar => "genvar",
            Constness::Runtime => "runtime",
            Constness::UnknownIndex => "indeks tak diketahui",
        }
    }

    pub fn semua() -> &'static [Constness] {
        use Constness::*;
        &[Constant, Parameter, Localparam, Genvar, Runtime, UnknownIndex]
    }
}

/// Knob semantik per deklarasi. Inilah "semantic state" yang diambil dari byte:
/// byte hanya memilih **kombinasi**, bukan isi program.
#[derive(Debug, Clone, Copy, Arbitrary)]
pub struct DeklarasiSemantik {
    pub lebar: KelasLebar,
    pub signed: bool,
    pub constness: Constness,
    /// true kalau sinyal ini juga array unpacked.
    pub array: bool,
    /// vraie kalau nilai awal memakai digit `x`/`z`.
    pub empat_state: bool,
}

impl DeklarasiSemantik {
    /// Ringkasan fitur semantik untuk coverage bitmap.
    pub fn fitur(&self) -> crate::sem::Fitur {
        use crate::sem::{self, Fitur};
        let mut f = Fitur::Kosong.union(Fitur::dari_kelas_lebar(self.lebar));
        if self.signed {
            f = f.union(sem::SIGNED);
        } else {
            f = f.union(sem::UNSIGNED);
        }
        if self.array {
            f = f.union(sem::UNPACKED);
        }
        if self.empat_state {
            f = f.union(sem::FOUR_STATE_MIXED);
        }
        f = f.union(match self.constness {
            Constness::Constant => sem::CONSTANT,
            Constness::Parameter => sem::PARAMETER,
            Constness::Localparam => sem::LOCALPARAM,
            Constness::Genvar => sem::GENVAR,
            Constness::Runtime => sem::RUNTIME_VALUE,
            Constness::UnknownIndex => sem::UNKNOWN_INDEX,
        });
        f
    }
}

/// Interaksi yang disengaja. Bug hampir selalu lahir dari **interaksi**, bukan
/// dari satu fitur tunggal — misalnya `signed` × `lebar > 64` × `part-select` ×
/// `NBA`. Generator harus bisa memaksa interaksi seperti itu secara langsung.
#[derive(Debug, Clone, Copy, Arbitrary)]
pub struct Interaksi {
    /// Operand kiri lebih lebar dari operand kanan.
    pub lebar_kanan_lt_kiri: bool,
    /// Signedness operand kiri dan kanan berbeda.
    pub signedness_berbeda: bool,
    /// Lebar konteks assignment lebih besar dari lebar ekspresi.
    pub konteks_lebih_lebar: bool,
    /// Indeks di luar jangkauan sinyal.
    pub indeks_oob: bool,
    /// Part-select pada LHS lalu NBA.
    pub masked_nba: bool,
    /// Indeks pada elemen array unpacked.
    pub array_index: bool,
}

impl Interaksi {
    pub fn fitur(&self) -> crate::sem::Fitur {
        use crate::sem::{self, Fitur};
        let mut f = Fitur::Kosong;
        if self.lebar_kanan_lt_kiri {
            f = f.union(sem::LEBAR_MISMATCH);
        }
        if self.signedness_berbeda {
            f = f.union(sem::SIGNEDNESS_MISMATCH);
        }
        if self.konteks_lebih_lebar {
            f = f.union(sem::KONTEKS_LEBIH_LEBAR);
        }
        if self.indeks_oob {
            f = f.union(sem::INDEKS_OOB);
        }
        if self.masked_nba {
            f = f.union(sem::NBA_MASKED);
        }
        if self.array_index {
            f = f.union(sem::ARRAY_INDEX);
        }
        f
    }
}

/// Bangun `LebarPool` dari byte; dipakai test.
pub fn pool_dari_data(data: &[u8]) -> LebarPool {
    let mut p = LebarPool::baru();
    for b in data.iter().take(8) {
        p.ambil(std::slice::from_ref(b));
    }
    p
}

/// Ambil satu `DeklarasiSemantik` dari `Unstructured`.
pub fn deklarasi_dari(u: &mut Unstructured<'_>) -> Option<DeklarasiSemantik> {
    DeklarasiSemantik::arbitrary(u).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setiap_batas_muncul_dalam_pool_yang_cukup_besar() {
        // Kalau satu kelas batas tidak pernah terpilih, kelas bug-nya tidak
        // akan pernah disentuh. Semua harus muncul dalam 512 draw.
        let mut p = LebarPool::baru();
        let mut terlihat = std::collections::BTreeSet::new();
        for i in 0u16..512 {
            let data = [1u8 + (i % 255) as u8, 1u8 + (i % 251) as u8];
            if let KelasLebar::Batas(x) = p.ambil(&data) {
                terlihat.insert(x);
            }
        }
        let hilang: Vec<&str> = Batas::semua()
            .iter()
            .filter(|b| !terlihat.contains(b))
            .map(|b| match b {
                Batas::Nol => "Nol",
                Batas::Satu => "Satu",
                Batas::IndeksMin => "IndeksMin",
                Batas::IndeksMinPlus1 => "IndeksMinPlus1",
                Batas::IndeksMaks => "IndeksMaks",
                Batas::IndeksDiAtasMaks => "IndeksDiAtasMaks",
                Batas::IndeksDiAtasMaks2 => "IndeksDiAtasMaks2",
                Batas::Delapan => "Delapan",
                Batas::U64Minus1 => "U64Minus1",
                Batas::U64 => "U64",
                Batas::U64Plus1 => "U64Plus1",
                Batas::U32 => "U32",
                Batas::Bit128 => "Bit128",
                Batas::Bit128Plus1 => "Bit128Plus1",
                Batas::Bit256 => "Bit256",
                Batas::ShiftU64 => "ShiftU64",
                Batas::MaxWidth => "MaxWidth",
            })
            .collect();
        assert!(
            hilang.is_empty(),
            "kelas batas tidak pernah terpilih: {hilang:?}"
        );
    }

    #[test]
    fn kelas_batas_diberi_bobot_lebih_tinggi_daripada_lebar_interior_tunggal() {
        // Separuh draw memang jatuh ke nilai interior (bodi interior = 50%).
        // Yang diuji: nilai batas `u64` harus muncul jauh lebih sering daripada
        // satu nilai interior tertentu — kalau tidak, kelas bug truncation
        // hanya tersentuh sesekali.
        let mut p = LebarPool::baru();
        let mut batas_u64 = 0;
        let mut interior: std::collections::BTreeMap<u8, u32> = Default::default();
        for i in 0u16..2048 {
            let data = [1u8 + (i % 255) as u8, 1u8 + (i % 241) as u8];
            match p.ambil(&data) {
                KelasLebar::Batas(Batas::U64Minus1)
                | KelasLebar::Batas(Batas::U64)
                | KelasLebar::Batas(Batas::U64Plus1) => batas_u64 += 1,
                KelasLebar::Interior(v) => *interior.entry(v).or_default() += 1,
                KelasLebar::Batas(_) => {}
            }
        }
        let interior_terbanyak = interior.values().copied().max().unwrap_or(0);
        assert!(
            batas_u64 > interior_terbanyak * 3,
            "batas u64={batas_u64}, interior terbanyak={interior_terbanyak}"
        );
    }

    #[test]
    fn interaksi_men_force_fitur_semantik() {
        use crate::sem::{self, Fitur};
        let i = Interaksi {
            lebar_kanan_lt_kiri: true,
            signedness_berbeda: true,
            konteks_lebih_lebar: true,
            indeks_oob: true,
            masked_nba: true,
            array_index: true,
        };
        let f = i.fitur();
        assert!(f.contains(sem::LEBAR_MISMATCH));
        assert!(f.contains(sem::SIGNEDNESS_MISMATCH));
        assert!(f.contains(sem::KONTEKS_LEBIH_LEBAR));
        assert!(f.contains(sem::INDEKS_OOB));
        assert!(f.contains(sem::NBA_MASKED));
        assert!(f.contains(sem::ARRAY_INDEX));
    }
}