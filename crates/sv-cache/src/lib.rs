// Tanggung jawab: content-addressed cache untuk parsing, elaboration, dan codegen.
// Setiap entri diidentifikasi oleh hash SHA256 dari konten sumber + hash dependensi,
// sehingga perubahan satu file tidak memaksa reprocessing seluruh OpenTitan.
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

use sv_ast::module::Module;
use sv_diag::Diagnostic;
use sv_ir::Design;

/// Kunci cache: hash konten sumber + hash dependensi (macro, package, include).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CacheKey(pub String);

impl CacheKey {
    /// Buat kunci dari konten berkas dan hash dependensi.
    pub fn baru(konten: &str, dependensi_hash: &str) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(konten.as_bytes());
        hasher.update(b"|");
        hasher.update(dependensi_hash.as_bytes());
        let hasil = hasher.finalize();
        Self(format!("{:x}", hasil))
    }
}

/// Hasil parsing yang di-cache.
#[derive(Debug, Clone)]
pub struct CachedParse {
    pub modul: Module,
    pub diagnistik: Vec<Diagnostic>,
}

/// Hasil elaboration yang di-cache.
#[derive(Debug, Clone)]
pub struct CachedElaborate {
    pub design: Design,
    pub diagnistik: Vec<Diagnostic>,
}

/// Hasil codegen yang di-cache.
#[derive(Debug, Clone)]
pub struct CachedCodegen {
    pub kode_rust: String,
    pub nama_modul_rust: String,
}

/// Cache content-addressed: kunci hash → hasil parsing / elaboration / codegen.
#[derive(Debug, Default)]
pub struct SvCache {
    parse_cache: HashMap<CacheKey, CachedParse>,
    elaborate_cache: HashMap<CacheKey, CachedElaborate>,
    codegen_cache: HashMap<CacheKey, CachedCodegen>,
    /// Hash dependensi per nama modul (package, macro, include yang digunakan).
    dependensi_hash: HashMap<String, String>,
}

impl SvCache {
    /// Buat cache baru.
    pub fn baru() -> Self {
        Self::default()
    }

    /// Simpan hash dependensi untuk suatu modul.
    pub fn simpan_dependensi_hash(&mut self, nama_modul: &str, hash: &str) {
        self.dependensi_hash
            .insert(nama_modul.to_string(), hash.to_string());
    }

    /// Ambil hash dependensi untuk suatu modul.
    pub fn ambil_dependensi_hash(&self, nama_modul: &str) -> Option<&str> {
        self.dependensi_hash
            .get(nama_modul)
            .map(|s| s.as_str())
    }

    /// Cek apakah parsing untuk kunci sudah ada di cache.
    pub fn cek_parse(&self, kunci: &CacheKey) -> Option<&CachedParse> {
        self.parse_cache.get(kunci)
    }

    /// Simpan hasil parsing ke cache.
    pub fn simpan_parse(&mut self, kunci: CacheKey, hasil: CachedParse) {
        self.parse_cache.insert(kunci, hasil);
    }

    /// Cek apakah elaboration untuk kunci sudah ada di cache.
    pub fn cek_elaborate(&self, kunci: &CacheKey) -> Option<&CachedElaborate> {
        self.elaborate_cache.get(kunci)
    }

    /// Simpan hasil elaboration ke cache.
    pub fn simpan_elaborate(&mut self, kunci: CacheKey, hasil: CachedElaborate) {
        self.elaborate_cache.insert(kunci, hasil);
    }

    /// Cek apakah codegen untuk kunci sudah ada di cache.
    pub fn cek_codegen(&self, kunci: &CacheKey) -> Option<&CachedCodegen> {
        self.codegen_cache.get(kunci)
    }

    /// Simpan hasil codegen ke cache.
    pub fn simpan_codegen(&mut self, kunci: CacheKey, hasil: CachedCodegen) {
        self.codegen_cache.insert(kunci, hasil);
    }

    pub fn jumlah_parse(&self) -> usize {
    self.parse_cache.len()
}

/// Hitung jumlah entri cache elaboration.
pub fn jumlah_elaborate(&self) -> usize {
    self.elaborate_cache.len()
}

/// Hitung jumlah entri cache codegen.
pub fn jumlah_codegen(&self) -> usize {
    self.codegen_cache.len()
}

/// Kosongkan seluruh cache.
pub fn bersihkan(&mut self) {
    self.parse_cache.clear();
    self.elaborate_cache.clear();
    self.codegen_cache.clear();
    self.dependensi_hash.clear();
}

}

/// Hash konten berkas (sha256 hex).
pub fn hash_konten(path: &Path) -> Result<String, std::io::Error> {
    let konten = fs::read_to_string(path)?;
    let mut hasher = Sha256::new();
    hasher.update(konten.as_bytes());
    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sv_diag::{Diagnostic, Severity};

    fn kunci_contoh() -> CacheKey {
        CacheKey::baru("module m; endmodule", "deps:0")
    }

    #[test]
    fn cache_parse_hit_dan_miss() {
        let mut cache = SvCache::baru();
        let kunci = kunci_contoh();
        assert!(cache.cek_parse(&kunci).is_none(), "cache kosong harus miss");

        cache.simpan_parse(
            kunci.clone(),
            CachedParse {
                modul: Module {
                    name: "m".to_string(),
                    span: sv_lexer::span::Span::dummy(),
                    params: vec![],
                    ports: vec![],
                    declarations: vec![],
                    statements: vec![],
                    instances: vec![],
                    routines: vec![],
                    typedefs: vec![],
                    generates: vec![],
                    genvars: vec![],
                    localparams: vec![],
                    dpis: vec![],
                    time_scale: sv_ast::time_scale::TimeScale::default(),
                },
                diagnistik: vec![],
            },
        );
        assert!(
            cache.cek_parse(&kunci).is_some(),
            "cache setelah simpan harus hit"
        );
    }

    #[test]
    fn cache_elaborate_hit_dan_miss() {
        let mut cache = SvCache::baru();
        let kunci = CacheKey::baru("elaborate", "deps:0");
        assert!(cache.cek_elaborate(&kunci).is_none());

        cache.simpan_elaborate(
            kunci.clone(),
            CachedElaborate {
                design: Design {
                    name: "d".to_string(),
                    ..Default::default()
                },
                diagnistik: vec![],
            },
        );
        assert!(cache.cek_elaborate(&kunci).is_some());
    }

    #[test]
    fn cache_codegen_hit_dan_miss() {
        let mut cache = SvCache::baru();
        let kunci = CacheKey::baru("codegen", "deps:0");
        assert!(cache.cek_codegen(&kunci).is_none());

        cache.simpan_codegen(
            kunci.clone(),
            CachedCodegen {
                kode_rust: "fn main(){}".to_string(),
                nama_modul_rust: "sv_design_d".to_string(),
            },
        );
        assert!(cache.cek_codegen(&kunci).is_some());
    }

    #[test]
    fn kunci_berbeda_jika_konten_berbeda() {
        let k1 = CacheKey::baru("module a; endmodule", "deps:0");
        let k2 = CacheKey::baru("module b; endmodule", "deps:0");
        assert_ne!(k1, k2, "konten berbeda harus menghasilkan kunci berbeda");
    }

    #[test]
    fn kunci_berbeda_jika_dependensi_berbeda() {
        let k1 = CacheKey::baru("module m; endmodule", "deps:0");
        let k2 = CacheKey::baru("module m; endmodule", "deps:1");
        assert_ne!(k1, k2, "dependensi berbeda harus menghasilkan kunci berbeda");
    }

    #[test]
    fn dependensi_hash_simpan_dan_ambil() {
        let mut cache = SvCache::baru();
        cache.simpan_dependensi_hash("root_a", "hash_abc");
        assert_eq!(
            cache.ambil_dependensi_hash("root_a"),
            Some("hash_abc")
        );
        assert_eq!(cache.ambil_dependensi_hash("root_b"), None);
    }

    #[test]
    fn diagnistik_default_kosong() {
        let d: Diagnostic = Diagnostic::new(
            Severity::Error,
            "E0001",
            "pesan contoh",
        );
        assert_eq!(d.code, "E0001");
        assert_eq!(d.message, "pesan contoh");
    }
}
