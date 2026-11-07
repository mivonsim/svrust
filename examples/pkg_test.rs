fn main() {
    let path = "opentitan/hw/ip/rram_ctrl/rtl/rram_ctrl_pkg.sv";
    let src = std::fs::read_to_string(path).unwrap();
    let pp = sv_preprocessor::preprocess(&src, None).unwrap();
    let tokens = sv_lexer::lex(&pp.source).unwrap();
    for (i, t) in tokens.iter().enumerate() {
        if matches!(t.token, sv_lexer::token::Token::Package) {
            println!("FOUND Package at token {}!", i);
            for j in i..i+10.min(tokens.len()) {
                println!("  [{}] {:?}", j, tokens[j].token);
            }
        }
    }
    // Also check parse_file
    let parsed = sv_parser::parse_file(&tokens).unwrap();
    println!("packages: {:?}", parsed.packages.iter().map(|p| &p.name).collect::<Vec<_>>());
    println!("modules: {}", parsed.modules.len());
}
