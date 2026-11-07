fn main() {
    let src = std::fs::read_to_string("opentitan/hw/ip/rram_ctrl/rtl/rram_ctrl_pkg.sv").unwrap();
    let pp = sv_preprocessor::preprocess(&src, None).unwrap();
    let tokens = sv_lexer::lex(&pp.source).unwrap();
    for (i, t) in tokens.iter().enumerate() {
        if matches!(t.token, sv_lexer::token::Token::Package) {
            println!("Found Package at token {}: {:?}", i, t.token);
            // Print next 5 tokens
            for j in i..i+6.min(tokens.len()) {
                println!("  token {}: {:?}", j, tokens[j].token);
            }
            break;
        }
    }
    println!("Total tokens: {}", tokens.len());
}
