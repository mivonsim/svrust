// Tanggung jawab: node AST untuk deklarasi signal.
use crate::expression::Expr;
use crate::width::WidthExpr;
use serde::{Deserialize, Serialize};
use sv_lexer::span::Span;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Declaration {
    pub name: String,
    pub width: WidthExpr,
    /// True bila ditulis `signed`; false berarti unsigned (default LRM §6.2.1).
    pub signed: bool,
    /// Nilai awal, misal `logic [7:0] s = 8'd5;` (LRM §6.2.2).
    ///
    /// Untuk tipe net berarti net declaration assign (driver kontinu), sedangkan
    /// untuk variabel berarti nilai awal pada waktu nol.
    pub init: Option<Expr>,
    /// Nama tipe dari `typedef` bila deklarasi memakainya, misal
    /// `byte_t data;`. `None` berarti tipe ditulis langsung (`logic [7:0]`).
    pub type_name: Option<String>,
    /// Dimensi unpacked (LRM §7.8), misal `logic [7:0] mem [0:15];` atau
    /// `logic [7:0] mem [0:3][0:15];`.
    ///
    /// Unpacked array bukan vektor: `$bits(mem)` mengembalikan lebar ELEMEN,
    /// bukan lebar total. Elemennya dipaketkan menjadi satu sinyal rata dengan
    /// lebar `lebar_elemen × size_1 × size_2 × ...`.
    pub unpacked: Option<UnpackedDims>,

    /// Field-field dalam `struct`/`union` packed (LRM §6.7, §8).
    /// `Some` bila deklarasi ini adalah struct/union, `None` untuk tipe biasa.
    pub struct_fields: Option<Vec<StructField>>,
    pub span: Span,
}

/// Dimensi unpacked satu tingkat (LRM §7.8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnpackedDim {
    /// Jumlah elemen.
    pub size: u64,
    /// Indeks PERTAMA yang tertulis di `[left:right]`, yaitu `left`.
    pub first: u64,
    /// `true` bila indeks menurun dari `first` (bentuk `[15:0]`), `false` bila
    /// menaik (`[0:15]`).
    pub descending: bool,
}

/// Dimensi unpacked multi-tingkat (LRM §7.8).
///
/// `logic [7:0] mem [0:3][0:15];` memiliki dua dimensi: 4 elemen di tingkat
/// pertama dan 16 elemen di tingkat kedua. Sinyal disimpan rata selebar
/// `lebar_elemen × size_1 × size_2 × ...`.
///
/// Indeks `mem[i][j]` menghasilkan offset bit:
///   `((i × size_2 + j) × lebar_elemen) .. (((i × size_2 + j + 1) × lebar_elemen) - 1)`
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnpackedDims {
    /// Dimensi dari luar ke dalam, misal untuk `[0:3][0:15]` ini
    /// `[UnpackedDim { size: 4, .. }, UnpackedDim { size: 16, .. }]`.
    pub dims: Vec<UnpackedDim>,
}

impl UnpackedDims {
    /// Buat dimensi unpacked satu tingkat.
    pub fn satu(dim: UnpackedDim) -> Self {
        Self { dims: vec![dim] }
    }

    /// Jumlah total elemen di semua dimensi.
    pub fn jumlah_elemen(&self) -> u64 {
        self.dims.iter().map(|d| d.size).product()
    }

    /// Hitung offset elemen dari indeks multi-tingkat, misal `mem[i][j]`.
    ///
    /// `indices` berisi indeks dari luar ke dalam. Mengembalikan `None` bila
    /// ada lebih banyak indeks dari dimensi. Indeks parsial (kurang dari
    /// jumlah dimensi) diizinkan: mengembalikan offset awal sub-array
    /// yang tersisa.
    pub fn offset(&self, indices: &[u64]) -> Option<u64> {
        if indices.len() > self.dims.len() {
            return None;
        }
        // perkalian = hasil kali ukuran SEMUA dimensi.
        // Untuk setiap indeks yang disediakan, bagi perkalian dengan ukuran
        // dimensi tersebut dan kalikan dengan indeksnya.
        let mut perkalian: u64 = self.dims.iter().map(|d| d.size).product();
        let mut jarak = 0u64;
        for (dim, &idx) in self.dims.iter().zip(indices.iter()) {
            perkalian = perkalian.checked_div(dim.size)?;
            let d = dim.offset(idx)?;
            jarak = jarak.checked_add(d.checked_mul(perkalian)?)?;
        }
        Some(jarak)
    }

    /// Bentuk penulisan yang setara, untuk pesan error.
    pub fn describe(&self) -> String {
        self.dims
            .iter()
            .map(UnpackedDim::describe)
            .collect::<Vec<_>>()
            .join(" ")
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StructField {
    pub name: String,
    pub width: WidthExpr,
    pub signed: bool,
}

impl UnpackedDim {
    /// Jarak elemen `index` dari elemen pertama, atau `None` bila di luar jangkauan.
    ///
    /// Jarak inilah yang jadi offset bit pada sinyal rata. Bentuk menurun
    /// (`[15:0]`) menulis elemen pertama di bit paling tinggi, jadi urutannya
    /// dibalik dibanding bentuk menaik.
    pub fn offset(&self, index: u64) -> Option<u64> {
        let jarak = if self.descending {
            self.first.checked_sub(index)?
        } else {
            index.checked_sub(self.first)?
        };
        (jarak < self.size).then_some(jarak)
    }

    /// Bentuk penulisan yang setara, untuk pesan error.
    ///
    /// Kedua arah menghasilkan bentuk `[first:akhir]` yang sama, jadi cukup
    /// satu jalur.
    pub fn describe(&self) -> String {
        let akhir = if self.descending {
            self.first.saturating_sub(self.size.saturating_sub(1))
        } else {
            self.first.saturating_add(self.size.saturating_sub(1))
        };
        format!("[{}:{}]", self.first, akhir)
    }
}
