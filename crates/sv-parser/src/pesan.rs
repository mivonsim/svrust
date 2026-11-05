// Tanggung jawab: pembentukan pesan error parser beserta posisi token penyebab.
use sv_lexer::spanned::SpannedToken;

/// Penanda posisi yang disisipkan ke pesan error.
///
/// Pelapor diagnostik (`svrust::pipeline`) membacanya untuk menunjuk token yang
/// benar-benar gagal di-parse. Tanpa penanda, error yang tidak menyebut nama
/// token jatuh ke baris terakhir berkas (`endmodule`) dan menyesatkan pembaca.
pub const PENANDA: &str = " [pos=";

/// Pesan `apa` yang menyebut token pada `idx` beserta posisinya.
pub fn harap(tokens: &[SpannedToken], idx: usize, apa: &str) -> String {
    match tokens.get(idx) {
        Some(token) => tandai(tokens, idx, format!("{apa}, ditemukan {:?}", token.token)),
        None => format!("{apa}, tetapi input habis"),
    }
}

/// Sisipkan penanda posisi `idx` ke `pesan`, bila belum ada penanda.
pub fn tandai(tokens: &[SpannedToken], idx: usize, pesan: String) -> String {
    if pesan.contains(PENANDA) {
        return pesan;
    }
    match tokens.get(idx) {
        Some(token) => format!("{pesan}{PENANDA}{}:{}]", token.span.line, token.span.col),
        None => pesan,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lex(source: &str) -> Vec<SpannedToken> {
        sv_lexer::lex(source).expect("lex")
    }

    #[test]
    fn harap_menyebut_token_dan_posisinya() {
        let tokens = lex("assign a b;");
        let pesan = harap(&tokens, 2, "expected '='");
        assert!(pesan.contains("Ident(\"b\")"), "{pesan}");
        assert!(pesan.ends_with("[pos=1:10]"), "{pesan}");
    }

    #[test]
    fn tandai_tidak_menimpa_penanda_yang_sudah_ada() {
        let tokens = lex("a\nb");
        let pesan = tandai(&tokens, 0, "pertama [pos=9:9]".to_string());
        assert_eq!(pesan, "pertama [pos=9:9]");
        assert_eq!(tandai(&tokens, 1, "kedua".to_string()), "kedua [pos=2:1]");
    }

    #[test]
    fn tandai_tanpa_token_tidak_menambah_penanda() {
        let tokens = lex("");
        assert_eq!(tandai(&tokens, 0, "kosong".to_string()), "kosong");
    }
}
