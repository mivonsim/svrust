// Tanggung jawab: belah token berkas menjadi daftar module.
use sv_ast::module::Module;
use sv_lexer::spanned::SpannedToken;
use sv_lexer::token::Token;

/// Parse seluruh token berkas menjadi daftar module.
/// Setiap irisan `module...endmodule` diparse terpisah.
pub fn parse_file(tokens: &[SpannedToken]) -> Result<Vec<Module>, String> {
    let mut out = Vec::new();
    let mut idx = 0;
    while idx < tokens.len() {
        if tokens[idx].token != Token::Module {
            return Err(format!("expected 'module', dapat {:?}", tokens[idx].token));
        }
        let awal = idx;
        let mut akhir = None;
        for (rel, t) in tokens[idx..].iter().enumerate() {
            if t.token == Token::Endmodule {
                akhir = Some(idx + rel);
                break;
            }
        }
        let akhir = akhir.ok_or_else(|| "expected 'endmodule'".to_string())?;
        let modul = super::module::parse_module(&tokens[awal..=akhir])?;
        // Tolak nama module duplikat dalam satu berkas.
        if out.iter().any(|m: &Module| m.name == modul.name) {
            return Err(format!("module duplikat: {}", modul.name));
        }
        out.push(modul);
        idx = akhir + 1;
    }
    if out.is_empty() {
        return Err("berkas kosong: tidak ada module".to_string());
    }
    Ok(out)
}
