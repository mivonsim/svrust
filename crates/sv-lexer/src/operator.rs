// Tanggung jawab: pindai operator dan tanda baca.
use crate::error::LexError;
use crate::span::Span;
use crate::token::{CompoundOp, Token};
use std::iter::Peekable;
use std::str::Chars;

#[cfg(test)]
use crate::lex;

/// Pindai compound assignment dua karakter, misal `+=` (LRM §11.3).
/// Mengembalikan None bila pasangan karakter bukan compound assignment.
pub fn scan_compound(
    ch: char,
    chars: &mut Peekable<Chars>,
    byte_offset: &mut usize,
    col: &mut usize,
    // Posisi awal tidak dipakai lagi: error "operator geser belum didukung"
    // yang pernah memakainya sudah hilang bersama `>>>` (LRM §11.4.10).
    _start_offset: usize,
    _start_line: usize,
    _start_col: usize,
) -> Result<Option<Token>, LexError> {
    let op = match (ch, chars.peek()) {
        ('+', Some('=')) => CompoundOp::Add,
        ('-', Some('=')) => CompoundOp::Sub,
        ('*', Some('=')) => CompoundOp::Mul,
        ('/', Some('=')) => CompoundOp::Div,
        ('%', Some('=')) => CompoundOp::Mod,
        ('&', Some('=')) => CompoundOp::BitAnd,
        ('|', Some('=')) => CompoundOp::BitOr,
        ('^', Some('=')) => CompoundOp::BitXor,
        ('<', Some('<')) | ('>', Some('>')) => {
            // `<<=`, `>>=`, `<<<=`, dan `>>>=` butuh tiga atau empat karakter.
            // `Peekable` hanya melihat satu karakter ke depan, jadi pakai
            // salinan iterator untuk memeriksa karakter berikutnya tanpa
            // mengonsumsi karakter yang sudah dilihat.
            let mut salinan = chars.clone();
            salinan.next();
            // `(CompoundOp, panjang)` — panjang memuat karakter setelah `ch`
            // yang sudah dibaca, jadi `<<=` panjang 2 dan `<<<=` panjang 3.
            let (geser, panjang) = match salinan.next() {
                Some('=') => (
                    if ch == '<' {
                        CompoundOp::Shl
                    } else {
                        CompoundOp::Shr
                    },
                    2,
                ),
                // `<<<` dan `>>>` hanya compound bila disusul `=`. Tanpa itu
                // operator biasa yang dibaca `scan_operator`.
                Some(ketiga) if ketiga == ch => {
                    // `salinan` sudah melewati karakter ketiga, jadi peek
                    // memeriksa karakter keempat.
                    if salinan.peek() != Some(&'=') {
                        return Ok(None);
                    }
                    // `<<<=` identik dengan `<<=`; `>>>=` memakai geser kanan
                    // aritmetik (LRM §11.4.10) sehingga dibedakan dari `>>=`.
                    let op = if ch == '<' {
                        CompoundOp::Shl
                    } else {
                        CompoundOp::Sar
                    };
                    (op, 3)
                }
                _ => return Ok(None),
            };
            for _ in 0..panjang {
                chars.next();
                *byte_offset += 1;
                *col += 1;
            }
            return Ok(Some(Token::CompoundAssign(geser)));
        }
        _ => return Ok(None),
    };
    chars.next();
    *byte_offset += 1;
    *col += 1;
    Ok(Some(Token::CompoundAssign(op)))
}

/// Pindai satu operator dimulai dari `ch`. Konsumsi lanjutan bila perlu.
pub fn scan_operator(
    ch: char,
    chars: &mut Peekable<Chars>,
    byte_offset: &mut usize,
    col: &mut usize,
    start_offset: usize,
    start_line: usize,
    start_col: usize,
) -> Result<Token, LexError> {
    let gagal = |pesan: String| {
        LexError::new(
            pesan,
            Span::new(start_offset, *byte_offset, start_line, start_col),
        )
    };
    let makan = |chars: &mut Peekable<Chars>, byte_offset: &mut usize, col: &mut usize| {
        *byte_offset += 1;
        *col += 1;
        chars.next();
    };

    match ch {
        '(' => Ok(Token::LParen),
        ')' => Ok(Token::RParen),
        '[' => Ok(Token::LBracket),
        ']' => Ok(Token::RBracket),
        '{' => Ok(Token::LBrace),
        '}' => Ok(Token::RBrace),
        ';' => Ok(Token::Semi),
        ':' => Ok(Token::Colon),
        ',' => Ok(Token::Comma),
        '#' => Ok(Token::Hash),
        '.' => Ok(Token::Dot),
        '?' => Ok(Token::Question),
        '~' => Ok(Token::Tilde),
        '+' => {
            // `++` harus dicek sebelum `+` biasa (LRM §12.5).
            if chars.peek() == Some(&'+') {
                makan(chars, byte_offset, col);
                Ok(Token::Incr)
            } else {
                Ok(Token::Plus)
            }
        }
        '-' => {
            if chars.peek() == Some(&'-') {
                makan(chars, byte_offset, col);
                Ok(Token::Decr)
            } else {
                Ok(Token::Minus)
            }
        }
        '*' => Ok(Token::Star),
        '/' => Ok(Token::Slash),
        '%' => Ok(Token::Percent),
        '@' => Ok(Token::At),
        '&' => {
            if chars.peek() == Some(&'&') {
                makan(chars, byte_offset, col);
                Ok(Token::AndAnd)
            } else {
                Ok(Token::And)
            }
        }
        '|' => {
            if chars.peek() == Some(&'|') {
                makan(chars, byte_offset, col);
                Ok(Token::OrOr)
            } else {
                Ok(Token::Or)
            }
        }
        '^' => Ok(Token::Xor),
        '!' => {
            if chars.peek() == Some(&'=') {
                makan(chars, byte_offset, col);
                Ok(Token::BangEq)
            } else {
                Ok(Token::Bang)
            }
        }
        '=' => {
            if chars.peek() == Some(&'=') {
                makan(chars, byte_offset, col);
                Ok(Token::EqEq)
            } else {
                Ok(Token::Eq)
            }
        }
        '<' => {
            if chars.peek() == Some(&'<') {
                makan(chars, byte_offset, col);
                // `<<<` identik dengan `<<`: LRM §11.4.10 menyatakan bahwa
                // pada geser kiri keduanya mengisi nol.
                if chars.peek() == Some(&'<') {
                    makan(chars, byte_offset, col);
                }
                Ok(Token::Shl)
            } else if chars.peek() == Some(&'=') {
                makan(chars, byte_offset, col);
                // LRM: `<=` ganda makna; konteks statement = nonblocking,
                // konteks ekspresi = less-equal (parser yang bedakan).
                Ok(Token::NonBlocking)
            } else {
                Ok(Token::Lt)
            }
        }
        '>' => {
            if chars.peek() == Some(&'>') {
                makan(chars, byte_offset, col);
                // `>>>` geser kanan aritmetik: mengisi bit sign bila tipe
                // hasil signed (LRM §11.4.10).
                if chars.peek() == Some(&'>') {
                    makan(chars, byte_offset, col);
                    return Ok(Token::Sar);
                }
                Ok(Token::Shr)
            } else if chars.peek() == Some(&'=') {
                makan(chars, byte_offset, col);
                Ok(Token::Ge)
            } else {
                Ok(Token::Gt)
            }
        }
        lain => Err(gagal(format!("unexpected character: {}", lain))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pindai(sumber: &str) -> Result<Token, LexError> {
        let mut chars = sumber.chars().peekable();
        let ch = chars.next().expect("sumber kosong");
        let mut offset = ch.len_utf8();
        let mut kolom = 2usize;
        scan_operator(ch, &mut chars, &mut offset, &mut kolom, 0, 1, 1)
    }

    #[test]
    fn ganda_terdeteksi() {
        assert_eq!(pindai("&&").unwrap(), Token::AndAnd);
        assert_eq!(pindai("==").unwrap(), Token::EqEq);
        assert_eq!(pindai("<=").unwrap(), Token::NonBlocking);
        assert_eq!(pindai("<<").unwrap(), Token::Shl);
        assert_eq!(pindai(">>").unwrap(), Token::Shr);
        assert_eq!(pindai(">=").unwrap(), Token::Ge);
    }

    #[test]
    fn tunggal_terdeteksi() {
        assert_eq!(pindai("&").unwrap(), Token::And);
        assert_eq!(pindai(";").unwrap(), Token::Semi);
        assert_eq!(pindai("<").unwrap(), Token::Lt);
        assert_eq!(pindai(">").unwrap(), Token::Gt);
    }

    #[test]
    fn karakter_asing_gagal() {
        assert!(pindai("\"").is_err());
    }

    #[test]
    fn compound_assign_terdeteksi() {
        assert_eq!(
            lex("+=").unwrap()[0].token,
            Token::CompoundAssign(CompoundOp::Add)
        );
        assert_eq!(
            lex("<<=").unwrap()[0].token,
            Token::CompoundAssign(CompoundOp::Shl)
        );
        assert_eq!(
            lex(">>=").unwrap()[0].token,
            Token::CompoundAssign(CompoundOp::Shr)
        );
    }

    #[test]
    fn bagi_dan_sisa_terdeteksi() {
        // BUG-16: `/` dan `%` harus punya token sendiri.
        assert_eq!(lex("a / b").unwrap()[1].token, Token::Slash);
        assert_eq!(lex("a % b").unwrap()[1].token, Token::Percent);
    }

    #[test]
    fn compound_bagi_dan_sisa_terdeteksi() {
        // BUG-17: `/=` dan `%=` harus jadi compound assignment.
        assert_eq!(
            lex("/=").unwrap()[0].token,
            Token::CompoundAssign(CompoundOp::Div)
        );
        assert_eq!(
            lex("%=").unwrap()[0].token,
            Token::CompoundAssign(CompoundOp::Mod)
        );
    }

    #[test]
    fn increment_dan_decrement_terdeteksi() {
        // BUG-20: `++` dan `--` harus jadi token sendiri, bukan dua `+`/`-`.
        assert_eq!(lex("i++").unwrap()[1].token, Token::Incr);
        assert_eq!(lex("i--").unwrap()[1].token, Token::Decr);
    }

    #[test]
    fn plus_tunggal_tetap_berfungsi() {
        // Regresi: `a + b` tidak boleh berubah jadi increment.
        let tokens = lex("a + b").unwrap();
        assert_eq!(tokens[1].token, Token::Plus);
        assert_eq!(tokens[2].token, Token::Ident("b".to_string()));
    }

    #[test]
    fn geser_tanpa_samadengan_ditolak() {
        assert!(lex("<<").is_err() || lex("<<").unwrap()[0].token == Token::Shl);
    }

    #[test]
    fn geser_aritmetik_dibatasi_tiga_karakter() {
        // LRM §11.4.10: `<<<` identik dengan `<<` (keduanya mengisi nol),
        // sedangkan `>>>` geser kanan aritmetik dan butuh token sendiri.
        assert_eq!(lex("<<<").unwrap()[0].token, Token::Shl);
        assert_eq!(lex("a <<< b").unwrap()[1].token, Token::Shl);
        assert_eq!(lex("a >>> b").unwrap()[1].token, Token::Sar);
        // `>>>` tidak boleh salah dibaca sebagai `>>` diikuti `>`.
        assert_eq!(
            lex(">>>=").unwrap()[0].token,
            Token::CompoundAssign(CompoundOp::Sar)
        );
        assert_eq!(
            lex("<<<= x").unwrap()[0].token,
            Token::CompoundAssign(CompoundOp::Shl)
        );
        // `>>=` dan `<<=` tetap compound logika.
        assert_eq!(
            lex(">>=").unwrap()[0].token,
            Token::CompoundAssign(CompoundOp::Shr)
        );
    }
}
