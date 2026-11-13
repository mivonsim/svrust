// Tanggung jawab: node AST untuk target assignment (sinyal atau irisannya).
use crate::expression::Expr;
use crate::width::WidthExpr;
use serde::{Deserialize, Serialize};
use sv_lexer::span::Span;

/// Irisan bit target: `[i]` untuk bit-select, `[msb:lsb]` untuk part-select.
///
/// `msb` dan `lsb` boleh literal atau ekspresi parameter (misal `[W-1:0]`).
/// Ekspresi non-literal disimpan sebagai `WidthExpr::Expr` dan di-resolve
/// saat elaborasi.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Slice {
    pub msb: WidthExpr,
    pub lsb: WidthExpr,
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
    /// Lebar irisan dalam bit. Hanya bisa dihitung bila kedua batas literal.
    pub fn width(&self) -> Option<u64> {
        match (&self.msb, &self.lsb) {
            (WidthExpr::Literal(m), WidthExpr::Literal(l)) => Some(*m as u64 - *l as u64 + 1),
            _ => None,
        }
    }

    /// Coba ubah jadi `Slice` dengan `u32` bila kedua batas literal.
    /// Digunakan elaborator setelah ekspresi parameter di-resolve.
    pub fn as_literal(self) -> Option<(u32, u32)> {
        match (&self.msb, &self.lsb) {
            (WidthExpr::Literal(m), WidthExpr::Literal(l)) => Some((*m as u32, *l as u32)),
            _ => None,
        }
    }
}

/// Sisi kiri assignment: nama sinyal dengan irisan bit opsional dan path
/// hierarkis melalui titik, misal `tl_if.h2d` atau `adc_chn_val_o[0].de`.
///
/// `name` adalah nama dasar, `slice` irisan pertama pada nama dasar,
/// `extra_slices` irisan bertingkat, `members` segmen path setelah titik
/// (misal `["h2d"]` pada `tl_if.h2d`), `genvar_index` indeks genvar.
///
/// `part_dinamis` diisi bila target assignment pakai indexed part-select.
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
            msb: WidthExpr::Literal(nilai as usize),
            lsb: WidthExpr::Literal(nilai as usize),
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
