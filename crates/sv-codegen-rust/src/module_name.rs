// Tanggung jawab: nama module Rust untuk wrapper satu design hasil elaborasi.
//
// Setiap design ditulis sebagai `mod <nama> { ... }` supaya beberapa modul
// root (LRM §23.1) bisa hidup dalam satu program tanpa bentrok di lingkup atas.
// Nama modul SV bebas, jadi nama Rust yang dihasilkan harus:
// - bukan kata kunci Rust (`module loop;` sah di SV, `mod loop` tidak sah di Rust),
// - tidak menabrak crate yang diimpor (`sv_runtime`, `sv_trace`),
// - unik walau dua nama SV berbeda menjadi identik setelah normalisasi
//   (`foo_bar` dan `FOO_BAR` sama-sama jadi `foo_bar`).
use std::collections::HashSet;

/// Awalan yang tidak mungkin menjadi nama modul SV_hasil normalisasi.
const AWALAN: &str = "sv_design_";

/// Nama module Rust untuk satu design, tanpa penanganan tabrakan.
pub fn module_name(nama: &str) -> String {
    let dasar: String = nama
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '_')
        .map(|c| c.to_ascii_lowercase())
        .collect();
    let dasar = if dasar.is_empty() {
        "m"
    } else {
        dasar.as_str()
    };
    format!("{AWALAN}{dasar}")
}

/// Berikan nama module unik untuk setiap design.
///
/// Dua nama SV bisa berbeda tapi menjadi sama setelah normalisasi
/// (`foo_bar` vs `FOO_BAR`); tanpa sufiks, kode hasil generate gagal dengan
/// `the name X is defined multiple times`. Kestabilan urutan penting supaya
/// hasil generate sama persis antar run.
pub fn module_names_unik(nama_design: &[String]) -> Vec<String> {
    let mut dipakai: HashSet<String> = HashSet::new();
    let mut out = Vec::with_capacity(nama_design.len());
    for nama in nama_design {
        let dasar = module_name(nama);
        if dipakai.insert(dasar.clone()) {
            out.push(dasar);
            continue;
        }
        let mut suffix = 2;
        loop {
            let kandidat = format!("{dasar}_{suffix}");
            if dipakai.insert(kandidat.clone()) {
                out.push(kandidat);
                break;
            }
            suffix += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kata_kunci_rust_boleh_jadi_nama_modul() {
        // `module loop;` sah di SystemVerilog; `mod loop` ditolak rustc.
        let nama = module_name("loop");
        assert!(!is_kata_kunci(&nama));
        assert_eq!(nama, "sv_design_loop");
    }

    #[test]
    fn crate_runtime_tidak_ditabrak_awalan() {
        assert_ne!(module_name("sv_runtime"), "sv_runtime");
    }

    #[test]
    fn tabrakan_normalisasi_diberi_sufiks() {
        let nama = module_names_unik(&[
            "foo_bar".to_string(),
            "FOO_BAR".to_string(),
            "foo-bar".to_string(),
        ]);
        assert_eq!(nama.len(), 3, "nama harus unik: {nama:?}");
        let mut set: HashSet<&str> = HashSet::new();
        for n in &nama {
            assert!(set.insert(n.as_str()), "nama ganda: {n}");
        }
    }

    #[test]
    fn nama_kosong_tidak_menghasilkan_mod_kosong() {
        assert!(!module_name("---").is_empty());
    }

    fn is_kata_kunci(nama: &str) -> bool {
        KATA_KUNCI.contains(&nama)
    }

    const KATA_KUNCI: [&str; 39] = [
        "as", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern", "false",
        "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub",
        "ref", "return", "self", "Self", "static", "struct", "super", "trait", "true", "type",
        "unsafe", "use", "where", "while", "async", "await", "abstract",
    ];
}
