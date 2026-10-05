// Tanggung jawab: implementasi seluruh mode oracle.
//
// Semua mode menerima byte yang sudah dipisahkan dari byte mode, lalu
// membangkitkan program dan memeriksa invarian masing-masing. Satu bin untuk
// semuanya supaya korpus dan cakupan oracle berbagi.
use sv_fuzzkit::pipeline;

use crate::oracle::Violasi;

/// Bentuk program dari byte; `None` kalau byte tidak cukup.
fn program(data: &[u8]) -> Option<sv_fuzzkit::Program> {
    sv_fuzzkit::dari_data(data)
}

fn gabung(mut a: Violasi, b: Violasi) -> Violasi {
    match (a, b) {
        (None, x) | (x, None) => x,
        (Some(x), Some(y)) => Some(format!("{x}\n{y}")),
    }
}

/// `Laporan` dari `sv-fuzzkit` diratakan jadi `Violasi` (pesan pertama saja):
/// satu program yang melanggar beberapa aturan tetap cukup untuk menghasilkan
/// satu artifact.
fn pertama(l: sv_fuzzkit::Laporan) -> Violasi {
    l.pesan.into_iter().next()
}

// --- mode 0: parse -------------------------------------------------------

pub fn parse(data: &[u8]) -> Violasi {
    // Jalur 1: byte mentah sebagai sumber SV — tokenizer harus menolak input
    // rusak dengan rapi, bukan crash.
    let apa = String::from_utf8_lossy(data).into_owned();
    let mut v = pertama(sv_fuzzkit::cek_lexer_parser(&[apa]));
    // Jalur 2: program dari generator — jalur yang menyentuh elaborasi hanya
    // bisa dicapai lewat program yang kalau diparse berhasil.
    if let Some(p) = program(data) {
        v = gabung(v, pertama(sv_fuzzkit::cek_lexer_parser(&[p.render()])));
    }
    v
}

// --- mode 1: preprocess --------------------------------------------------

pub fn preprocess(data: &[u8]) -> Violasi {
    let Some(p) = program(data) else {
        return None;
    };
    let src = p.render();
    let hasil = std::panic::catch_unwind(|| sv_fuzzkit::preprocess_sv(&src));
    if let Err(_) = hasil {
        return Some("praprosesan panic".to_string());
    }
    // Praprosesan boleh menolak directive yang tidak lengkap; yang禁止 adalah
    // panic dan ledakan ekspansi makro. Ukuran keluaran dibandingkan dengan
    // masukan sebagai pagar kasar.
    if let Ok(keluar) = sv_fuzzkit::preprocess_sv(&src) {
        if keluar.len() > src.len() * 64 + 4096 {
            return Some(format!(
                "ekspansi preprocessor tak terikat: {} byte -> {} byte",
                src.len(),
                keluar.len()
            ));
        }
    }
    None
}

// --- mode 2: elaborate ----------------------------------------------------

pub fn elaborate(data: &[u8]) -> Violasi {
    let Some(p) = program(data) else {
        return None;
    };
    let src = p.render();
    let h = pipeline::jalankan(&src);
    if let Some(p) = &h.panic {
        return Some(format!("PANIC di elaborator: {p}\n--- sumber ---\n{src}"));
    }
    if let Some(e) = &h.elaborate_error {
        // AGENTS.md aturan 3: error harus bisa di-trace ke sumber SV asli.
        if e.span.line < 1 || e.span.col < 1 {
            return Some(format!(
                "span tidak valid {:?} pada pesan '{}'",
                e.span, e.message
            ));
        }
        if e.span.start > e.span.end {
            return Some(format!("span terbalik {:?} pada pesan '{}'", e.span, e.message));
        }
        if e.span.end > src.len() {
            return Some(format!(
                "span melewati panjang sumber ({:?} > {}) pada pesan '{}'",
                e.span,
                src.len(),
                e.message
            ));
        }
    }
    None
}

// --- mode 3: codegen -----------------------------------------------------

pub fn codegen(data: &[u8]) -> Violasi {
    let Some(p) = program(data) else {
        return None;
    };
    let src = p.render();
    let h = pipeline::jalankan(&src);
    if h.panic.is_some() || h.codegen.is_none() {
        return None;
    }
    let code = h.codegen.as_ref().expect("codegen ada");
    if let Some(k) = pipeline::kurung_tidak_seimbang(code) {
        return Some(format!("kurung tak seimbang {k:?}\n--- kode ---\n{code}"));
    }
    for (nama, lebar_logis, max_width) in &h.design_lebar {
        if lebar_logis > max_width {
            return Some(format!(
                "sinyal '{nama}' lebar logis {lebar_logis} > MAX_WIDTH {max_width}"
            ));
        }
    }
    // I-4: bit-select/part-select selalu unsigned (LRM §11.8.1).
    if let Some(d) = &h.design {
        if let Some(p) = pipeline::seleksi_bertanda(&d.processes) {
            return Some(p);
        }
    }
    None
}

// --- mode 4: determinisme ------------------------------------------------

pub fn determinisme(data: &[u8]) -> Violasi {
    let Some(p) = program(data) else {
        return None;
    };
    let src = p.render();
    pertama(sv_fuzzkit::cek_determinisme(&[src]))
}

// --- mode 5: hierarki ----------------------------------------------------

pub fn hierarki(data: &[u8]) -> Violasi {
    let Some(g) = sv_fuzzkit::hier::dari_data(data) else {
        return None;
    };
    sv_fuzzkit::cek_hierarki(&g)
}

// --- mode 6: metamorphic -------------------------------------------------

pub fn metamorphic(data: &[u8]) -> Violasi {
    sv_fuzzkit::cek_metamorphic(data)
}