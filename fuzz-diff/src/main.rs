// Tanggung jawab: runner pembanding multimah/oracle dengan klasifikasi
// temuan.
//
// Prinsip: **SVRust yang berbeda dengan satu simulator lain BUKAN bukti bahwa
// SVRust salah.** Kalau desainnya `SVRust != Icarus -> SVRust bug`, maka
// setiap keterbatasan Icarus akan dilaporkan sebagai bug SVRust. Kalau
// SVRust dan Icarus sama-sama salah, fuzzer akan menganggapnya benar.
//
// Jadi runner ini menjalankan N simulator dan mengklasifikasikan hasilnya:
//
//   COCOK              semua oracle sepakat → program ini benar secara consensual
//   BUG_SVRUST         SVRust berbeda dari KEDUA acuan → kandidat bug SVRust
//   ACAKAN_Iverilog    SVRust == Verilator, tapi Icarus beda → kemungkinan besar limitasi Icarus
//   TIGA_BEDA          ketiga simulator saling berbeda → perlu triase manual
//   TIDAK_DIDUKUNG     simulator acuan menolak program → bukan temuan
//
// Verilator sengaja memakai `--binary` sehingga menghasilkan simulator native;
// kompilasinya lambat (±detik), jadi dipakai sebagai oracle pihak ketiga yang
// Deliberate, bukan di setiap iterasi.
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Klasifikasi {
    Cocok,
    /// SVRust berbeda dari kedua acuan.
    BugSvrust,
    /// SVRust sama dengan Verilator, Icarus yang beda.
    KemungkinanIcarus,
    /// Ketiga simulator berbeda.
    TigaBeda,
    /// Simulator acuan menolak program: fitur belum didukung di sana.
    TidakDidukung,
    /// Hanya satu simulator yang punya jawaban.
    TidakCukup,
}

impl Klasifikasi {
    fn nama(self) -> &'static str {
        match self {
            Klasifikasi::Cocok => "COCOK",
            Klasifikasi::BugSvrust => "BUG_SVRUST",
            Klasifikasi::KemungkinanIcarus => "ACAKAN_Iverilog",
            Klasifikasi::TigaBeda => "TIGA_BEDA",
            Klasifikasi::TidakDidukung => "TIDAK_DIDUKUNG",
            Klasifikasi::TidakCukup => "TIDAK_CUKUP",
        }
    }

    /// Hanya dua kelas yang layak ditindaklanjuti sebagai bug SVRust.
    fn sb_penjelasan(self) -> bool {
        matches!(self, Klasifikasi::BugSvrust | Klasifikasi::TigaBeda)
    }
}

fn dir_kerja() -> PathBuf {
    let d = PathBuf::from("fuzz-diff/artifacts");
    std::fs::create_dir_all(&d).expect("buat direktori artefak");
    d
}

fn main() {
    let mulai: u64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    let jumlah: u64 = std::env::args()
        .nth(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(50);
    let pakai_verilator = std::env::var("SV_FUZZ_VERILATOR").is_ok();

    let dir = dir_kerja();
    let mut hitung = [0usize; 6];
    let mut temuan: Vec<(u64, Klasifikasi, String)> = Vec::new();

    for seed in mulai..mulai + jumlah {
        let src = sv_fuzzkit::acak(seed).render();
        let sv = jalankan_svrust(&src);
        let iv = jalankan_iverilog(&dir, seed, &src);
        let vl = if pakai_verilator {
            jalankan_verilator(&dir, seed, &src)
        } else {
            None
        };

        // Tanpa jawaban dari SVRust sendiri, program tidak bisa dinilai.
        let Some(sv) = sv else {
            hitung[Klasifikasi::TidakCukup as usize] += 1;
            continue;
        };
        let sv_map = petakan(&sv);

        let (klas, laporan) = klasifikasi(&sv_map, iv.as_deref(), vl.as_deref());
        hitung[klas as usize] += 1;
        if klas.sb_penjelasan() {
            let nama = dir.join(format!("seed{seed}.sv"));
            let _ = std::fs::write(&nama, &src);
            let isi = format!(
                "klasifikasi: {}\nprogram: {}\n{laporan}",
                klas.nama(),
                nama.display()
            );
            let _ = std::fs::write(dir.join(format!("seed{seed}.txt")), &isi);
            temuan.push((seed, klas, laporan));
        }
    }

    println!("seed {mulai}..{}", mulai + jumlah);
    for k in [
        Klasifikasi::Cocok,
        Klasifikasi::BugSvrust,
        Klasifikasi::KemungkinanIcarus,
        Klasifikasi::TigaBeda,
        Klasifikasi::TidakDidukung,
        Klasifikasi::TidakCukup,
    ] {
        println!("  {:<20} {}", k.nama(), hitung[k as usize]);
    }
    if temuan.is_empty() {
        println!("tidak ada temuan yang layak ditindaklanjuti");
    } else {
        println!("\ntemuan:");
        for (seed, k, laporan) in &temuan {
            println!("--- seed {seed}: {} ---", k.nama());
            println!("{laporan}");
        }
    }
}

/// Bandingkan peta keluaran ketiga simulator.
fn klasifikasi(
    sv: &BTreeMap<String, String>,
    iv: Option<&str>,
    vl: Option<&str>,
) -> (Klasifikasi, String) {
    let iv = iv.map(petakan);
    let vl = vl.map(petakan);

    // Simulator acuan yang menolak program: fitur belum didukung di sana, jadi
    // ini BUKAN temuan SVRust.
    if iv.is_none() && vl.is_none() {
        return (Klasifikasi::TidakDidukung, String::new());
    }

    let mut laporan = String::new();
    let mut beda_sv_iv = 0usize;
    let mut beda_sv_vl = 0usize;
    let mut beda_iv_vl = 0usize;
    let mut cocok = 0usize;
    let mut cocok_count = 0usize;

    let semua_kunci: std::collections::BTreeSet<&String> = sv
        .keys()
        .chain(iv.as_ref().map(|m| m.keys()).into_iter().flatten())
        .chain(vl.as_ref().map(|m| m.keys()).into_iter().flatten())
        .collect();

    for k in semua_kunci {
        let a = sv.get(k);
        let b = iv.as_ref().and_then(|m| m.get(k));
        let c = vl.as_ref().and_then(|m| m.get(k));
        if a.is_some() && b == a && (c.is_none() || c == a) {
            cocok_count += 1;
            continue;
        }
        match (a, b, c) {
            (Some(x), Some(y), _) if x != y => {
                beda_sv_iv += 1;
                laporan.push_str(&format!(
                    "  {k}: SVRust={x} Icarus={y}{}\n",
                    c.map(|z| format!(" Verilator={z}")).unwrap_or_default()
                ));
            }
            (Some(x), _, Some(z)) if x != z => {
                beda_sv_vl += 1;
                laporan.push_str(&format!(
                    "  {k}: SVRust={x} Verilator={z}{}\n",
                    b.map(|y| format!(" Icarus={y}")).unwrap_or_default()
                ));
            }
            (Some(x), Some(y), Some(z)) => {
                beda_iv_vl += 1;
                laporan.push_str(&format!("  {k}: SVRust={x} Icarus={y} Verilator={z}\n"));
            }
            _ => {}
        }
    }
    let _ = &mut cocok;

    let klas = if beda_iv_vl > 0 {
        Klasifikasi::TigaBeda
    } else if beda_sv_iv > 0 && beda_sv_vl > 0 {
        Klasifikasi::BugSvrust
    } else if beda_sv_vl == 0 && beda_sv_iv > 0 {
        // SVRust sama dengan Verilator (atau Verilator tidak punya jawaban).
        Klasifikasi::KemungkinanIcarus
    } else if beda_sv_iv == 0 && beda_sv_vl > 0 {
        // Icarus setuju dengan SVRust, Verilator yang beda — tetap triase.
        Klasifikasi::KemungkinanIcarus
    } else if beda_sv_iv == 0 && beda_sv_vl == 0 {
        Klasifikasi::Cocok
    } else {
        Klasifikasi::TidakCukup
    };
    (klas, laporan)
}

/// Ubah baris keluaran jadi peta `label -> nilai`.
fn petakan(keluar: &str) -> BTreeMap<String, String> {
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

fn jalankan_svrust(src: &str) -> Option<String> {
    let f = PathBuf::from(format!("/tmp/sv-fuzz-diff-{}.sv", std::process::id()));
    std::fs::write(&f, src).ok()?;
    let out = Command::new("cargo")
        .args([
            "run",
            "-q",
            "-p",
            "cargo-sv",
            "--",
            "sv",
            "run",
            "--rtl",
            f.to_str()?,
            "--top",
            "fz",
            "--steps",
            "40",
        ])
        .current_dir(akar_repo())
        .output()
        .ok()?;
    let _ = std::fs::remove_file(&f);
    let teks = String::from_utf8_lossy(&out.stdout).into_owned();
    if !out.status.success() || teks.is_empty() {
        return None;
    }
    Some(teks)
}

fn jalankan_iverilog(dir: &Path, seed: u64, src: &str) -> Option<String> {
    let sv = dir.join(format!("iv{seed}.sv"));
    let bin = dir.join(format!("iv{seed}"));
    std::fs::write(&sv, src).ok()?;
    let out = Command::new("iverilog")
        .args(["-g2012", "-o", bin.to_str()?, sv.to_str()?])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let out = Command::new(bin).output().ok()?;
    let teks = String::from_utf8_lossy(&out.stdout).into_owned();
    if teks.trim().is_empty() {
        return None;
    }
    Some(teks)
}

fn jalankan_verilator(dir: &Path, seed: u64, src: &str) -> Option<String> {
    let sv = dir.join(format!("vl{seed}.sv"));
    let build = dir.join(format!("vlbuild{seed}"));
    std::fs::write(&sv, src).ok()?;
    std::fs::create_dir_all(&build).ok()?;
    let out = Command::new("verilator")
        .args([
            "--binary",
            "-Wno-fatal",
            "--timing",
            "-o",
            "sim",
            "--Mdir",
            build.to_str()?,
            sv.to_str()?,
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let bin = build.join("sim");
    let out = Command::new(bin).output().ok()?;
    // Verilator menulis pesan `$finish` ke stderr; stdout hanya berisi
    // `$display`.
    let teks = String::from_utf8_lossy(&out.stdout).into_owned();
    if teks.trim().is_empty() {
        return None;
    }
    Some(teks)
}

fn akar_repo() -> PathBuf {
    PathBuf::from("..")
}
