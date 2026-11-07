// Tanggung jawab: test deklarasi supply0/supply1 (LRM §6.6).
use sv_elaborator::elaborate;
use sv_lexer::lex;
use sv_parser::parse_module;

fn elaborate_sv(source: &str) {
    let tokens = lex(source).expect("lex");
    let ast = parse_module(&tokens).expect("parse");
    elaborate(&ast).expect("supply0/supply1 harus dapat dielaborasi");
}

#[test]
fn supply0_supply1_dapat_dielaborasi() {
    elaborate_sv("module top; supply0 z; supply1 o; endmodule");
}

#[test]
fn supply0_supply1_bersama_wire_dan_logic() {
    elaborate_sv("module top; supply0 z; supply1 o; wire w; logic [7:0] v; endmodule");
}
