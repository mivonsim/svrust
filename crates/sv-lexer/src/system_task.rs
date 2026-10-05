// Tanggung jawab: pindai system task berawalan `$` seperti `$display`.
use crate::error::LexError;
use crate::span::Span;
use crate::spanned::SpannedToken;
use crate::token::Token;
use std::iter::Peekable;
use std::str::Chars;

/// Pindai system task, misal `$display`, `$finish`.
///
/// Nama task dibaca sampai karakter bukan alphanumeric/underscore. Task yang
/// belum dikenal tetap dikembalikan sebagai `Token::SystemTask` supaya parser
/// bisa memberi pesan error yang menyebut nama aslinya.
pub fn scan_system_task(
    chars: &mut Peekable<Chars>,
    byte_offset: &mut usize,
    col: &mut usize,
    start_offset: usize,
    start_line: usize,
    start_col: usize,
) -> Result<SpannedToken, LexError> {
    // Lewati tanda `$` sendiri; pemanggil sudah dikonsumsi karakter itu.
    let mut name = String::new();
    while let Some(&c) = chars.peek() {
        if c.is_alphanumeric() || c == '_' || c == '$' {
            name.push(c);
            chars.next();
            *byte_offset += c.len_utf8();
            *col += 1;
        } else {
            break;
        }
    }

    let token = match name.as_str() {
        "display" => Token::SystemTask("display".to_string()),
        "finish" => Token::SystemTask("finish".to_string()),
        _ => Token::SystemTask(name),
    };

    Ok(SpannedToken::new(
        token,
        Span::new(start_offset, *byte_offset, start_line, start_col),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lex;

    #[test]
    fn display_dikenali() {
        assert_eq!(
            lex("$display").unwrap()[0].token,
            Token::SystemTask("display".to_string())
        );
    }

    #[test]
    fn finish_dikenali() {
        assert_eq!(
            lex("$finish").unwrap()[0].token,
            Token::SystemTask("finish".to_string())
        );
    }

    #[test]
    fn task_tidak_dikenal_tetap_kembalikan_nama() {
        // supaya pesan error bisa menyebut nama aslinya.
        match &lex("$unknown").unwrap()[0].token {
            Token::SystemTask(nama) => assert_eq!(nama, "unknown"),
            lain => panic!("dapat {:?}", lain),
        }
    }

    #[test]
    fn dollar_terpisah_dari_nama() {
        let tokens = lex("$display ( \"x\" )").expect("lex");
        assert_eq!(tokens[0].token, Token::SystemTask("display".to_string()));
        assert_eq!(tokens[1].token, Token::LParen);
    }

    #[test]
    fn argumen_string_berikutnya_terlex() {
        // $display harus bisa diikuti string literal sebagai argumen.
        let tokens = lex("$display ( \"x\" )").expect("lex");
        assert_eq!(tokens[2].token, Token::StringLiteral("x".to_string()));
    }
}
