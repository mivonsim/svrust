// Tanggung jawab: node AST untuk target assignment (sinyal atau irisannya).
use serde::{Deserialize, Serialize};
use crate::expression::Expr;
use sv_lexer::span::Span;

/// Irisan bit target: `[i]` untuk bit-select, `[msb:lsb]` untuk part-select.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Slice {
    pub msb: u32,
    pub lsb: u32,
}

/// Indexed part-select `[base +: width]` atau `[base -: width]` (LRM §7.8)
/// pada sisi kiri assignment. `base` adalah indeks awal, `width` lebar,
/// `arah` menentukan `+` atau `-`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PartDinamis {
    pub base: Expr,
    pub width: Expr,
    pub arah: Arah,
}

/// Arah indexed part-select.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Arah {
    Plus,  // `[base +: width]`
    Minus, // `[base -: width]`
}

impl Slice {
    /// Lebar irisan dalam bit.
    pub fn width(self) -> u32 {
        self.msb - self.lsb + 1
    }
}

/// Sisi kiri assignment: nama sinyal dengan irisan bit opsional dan path
/// hierarkis melalui titik, misal `tl_if.h2d` atau `adc_chn_val_o[0].de`.
///
/// `name` adalah nama dasar, `slice` irisan pertama pada nama dasar,
/// `extra_slices` irisan bertingkat, `members` segmen path setelah titik
/// (misal `["h2d"]` pada `tl_if.h2d`), `genvar_index` indeks genvar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lvalue {
    pub name: String,
    /// `None` berarti seluruh sinyal, atau indeks genvar yang belum selesai.
    pub slice: Option<Slice>,
    /// Irisan tambahan untuk seleksi bertingkat, misal `[0]` pada `y[i][0]`.
    pub extra_slices: Vec<Slice>,
    /// Segmen path hierarkis setelah titik, misal `["h2d"]` pada `tl_if.h2d`.
    /// Kosong bila target adalah sinyal polos.
    pub members: Vec<String>,
    /// Nama genvar pada `nama[genvar]`, misal `y[i]` di dalam loop generate.
    ///
    /// Nilai genvar baru diketahui saat elaborasi, sehingga irisan disimpan
    /// terpisah dan diselesaikan menjadi `slice` pada saat itu.
    pub genvar_index: Option<String>,
    /// Indexed part-select `[base +: width]` / `[base -: width]` pada LHS.
    /// Diisi bila target assignment pakai indexed part-select.
    pub part_dinamis: Option<PartDinamis>,
    pub span: Span,
}

impl Lvalue {
    /// Target tanpa irisan, misal `y` pada `assign y = a;`.
    pub fn simple(name: impl Into<String>, span: Span) -> Self {
        Self {
            name: name.into(),
            slice: None,
            extra_slices: Vec::new(),
            members: Vec::new(),
            genvar_index: None,
            part_dinamis: None,
            span,
        }
    }

    /// Target dengan irisan, misal `y[3:0]` atau `y[0]`.
    pub fn sliced(name: impl Into<String>, slice: Slice, span: Span) -> Self {
        Self {
            name: name.into(),
            slice: Some(slice),
            extra_slices: Vec::new(),
            members: Vec::new(),
            genvar_index: None,
            part_dinamis: None,
            span,
        }
    }

    /// Target berindeks genvar, misal `y[i]` di dalam loop generate.
    pub fn dengan_genvar(name: impl Into<String>, genvar: impl Into<String>, span: Span) -> Self {
        Self {
            name: name.into(),
            slice: None,
            extra_slices: Vec::new(),
            members: Vec::new(),
            genvar_index: Some(genvar.into()),
            part_dinamis: None,
            span,
        }
    }

    pub fn is_sliced(&self) -> bool {
        self.slice.is_some() || self.part_dinamis.is_some()
    }

    /// Ganti indeks genvar menjadi irisan konkret.
    ///
    /// Dipanggil elaborator saat nilai genvar sudah diketahui. Mengembalikan
    /// `None` bila target ini tidak memakai indeks genvar.
    pub fn resolve_genvar(&mut self, nilai: u32) -> Option<()> {
        self.genvar_index.take()?;
        self.slice = Some(Slice {
            msb: nilai,
            lsb: nilai,
        });
        Some(())
    }

    /// Ubah nama target sambil mempertahankan irisan dan indeks genvar.
    pub fn dengan_nama(&self, nama: impl Into<String>) -> Self {
        let mut hasil = self.clone();
        hasil.name = nama.into();
        hasil
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lvalue_tanpa_irisan() {
        let l = Lvalue::simple("y", Span::dummy());
        assert_eq!(l.name, "y");
        assert!(!l.is_sliced());
    }

    #[test]
    fn lvalue_dengan_irisan() {
        let l = Lvalue::sliced("y", Slice { msb: 7, lsb: 4 }, Span::dummy());
        assert!(l.is_sliced());
        assert_eq!(l.slice.unwrap().width(), 4);
    }

    #[test]
    fn lebar_irisan_satu_bit() {
        let l = Lvalue::sliced("y", Slice { msb: 3, lsb: 3 }, Span::dummy());
        assert_eq!(l.slice.unwrap().width(), 1);
    }

    #[test]
    fn lvalue_tetap_bisa_dibandingkan() {
        // Dipakai sebagai kunci hashMap saat instans module di-flatten.
        let a = Lvalue::simple("y", Span::dummy());
        let b = Lvalue::sliced("y", Slice { msb: 3, lsb: 0 }, Span::dummy());
        assert_ne!(a, b);
    }

    // --- indeks genvar pada LHS ---

    #[test]
    fn indeks_genvar_disimpan_terpisah_dari_irisan() {
        let l = Lvalue::dengan_genvar("y", "i", Span::dummy());
        assert_eq!(l.genvar_index.as_deref(), Some("i"));
        // Belum ada irisan konkret sampai genvar resolved.
        assert!(!l.is_sliced());
    }

    #[test]
    fn resolve_genvar_menjadi_bit_select() {
        let mut l = Lvalue::dengan_genvar("y", "i", Span::dummy());
        l.resolve_genvar(3);
        assert_eq!(l.genvar_index, None);
        assert_eq!(l.slice.unwrap(), Slice { msb: 3, lsb: 3 });
    }

    #[test]
    fn resolve_genvar_pada_target_biasa_berarti_tidak_ada() {
        let mut l = Lvalue::simple("y", Span::dummy());
        assert_eq!(l.resolve_genvar(2), None);
        assert!(l.slice.is_none());
    }

    #[test]
    fn dengan_nama_mempertahankan_indeks_genvar() {
        let l = Lvalue::dengan_genvar("y", "i", Span::dummy()).dengan_nama("blk_y");
        assert_eq!(l.name, "blk_y");
        assert_eq!(l.genvar_index.as_deref(), Some("i"));
    }
}
