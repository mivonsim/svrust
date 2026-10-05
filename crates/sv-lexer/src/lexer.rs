// Tanggung jawab: orkestrasi pemindaian token SystemVerilog.
use crate::error::LexError;
use crate::keyword::keyword_or_ident;
use crate::literal::{scan_number, scan_string};
use crate::operator::{scan_compound, scan_operator};
use crate::skip::skip_trivia;
use crate::span::Span;
use crate::spanned::SpannedToken;
use crate::system_task::scan_system_task;
use crate::token::Token;
use std::iter::Peekable;

pub fn lex(source: &str) -> Result<Vec<SpannedToken>, LexError> {
    let mut tokens = Vec::new();
    let mut chars = source.chars().peekable();
    let mut line = 1usize;
    let mut col = 1usize;
    let mut byte_offset = 0usize;
    while chars.peek().is_some() {
        if skip_trivia(&mut chars, &mut line, &mut col, &mut byte_offset) {
            continue;
        }
        let ch = match chars.peek() {
            Some(&c) => c,
            None => break,
        };
        let start_line = line;
        let start_col = col;
        let start_offset = byte_offset;
        if ch == '`' {
            tokens.push(scan_directive(
                &mut chars,
                &mut line,
                &mut col,
                &mut byte_offset,
                start_offset,
                start_line,
                start_col,
            ));
            continue;
        }
        // String literal diawali `"` (LRM §5.7.3).
        if ch == '"' {
            byte_offset += ch.len_utf8();
            col += 1;
            chars.next();
            let token = scan_string(
                &mut chars,
                &mut byte_offset,
                &mut col,
                start_offset,
                start_line,
                start_col,
            )?;
            tokens.push(SpannedToken::new(
                token,
                Span::new(start_offset, byte_offset, start_line, start_col),
            ));
            continue;
        }
        // System task diawali `$`, misal `$display` (LRM §20).
        if ch == '$' {
            byte_offset += ch.len_utf8();
            col += 1;
            chars.next();
            tokens.push(scan_system_task(
                &mut chars,
                &mut byte_offset,
                &mut col,
                start_offset,
                start_line,
                start_col,
            )?);
            continue;
        }
        if ch.is_alphabetic() || ch == '_' {
            tokens.push(scan_ident(
                &mut chars,
                &mut col,
                &mut byte_offset,
                start_offset,
                start_line,
                start_col,
            ));
            continue;
        }
        // Angka desimal polos atau based literal sized seperti `8'hFF` (LRM §5.7.1).
        // Tanda `'` yang tidak diikuti basis adalah awal type cast (LRM §6.14),
        // misalnya `byte_t'(x)` atau `signed'(x)`.
        if ch == '\'' && !mulai_based_literal(chars.clone()) {
            chars.next();
            byte_offset += 1;
            col += 1;
            tokens.push(SpannedToken::new(
                Token::Cast,
                Span::new(start_offset, byte_offset, start_line, start_col),
            ));
            continue;
        }
        if ch.is_ascii_digit() || ch == '\'' {
            let token = scan_number(
                &mut chars,
                &mut line,
                &mut byte_offset,
                &mut col,
                start_offset,
                start_line,
                start_col,
            )?;
            tokens.push(SpannedToken::new(
                token,
                Span::new(start_offset, byte_offset, start_line, start_col),
            ));
            continue;
        }
        byte_offset += ch.len_utf8();
        col += 1;
        chars.next();

        // Compound assignment punya prioritas di atas operator biasa:
        // `+=` harus dikenali sebelum `+` biasa.
        let token = match scan_compound(
            ch,
            &mut chars,
            &mut byte_offset,
            &mut col,
            start_offset,
            start_line,
            start_col,
        )? {
            Some(token) => token,
            None => scan_operator(
                ch,
                &mut chars,
                &mut byte_offset,
                &mut col,
                start_offset,
                start_line,
                start_col,
            )?,
        };
        tokens.push(SpannedToken::new(
            token,
            Span::new(start_offset, byte_offset, start_line, start_col),
        ));
    }
    Ok(tokens)
}

/// True bila `'` diikuti pola based literal, yaitu basis opsional bertanda
/// `s` lalu basis wajib `b`/`o`/`d`/`h` (LRM §5.7.1).
///
/// Kalau tidak, `'` adalah awal type cast (LRM §6.14) seperti `byte_t'(x)`.
fn mulai_based_literal(mut chars: Peekable<std::str::Chars>) -> bool {
    // Pemanggil memanggil fungsi ini saat `chars` masih menunjuk ke `'` itu
    // sendiri, jadi karakter itu harus dilewati lebih dulu.
    chars.next();
    if matches!(chars.peek(), Some('s') | Some('S')) {
        chars.next();
    }
    matches!(
        chars.peek(),
        Some('b')
            | Some('o')
            | Some('d')
            | Some('h')
            | Some('B')
            | Some('O')
            | Some('D')
            | Some('H')
    )
}

#[cfg(test)]
mod cast_tests {
    use super::*;
    use crate::lex;

    #[test]
    fn apos_pada_based_literal_bukan_cast() {
        // LRM §5.7.1: `8'hFF` tetap satu token angka.
        assert_eq!(lex("8'hFF").expect("lex").len(), 1);
        assert_eq!(lex("'o17").expect("lex").len(), 1);
        assert_eq!(lex("4'sb1010").expect("lex").len(), 1);
    }

    #[test]
    fn apos_sebelum_kurung_adalah_cast() {
        // LRM §6.14: `byte_t'(x)` memisahkan `'` sebagai token sendiri.
        let tokens = lex("byte_t'(8'h01)").expect("lex");
        let apos: Vec<usize> = tokens
            .iter()
            .enumerate()
            .filter(|(_, t)| t.token == Token::Cast)
            .map(|(i, _)| i)
            .collect();
        assert_eq!(apos, vec![1]);
    }

    #[test]
    fn apos_tanpa_basis_dan_tanpa_kurung_tetap_cast() {
        // `x'` diakhiri titik koma, jadi bukan based literal.
        let tokens = lex("signed'(x)").expect("lex");
        assert!(tokens.iter().any(|t| t.token == Token::Cast));
    }

    #[test]
    fn apos_berikutnya_basis_kecil_besar_keduanya_berlaku() {
        assert_eq!(lex("8'HFF").expect("lex").len(), 1);
        assert_eq!(lex("8'b1010").expect("lex").len(), 1);
    }
}

fn scan_directive(
    chars: &mut std::iter::Peekable<std::str::Chars>,
    _line: &mut usize,
    col: &mut usize,
    byte_offset: &mut usize,
    start_offset: usize,
    start_line: usize,
    start_col: usize,
) -> SpannedToken {
    chars.next();
    *byte_offset += 1;
    *col += 1;
    let mut ident = String::new();
    while let Some(&c) = chars.peek() {
        if c.is_alphanumeric() || c == '_' {
            ident.push(c);
            *byte_offset += c.len_utf8();
            *col += 1;
            chars.next();
        } else {
            break;
        }
    }
    SpannedToken::new(
        Token::Directive(ident),
        Span::new(start_offset, *byte_offset, start_line, start_col),
    )
}

fn scan_ident(
    chars: &mut std::iter::Peekable<std::str::Chars>,
    col: &mut usize,
    byte_offset: &mut usize,
    start_offset: usize,
    start_line: usize,
    start_col: usize,
) -> SpannedToken {
    let mut ident = String::new();
    while let Some(&c) = chars.peek() {
        if c.is_alphanumeric() || c == '_' {
            ident.push(c);
            *byte_offset += c.len_utf8();
            *col += 1;
            chars.next();
        } else {
            break;
        }
    }
    let token = keyword_or_ident(&ident);
    SpannedToken::new(
        token,
        Span::new(start_offset, *byte_offset, start_line, start_col),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lex_kosong() {
        assert_eq!(lex("").unwrap(), vec![]);
    }
    #[test]
    fn lex_module_sederhana() {
        let tokens = lex("module hello; endmodule").unwrap();
        assert_eq!(tokens[0].token, Token::Module);
        assert_eq!(tokens[3].token, Token::Endmodule);
    }
    #[test]
    fn lex_komentar_dilewati() {
        let tokens = lex("// komen\nmodule").unwrap();
        assert_eq!(tokens.len(), 1);
    }
    #[test]
    fn lex_direktif_dan_angka() {
        let tokens = lex("`define FOO 42").unwrap();
        assert_eq!(tokens[0].token, Token::Directive("define".to_string()));
        assert_eq!(tokens[2].token, Token::Number(42));
    }
    #[test]
    fn lex_karakter_asing_gagal() {
        assert!(lex("\"").is_err());
    }
}
