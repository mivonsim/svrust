// Tanggung jawab: belah token berkas menjadi daftar module dan typedef global.
use crate::pesan;
use sv_ast::module::Module;
use sv_ast::package::PackageDecl;
use sv_ast::typedef::TypeDefDecl;
use sv_lexer::spanned::SpannedToken;
use sv_lexer::token::Token;

/// Hasil parse satu berkas: daftar module beserta `typedef` di luar module
/// (compilation-unit scope, LRM §8.20) dan deklarasi paket dengan parameternya.
pub struct ParseResult {
    pub modules: Vec<Module>,
    pub typedefs_global: Vec<TypeDefDecl>,
    pub packages: Vec<PackageDecl>,
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
    let mut packages = Vec::new();
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
        // SVRust mengumpulkan parameter paket agar referensi `pkg::nama` (misal
        // `rram_ctrl_pkg::TotalDataPages`) dapat dievaluasi saat elaborasi. Isi
        // paket selain parameter dilewati.
        if tokens[idx].token == Token::Package {
            if let Some((pkg, next)) = parse_package(&tokens[idx..])? {
                packages.push(pkg);
                idx += next;
            } else {
                idx += 1;
            }
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
        // `interface` — definisi antarmuka (LRM §25). SVRust tidak memproses
        // isi interface; lewati seluruh `interface ... endinterface` agar
        // `typedef` di dalamnya tidak bocor ke scope global (compilation-unit)
        // yang dielaborasi untuk design `--top` dan memicu error di file DV
        // yang sebenarnya tak termasuk design.
        if let Token::Ident(nama) = &tokens[idx].token {
            if nama == "interface" {
                let mut end_idx = idx;
                while end_idx < tokens.len() {
                    if let Token::Ident(e) = &tokens[end_idx].token {
                        if e == "endinterface" {
                            end_idx += 1;
                            break;
                        }
                    }
                    end_idx += 1;
                }
                idx = end_idx;
                continue;
            }
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
                return Err(pesan::tandai(
                    tokens,
                    idx,
                    format!("module duplikat: {}", modul.name),
                ));
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
        typedefs_global: gabung_typedef_paket(typedefs_global, &packages),
        packages,
    })
}

/// Gabungkan `typedef` dari paket ke daftar typedef global dengan nama
/// berkualifikasi `pkg::nama` (LRM §8.20) sehingga deklarasi tipe
/// `pkg::nama var;` bisa di-resolve ke lebar konkret.
fn gabung_typedef_paket(
    mut global: Vec<sv_ast::typedef::TypeDefDecl>,
    packages: &[sv_ast::package::PackageDecl],
) -> Vec<sv_ast::typedef::TypeDefDecl> {
    for pkg in packages {
        for td in &pkg.typedefs {
            global.push(sv_ast::typedef::TypeDefDecl {
                name: format!("{}::{}", pkg.name, td.name),
                body: td.body.clone(),
                span: td.span,
            });
        }
    }
    global
}

/// Parse satu deklarasi `package Nama ... endpackage` dan kumpulkan
/// parameternya (LRM §26).
///
/// Tokens dimulai pada `Token::Package`. Mengembalikan `(PackageDecl, jumlah
/// token yang dikonsumsi)`, atau `None` bila paket tidak dapat diparse
/// lengkap (misal `endpackage` tak ditemukan) — dalam hal itu pemanggil
/// melompat satu token agar parse tidak gagal total.
fn parse_package(tokens: &[SpannedToken]) -> Result<Option<(PackageDecl, usize)>, String> {
    if tokens.is_empty() || tokens[0].token != Token::Package {
        return Ok(None);
    }
    let span = tokens[0].span;
    let mut idx = 1; // lewati 'package'

    // Nama paket.
    if idx >= tokens.len() {
        return Err(pesan::harap(
            tokens,
            idx,
            "expected nama paket setelah 'package'",
        ));
    }
    let Token::Ident(nama) = &tokens[idx].token else {
        return Err(pesan::harap(
            tokens,
            idx,
            "expected nama paket setelah 'package'",
        ));
    };
    let nama = nama.clone();
    idx += 1;

    // Lebarkan tanda `;` setelah nama paket (biasa ada).
    if idx < tokens.len() && tokens[idx].token == Token::Semi {
        idx += 1;
    }

    let mut params: Vec<sv_ast::width::LocalParamDecl> = Vec::new();
    let mut typedefs: Vec<sv_ast::typedef::TypeDefDecl> = Vec::new();

    // Isi paket: kumpulkan deklarasi `parameter Nama = EXPR;` dan `typedef`
    // hingga `endpackage`. Konstruk lain (function, import, task, net,
    // variabel, dll.) dilewati.
    while idx < tokens.len() && tokens[idx].token != Token::Endpackage {
        if tokens[idx].token == Token::Parameter || tokens[idx].token == Token::LocalParam {
            match parse_package_parameter(&tokens[idx..]) {
                Some((p, next)) => {
                    params.push(p);
                    idx += next;
                }
                None => {
                    // Tidak bisa di-parse sebagai parameter; lewati satu token.
                    idx += 1;
                }
            }
        } else if tokens[idx].token == Token::Typedef {
            // `typedef` di dalam paket (misal `typedef struct packed {..} t;`)
            // dikumpulkan agar tipe berkualifikasi `pkg::t` bisa di-resolve.
            match super::typedef::parse_typedef(&tokens[idx..]) {
                Ok((decl, next)) => {
                    typedefs.push(decl);
                    idx += next;
                }
                Err(_) => {
                    // Tidak bisa di-parse; lewati satu token agar parse tidak
                    // gagal total.
                    idx += 1;
                }
            }
        } else {
            // Lewati konstruk lain di dalam paket (import, function, task, net,
            // variabel, dll.).
            idx += 1;
        }
    }

    if idx >= tokens.len() {
        return Err(pesan::harap(
            tokens,
            idx,
            "expected 'endpackage' menutup deklarasi paket",
        ));
    }
    idx += 1; // lewati 'endpackage'

    Ok(Some((
        PackageDecl {
            name: nama,
            span,
            params,
            typedefs,
        },
        idx,
    )))
}

/// Parse satu deklarasi `parameter Nama = EXPR;` di dalam paket.
///
/// Mengembalikan `Some((decl, consumed))` atau `None` bila bentuknya bukan
/// deklarasi parameter yang valid. Nilai disimpan sebagai ekspresi (seperti
/// `localparam`) dan dievaluasi saat elaborasi.
fn parse_package_parameter(
    tokens: &[SpannedToken],
) -> Option<(sv_ast::width::LocalParamDecl, usize)> {
    if tokens.is_empty()
        || (tokens[0].token != Token::Parameter && tokens[0].token != Token::LocalParam)
    {
        return None;
    }
    let mut idx = 1; // lewati 'parameter'

    // Lewati tipe opsional (misal `int unsigned`, `bit [7:0]`).
    if idx < tokens.len() && sv_lexer::token::tipe::bisa_mulai_deklarasi(&tokens[idx].token) {
        idx += 1;
        while idx < tokens.len() && matches!(tokens[idx].token, Token::Signed | Token::Unsigned) {
            idx += 1;
        }
        while idx < tokens.len() && tokens[idx].token == Token::LBracket {
            idx = crate::module::lewati_kurung_siku(tokens, idx);
        }
    } else if idx < tokens.len()
        && matches!(tokens[idx].token, Token::Ident(_))
        && tokens
            .get(idx + 1)
            .map(|t| t.token == Token::Colon)
            .unwrap_or(false)
    {
        // `parameter pkg::tipe Nama = ...`
        idx += 1;
        while idx < tokens.len() && tokens[idx].token == Token::Colon {
            idx += 1;
        }
        if idx < tokens.len() && matches!(tokens[idx].token, Token::Ident(_)) {
            idx += 1;
        }
        while idx < tokens.len() && tokens[idx].token == Token::LBracket {
            idx = crate::module::lewati_kurung_siku(tokens, idx);
        }
    } else if idx < tokens.len()
        && matches!(tokens[idx].token, Token::Ident(_))
        && tokens
            .get(idx + 1)
            .is_some_and(|t| matches!(&t.token, Token::Ident(_)))
        && tokens.get(idx + 2).is_some_and(|t| t.token == Token::Eq)
    {
        // `parameter type Nama = ...` — tipe tanpa scope paket
        // (misal `parameter lc_tx_t LC_TX_DEFAULT = ...`).
        idx += 2; // lewati nama tipe DAN nama parameter
    }

    // Nama parameter.
    let Token::Ident(nama) = tokens.get(idx)?.token.clone() else {
        return None;
    };
    let nama = nama.clone();
    let span = tokens[idx].span;
    idx += 1;

    // Lewati signedness setelah nama.
    if idx < tokens.len() && matches!(tokens[idx].token, Token::Signed | Token::Unsigned) {
        idx += 1;
    }

    // Lewati dimensi unpacked setelah nama (misal `Nama [0:15]`).
    while idx < tokens.len() && tokens[idx].token == Token::LBracket {
        idx = crate::module::lewati_kurung_siku(tokens, idx);
    }

    // `= EXPR`.
    if idx >= tokens.len() || tokens[idx].token != Token::Eq {
        return None;
    }
    idx += 1;

    let (value, eaten) = super::expression::parse_expression(&tokens[idx..]).ok()?;
    idx += eaten;

    // Lewati sisa hingga `;` atau koma.
    while idx < tokens.len()
        && tokens[idx].token != Token::Semi
        && tokens[idx].token != Token::Comma
    {
        idx += 1;
    }
    let mut consumed = idx;
    if consumed < tokens.len() && tokens[consumed].token == Token::Semi {
        consumed += 1;
    }

    Some((
        sv_ast::width::LocalParamDecl {
            name: nama,
            value,
            span,
        },
        consumed,
    ))
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
