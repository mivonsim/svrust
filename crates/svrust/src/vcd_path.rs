// Tanggung jawab: penentuan berkas VCD untuk setiap modul root (LRM §23.2).
//
// Setiap modul root adalah simulasi sendiri, jadi `$dumpvars` pada salah
// satunya tidak boleh mengabaikan modul lain: memakai path dari root pertama
// saja membuat VCD root kedua hilang tanpa pesan.
use crate::pipeline::{absolutize, resolve_vcd, BAWAAN_VCD};
use std::path::{Path, PathBuf};
use sv_ir::Design;

/// Path VCD untuk tiap modul root, urut sama dengan `designs`.
///
/// Aturannya mengikuti LRM §23.2 dan `resolve_vcd`: SV menang atas `--vcd`, dan
/// `--vcd` menang atas nama bawaan. Dua root yang tanpa `$dumpfile` memakai
/// `--vcd` yang sama akan jatuh pada nama yang sama, jadi suffix per modul
/// ditambahkan supaya keduanya benar-benar terekam — tanpa itu file kedua
/// menimpa file pertama.
pub fn vcd_paths(designs: &[Design], cli: Option<&Path>) -> Result<Vec<Option<PathBuf>>, String> {
    let mut paths: Vec<Option<PathBuf>> = Vec::with_capacity(designs.len());
    for design in designs {
        paths.push(resolve_vcd(design, cli)?);
    }
    let mut dipakai: Vec<PathBuf> = Vec::new();
    for (idx, path) in paths.iter_mut().enumerate() {
        let Some(p) = path.clone() else { continue };
        if dipakai.contains(&p) && designs.len() > 1 {
            let stem = p
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(BAWAAN_VCD)
                .to_string();
            *path = Some(p.with_file_name(format!("{stem}_{}", designs[idx].name)));
            continue;
        }
        dipakai.push(p);
    }
    Ok(paths)
}

/// Path absolut dari setiap path VCD yang dipakai.
pub fn absolut_paths(paths: &[Option<PathBuf>]) -> Vec<Option<PathBuf>> {
    paths
        .iter()
        .map(|p| {
            p.as_ref()
                .map(|p| absolutize(p).expect("cwd tidak terbaca"))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn design_tanpa_dumpvars_tidak_minta_berkas() {
        let design = Design::new("m");
        let paths = vcd_paths(&[design], None).expect("paths");
        assert_eq!(paths.len(), 1);
    }

    /// BUG: semua modul root memakai path VCD dari root pertama, jadi
    /// `$dumpvars` di root kedua diabaikan dan tidak ada berkas sama sekali.
    #[test]
    fn bug_dumpvars_di_root_kedua_tidak_diabaikan() {
        // Dua design; keduanya tanpa informasi `$dumpfile` (dipalsukan dengan
        // nama default), tapi `--vcd` hanya berlaku bila SV tidak memanggil
        // `$dumpvars` — di sini keduanya tidak, jadi keduanya dapat path CLI.
        let designs = vec![Design::new("satu"), Design::new("dua")];
        let paths = vcd_paths(&designs, Some(Path::new("kel.vcd"))).expect("paths");
        assert_eq!(paths.len(), 2);
        // Path yang sama dipecah jadi dua berkas, bukan saling menimpa.
        let unique: Vec<&PathBuf> = paths.iter().flatten().collect();
        assert_eq!(unique.len(), 2, "path: {unique:?}");
        assert_ne!(unique[0], unique[1], "kedua design menulis berkas sama");
    }
}
