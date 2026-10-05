// Tanggung jawab: integration test jangkauan bit/part-select saat elaborasi.
use sv_elaborator::elaborate;

/// Elabrasi dan kembalikan pesan error, atau `None` kalau sukses.
fn err(source: &str) -> Option<String> {
    let tokens = sv_lexer::lex(source).expect("lex");
    let ast = sv_parser::parse_module(&tokens).expect("parse");
    elaborate(&ast).err().map(|e| e.message)
}

// Bug: `a[i]` pada sinyal packed dan `mem[i]` pada array unpacked punya sintaks
// sama, tapi lebar elemen dan pergeseran posisinya berbeda. IR memakai satu
// varian `Expr::Index` tanpa penanda, sehingga keduanya dilayani helper yang
// sama — bit-select packed selalu menggeser `idx * lebar` dan hasilnya nol.
#[test]
fn indeks_packed_menandai_bukan_elemen_rata() {
    let tokens = sv_lexer::lex(
        "module top; logic [7:0] a; logic [2:0] i; logic b; assign b = a[i]; endmodule",
    )
    .expect("lex");
    let ast = sv_parser::parse_module(&tokens).expect("parse");
    let design = elaborate(&ast).expect("elaborate");
    let (flat, lebar) = cari_index(&design);
    assert!(!flat, "sinyal packed harus bit-select, bukan elemen rata");
    assert_eq!(lebar, 1, "bit-select punya lebar 1 bit");
}

#[test]
fn indeks_unpacked_menandai_elemen_rata() {
    let tokens = sv_lexer::lex(
        "module top; logic [7:0] mem [0:3]; logic [2:0] i; logic [7:0] y; \
         assign y = mem[i]; endmodule",
    )
    .expect("lex");
    let ast = sv_parser::parse_module(&tokens).expect("parse");
    let design = elaborate(&ast).expect("elaborate");
    let (flat, lebar) = cari_index(&design);
    assert!(flat, "array unpacked harus elemen rata");
    assert_eq!(lebar, 8, "elemen array selebar sinyal");
}

/// Ambil `(flat_element, lebar)` dari node `Expr::Index` pertama.
fn cari_index(design: &sv_ir::Design) -> (bool, u32) {
    let (flat, lebar, _) = cari_index_detail(design);
    (flat, lebar)
}

/// Ambil `(flat_element, lebar, signed)` dari node `Expr::Index` pertama.
fn cari_index_detail(design: &sv_ir::Design) -> (bool, u32, bool) {
    fn dalam(expr: &sv_ir::Expr) -> Option<(bool, u32, bool)> {
        match expr {
            sv_ir::Expr::Index {
                data_type,
                flat_element,
                ..
            } => Some((*flat_element, data_type.width, data_type.signed)),
            sv_ir::Expr::Bin { lhs, rhs, .. } => dalam(lhs).or_else(|| dalam(rhs)),
            sv_ir::Expr::Un { operand, .. } => dalam(operand),
            // Assignment pada sinyal lebih lebar membungkus operand dalam
            // cast, jadi penelusuran harus memasukinya.
            sv_ir::Expr::Cast { operand, .. } => dalam(operand),
            _ => None,
        }
    }
    for proses in &design.processes {
        for stmt in &proses.body {
            if let sv_ir::Statement::Assign { assignment, .. } = stmt {
                if let Some(ditemukan) = dalam(&assignment.value) {
                    return ditemukan;
                }
            }
        }
    }
    panic!("tidak ada Expr::Index pada design");
}

// BUG: `lebar_base_konstan` menghitung lebar base `Select` sebagai
// `msb - lsb + 1` dari AST. Untuk array unpacked, `mem[2]` di-lowering jadi
// `Select` pada bit [16..24) sehingga lebarnya terbaca 9, bukan 8 —
// `mem[2][3]` yang sah LRM ditolak dengan pesan "di luar lebar operand".
#[test]
fn bit_select_pada_elemen_array_lebar_benar_diterima() {
    let tokens = sv_lexer::lex(
        "module top; logic [7:0] mem [0:3]; logic [7:0] y; assign y = mem[2][3]; endmodule",
    )
    .expect("lex");
    let ast = sv_parser::parse_module(&tokens).expect("parse");
    elaborate(&ast).expect("bit 3 dari elemen 8 bit harus sah");
}

#[test]
fn part_select_pada_elemen_array_batas_akhir_diterima() {
    let tokens = sv_lexer::lex(
        "module top; logic [7:0] mem [0:3]; logic [7:0] y; assign y = mem[2][7:0]; endmodule",
    )
    .expect("lex");
    let ast = sv_parser::parse_module(&tokens).expect("parse");
    elaborate(&ast).expect("part-select penuh elemen harus sah");
}

// BUG: `simbol_dasar` mencari simbol rekursif, jadi untuk base `mem[2]`
// (yang sudah diekstrak jadi skalar 8 bit) ia mengembalikan simbol `mem` yang
// punya `unpacked`. Akibatnya `flat_element` jadi `true` untuk `mem[2][k]`
// dan hasilnya salah senyap.
#[test]
fn indeks_genap_pada_elemen_array_adalah_bit_select() {
    let tokens = sv_lexer::lex(
        "module top; logic [7:0] mem [0:3]; logic [2:0] k; logic y; assign y = mem[2][k]; \
         endmodule",
    )
    .expect("lex");
    let ast = sv_parser::parse_module(&tokens).expect("parse");
    let design = elaborate(&ast).expect("elaborate");
    let (flat, lebar) = cari_index(&design);
    assert!(
        !flat,
        "base sudah diekstrak jadi skalar, jadi bukan elemen rata"
    );
    assert_eq!(lebar, 1, "bit-select punya lebar 1 bit");
}

// BUG: `with_signed` hilang di jalur indeks dinamis — elemen array unpacked
// kehilangan signedness yang seharusnya ikut dari tipe elemennya.
#[test]
fn signedness_elemen_array_ikut_terbawa_pada_indeks_dinamis() {
    let tokens = sv_lexer::lex(
        "module top; logic signed [7:0] mem [0:1]; logic [2:0] i; logic signed b; \
         assign b = mem[i]; endmodule",
    )
    .expect("lex");
    let ast = sv_parser::parse_module(&tokens).expect("parse");
    let design = elaborate(&ast).expect("elaborate");
    let (flat, lebar, signed) = cari_index_detail(&design);
    assert!(flat, "harus elemen rata");
    assert_eq!(lebar, 8);
    assert!(signed, "signedness elemen array harus ikut terbawa");
}

// BUG: bit-select mewarisi signedness base. LRM §11.8.1 menyatakan hasil
// bit-select dan part-select SELALU unsigned, apa pun signedness base-nya.
// Dengan signed, `a[i]` pada `logic signed [7:0] a` jadi 1 bit signed lalu
// di-sign-extend — `a = -1; a[7]` jadi 0xFF, bukan 0x01.
#[test]
fn bit_select_pada_sinyal_signed_tetap_unsigned() {
    let tokens = sv_lexer::lex(
        "module top; logic signed [7:0] a; logic [2:0] i; logic signed b; \
         assign b = a[i]; endmodule",
    )
    .expect("lex");
    let ast = sv_parser::parse_module(&tokens).expect("parse");
    let design = elaborate(&ast).expect("elaborate");
    let (_, lebar, signed) = cari_index_detail(&design);
    assert_eq!(lebar, 1, "bit-select selalu 1 bit");
    assert!(!signed, "LRM §11.8.1: bit-select selalu unsigned");
}

#[test]
fn part_select_pada_sinyal_signed_tetap_unsigned() {
    let tokens = sv_lexer::lex(
        "module top; logic signed [7:0] a; logic [7:0] y; assign y = a[7:4]; endmodule",
    )
    .expect("lex");
    let ast = sv_parser::parse_module(&tokens).expect("parse");
    let design = elaborate(&ast).expect("elaborate");
    let (_, _, signed) = cari_select_detail(&design);
    assert!(!signed, "LRM §11.8.1: part-select selalu unsigned");
}

/// Ambil `(flat_element, lebar, signed)` dari node `Expr::Select` pertama.
fn cari_select_detail(design: &sv_ir::Design) -> (bool, u32, bool) {
    fn dalam(expr: &sv_ir::Expr) -> Option<(bool, u32, bool)> {
        match expr {
            sv_ir::Expr::Select { data_type, .. } => {
                Some((false, data_type.width, data_type.signed))
            }
            sv_ir::Expr::Bin { lhs, rhs, .. } => dalam(lhs).or_else(|| dalam(rhs)),
            sv_ir::Expr::Un { operand, .. } => dalam(operand),
            sv_ir::Expr::Cast { operand, .. } => dalam(operand),
            _ => None,
        }
    }
    for proses in &design.processes {
        for stmt in &proses.body {
            if let sv_ir::Statement::Assign { assignment, .. } = stmt {
                if let Some(ditemukan) = dalam(&assignment.value) {
                    return ditemukan;
                }
            }
        }
    }
    panic!("tidak ada Expr::Select pada design");
}

// BUG: `MAX_WIDTH` dihitung dari lebar tipenya, tapi konstanta dikecualikan
// (agar `== 0` tidak memaksa MAX_WIDTH jadi 32). Akibatnya base pada
// `Expr::Select`/`Expr::Index` bisa lebih lebar dari `MAX_WIDTH`, dan helper
// runtime memakai `Bits::<MAX_WIDTH>` — indeks di atas kapasitas membaca bit
// yang tidak ada sehingga hasilnya `X` (iverilog: `0`).
#[test]
fn max_width_mencakup_lebar_logis_base_seleksi_dan_indeks() {
    use sv_ir::Expr;
    fn max_lebar(expr: &Expr) -> u32 {
        match expr {
            Expr::Const { .. } => 0,
            other => other.data_type().width,
        }
        .max(match expr {
            Expr::Bin { lhs, rhs, .. } => max_lebar(lhs).max(max_lebar(rhs)),
            Expr::Un { operand, .. } | Expr::Cast { operand, .. } => max_lebar(operand),
            Expr::Select { base, .. } => max_lebar(base).max(base.data_type().width),
            Expr::Index { base, index, .. } => max_lebar(base)
                .max(max_lebar(index))
                .max(base.data_type().width),
            Expr::SignalRef { .. } | Expr::SimTime { .. } => 0,
            _ => 0,
        })
    }
    // Base 32-bit, MAX_WIDTH sisa 8 bit.
    let tokens =
        sv_lexer::lex("module top; logic [7:0] k; logic [7:0] y; assign y = 32'h1[k]; endmodule")
            .expect("lex");
    let ast = sv_parser::parse_module(&tokens).expect("parse");
    let design = elaborate(&ast).expect("elaborate");
    let mut lebar = 0;
    for proses in &design.processes {
        for stmt in &proses.body {
            if let sv_ir::Statement::Assign { assignment, .. } = stmt {
                lebar = lebar.max(max_lebar(&assignment.value));
            }
        }
    }
    assert!(
        lebar >= 32,
        "lebar perhitungan harus mencakup lebar logis base 32 bit, dapat {lebar}"
    );
}

#[test]
fn bit_select_dalam_jangkauan_diterima() {
    let tokens =
        sv_lexer::lex("module top; logic [7:0] a, y; assign a = 8'hA5; assign y = a[0]; endmodule")
            .expect("lex");
    let ast = sv_parser::parse_module(&tokens).expect("parse");
    elaborate(&ast).expect("bit-select dalam jangkauan harus sah");
}

#[test]
fn part_select_penuh_dalam_jangkauan_diterima() {
    let tokens =
        sv_lexer::lex("module top; logic [7:0] a, y; assign y = a[7:0]; endmodule").expect("lex");
    let ast = sv_parser::parse_module(&tokens).expect("parse");
    elaborate(&ast).expect("part-select penuh harus sah");
}

// Bug: bit-select konstan di luar lebar sinyal lolos elaborasi, lalu
// `select_kbits` mengembalikan 0 karena penjaga OOB-nya compares dengan
// `MAX_WIDTH` (kapasitas vektor Rust) yang selalu lebih besar dari lebar
// logis sinyal. Hasilnya nol, bukan `X` (LRM §7.8), dan tanpa pesan apa pun.
#[test]
fn bit_select_di_luar_lebar_sinyal_ditolak() {
    let pesan = err("module top; logic [7:0] a, y; assign y = a[100]; endmodule")
        .expect("indeks di luar lebar harus ditolak");
    assert!(pesan.contains("di luar lebar"), "pesan: {pesan}");
    assert!(
        pesan.contains("8 bit"),
        "pesan harus menyebut lebar sinyal: {pesan}"
    );
}

#[test]
fn part_select_di_luar_lebar_sinyal_ditolak() {
    let pesan = err("module top; logic [7:0] a, y; assign y = a[100:90]; endmodule")
        .expect("part-select di luar lebar harus ditolak");
    assert!(pesan.contains("di luar lebar"), "pesan: {pesan}");
}

#[test]
fn batas_msb_di_luar_lebar_ditolak_walaupun_lsb_sah() {
    let pesan = err("module top; logic [7:0] a, y; assign y = a[8:0]; endmodule")
        .expect("msb di luar lebar harus ditolak");
    assert!(pesan.contains("di luar lebar"), "pesan: {pesan}");
}

// Sinyal 1 bit: hanya indeks 0 yang sah.
#[test]
fn sinyal_satu_bit_hanya_indeks_nol() {
    assert!(
        err("module top; logic a, y; assign y = a[1]; endmodule").is_some(),
        "indeks 1 pada sinyal 1 bit harus ditolak"
    );
    let tokens = sv_lexer::lex("module top; logic a, y; assign y = a[0]; endmodule").expect("lex");
    let ast = sv_parser::parse_module(&tokens).expect("parse");
    elaborate(&ast).expect("indeks 0 pada sinyal 1 bit harus sah");
}

// Pesan error harus menunjuk ke posisi token, bukan `line 1 col 1`
// (AGENTS.md aturan 3). Dulu part-select terbalik memakai `Span::dummy()`.
// Bug: range select pada array unpacked `mem[3:1]` melewati jalur `unpacked`
// yang melakukan `return` lebih dulu, sehingga `lsb` diabaikan total dan
// hasilnya hanya 1 elemen (8 bit) — nilai salah tanpa pesan apa pun.
#[test]
fn range_select_pada_array_unpacked_ditolak_eksplisit() {
    let pesan =
        err("module top; logic [7:0] mem [0:3]; logic [31:0] y; assign y = mem[3:1]; endmodule")
            .expect("range select pada array unpacked harus ditolak");
    assert!(
        pesan.contains("range select") && pesan.contains("unpacked"),
        "pesan: {pesan}"
    );
}

#[test]
fn bit_select_pada_array_unpacked_tetap_diterima() {
    let tokens = sv_lexer::lex(
        "module top; logic [7:0] mem [0:3]; logic [7:0] y; assign y = mem[2]; endmodule",
    )
    .expect("lex");
    let ast = sv_parser::parse_module(&tokens).expect("parse");
    elaborate(&ast).expect("bit-select elemen pada array unpacked harus sah");
}

// Bug: cek jangkauan hanya jalan untuk `Ident`. Base berupa seleksi
// (`mem[2][9]`) lolos, padahal bit 9 di luar lebar elemen 8 bit.
#[test]
fn bit_select_di_luar_lebar_elemen_array_ditolak() {
    let pesan =
        err("module top; logic [7:0] mem [0:3]; logic [7:0] y; assign y = mem[2][9]; endmodule")
            .expect("bit 9 pada elemen 8 bit harus ditolak");
    assert!(pesan.contains("di luar lebar"), "pesan: {pesan}");
}

#[test]
fn bit_select_di_luar_lebar_pada_hasil_seleksi_ditolak() {
    let pesan = err("module top; logic [31:0] a, y; assign y = a[3:0][5]; endmodule")
        .expect("bit 5 pada hasil seleksi 4 bit harus ditolak");
    assert!(pesan.contains("di luar lebar"), "pesan: {pesan}");
    assert!(
        pesan.contains("4 bit"),
        "pesan harus menyebut lebar: {pesan}"
    );
}

#[test]
fn bit_select_dalam_lebar_hasil_seleksi_diterima() {
    let tokens = sv_lexer::lex("module top; logic [31:0] a, y; assign y = a[3:0][3]; endmodule")
        .expect("lex");
    let ast = sv_parser::parse_module(&tokens).expect("parse");
    elaborate(&ast).expect("bit 3 pada hasil seleksi 4 bit harus sah");
}

// Bug: `span_expr` tidak exhaustive sehingga 7 varian (FunctionCall, Cast,
// SizeCast, SignCast, Bits, BuiltinCast) jatuh ke `Span::dummy()` dan pesan
// dilaporkan di `line 1 col 1` — melanggar AGENTS.md aturan 3.
#[test]
fn pesan_select_dengan_base_cast_menunjuk_ke_sumber() {
    let sumber =
        "module top;\n  logic [7:0] x;\n  logic [7:0] y;\n  assign y = 8'(x)[3:7];\nendmodule";
    let pesan = err(sumber).expect("part-select terbalik pada base cast harus ditolak");
    assert!(pesan.contains("part-select tidak valid"), "pesan: {pesan}");
    let tokens = sv_lexer::lex(sumber).expect("lex");
    let ast = sv_parser::parse_module(&tokens).expect("parse");
    let e = elaborate(&ast).expect_err("harus gagal");
    assert_eq!(
        e.span.line, 4,
        "span harus baris assign, dapat {:?}",
        e.span
    );
}

#[test]
fn pesan_select_dengan_base_panggilan_fungsi_menunjuk_ke_sumber() {
    let sumber =
        "module top;\n  logic [7:0] x;\n  logic [7:0] y;\n  function automatic [7:0] f(input [7:0] a); f = a; endfunction\n  assign y = f(x)[3:7];\nendmodule";
    let tokens = sv_lexer::lex(sumber).expect("lex");
    let ast = sv_parser::parse_module(&tokens).expect("parse");
    let e = elaborate(&ast).expect_err("harus gagal");
    assert_eq!(
        e.span.line, 5,
        "span harus baris assign, dapat {:?}",
        e.span
    );
}

#[test]
fn pesan_select_menyebut_baris_yang_benar() {
    let sumber = "module top;\n  logic [7:0] a, y;\n  assign y = a[0:3];\nendmodule";
    let pesan = err(sumber).expect("part-select terbalik harus ditolak");
    assert!(pesan.contains("part-select tidak valid"), "pesan: {pesan}");
    let tokens = sv_lexer::lex(sumber).expect("lex");
    let ast = sv_parser::parse_module(&tokens).expect("parse");
    let e = elaborate(&ast).expect_err("harus gagal");
    assert_eq!(
        e.span.line, 3,
        "span harus baris `assign`, dapat {:?}",
        e.span
    );
}
