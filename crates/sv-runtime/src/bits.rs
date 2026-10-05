// Tanggung jawab: vektor bit 4-state dengan lebar generik const N.
use crate::logic::Logic;
use std::fmt;

/// Vektor `N` digit logika, LSB-first di dalam limb.
///
/// Tiga sideband paralel per limb:
/// - `value`  — bit 1 berarti `One`
/// - `xmask`  — bit 1 berarti `X`
/// - `zmask`  — bit 1 berarti `Z`
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Bits<const N: usize> {
    value: Vec<u64>,
    xmask: Vec<u64>,
    zmask: Vec<u64>,
}

impl<const N: usize> Bits<N> {
    pub fn zero() -> Self {
        let count = Self::limb_count();
        Self {
            value: vec![0u64; count],
            xmask: vec![0u64; count],
            zmask: vec![0u64; count],
        }
    }

    /// Vektor seluruhnya `1` — masker "tulis seluruh sinyal".
    pub fn satu() -> Self {
        Self::all(Logic::One)
    }

    /// Vektor homogen: semua digit bernilai sama.
    pub fn all(digit: Logic) -> Self {
        let mut result = Self::zero();
        for i in 0..N {
            result.set(i, digit);
        }
        result
    }

    pub fn from_u64(value: u64) -> Self {
        let mut result = Self::zero();
        if let Some(cell) = result.value.first_mut() {
            *cell = value;
        }
        result.clear_padding();
        result
    }

    pub fn from_u32(value: u32) -> Self {
        Self::from_u64(value as u64)
    }

    /// Vektor dengan digit tak diketahui pada posisi `unknown` (LRM §5.7.1).
    ///
    /// `xmask` bit 1 berarti digit `X` di posisi itu, `zmask` bit 1 berarti `Z`.
    /// LRM §5.7.1: digit `?` diperlakukan sebagai `z` bila nilainya 0 atau 1,
    /// dan sebagai `x` bila nilainya `x` atau `z` — pemanggil sudah diselesaikan
    /// lebih dulu oleh lexer, jadi di sini bit `?` tidak perlu ditangani lagi.
    ///
    /// Lebar logis `width` dipakai untuk membatasi: digit di luar lebar itu
    /// bukan bagian nilai, jadi tidak boleh muncul sebagai `X`/`Z`.
    pub fn from_unknown(value: u64, xmask: u64, zmask: u64, width: u32) -> Self {
        let mut result = Self::from_u64(value);
        let w = (width as usize).min(N);
        // Masker hanya mencakup 64 bit, jadi bit di atas 64 tidak boleh ikut
        // diuji: `1u64 << (i % 64)` untuk `i = 68` menghasilkan bit 4 dan
        // akan menandai limb kedua dengan `z` yang sebenarnya tidak ada.
        for i in 0..w.min(64) {
            let bit = 1u64 << i;
            // `zmask` diperiksa lebih dulu: LRM §5.7.1 memberi `z` makna
            // impedansi tinggi yang lebih spesifik daripada `x`. `xmask` dari
            // lexer masih memuat posisi `z`, jadi `x` yang diperiksa duluan
            // membuat setiap `z` tercetak sebagai `x`.
            if zmask & bit != 0 {
                result.set(i, Logic::Z);
            } else if xmask & bit != 0 {
                result.set(i, Logic::X);
            }
        }
        result
    }

    pub fn from_logic(digit: Logic) -> Self {
        let mut result = Self::zero();
        result.set(0, digit);
        result
    }

    pub fn width() -> usize {
        N
    }

    fn limb_count() -> usize {
        N.div_ceil(64).max(1)
    }

    /// Bit di luar `N` dibuang dari ketiga sideband.
    fn clear_padding(&mut self) {
        let valid = N % 64;
        if valid == 0 {
            return;
        }
        let mask = (1u64 << valid) - 1;
        let limb = self.value.len().saturating_sub(1);
        if limb < self.value.len() {
            self.value[limb] &= mask;
            self.xmask[limb] &= mask;
            self.zmask[limb] &= mask;
        }
    }

    pub fn get(&self, index: usize) -> Logic {
        if index >= N {
            return Logic::X;
        }
        let limb = index / 64;
        let offset = index % 64;
        let bit = 1u64 << offset;
        let is_z = self.zmask.get(limb).is_some_and(|w| w & bit != 0);
        if is_z {
            return Logic::Z;
        }
        let is_x = self.xmask.get(limb).is_some_and(|w| w & bit != 0);
        if is_x {
            return Logic::X;
        }
        match self.value.get(limb) {
            Some(w) if w & bit != 0 => Logic::One,
            _ => Logic::Zero,
        }
    }

    pub fn set(&mut self, index: usize, digit: Logic) {
        if index >= N {
            return;
        }
        let limb = index / 64;
        let offset = index % 64;
        let bit = 1u64 << offset;
        let is_one = matches!(digit, Logic::One);
        let is_z = matches!(digit, Logic::Z);

        if let Some(cell) = self.value.get_mut(limb) {
            if is_one {
                *cell |= bit;
            } else {
                *cell &= !bit;
            }
        }
        // X: bit cleared di value, diset di xmask.
        if let Some(cell) = self.xmask.get_mut(limb) {
            if matches!(digit, Logic::X) {
                *cell |= bit;
            } else {
                *cell &= !bit;
            }
        }
        if let Some(cell) = self.zmask.get_mut(limb) {
            if is_z {
                *cell |= bit;
            } else {
                *cell &= !bit;
            }
        }
    }

    /// Nilai sebagai integer; mengabaikan X/Z (diperlakukan 0).
    pub fn to_u64(&self) -> u64 {
        let mut result = 0u64;
        for i in 0..N.min(64) {
            if matches!(self.get(i), Logic::One) {
                result |= 1u64 << i;
            }
        }
        result
    }

    /// Representasi hex dengan lebar digit sesuai `N`.
    pub fn to_hex(&self) -> String {
        let digits = N.div_ceil(4);
        let mut out = String::with_capacity(digits);
        for d in (0..digits).rev() {
            let shift = d * 4;
            let mut nibble = 0u8;
            for bit in 0..4 {
                if matches!(self.get(shift + bit), Logic::One) {
                    nibble |= 1 << bit;
                }
            }
            out.push(std::char::from_digit(nibble as u32, 16).unwrap_or('0'));
        }
        out
    }

    /// Digit heksadesimal sepanjang `width` bit, dengan `x`/`z` dipertahankan.
    ///
    /// `to_hex` tidak tahu lebar logis sinyal, jadi tidak bisa membedakan
    /// "nibble benar-benar 0" dari "nibble belum diketahui". Driver yang
    /// mencetak nilai port harus memakai varian ini — kalau tidak, variabel
    /// yang belum diinisialisasi tercetak `00` padahal LRM §4.3.1 memberi
    /// nilai awal `x`.
    pub fn to_hex_lebar(&self, width: u32) -> String {
        let digits = width.div_ceil(4);
        let mut out = String::with_capacity(digits as usize);
        for d in (0..digits).rev() {
            let mut nibble = 0u8;
            let mut ada_x = false;
            let mut ada_z = false;
            for bit in 0..4usize {
                let idx = d * 4 + bit as u32;
                if idx >= width {
                    continue;
                }
                match self.get(idx as usize) {
                    Logic::One => nibble |= 1 << bit,
                    Logic::X => ada_x = true,
                    Logic::Z => ada_z = true,
                    Logic::Zero => {}
                }
            }
            out.push(if ada_z {
                'z'
            } else if ada_x {
                'x'
            } else {
                std::char::from_digit(nibble as u32, 16).unwrap_or('0')
            });
        }
        out
    }

    /// Render seluruh bit MSB-dulu sebagai string (0/1/x/z) — format nilai VCD.
    pub fn to_vcd_string(&self) -> String {
        let mut out = String::with_capacity(N);
        for i in (0..N).rev() {
            out.push(self.get(i).vcd_char());
        }
        out
    }

    pub fn value_slice(&self) -> &[u64] {
        &self.value
    }

    pub fn is_fully_known(&self) -> bool {
        (0..N).all(|i| self.get(i).is_definite())
    }
}

impl<const N: usize> Default for Bits<N> {
    fn default() -> Self {
        Self::zero()
    }
}

impl<const N: usize> fmt::Debug for Bits<N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Bits<{}>(0b", N)?;
        for i in (0..N).rev() {
            write!(f, "{}", self.get(i).vcd_char())?;
        }
        write!(f, ")")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logic::Logic;

    #[test]
    fn zero_is_all_zero() {
        let value = Bits::<8>::zero();
        assert_eq!(value.get(0), Logic::Zero);
        assert_eq!(value.get(7), Logic::Zero);
        assert!(value.is_fully_known());
    }

    #[test]
    fn from_u64_sets_low_bit() {
        let value = Bits::<8>::from_u64(0b1010_1010);
        assert_eq!(value.get(0), Logic::Zero);
        assert_eq!(value.get(1), Logic::One);
        assert_eq!(value.to_u64(), 0b1010_1010);
    }

    #[test]
    fn width_over_64_uses_multiple_limbs() {
        let value = Bits::<128>::from_u64(1);
        assert_eq!(value.get(0), Logic::One);
        assert_eq!(value.get(64), Logic::Zero);
        assert_eq!(value.get(127), Logic::Zero);
    }

    #[test]
    fn padding_bits_cleared() {
        let value = Bits::<4>::from_u64(0xFF);
        assert_eq!(value.get(0), Logic::One);
        assert_eq!(value.get(3), Logic::One);
        assert_eq!(value.get(4), Logic::X);
    }

    #[test]
    fn z_survives_storage_roundtrip() {
        let mut value = Bits::<2>::zero();
        value.set(0, Logic::Z);
        value.set(1, Logic::One);
        assert_eq!(value.get(0), Logic::Z);
        assert_eq!(value.get(1), Logic::One);
    }

    #[test]
    fn all_ones_fills_width() {
        let value = Bits::<8>::all(Logic::One);
        assert_eq!(value.to_u64(), 0xFF);
        assert_eq!(value.get(7), Logic::One);
    }

    #[test]
    fn all_x_is_not_fully_known() {
        let value = Bits::<4>::all(Logic::X);
        assert!(!value.is_fully_known());
        assert_eq!(value.get(0), Logic::X);
    }

    #[test]
    fn overwriting_x_with_one_clears_mask() {
        let mut value = Bits::<1>::from_logic(Logic::X);
        value.set(0, Logic::One);
        assert_eq!(value.get(0), Logic::One);
    }

    #[test]
    fn overwriting_one_with_z_clears_value() {
        let mut value = Bits::<1>::from_logic(Logic::One);
        value.set(0, Logic::Z);
        assert_eq!(value.get(0), Logic::Z);
    }

    #[test]
    fn x_in_wide_vector_stays_at_position() {
        let mut value = Bits::<128>::zero();
        value.set(70, Logic::X);
        assert_eq!(value.get(69), Logic::Zero);
        assert_eq!(value.get(70), Logic::X);
        assert_eq!(value.get(71), Logic::Zero);
    }

    #[test]
    fn to_vcd_string_msb_first() {
        let value = Bits::<4>::from_u64(0b1010);
        assert_eq!(value.to_vcd_string(), "1010");
    }

    #[test]
    fn to_vcd_string_with_x_mid() {
        let mut value = Bits::<4>::zero();
        value.set(1, Logic::X);
        assert_eq!(value.to_vcd_string(), "00x0");
    }

    #[test]
    fn from_unknown_hanya_menerapkan_masker_di_64_bit_bawah() {
        // BUG: masker `xmask`/`zmask` bertipe u64, jadi hanya mencakup 64 bit.
        // Menguji `1u64 << (i % 64)` untuk `i > 64` membuat limb kedua ikut
        // ditandai — `from_unknown(10, 0xF0, 0xF0, 128)` lalu menghasilkan
        // `Z` di bit 68 dan seterusnya, padahal tidak ada digit unknown di sana.
        let v = Bits::<128>::from_unknown(10, 0xF0, 0xF0, 128);
        for i in 0..128usize {
            let diharapkan = match i {
                4..=7 => Logic::Z,
                1 | 3 => Logic::One,
                _ => Logic::Zero,
            };
            assert_eq!(v.get(i), diharapkan, "bit {i}");
        }
    }

    #[test]
    fn from_unknown_membatasi_lebar_logis() {
        // Digit di luar lebar logis bukan bagian nilai, jadi tidak boleh muncul
        // sebagai `Z` meski masker menyatakannya. Nilai 2 = 0b0010, jadi bit 3
        // dan seterusnya memang harus nol.
        let v = Bits::<128>::from_unknown(2, 0, 0xF0, 4);
        assert_eq!(v.get(3), Logic::Zero, "bit 3 di luar lebar 4");
        assert_eq!(v.get(4), Logic::Zero, "bit 4 di luar lebar 4");
        assert_eq!(v.get(1), Logic::One);
    }

    #[test]
    fn from_unknown_z_mengalahkan_x() {
        // LRM §5.7.1: `z` lebih spesifik dari `x`, dan `xmask` dari lexer masih
        // memuat posisi `z` — jadi `z` harus diperiksa lebih dulu.
        let v = Bits::<8>::from_unknown(0, 0x0F, 0x0F, 8);
        for i in 0..4usize {
            assert_eq!(v.get(i), Logic::Z, "bit {i} harus z bukan x");
        }
    }

    #[test]
    fn from_unknown_hanya_x_bukan_z() {
        let v = Bits::<8>::from_unknown(0, 0xF0, 0, 8);
        for i in 4..8usize {
            assert_eq!(v.get(i), Logic::X, "bit {i} harus x");
        }
    }
}
