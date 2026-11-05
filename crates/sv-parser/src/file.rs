// Tanggung jawab: belah token berkas menjadi daftar module dan typedef global.
use sv_ast::module::Module;
use sv_ast::typedef::TypeDefDecl;
use sv_lexer::spanned::SpannedToken;
use sv_lexer::token::Token;

/// Hasil parse satu berkas: daftar module beserta `typedef` di luar module
/// (compilation-unit scope, LRM §8.20).
pub struct ParseResult {
    pub modules: Vec<Module>,
    pub typedefs_global: Vec<TypeDefDecl>,
}

/// Parse seluruh token berkas menjadi daftar module.
/// Setiap irisan `module...endmodule` diparse terpisah.
///
/// Direktif `timescale` yang ditemukan di antara modul diterapkan ke
/// menyusulnya (LRM §21.8): satu direktif boleh berlaku untuk beberapa modul
/// dan bisa berubah di tengah berkas.
///
/// `typedef` di luar module (antara modul atau sebelum modul pertama) dikumpulkan
/// sebagai compilation-unit scope (LRM §8.20) dan tersedia untuk semua module.
pub fn parse_file(tokens: &[SpannedToken]) -> Result<ParseResult, String> {
    let mut out = Vec::new();
    let mut typedefs_global = Vec::new();
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
        // `typedef` di luar module — compilation-unit scope (LRM §8.20).
        if tokens[idx].token == Token::Typedef {
            let (decl, next) = super::typedef::parse_typedef(&tokens[idx..])?;
            typedefs_global.push(decl);
            idx += next;
            continue;
        }
        // `package` — deklarasi paket yang berisi definisi tipe, parameter, dan subroutine (LRM §26).
        // SVRust tidak memproses isi paket; lewati seluruh `package ... endpackage`.
        if tokens[idx].token == Token::Package {
            let mut end_idx = idx;
            while end_idx < tokens.len() {
                if tokens[end_idx].token == Token::Endpackage {
                    end_idx += 1;
                    break;
                }
                end_idx += 1;
            }
            idx = end_idx;
            continue;
        }
        // `class` — deklarasi kelas OOP (LRM §8). SVRust tidak memproses isi
        // kelas; lewati seluruh `class ... endclass` agar berkas verifikasi
        // (UVM/DV) tetap bisa di-parse tanpa error.
        if tokens[idx].token == Token::Class {
            let mut end_idx = idx;
            while end_idx < tokens.len() {
                if tokens[end_idx].token == Token::Endclass {
                    end_idx += 1;
                    break;
                }
                end_idx += 1;
            }
            idx = end_idx;
            continue;
        }
        // `function` di luar module — definisi metode kelas dengan :: scope
        // (contoh `function kmac_app_device_agent::new(...)`). Lewati hingga
        // `endfunction` agar berkas verifikasi tidak menyebabkan error.
        if tokens[idx].token == Token::Function {
            let mut end_idx = idx;
            while end_idx < tokens.len() {
                if tokens[end_idx].token == Token::Endfunction {
                    end_idx += 1;
                    break;
                }
                end_idx += 1;
            }
            idx = end_idx;
            continue;
        }
        // `task` di luar module — definisi task kelas. Lewati hingga `endtask`.
        if tokens[idx].token == Token::Task {
            let mut end_idx = idx;
            while end_idx < tokens.len() {
                if tokens[end_idx].token == Token::Endtask {
                    end_idx += 1;
                    break;
                }
                end_idx += 1;
            }
            idx = end_idx;
            continue;
        }
        // Modul SV — parse seluruh `module ... endmodule`.
        if tokens[idx].token == Token::Module {
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
            // `endmodule : label` bentuk labeled endmodule (LRM §12.9).
            if idx < tokens.len() && tokens[idx].token == Token::Colon {
                idx += 2; // skip ':' and label name
            }
            continue;
        }
        // Konstruk tak dikenal di level berkas (mis. deklarasi variabel kelas
        // di luar module, `rand`, `bit`, `logic`, dll.) — lewati satu token
        // dan lanjutkan agar parse tidak gagal total pada berkas verifikasi.
        idx += 1;
        continue;
    }
    Ok(ParseResult {
        modules: out,
        typedefs_global,
    })
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

#[cfg(test)]
mod tests {
    use super::*;
    use sv_lexer::lex;

    fn parse(src: &str) -> ParseResult {
        let tokens = lex(src).expect("lex");
        parse_file(&tokens).expect("parse")
    }

    #[test]
    fn class_dilewati_pada_toplevel() {
        // Berkas yang berisi class OOP (UVM/DV) harus bisa di-parse tanpa error;
        // isi class dilewati, hanya module yang dihasilkan.
        let src = "class my_agent extends uvm_agent;\n  function new();\n  endfunction\nendclass\nmodule m;\nendmodule";
        let result = parse(src);
        assert_eq!(result.modules.len(), 1);
        assert_eq!(result.modules[0].name, "m");
    }

    #[test]
    fn class_tanpa_endmodule_tetap_dilewati() {
        // class sebelum module: class dilewati, module tetap terparse.
        let src = "class my_seq_item extends uvm_sequence_item;\n  rand bit [7:0] data;\nendclass\nmodule top;\nendmodule";
        let result = parse(src);
        assert_eq!(result.modules.len(), 1);
        assert_eq!(result.modules[0].name, "top");
    }

    #[test]
    fn function_dan_task_dilewati_di_luar_module() {
        // Definisi metode kelas dengan :: scope di luar module harus dilewati.
        let src = "function kmac_app_device_agent::new(string name, uvm_component parent);\n  super.new(name, parent);\nendfunction\nmodule top;\nendmodule";
        let result = parse(src);
        assert_eq!(result.modules.len(), 1);
        assert_eq!(result.modules[0].name, "top");
    }

    #[test]
    fn task_dilewati_di_luar_module() {
        let src = "task kmac_app_device_agent::run_device_seq();\n  forever begin\n  end\nendtask\nmodule top;\nendmodule";
        let result = parse(src);
        assert_eq!(result.modules.len(), 1);
        assert_eq!(result.modules[0].name, "top");
    }

    #[test]
    fn konstruk_tak_dikenal_dilewati_satu_token() {
        // Token yang tidak dikenal di level berkas tidak boleh menyebabkan error total.
        let src = "rand bit [7:0] data;\nmodule top;\nendmodule";
        let result = parse(src);
        assert_eq!(result.modules.len(), 1);
        assert_eq!(result.modules[0].name, "top");
    }
}
