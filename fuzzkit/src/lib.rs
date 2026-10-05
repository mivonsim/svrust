// Tanggung jawab: permukaan fuzzing SVRust (generator + invariant).
//
// `gen`      — generator program SystemVerilog berbasis data (tanpa template).
// `pipeline` — menjalankan pipeline SV → IR → Rust dengan panic capture.
// `invariant`— invariant SVRust yang diturunkan dari kelas bug yang benar-benar
//              terjadi di repo ini.
pub mod boundary;
pub mod gen;
pub mod hier;
pub mod invariant;
pub mod oracle;
pub mod pipeline;
pub mod sem;

pub use boundary::{Batas, Constness, DeklarasiSemantik, Interaksi, KelasLebar, LebarPool};
pub use gen::{acak, dari_data, Program};
pub use hier::{Graph as GraphModul};
pub use sem::{sidik_jari, Coverage, Fitur, InteraksiFitur};

use sv_preprocessor::preprocess;

/// Jalankan praprosesan dengan `include` dinonaktifkan lewat basis data kosong,
/// supaya fuzzing tidak menyentuh berkas di luar repo.
pub fn preprocess_sv(src: &str) -> Result<String, String> {
    preprocess(src, None).map(|p| p.source).map_err(|e| e.to_string())
}
pub use invariant::{cek_determinisme, cek_lexer_parser, cek_pipeline, Laporan};
pub use oracle::{cek_hierarki, cek_metamorphic, petakan};
pub use pipeline::{jalankan, Hasil};
