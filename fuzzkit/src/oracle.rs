// Tanggung jawab: oracle hierarki multi-file dan metamorphic.
//
// Dipisah dari `invariant` supaya file itu tetap satu tanggung jawab
// (invariants struktural pipeline), sementara ini mencakup pemeriksaan yang
// butuh program multi-file dan transformasi.
use std::collections::BTreeMap;
use std::panic::{catch_unwind, AssertUnwindSafe};

use crate::hier::Graph;
use crate::pipeline;

/// Lulus kalau program multi-file tidak panic, error-nya berbearer pesan, dan
/// hierarki yang sah tetap bisa dielaborasi.
pub fn cek_hierarki(g: &Graph) -> Option<String> {
    let dir = std::env::temp_dir().join(format!("sv-fuzzkit-hier-{}", std::process::id()));
    let berkas = g.tulis(&dir).ok()?;
    let top = g.nama_top();
    if top.is_empty() {
        return None;
    }
    let hasil = catch_unwind(AssertUnwindSafe(|| {
        let mut semua: Vec<sv_ast::module::Module> = Vec::new();
        for b in &berkas {
            let isi = match std::fs::read_to_string(dir.join(b)) {
                Ok(s) => s,
                Err(_) => return None,
            };
            let cpp = match sv_preprocessor::preprocess(&isi, None) {
                Ok(p) => p.source,
                Err(_) => return None,
            };
            let tokens = match sv_lexer::lex(&cpp) {
                Ok(t) => t,
                Err(_) => return None,
            };
            match sv_parser::parse_file(&tokens) {
                Ok(m) => semua.extend(m),
                Err(_) => return None,
            }
        }
        if semua.is_empty() {
            return None;
        }
        let mut design = sv_elaborator::elaborate_top(&semua, &top).ok()?;
        sv_opt::optimize(&mut design);
        let code = sv_codegen_rust::generate_module(&design);
        Some((design, code))
    }));
    let _ = std::fs::remove_dir_all(&dir);

    match hasil {
        Err(_) => Some("PANIC di elaborasi hierarki".to_string()),
        Ok(None) => None,
        Ok(Some((design, code))) => {
            if let Some(k) = pipeline::kurung_tidak_seimbang(&code) {
                return Some(format!("kurung tak seimbang pada kode hierarki {k:?}"));
            }
            let max_width = pipeline::max_width_dari_kode_publik(&code);
            for v in &design.variables {
                if v.data_type.width > max_width {
                    return Some(format!(
                        "sinyal '{}' lebar {} > MAX_WIDTH {max_width}",
                        v.name, v.data_type.width
                    ));
                }
            }
            if let Some(p) = pipeline::seleksi_bertanda(&design.processes) {
                return Some(p);
            }
            None
        }
    }
}

/// Relasi metamorphic yang diperiksa dari byte yang sama: program yang sama
/// harus menghasilkan design yang sama setelah transformasi yang dijaga LRM.
pub fn cek_metamorphic(data: &[u8]) -> Option<String> {
    let p = crate::Program::dari_bytes(data)?;
    let asli = p.render();
    let sebelum = bentuk(&asli);
    let Bentuk::Some(f) = &sebelum else {
        // Program ditolak: transformasi tidak boleh mengubah itu menjadi
        // crash, tapi tidak ada yang bisa dibandingkan.
        return None;
    };
    if f.starts_with("ERR:") {
        return None;
    }

    // Relasi 1 — dead code: deklarasi yang tidak pernah dibaca tidak boleh
    // mengubah design.
    let dengan_dead = sisip_dead_code(&asli);
    if bentuk(&dengan_dead) != sebelum {
        return Some(format!(
            "dead code mengubah design\n--- asli ---\n{asli}\n--- dengan dead code ---\n{dengan_dead}"
        ));
    }

    // Relasi 2 — konstanta ekuivalen.
    for (dari, ke) in [("4'd4", "4'b0100"), ("8'h0F", "8'b0000_1111")] {
        if !asli.contains(dari) {
            continue;
        }
        let lain = asli.replacen(dari, ke, 1);
        if bentuk(&lain) != sebelum {
            return Some(format!(
                "konstanta ekuivalen {dari} ≡ {ke} menghasilkan design berbeda"
            ));
        }
    }

    // Relasi 3 — parenthesisasi.
    if let Some(e) = cari_polos(&asli) {
        let lain = sisip_kurung(&asli, &e);
        if lain != asli && bentuk(&lain) != sebelum {
            return Some(format!(
                "parenthesisasi mengubah design\n--- tanpa ---\n{asli}\n--- dengan ---\n{lain}"
            ));
        }
    }
    None
}

/// Sidik jari perilaku design: **hanya daftar proses**, bukan daftar variabel.
///
/// Ini koreksi penting. Versi awal membandingkan seluruh design, sehingga
/// relasi "dead code tidak mengubah design" langsung gagal pada SEMUA program —
/// menambah `logic` memang menambah satu variabel ke design, jadi sidik jarinya
/// pasti berubah. Itu kelemahan oracle, bukan bug SVRust.
///
/// Yang benar yang dibandingkan adalah perilaku: proses apa yang dijalankan dan
/// apa yang masing-masing tulis. Daftar variabel bukan bagian observasi.
#[derive(Debug, PartialEq, Eq)]
enum Bentuk {
    Some(String),
    TidakBerlaku,
}

fn bentuk(src: &str) -> Bentuk {
    let h = pipeline::jalankan(src);
    if h.panic.is_some() {
        return Bentuk::TidakBerlaku;
    }
    if let Some(e) = h.elaborate_error {
        return Bentuk::Some(format!("ERR:{}", e.message));
    }
    match h.proses {
        Some(p) => Bentuk::Some(format!("{p:#?}")),
        None => Bentuk::TidakBerlaku,
    }
}

fn sisip_dead_code(src: &str) -> String {
    let pos = src.find("endmodule").unwrap_or(src.len());
    let lebar = cari_lebar_pertama(src).unwrap_or(8);
    format!(
        "{}  logic [{}:0] svr_dead_code;\n{}",
        &src[..pos],
        lebar - 1,
        &src[pos..]
    )
}

fn cari_lebar_pertama(src: &str) -> Option<u32> {
    src.lines().find_map(|l| {
        let pos = l.find('[')?;
        let isi = &l[pos + 1..];
        let titik = isi.find(':')?;
        let msb: u32 = isi[..titik].trim().parse().ok()?;
        Some(msb + 1)
    })
}

fn cari_polos(src: &str) -> Option<String> {
    for n in ["a", "b", "c"] {
        for pola in [format!("{n} + {n}"), format!("{n} + b")] {
            if src.contains(&pola) {
                return Some(pola);
            }
        }
    }
    None
}

fn sisip_kurung(src: &str, ekspresi: &str) -> String {
    let mut out = String::with_capacity(src.len() + 2);
    let mut sisa = src;
    let mut done = false;
    while let Some(i) = sisa.find(ekspresi) {
        let sebelum = sisa[..i].chars().next_back();
        let aman = !done && !matches!(sebelum, Some('(') | Some('\'') | Some('0'..='9'));
        out.push_str(&sisa[..i]);
        if aman {
            out.push('(');
            out.push_str(ekspresi);
            out.push(')');
            done = true;
        } else {
            out.push_str(ekspresi);
        }
        sisa = &sisa[i + ekspresi.len()..];
    }
    out.push_str(sisa);
    out
}

/// Peta `label -> nilai` dari baris keluaran — dipakai pembanding multimah.
pub fn petakan(keluar: &str) -> BTreeMap<String, String> {
    let mut m = BTreeMap::new();
    for baris in keluar.lines() {
        let baris = baris.trim();
        if baris.is_empty() {
            continue;
        }
        if let Some((label, nilai)) = baris.split_once(' ') {
            if label
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '[' || c == ']' || c == '_')
            {
                m.insert(label.to_string(), nilai.to_string());
                continue;
            }
        }
        m.insert(format!("baris{}", m.len()), baris.to_string());
    }
    m
}