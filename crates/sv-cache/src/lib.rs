// Tanggung jawab: content-addressed cache untuk parsing, elaboration, dan codegen.
// Cache disimpan ke disk di `target/sv/cache/` sehingga tetap hit antar panggilan
// CLI, asalkan konten sumber tidak berubah. Setiap entri diidentifikasi oleh
// hash SHA256 dari konten sumber + hash dependensi.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use sv_diag::Diagnostic;

/// Kunci cache: hash SHA256 dari konten sumber gabungan + hash dependensi
/// (macro, package, include). Perubahan satu file merubah hash → kunci baru →
/// cache miss untuk file itu saja.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
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

/// Hasil parsing yang di-cache. Tidak menyimpan `Module` (terlalu banyak tipe
/// nested untuk diserialisasi); cukup simpan diagnostik + apakah parsing sukses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedParse {
    pub diagnistik: Vec<Diagnostic>,
    pub sukses: bool,
}

/// Hasil elaboration yang di-cache. Tidak menyimpan `Design`; cukup simpan
/// diagnostik + apakah elaborasi sukses. Untuk `svrust check`, ini cukup.
/// Untuk `build`/`run`, codegen cache (terpisah) yang menyimpan Rust source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedElaborate {
    pub nama_modul: String,
    pub diagnistik: Vec<Diagnostic>,
    pub sukses: bool,
}

/// Hasil codegen yang di-cache. Menyimpan source Rust yang dihasilkan (String,
/// mudah diskarkan) per modul top.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedCodegen {
    pub kode_rust: String,
    pub nama_modul_rust: String,
}

/// Cache content-addressed: kunci hash → hasil parsing / elaboration / codegen.
/// Disimpan ke disk di `target/sv/cache/<jenis>/<kunci>.json`.
#[derive(Debug, Default)]
pub struct SvCache {
    /// Direktori root cache (diisi pada `muat()` atau `baru_dengan_root()`).
    root: Option<PathBuf>,
    parse_cache: HashMap<CacheKey, CachedParse>,
    elaborate_cache: HashMap<CacheKey, CachedElaborate>,
    codegen_cache: HashMap<CacheKey, CachedCodegen>,
    /// Hash dependensi per nama modul (package, macro, include yang digunakan).
    dependensi_hash: HashMap<String, String>,
}

impl SvCache {
    /// Buat cache baru (kosong, tidak diformkan ke disk sampai `simpan_*`).
    pub fn baru() -> Self {
        Self::default()
    }

    /// Buat cache dengan root direktori. Cache akan dimuat dari disk jika ada.
    pub fn baru_dengan_root(root: PathBuf) -> Result<Self, std::io::Error> {
        let mut cache = Self {
            root: Some(root.clone()),
            ..Default::default()
        };
        cache.muat_dari_disk()?;
        Ok(cache)
    }

    /// Muat semua cache dari disk.
    fn muat_dari_disk(&mut self) -> Result<(), std::io::Error> {
        let root = match &self.root {
            Some(r) => r.clone(),
            None => return Ok(()),
        };
        SvCache::muat_cache(
            &mut self.parse_cache,
            &root.join("cache").join("parse"),
        )?;
        SvCache::muat_cache(
            &mut self.elaborate_cache,
            &root.join("cache").join("elaborate"),
        )?;
        SvCache::muat_cache(
            &mut self.codegen_cache,
            &root.join("cache").join("codegen"),
        )?;
        Ok(())
    }

    /// Muat satu jenis cache dari disk ke `map`.
    fn muat_cache<T: for<'de> serde::Deserialize<'de>>(
        map: &mut HashMap<CacheKey, T>,
        dir: &Path,
    ) -> Result<(), std::io::Error> {
        if !dir.is_dir() {
            return Ok(());
        }
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let kunci = CacheKey(
                path.file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_string(),
            );
            let data = fs::read_to_string(&path)?;
            let nilai: T = serde_json::from_str(&data)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
            map.insert(kunci, nilai);
        }
        Ok(())
    }

    /// Simpan semua cache ke disk.
    pub fn simpan_ke_disk(&self) -> Result<(), std::io::Error> {
        let root = match &self.root {
            Some(r) => r.clone(),
            None => return Ok(()),
        };

        SvCache::simpan_satu(&self.parse_cache, "parse", &root)?;
        SvCache::simpan_satu(&self.elaborate_cache, "elaborate", &root)?;
        SvCache::simpan_satu(&self.codegen_cache, "codegen", &root)?;
        Ok(())
    }

    /// Simpan satu jenis cache ke disk.
    fn simpan_satu<T: serde::Serialize>(
        map: &HashMap<CacheKey, T>,
        jenis: &str,
        root: &Path,
    ) -> Result<(), std::io::Error> {
        let dir = root.join("cache").join(jenis);
        fs::create_dir_all(&dir)?;
        for (kunci, nilai) in map.iter() {
            let path = dir.join(format!("{}.json", kunci.0));
            let json = serde_json::to_string_pretty(nilai)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
            fs::write(&path, json)?;
        }
        Ok(())
    }

    /// Simpan hash dependensi untuk suatu modul.
    pub fn simpan_dependensi_hash(&mut self, nama_modul: &str, hash: &str) {
        self.dependensi_hash
            .insert(nama_modul.to_string(), hash.to_string());
    }

    /// Ambil hash dependensi untuk suatu modul.
    pub fn ambil_dependensi_hash(&self, nama_modul: &str) -> Option<&str> {
        self.dependensi_hash.get(nama_modul).map(|s| s.as_str())
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

    pub fn jumlah_elaborate(&self) -> usize {
        self.elaborate_cache.len()
    }

    pub fn jumlah_codegen(&self) -> usize {
        self.codegen_cache.len()
    }

    /// Kosongkan seluruh cache (memori dan disk).
    pub fn bersihkan(&mut self) {
        self.parse_cache.clear();
        self.elaborate_cache.clear();
        self.codegen_cache.clear();
        self.dependensi_hash.clear();
        if let Some(root) = &self.root {
            for jenis in ["parse", "elaborate", "codegen"] {
                let _ = fs::remove_dir_all(root.join("cache").join(jenis));
            }
        }
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

    fn diagnistik_contoh() -> Vec<Diagnostic> {
        vec![Diagnostic::error("E0001", "pesan contoh", sv_diag::Span::empty(sv_diag::SourceId(0), 0))]
    }

    #[test]
    fn cache_parse_hit_dan_miss() {
        let mut cache = SvCache::baru();
        let kunci = kunci_contoh();
        assert!(cache.cek_parse(&kunci).is_none(), "cache kosong harus miss");

        cache.simpan_parse(
            kunci.clone(),
            CachedParse {
                diagnistik: diagnistik_contoh(),
                sukses: true,
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
                nama_modul: "modul_uji".to_string(),
                diagnistik: diagnistik_contoh(),
                sukses: true,
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
        assert_ne!(
            k1, k2,
            "dependensi berbeda harus menghasilkan kunci berbeda"
        );
    }

    #[test]
    fn dependensi_hash_simpan_dan_ambil() {
        let mut cache = SvCache::baru();
        cache.simpan_dependensi_hash("root_a", "hash_abc");
        assert_eq!(cache.ambil_dependensi_hash("root_a"), Some("hash_abc"));
        assert_eq!(cache.ambil_dependensi_hash("root_b"), None);
    }

    #[test]
    fn diagnistik_default_kosong() {
        let d: Diagnostic = Diagnostic::new(Severity::Error, "E0001", "pesan contoh");
        assert_eq!(d.code, "E0001");
        assert_eq!(d.message, "pesan contoh");
    }
}
