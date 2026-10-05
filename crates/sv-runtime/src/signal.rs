// Tanggung jawab: sel sinyal runtime dengan deteksi perubahan.
use crate::bits::Bits;
use crate::logic::Logic;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignalCell<const N: usize> {
    value: Bits<N>,
}

impl<const N: usize> SignalCell<N> {
    pub fn new(initial: Bits<N>) -> Self {
        Self { value: initial }
    }

    pub fn read(&self) -> Bits<N> {
        self.value.clone()
    }

    /// Tulis nilai; kembalikan true bila benar-benar berubah.
    pub fn write(&mut self, next: Bits<N>) -> bool {
        if self.value == next {
            return false;
        }
        self.value = next;
        true
    }

    pub fn value(&self) -> &Bits<N> {
        &self.value
    }

    /// Tulis hanya digit yang di-set pada `mask`; digit lain dipertahankan.
    /// Dipakai untuk part-select pada LHS assignment (LRM §10.10.1).
    ///
    /// Masker bertipe `Bits<N>`, bukan `u64`. Dua alasan: sinyal dapat lebih
    /// lebar dari 64 bit (`logic [127:0]`, atau array unpacked yang disimpan
    /// rata), dan `to_u64()` yang dulu dipakai di sini membuang digit `X`/`Z`
    /// sekaligus memotong bit di atas 63 — jadi part-select pada sinyal lebar
    /// kehilangan nilai tanpa pesan.
    pub fn write_masked(&mut self, next: Bits<N>, mask: Bits<N>) -> bool {
        // Jalur cepat: masker penuh berarti tulis biasa. Tanpa ini setiap
        // penulisan NBA melelahkan loop `0..N` per digit — jalur paling panas
        // di loop simulasi (Commit pending tiap timestep).
        if mask == Bits::satu() {
            return self.write(next);
        }
        let lama = self.value.clone();
        let mut merged = lama.clone();
        for i in 0..N {
            if matches!(mask.get(i), Logic::One) {
                merged.set(i, next.get(i));
            } else {
                merged.set(i, lama.get(i));
            }
        }
        self.write(merged)
    }
}

impl<const N: usize> Default for SignalCell<N> {
    fn default() -> Self {
        Self::new(Bits::zero())
    }
}

/// Penulisan non-blocking yang menunggu region's NBA.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingWrite<const N: usize> {
    pub target: usize,
    pub value: Bits<N>,
    /// Digit yang boleh berubah; seluruh `1` berarti seluruh sinyal.
    pub mask: Bits<N>,
}

impl<const N: usize> PendingWrite<N> {
    /// Tulis ke seluruh sinyal.
    pub fn new(target: usize, value: Bits<N>) -> Self {
        Self {
            target,
            value,
            mask: Bits::satu(),
        }
    }

    /// Tulis hanya ke digit yang di-set pada `mask` (part-select pada LHS).
    pub fn new_masked(target: usize, value: Bits<N>, mask: Bits<N>) -> Self {
        Self {
            target,
            value,
            mask,
        }
    }
}

/// Baca signal sebagai digit tunggal.
pub fn read_bit<const N: usize>(cell: &SignalCell<N>, index: usize) -> Logic {
    cell.read().get(index)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_menylap_kembalikan_true() {
        let mut cell = SignalCell::<8>::default();
        assert!(cell.write(Bits::from_u64(0xAA)));
        // Nilai sama tidak dianggap perubahan.
        assert!(!cell.write(Bits::from_u64(0xAA)));
    }

    #[test]
    fn masked_write_hanya_mengubah_bit_dalam_masker() {
        let mut cell = SignalCell::<8>::new(Bits::from_u64(0xF0));
        // Tulis 0x0C hanya ke nibble bawah.
        cell.write_masked(Bits::from_u64(0x0C), Bits::from_u64(0x0F));
        assert_eq!(cell.read().to_u64(), 0xFC);
    }

    #[test]
    fn masked_write_mempertahankan_bit_luar_masker() {
        let mut cell = SignalCell::<8>::new(Bits::from_u64(0xFF));
        cell.write_masked(Bits::from_u64(0x00), Bits::from_u64(0x0F));
        // Nibble atas harus tetap 0xF0.
        assert_eq!(cell.read().to_u64(), 0xF0);
    }

    #[test]
    fn masked_write_bit_tunggal() {
        let mut cell = SignalCell::<8>::new(Bits::from_u64(0x00));
        cell.write_masked(Bits::from_u64(1), Bits::from_u64(0x01));
        assert_eq!(cell.read().to_u64(), 0x01);
        cell.write_masked(Bits::from_u64(0), Bits::from_u64(0x01));
        assert_eq!(cell.read().to_u64(), 0x00);
    }

    #[test]
    fn masked_write_nilai_sama_tidak_perubahan() {
        let mut cell = SignalCell::<8>::new(Bits::from_u64(0xF0));
        assert!(!cell.write_masked(Bits::from_u64(0x00), Bits::from_u64(0x0F)));
    }

    #[test]
    fn masked_write_mendeteksi_perubahan_nyata() {
        let mut cell = SignalCell::<8>::new(Bits::from_u64(0xF0));
        assert!(cell.write_masked(Bits::from_u64(0x03), Bits::from_u64(0x0F)));
    }

    #[test]
    fn pending_write_default_menutup_seluruh_sinyal() {
        let write = PendingWrite::new(2, Bits::<8>::from_u64(0xFF));
        assert_eq!(write.target, 2);
        assert_eq!(write.mask.to_u64(), 0xFF);
        assert!(write.mask.is_fully_known());
    }

    #[test]
    fn pending_write_masked_simpan_masker() {
        let write =
            PendingWrite::new_masked(2, Bits::<8>::from_u64(0x0C), Bits::<8>::from_u64(0x0F));
        assert_eq!(write.mask.to_u64(), 0x0F);
    }
}
