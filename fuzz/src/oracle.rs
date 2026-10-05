// Tanggung jawab: check memusatkan — setiap aturan_oracle ditulis sebagai
// fungsi mode yang menerima byte dan mengembalikan pesan pelanggaran.
//
// Satu bin fuzz untuk semua mode: mode dipilih dari byte pertama, badan
// program dari sisanya. Alasannya bukan sekadar hemat effort — korpus jadi
// **berbagi**: program yang sanctity untuk `elaborate` otomatisdicoba juga
// oleh `codegen`, `determinisme`, dan `metamorphic`, sehingga cakupan oracle
// yang berbeda attainment terAkumulasi pada input yang sama.
use std::panic::{catch_unwind, AssertUnwindSafe};

/// Hasil pemeriksaan satu mode.
pub type Violasi = Option<String>;

/// Semua mode yang tersedia. Urutan menentukan angka pada pesan.
pub const MODE: &[&str] = &[
    "parse",         // 0 — lexer + parser tidak boleh panic
    "preprocess",    // 1 — directive + makro tidak boleh panic/eksplosif
    "elaborate",     // 2 — tidak panic; setiap error punya Span sah
    "codegen",       // 3 — kurung seimbang; lebar logis ≤ MAX_WIDTH
    "determinisme",  // 4 — pipeline compiler deterministik
    "hierarki",      // 5 — multi-file: rekursi tak terbatas, Span
    "metamorphic",   // 6 — transformasi menjaga design
];

/// Pilih mode dari byte pertama. Semua mode harus tetap bisaDieksekusi walau
/// hanya satu yang dipilih per input — byte pertama hanya memilih, tidak
/// membatasi.
pub fn pilih(data: &[u8]) -> usize {
    match data.first() {
        Some(b) => usize::from(*b) % MODE.len(),
        None => 0,
    }
}

/// Badan program (tanpa byte mode).
pub fn badan(data: &[u8]) -> &[u8] {
    if data.is_empty() {
        data
    } else {
        &data[1..]
    }
}

/// Jalankan mode yang dipilih. Mengembalikan pesan pelanggaran kalau ada.
pub fn jalankan(data: &[u8]) -> Violasi {
    let mode = pilih(data);
    let body = badan(data);
    let hasil = catch_unwind(AssertUnwindSafe(|| jalankan_mode(mode, body)));
    match hasil {
        Ok(v) => v.map(|p| format!("[{}] {p}", MODE[mode])),
        Err(payload) => {
            let pesan = if let Some(s) = payload.downcast_ref::<&str>() {
                (*s).to_string()
            } else if let Some(s) = payload.downcast_ref::<String>() {
                s.clone()
            } else {
                "<payload panic>".to_string()
            };
            Some(format!("[{}] PANIC: {pesan}", MODE[mode]))
        }
    }
}

/// Dispatch ke mode tertentu. `body` sudah tanpa byte mode.
pub fn jalankan_mode(mode: usize, body: &[u8]) -> Violasi {
    match mode {
        0 => mode::parse(body),
        1 => mode::preprocess(body),
        2 => mode::elaborate(body),
        3 => mode::codegen(body),
        4 => mode::determinisme(body),
        5 => mode::hierarki(body),
        _ => mode::metamorphic(body),
    }
}

#[path = "oracle_mode.rs"]
pub mod mode;