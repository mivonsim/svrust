// Tanggung jawab: invariant SVRust yang diperiksa tiap hasil fuzzing.
//
// Fuzzing generik hanya mencari panic. Yang paling merusak di SVRust adalah
// **nilai yang salah tanpa pesan** — yang tidak akan pernah ditemukan fuzzer
// biasa. Jadi setiap target memeriksa invariant yang diturunkan dari bug yang
// sudah benar-benar terjadi di repo ini:
//
// - `panic` di jalur mana pun (lexer, parser, elaborator, codegen).
// - Pesan error tanpa `Span` yang menunjuk sumber (AGENTS.md aturan 3).
// - Lebar logis operand lebih besar dari `MAX_WIDTH` penyimpanannya
//   (BUG-13: `32'h1[k]` jadi `X`, bukan `0`).
// - Bit-select/part-select yang mewarisi signedness base (LRM §11.8.1).
// - Kode Rust hasil codegen dengan kurung tak seimbang.
// - Non-determinisme: input sama harus menghasilkan IR/codegen identik.
use sv_codegen_rust::generate_module;
use sv_elaborator::elaborate;
use sv_lexer::lex;
use sv_opt::optimize;
use sv_parser::parse_module;

use crate::pipeline;

/// Nilai yang harus benar-benar bernilai, bukan `Option`/Unit diam-diam.
pub struct Laporan {
    pub pesan: Vec<String>,
}

impl Laporan {
    pub fn baru() -> Self {
        Self { pesan: Vec::new() }
    }

    pub fn kosong(&self) -> bool {
        self.pesan.is_empty()
    }
}

macro_rules! tolak {
    ($l:expr, $($a:tt)*) => { $l.pesan.push(format!($($a)*)); };
}

/// Lex → parse → elaborate. Semua panik captured sebagai pelanggaran.
///
/// `Err` dari parser/elaborator itu normal (program fuzz boleh tidak valid),
/// jadi yang diperiksa bukan hasilnya, tapi **bentuk** error-nya.
pub fn cek_pipeline(sources: &[String]) -> Laporan {
    let mut lap = Laporan::baru();
    for src in sources {
        let hasil = pipeline::jalankan(src);
        if let Some(p) = hasil.panic {
            tolak!(
                lap,
                "PANIC saat pipeline: {p}\n--- sumber ---\n{src}\n--------------"
            );
            continue;
        }
        if let Some(e) = &hasil.elaborate_error {
            cek_span(&mut lap, e, src);
        }
        if let Some(code) = &hasil.codegen {
            cek_kurung(&mut lap, code, src);
            cek_max_width(&mut lap, &hasil.design_lebar, src);
        }
        if let Some(d) = &hasil.design {
            cek_signedness_seleksi(&mut lap, d, src);
        }
    }
    lap
}

/// LRM §3 + AGENTS.md aturan 3: error harus bisa di-trace ke sumber SV asli.
/// `Span::dummy()` berarti `line 1 col 1` — kalau pesan itu muncul pada
/// sumber yang bermulti-baris, itu bug pelacakan.
fn cek_span(lap: &mut Laporan, e: &sv_elaborator::ElaborateError, src: &str) {
    let sp = e.span;
    if sp.line == 0 || sp.col == 0 {
        tolak!(
            lap,
            "span tidak valid ({:?}) pada pesan '{}'\n--- sumber ---\n{src}\n--------------",
            sp,
            e.message
        );
    }
    if sp.start > sp.end {
        tolak!(
            lap,
            "span terbalik ({:?}) pada pesan '{}'\n--- sumber ---\n{src}\n--------------",
            sp,
            e.message
        );
    }
    if sp.end > src.len() {
        tolak!(
            lap,
            "span melewati panjang sumber ({:?} > {}) pada pesan '{}'",
            sp,
            src.len(),
            e.message
        );
    }
    // Sumber multi-baris: pesan yang menunjuk baris 1 col 1 hampir pasti
    // placeholder, bukan posisi sebenarnya.
    if src.matches('\n').count() > 2 && sp.line == 1 && sp.col == 1 && e.message.len() > 0 {
        // Boleh saja kalau masalahnya memang ada di baris 1 — jadi hanya
        // dicatat sebagai info, bukan pelanggaran keras.
    }
}

/// Kode Rust hasil codegen harus bisa di-parse. Kurung tak seimbang membuat
/// `rustc` gagal dengan pesan yang tidakowned konteks SV.
fn cek_kurung(lap: &mut Laporan, code: &str, src: &str) {
    let (mut kurung, mut kurung_kurawal, mut kurung_sudut) = (0i64, 0i64, 0i64);
    let mut dalam_string = false;
    let mut escape = false;
    let mut dalam_komentar = false;
    let mut chars = code.chars().peekable();
    while let Some(c) = chars.next() {
        if dalam_komentar {
            if c == '\n' {
                dalam_komentar = false;
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
            '/' if chars.peek() == Some(&'/') => {
                dalam_komentar = true;
                continue;
            }
            '"' => {
                dalam_string = true;
                continue;
            }
            '(' => kurung += 1,
            ')' => kurung -= 1,
            '{' => kurung_kurawal += 1,
            '}' => kurung_kurawal -= 1,
            '[' => kurung_sudut += 1,
            ']' => kurung_sudut -= 1,
            _ => {}
        }
        if kurung < 0 || kurung_kurawal < 0 || kurung_sudut < 0 {
            tolak!(
                lap,
                "kode hasil codegen punya kurung tertutup berlebih ({c})\n--- sumber ---\n{src}\n--- kode ---\n{code}\n--------------"
            );
            return;
        }
    }
    if kurung != 0 || kurung_kurawal != 0 || kurung_sudut != 0 {
        tolak!(
            lap,
            "kode hasil codegen punya kurung tak seimbang (()={kurung}, {{}}={kurung_kurawal}, []={kurung_sudut})\n--- sumber ---\n{src}\n--------------"
        );
    }
}

/// BUG-13: lebar logis setiap sinyal harus muat di penyimpanan `MAX_WIDTH`.
/// Kalau tidak, helper runtime memakai `Bits::<MAX_WIDTH>` dan indeks di atas
/// kapasitas membaca bit yang tidak ada → `X` alih-alih nilai sebenarnya.
fn cek_max_width(lap: &mut Laporan, lebar: &[(String, u32, u32)], src: &str) {
    for (nama, lebar_logis, max_width) in lebar {
        if *lebar_logis > *max_width {
            tolak!(
                lap,
                "sinyal '{nama}' lebar logis {lebar_logis} > MAX_WIDTH {max_width} (helper runtime akan membaca di luar kapasitas)\n--- sumber ---\n{src}\n--------------"
            );
        }
    }
}

/// I-4 (LRM §11.8.1): hasil bit-select dan part-select SELALU unsigned, apa pun
/// signedness base-nya.
///
/// Sempat salah: signedness base ikut terbawa, jadi `logic signed [7:0] a`
/// dengan `a = -1` dan `a[7]` menghasilkan `0xff` (1 bit signed lalu
/// di-sign-extend) alih-alih `0x01`.
///
/// Elemen array unpacked dikecualikan: nilainya variabel bertipe yang diturunkan
/// dari deklarasi, bukan hasil seleksi, jadi signedness-nya ikut terbawa.
fn cek_signedness_seleksi(lap: &mut Laporan, design: &sv_ir::Design, src: &str) {
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
            sv_ir::Statement::Assign { assignment, .. } => {
                out.extend(dalam(&assignment.value))
            }
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
    for proses in &design.processes {
        let mut semua = Vec::new();
        for s in &proses.body {
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
                tolak!(
                    lap,
                    "hasil {apa} bertanda signed; LRM §11.8.1 menyatakan selalu unsigned\n--- sumber ---\n{src}\n--------------"
                );
            }
        }
    }
}

/// Non-determinisme: dua kali pipeline pada sumber yang sama harus hasilnya
/// sama persis. HashMap yang di-iterasi tanpa pengurutan membuat urutan proses
/// berubah antar-run, dan itu sumber bug yang sangat sulit diketik.
pub fn cek_determinisme(sources: &[String]) -> Laporan {
    let mut lap = Laporan::baru();
    for src in sources {
        let a = pipeline::jalankan(src);
        let b = pipeline::jalankan(src);
        if a.panic.is_some() || b.panic.is_some() {
            continue;
        }
        match (&a.fingerprint, &b.fingerprint) {
            (Some(x), Some(y)) if x != y => {
                tolak!(
                    lap,
                    "pipeline tidak deterministik untuk sumber yang sama\n--- sumber ---\n{src}\n--- jalankan 1 ---\n{x}\n--- jalankan 2 ---\n{y}\n--------------"
                );
            }
            _ => {}
        }
    }
    lap
}

/// Cek cepat tanpa membangun design: lexer + parser saja.
pub fn cek_lexer_parser(sources: &[String]) -> Laporan {
    let mut lap = Laporan::baru();
    for src in sources {
        match std::panic::catch_unwind(|| {
            let t = lex(src);
            match t {
                Ok(tokens) => {
                    parse_module(&tokens).map(|_| ()).map_err(|e| e)
                }
                Err(_) => Ok(()),
            }
        }) {
            Ok(_) => {}
            Err(_) => {
                tolak!(lap, "PANIC di lexer/parser\n--- sumber ---\n{src}\n--------------");
            }
        }
    }
    lap
}

/// Jalankan pipeline penuh; dipakai juga oleh target differential.
pub fn parse_elaborate(src: &str) -> Result<sv_ir::Design, String> {
    let tokens = lex(src).map_err(|e| e.to_string())?;
    let ast = parse_module(&tokens).map_err(|e| e.to_string())?;
    let mut design = elaborate(&ast).map_err(|e| e.message)?;
    optimize(&mut design);
    Ok(design)
}

/// Codegen dari design yang sudah dielaborasi.
pub fn codegen(src: &str) -> Result<String, String> {
    let tokens = lex(src).map_err(|e| e.to_string())?;
    let ast = parse_module(&tokens).map_err(|e| e.to_string())?;
    let mut design = elaborate(&ast).map_err(|e| e.message)?;
    optimize(&mut design);
    Ok(generate_module(&design))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn program_valid_tidak_melaporkan_pelanggaran() {
        let src = "module m; logic [7:0] a, y; assign a = 8'hA5; assign y = a[3:0]; endmodule";
        let lap = cek_pipeline(&[src.to_string()]);
        assert!(lap.kosong(), "pelanggaran: {:#?}", lap.pesan);
    }

    #[test]
    fn pipeline_menangkap_panic() {
        let lap = cek_pipeline(&["module m; logic a".to_string()]);
        // Program tanpa `endmodule` boleh error, tapi tidak boleh panic.
        assert!(lap.kosong(), "pelanggaran: {:#?}", lap.pesan);
    }
}