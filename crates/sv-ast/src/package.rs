// Tanggung jawab: deklarasi paket SystemVerilog dan parameter paketnya.
use serde::{Deserialize, Serialize};
use crate::typedef::TypeDefDecl;
use crate::width::LocalParamDecl;
use sv_lexer::span::Span;

/// Deklarasi satu paket `package Nama ... endpackage` (LRM §26).
///
/// Hanya parameter dan `typedef` paket yang dikumpulkan. Parameter paket
/// (misal `rram_ctrl_pkg::TotalDataPages`) perlu tersedia saat evaluasi
/// ekspresi generate. `typedef` paket (misal `prim_alert_pkg::alert_tx_t`,
/// sebuah `struct packed`) perlu di-resolve jadi lebar sinyal konkret saat
/// sebuah deklarasi memakai tipe berkualifikasi paket.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PackageDecl {
    pub name: String,
    pub span: Span,
    /// Parameter paket sebagai ekspresi (belum dievaluasi). Urutan deklarasi
    /// dipakai karena parameter boleh merujuk parameter yang dideklarasikan
    /// sebelumnya.
    pub params: Vec<LocalParamDecl>,
    /// `typedef` di dalam paket; namanya akan dikualifikasi `pkg::nama` saat
    /// digabung ke daftar typedef global.
    pub typedefs: Vec<TypeDefDecl>,
}
