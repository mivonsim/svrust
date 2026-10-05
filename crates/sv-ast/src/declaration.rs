// Tanggung jawab: node AST untuk deklarasi signal.
use crate::expression::Expr;
use crate::width::WidthExpr;
use sv_lexer::span::Span;

#[derive(Debug, Clone, PartialEq)]
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
    /// Dimensi unpacked (LRM §7.8), misal `logic [7:0] mem [0:15];`.
    ///
    /// Unpacked array bukan vektor: `$bits(mem)` mengembalikan lebar ELEMEN,
    /// bukan lebar total. Elemennya dipaketkan menjadi satu sinyal rata dengan
    /// lebar `lebar_elemen * size` saat elaborasi.
    pub unpacked: Option<UnpackedDim>,
    pub span: Span,
}

/// Dimensi unpacked satu tingkat (LRM §7.8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnpackedDim {
    /// Jumlah elemen.
    pub size: u64,
    /// Indeks PERTAMA yang tertulis di `[left:right]`, yaitu `left`.
    pub first: u64,
    /// `true` bila indeks menurun dari `first` (bentuk `[15:0]`), `false` bila
    /// menaik (`[0:15]`).
    pub descending: bool,
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
