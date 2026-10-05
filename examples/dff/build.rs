fn main() {
    let src = "module dff(input clk, input d, output q); always_ff @(posedge clk) begin q <= d; end endmodule";
    let tokens = sv_lexer::lex(src).expect("lex");
    let ast = sv_parser::parse_module(&tokens).expect("parse");
    let design = sv_elaborator::elaborate(&ast).expect("elaborate");
    println!("{}", sv_codegen_rust::generate_module(&design));
}
