// Tanggung jawab: test konversi pada batas assignment (LRM §11.6.1, Tabel 11-21).
mod support;

use support::evaluasi;
//
// Ekspresi kanan pada assignment adalah context-determined. Dua aturan yang
// diuji di sini:
//   - lebar ekspresi disesuaikan ke lebar target (memotong atau melebar);
//   - arah perluasan mengikuti signedness *ekspresi* itu sendiri, bukan target.
//
// Bug yang diperbaiki: batas assignment dulu hanya memotong, jadi
// `logic [31:0] o = sa` dengan `sa` signed 8-bit menghasilkan 0x000000FD
// alih-alih 0xFFFFFFFD — membuat `signed'(x)` jadi no-op di setiap assignment.
//
// Semua nilai diverifikasi terhadap iverilog 12.0 (`-g2012`).

/// Source standard: `a` signed 8-bit = -3, `u` unsigned 8-bit = 0xFD.
const SRC: &str = "\
module m(input signed [7:0] a, input [7:0] u, output [31:0] y);
  assign y = EXPR;
endmodule";

fn eval(expr: &str, isi: &[(&str, u64)]) -> u64 {
    let src = SRC.replace("EXPR", expr);
    let tokens = sv_lexer::lex(&src).expect("lex");
    let ast = sv_parser::parse_module(&tokens).expect("parse");
    let design = sv_elaborator::elaborate(&ast).expect("elaborate");
    let expr_ir = support::nilai_rhs(&design);
    let mut signals = vec![0u64; design.variables.len()];
    for (nama, nilai) in isi {
        let var = design.find_variable(nama).expect("sinyal ada");
        signals[var.signal_id as usize] = *nilai;
    }
    evaluasi(&expr_ir, &mut signals)
}

fn eval_a(expr: &str) -> u64 {
    eval(expr, &[("a", 0xFD)])
}

fn eval_u(expr: &str) -> u64 {
    eval(expr, &[("u", 0xFD)])
}

#[test]
fn melebar_dari_operand_signed_sign_extend() {
    // LRM §11.6.1: operand signed di-sign-extend saat assignment melebar.
    // iverilog: fffffffd
    assert_eq!(eval_a("a"), 0xFFFFFFFD);
}

#[test]
fn melebar_dari_operand_unsigned_zero_extend() {
    // Kontras langsung: operand unsigned di-zero-extend.
    // iverilog: 000000fd
    assert_eq!(eval_u("u"), 0x000000FD);
}

#[test]
fn lebarnya_ekspresi_kanan_disesuaikan_ke_target() {
    // Ekspresi kanan adalah context-determined, jadi lebarnya harus sama
    // dengan lebar target setelah assignment — bukan lebar operand aslinya.
    let src = "module m(input signed [7:0] a, output [15:0] y);\n  assign y = a;\nendmodule";
    let tokens = sv_lexer::lex(src).expect("lex");
    let ast = sv_parser::parse_module(&tokens).expect("parse");
    let design = sv_elaborator::elaborate(&ast).expect("elaborate");
    assert_eq!(support::nilai_rhs(&design).data_type().width, 16);
}

#[test]
fn lebar_sama_tidak_membungkus_cast() {
    // Tanpa perubahan lebar tidak perlu `Cast`; membungkusnya hanya menambah
    // node tanpa efek.
    let src = "module m(input signed [7:0] a, output [7:0] y);\n  assign y = a;\nendmodule";
    let tokens = sv_lexer::lex(src).expect("lex");
    let ast = sv_parser::parse_module(&tokens).expect("parse");
    let design = sv_elaborator::elaborate(&ast).expect("elaborate");
    let expr = support::nilai_rhs(&design);
    assert!(
        !matches!(expr, sv_ir::Expr::Cast { .. }),
        "lebar sama tidak perlu cast: {expr:?}"
    );
    assert_eq!(expr.data_type().width, 8);
}

#[test]
fn memotong_ke_target_lebih_sempit() {
    // LRM §11.6.1: target lebih sempit memotong bit atas.
    // iverilog: d
    let src = "module m(input signed [7:0] a, output [3:0] y);\n  assign y = a;\nendmodule";
    let tokens = sv_lexer::lex(src).expect("lex");
    let ast = sv_parser::parse_module(&tokens).expect("parse");
    let design = sv_elaborator::elaborate(&ast).expect("elaborate");
    let mut signals = vec![0u64; design.variables.len()];
    signals[design.find_variable("a").expect("a ada").signal_id as usize] = 0xFD;
    assert_eq!(evaluasi(&support::nilai_rhs(&design), &mut signals), 0xD);
}

#[test]
fn signedness_target_tidak_mempengaruhi_arah_perluasan() {
    // Aturan assignment melihat signedness ekspresi, bukan target: `u`
    // unsigned harus tetap zero-extend walau target signed.
    let src = "module m(input [7:0] u, output signed [31:0] y);\n  assign y = u;\nendmodule";
    let tokens = sv_lexer::lex(src).expect("lex");
    let ast = sv_parser::parse_module(&tokens).expect("parse");
    let design = sv_elaborator::elaborate(&ast).expect("elaborate");
    let mut signals = vec![0u64; design.variables.len()];
    signals[design.find_variable("u").expect("u ada").signal_id as usize] = 0xFD;
    // iverilog: 000000fd
    assert_eq!(
        evaluasi(&support::nilai_rhs(&design), &mut signals),
        0x000000FD
    );
}

#[test]
fn operand_signed_ke_target_signed_sign_extend() {
    let src = "module m(input signed [7:0] a, output signed [31:0] y);\n  assign y = a;\nendmodule";
    let tokens = sv_lexer::lex(src).expect("lex");
    let ast = sv_parser::parse_module(&tokens).expect("parse");
    let design = sv_elaborator::elaborate(&ast).expect("elaborate");
    let mut signals = vec![0u64; design.variables.len()];
    signals[design.find_variable("a").expect("a ada").signal_id as usize] = 0xFD;
    // iverilog: fffffffd
    assert_eq!(
        evaluasi(&support::nilai_rhs(&design), &mut signals),
        0xFFFFFFFD
    );
}

#[test]
fn assignment_dengan_campuran_tetap_mengikuti_ekspresi() {
    // `sa * 2` menghasilkan ekspresi signed 32-bit (LRM §11.6.1: `2` signed
    // karena literal polos bertipe `integer`), jadi assignment tetap
    // sign-extend.
    // iverilog: fffffffa
    assert_eq!(eval_a("a * 2"), 0xFFFFFFFA);
}

#[test]
fn assignment_dengan_literal_polos_menjaga_signed() {
    // LRM §5.7.1: literal desimal polos bertipe `integer` 32-bit signed, jadi
    // `a + 0` tetap signed.
    // iverilog: fffffffd
    assert_eq!(eval_a("a + 0"), 0xFFFFFFFD);
}

#[test]
fn assignment_dengan_size_cast_ikuti_signed_operand() {
    // `32'(a)` tidak mengubah lebar maupun tanda, jadi hasilnya sama dengan
    // `a` polos.
    // iverilog: fffffffd
    assert_eq!(eval_a("32'(a)"), 0xFFFFFFFD);
}

#[test]
fn lhs_teriris_memakai_lebar_irisan_bukan_lebar_sinyal() {
    // LRM §11.6.1: untuk `y[15:0] = a` lebar konteksnya 16 bit, bukan 32 bit
    // milik `y`. Kalau penuh, `a` yang 8-bit akan tetap sama — tapi untuk
    // operand yang lebih lebar dari irisan hasilnya berbeda.
    let src = "\
module m(input signed [7:0] a, output [31:0] y);
  assign y = 16'b0;
  assign y[15:0] = a;
endmodule";
    let tokens = sv_lexer::lex(src).expect("lex");
    let ast = sv_parser::parse_module(&tokens).expect("parse");
    let design = sv_elaborator::elaborate(&ast).expect("elaborate");
    // Assignment ke irisan harus punya slice, dan lebar ekspresinya 16.
    let iris_assign = design
        .processes
        .iter()
        .flat_map(|p| p.body.iter())
        .find_map(|s| match s {
            sv_ir::Statement::Assign { assignment, .. } if assignment.slice.is_some() => {
                Some(assignment)
            }
            _ => None,
        })
        .expect("ada assignment ke irisan");
    let slice = iris_assign.slice.expect("slice ada");
    assert_eq!(slice.width(), 16);
    assert_eq!(iris_assign.value.data_type().width, 16);
    let mut signals = vec![0u64; design.variables.len()];
    signals[design.find_variable("a").expect("a ada").signal_id as usize] = 0xFD;
    assert_eq!(evaluasi(&iris_assign.value, &mut signals), 0xFFFD);
}

#[test]
fn perbandingan_tidak_terpengaruh_konversi_assignment() {
    // `a < 0` harus signed: -3 < 0 = true. Kalau batas assignment ikut
    // memotong operand ke lebar target, hasil perbandingan bisa berubah.
    // iverilog: 01
    assert_eq!(eval_a("(a < 0) ? 32'd1 : 32'd0"), 1);
}

#[test]
fn perbandingan_operand_berbeda_lebar_memakai_lebar_maks() {
    // LRM §11.6.1: perbandingan adalah context-determined antar-operand, jadi
    // lebarnya max keduanya — bukan lebar hasil 1 bit. Memotong ke 1 bit akan
    // membuat `a > 16'h0100` selalu benar.
    let src = "\
module m(input [15:0] a, output [31:0] y);
  assign y = (a > 16'h0100) ? 32'd1 : 32'd0;
endmodule";
    let tokens = sv_lexer::lex(src).expect("lex");
    let ast = sv_parser::parse_module(&tokens).expect("parse");
    let mut design = sv_elaborator::elaborate(&ast).expect("elaborate");
    sv_opt::optimize(&mut design);
    let mut signals = vec![0u64; design.variables.len()];
    let var = design.find_variable("a").expect("a ada");
    for (nilai, harap) in [(0x0200u64, 1u64), (0x0001, 0)] {
        signals[var.signal_id as usize] = nilai;
        assert_eq!(
            evaluasi(&support::nilai_rhs(&design), &mut signals),
            harap,
            "a = {nilai:#06x}"
        );
    }
}
