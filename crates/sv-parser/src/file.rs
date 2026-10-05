// Tanggung jawab: belah token berkas menjadi daftar module.
use sv_ast::module::Module;
use sv_lexer::spanned::SpannedToken;
use sv_lexer::token::Token;

/// Parse seluruh token berkas menjadi daftar module.
/// Setiap irisan `module...endmodule` diparse terpisah.
///
/// Direktif `timescale` yang ditemukan di antara modul diterapkan ke
/// menyusulnya (LRM §21.8): satu direktif boleh berlaku untuk beberapa modul
/// dan bisa berubah di tengah berkas.
pub fn parse_file(tokens: &[SpannedToken]) -> Result<Vec<Module>, String> {
    let mut out = Vec::new();
    let mut idx = 0;
    let mut time_scale = sv_ast::time_scale::TimeScale::default();
    while idx < tokens.len() {
        // Direktif `timescale` berlaku untuk modul yang dibaca setelahnya.
        if let Token::Directive(nama) = &tokens[idx].token {
            if nama == "timescale" {
                let (dipakai, eaten) = parse_timescale(&tokens[idx + 1..])?;
                time_scale = dipakai;
                idx += 1 + eaten;
                continue;
            }
        }
        if tokens[idx].token != Token::Module {
            return Err(format!("expected 'module', dapat {:?}", tokens[idx].token));
        }
        let awal = idx;
        let mut akhir = None;
        for (rel, t) in tokens[idx..].iter().enumerate() {
            if t.token == Token::Endmodule {
                akhir = Some(idx + rel);
                break;
            }
        }
        let akhir = akhir.ok_or_else(|| "expected 'endmodule'".to_string())?;
        let mut modul = super::module::parse_module(&tokens[awal..=akhir])?;
        // LRM §21.8: `#n` tanpa satuan memakai `timeunit` modul, jadi satuan
        // `Bawaan` diselesaikan di sini — setelah `timescale` diketahui dan
        // sebelum modul masuk elaborator.
        modul.time_scale = time_scale;
        sv_ast::delay_unit::terapkan_module(&mut modul, time_scale);
        // Tolak nama module duplikat dalam satu berkas.
        if out.iter().any(|m: &Module| m.name == modul.name) {
            return Err(format!("module duplikat: {}", modul.name));
        }
        out.push(modul);
        idx = akhir + 1;
    }
    if out.is_empty() {
        return Err("berkas kosong: tidak ada module".to_string());
    }
    Ok(out)
}

/// Parse argumen `timescale` setelah token direktif: `1us/1ps`.
///
/// Lexer memindai `1us` sebagai `Number(1)` diikuti `Ident("us")`, jadi pola
/// tokennya `Number Ident Slash Number Ident`. Bentuk lain ditolak dengan
/// pesan, bukan diam-diam memakai satuan bawaan — `timescale` yang salah
/// membuat seluruh `#delay` modul bergeser.
fn parse_timescale(
    tokens: &[SpannedToken],
) -> Result<(sv_ast::time_scale::TimeScale, usize), String> {
    // AGENTS.md aturan 3: setiap error membawa posisi sumber. Token pertama
    // setelah `timescale` sudah punya span lexer, jadi pesan bisa menunjuk
    // baris/kolom yang benar alih-alih hanya nama direktif.
    let span = tokens.first().map(|t| t.span);
    let salah = |apa: &str| match span {
        Some(s) => format!(
            "`timescale: {apa}, diharapkan `1unit/1unit` (line {} col {})",
            s.line, s.col
        ),
        None => format!("`timescale: {apa}, diharapkan `1unit/1unit`"),
    };
    let angka = |i: usize| -> Result<u64, String> {
        match tokens.get(i).map(|t| &t.token) {
            Some(Token::Number(n)) => Ok(*n),
            _ => Err(salah("unit tanpa pengali 1")),
        }
    };
    let unit = |i: usize| -> Result<sv_ast::time_unit::TimeUnit, String> {
        match tokens.get(i).map(|t| &t.token) {
            Some(Token::Ident(nama)) => sv_ast::time_unit::TimeUnit::from_name(nama)
                .ok_or_else(|| salah("satuan tak dikenal")),
            _ => Err(salah("satuan hilang")),
        }
    };
    if tokens.len() < 5 || tokens[2].token != Token::Slash {
        return Err(salah("bentuk tidak lengkap"));
    }
    let pengali_unit = angka(0)?;
    let pengali_presisi = angka(3)?;
    let satuan_unit = unit(1)?;
    let satuan_presisi = unit(4)?;
    // LRM §21.8: `timeunit` harus >= `timeprecision`. Pesan khusus karena
    // "satuan tidak dikenal" akan menyesatkan — satuannya dikenal, hanya
    // urutannya terbalik.
    if satuan_presisi.femtos() > satuan_unit.femtos() {
        return Err(salah(&format!(
            "timeprecision {} lebih kasar dari timeunit {}",
            unit_text(satuan_presisi),
            unit_text(satuan_unit)
        )));
    }
    let teks = format!(
        "{pengali_unit}{}/{pengali_presisi}{}",
        unit_text(satuan_unit),
        unit_text(satuan_presisi)
    );
    let scale =
        sv_ast::time_scale::TimeScale::parse(&teks).ok_or_else(|| salah("satuan tidak dikenal"))?;
    Ok((scale, 5))
}

/// Teks satuan `TimeUnit` untuk menyusun ulang string `timescale`.
fn unit_text(unit: sv_ast::time_unit::TimeUnit) -> &'static str {
    use sv_ast::time_unit::TimeUnit::*;
    match unit {
        Seconds => "s",
        MilliSeconds => "ms",
        MicroSeconds => "us",
        NanoSeconds => "ns",
        PicoSeconds => "ps",
        FectoSeconds => "fs",
        Bawaan => "?",
    }
}
