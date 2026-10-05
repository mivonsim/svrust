// Tanggung jawab: menjalankan pipeline SV → IR → Rust dengan panic capture.
use std::panic::{catch_unwind, AssertUnwindSafe};

use sv_codegen_rust::generate_module;
use sv_elaborator::elaborate;
use sv_lexer::lex;
use sv_opt::optimize;
use sv_parser::parse_module;

/// Hasil satu kali jalan pipeline.
#[derive(Default)]
pub struct Hasil {
    /// Pesan panic kalau salah satu tahap panic.
    pub panic: Option<String>,
    /// Pesan error elaborasi kalau gagal (bukan panic).
    pub elaborate_error: Option<sv_elaborator::ElaborateError>,
    /// Kode Rust yang dihasilkan.
    pub codegen: Option<String>,
    /// `(nama, lebar_logis, MAX_WIDTH)` untuk setiap sinyal.
    pub design_lebar: Vec<(String, u32, u32)>,
    /// Sidik jari deterministik: bentuk IR dalam bentuk teks.
    pub fingerprint: Option<String>,
    /// Daftar proses — sidik jari *perilaku* yang dipakai oracle metamorphic.
    ///
    /// Dipisah dari `fingerprint` karena menambah deklarasi yang tidak terpakai
    /// memang mengubah daftar variabel, tapi tidak mengubah perilaku.
    pub proses: Option<Vec<sv_ir::Process>>,
    /// Design hasil elaborasi, untuk invariant yang memeriksa IR.
    pub design: Option<sv_ir::Design>,
}

/// Mengembalikan `Some` kalau kode punya kurung tak seimbang.
pub fn kurung_tidak_seimbang(code: &str) -> Option<(i64, i64, i64)> {
    let (mut k, mut kk, mut ks) = (0i64, 0i64, 0i64);
    let mut dalam_string = false;
    let mut escape = false;
    let mut komentar = false;
    let mut it = code.chars().peekable();
    while let Some(c) = it.next() {
        if komentar {
            if c == '\n' {
                komentar = false;
            }
            continue;
        }
        if dalam_string {
            if escape {
                escape = false;
            } else if c == '\\' {
                escape = true;
            } else if c == '"' {
                dalam_string = false;
            }
            continue;
        }
        match c {
            '/' if it.peek() == Some(&'/') => {
                komentar = true;
                continue;
            }
            '"' => {
                dalam_string = true;
                continue;
            }
            '(' => k += 1,
            ')' => k -= 1,
            '{' => kk += 1,
            '}' => kk -= 1,
            '[' => ks += 1,
            ']' => ks -= 1,
            _ => {}
        }
        if k < 0 || kk < 0 || ks < 0 {
            return Some((k, kk, ks));
        }
    }
    (k != 0 || kk != 0 || ks != 0).then_some((k, kk, ks))
}

/// Ambil lebar penyimpanan dari kode yang dihasilkan; dipakai target hierarki
/// yang tidak menyimpan design-nya sendiri.
pub fn max_width_dari_kode_publik(code: &str) -> u32 {
    max_width_dari_kode(code)
}

/// Ambil lebar penyimpanan dari kode yang dihasilkan.
///
/// `MAX_WIDTH` adalah konstanta yang di-emit codegen; membacanya dari teks
/// membuat pemeriksa tidak perluaszimuthodu regenerate design.
fn max_width_dari_kode(code: &str) -> u32 {
    for baris in code.lines() {
        if let Some(rest) = baris.strip_prefix("pub const MAX_WIDTH: usize = ") {
            if let Some(n) = rest.split(';').next() {
                if let Ok(v) = n.trim().parse::<u32>() {
                    return v;
                }
            }
        }
    }
    0
}

fn nama_sinyal(design: &sv_ir::Design) -> Vec<String> {
    design
        .variables
        .iter()
        .map(|v| v.name.clone())
        .collect()
}

/// Jalankan seluruh pipeline; tidak pernah panic.
pub fn jalankan(src: &str) -> Hasil {
    let mut hasil = Hasil::default();
    // Lexer/parser gagal adalah hasil normal untuk input fuzz, jadi tahap itu
    // dipisahkan dari tahap yang error-nya justru diperiksa.
    let tokens = match lex(src) {
        Ok(t) => t,
        Err(_) => return hasil,
    };
    let ast = match parse_module(&tokens) {
        Ok(a) => a,
        Err(_) => return hasil,
    };
    let r = catch_unwind(AssertUnwindSafe(|| {
        let mut design = elaborate(&ast).map_err(Len::Elab)?;
        optimize(&mut design);
        let code = generate_module(&design);
        Ok::<_, Len>((design, code))
    }));
    /// Bedakan "elaborasi gagal" dari "lexer/parser gagal".
    enum Len {
        Elab(sv_elaborator::ElaborateError),
    }
    match r {
        Err(payload) => {
            hasil.panic = Some(describe_panic(&payload));
            hasil
        }
        Ok(Err(Len::Elab(e))) => {
            hasil.elaborate_error = Some(e);
            hasil
        }
        Ok(Ok((design, code))) => {
            hasil.design_lebar = nama_sinyal(&design)
                .iter()
                .map(|n| {
                    let w = design
                        .variables
                        .iter()
                        .find(|v| &v.name == n)
                        .map(|v| v.data_type.width)
                        .unwrap_or(0);
                    (n.clone(), w, max_width_dari_kode(&code))
                })
                .collect();
            hasil.fingerprint = Some(format!("{design:#?}"));
            hasil.proses = Some(design.processes.clone());
            hasil.design = Some(design);
            hasil.codegen = Some(code);
            hasil
        }
    }
}

fn describe_panic(payload: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "<payload panic bukan &str/String>".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn program_valid_menghasilkan_kode() {
        let h = jalankan("module m; logic [7:0] a, y; assign a = 8'hA5; assign y = a[3:0]; endmodule");
        assert!(h.panic.is_none(), "{}", h.panic.unwrap_or_default());
        assert!(h.codegen.is_some());
    }

    #[test]
    fn program_terpotong_tidak_panic() {
        let h = jalankan("module m; logic [7:0] a; assign a = ");
        assert!(h.panic.is_none(), "{}", h.panic.unwrap_or_default());
    }
}
/// Nama proses yang sedang di-`$display` bertanda signed padahal LRM §11.8.1
/// menyatakan hasil bit-select dan part-select selalu unsigned.
///
/// Mengembalikan `Some` kalau ada pelanggaran, `None` kalau bersih.
pub fn seleksi_bertanda(proses: &[sv_ir::Process]) -> Option<String> {
    fn dalam(e: &sv_ir::Expr) -> Vec<&sv_ir::Expr> {
        match e {
            sv_ir::Expr::Select { .. } | sv_ir::Expr::Index { .. } => vec![e],
            sv_ir::Expr::Bin { lhs, rhs, .. } => {
                let mut v = dalam(lhs);
                v.extend(dalam(rhs));
                v
            }
            sv_ir::Expr::Un { operand, .. } => dalam(operand),
            sv_ir::Expr::Cast { operand, .. } => dalam(operand),
            sv_ir::Expr::Ternary {
                condition,
                when_true,
                when_false,
                ..
            } => {
                let mut v = dalam(condition);
                v.extend(dalam(when_true));
                v.extend(dalam(when_false));
                v
            }
            sv_ir::Expr::Concat { items, .. } => items.iter().flat_map(dalam).collect(),
            _ => Vec::new(),
        }
    }
    fn stmt_dalam<'a>(s: &'a sv_ir::Statement, out: &mut Vec<&'a sv_ir::Expr>) {
        match s {
            sv_ir::Statement::Assign { assignment, .. } => out.extend(dalam(&assignment.value)),
            sv_ir::Statement::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                out.extend(dalam(condition));
                for st in then_branch.iter().chain(else_branch) {
                    stmt_dalam(st, out);
                }
            }
            sv_ir::Statement::Block { body, .. } => {
                for st in body {
                    stmt_dalam(st, out);
                }
            }
            _ => {}
        }
    }
    for p in proses {
        let mut semua = Vec::new();
        for s in &p.body {
            stmt_dalam(s, &mut semua);
        }
        for e in semua {
            let (signed, apa) = match e {
                sv_ir::Expr::Select { data_type, .. } => (data_type.signed, "part-select"),
                sv_ir::Expr::Index {
                    data_type,
                    flat_element,
                    ..
                } => {
                    if *flat_element {
                        continue;
                    }
                    (data_type.signed, "bit-select")
                }
                _ => continue,
            };
            if signed {
                return Some(format!(
                    "hasil {apa} bertanda signed; LRM §11.8.1 menyatakan selalu unsigned"
                ));
            }
        }
    }
    None
}
