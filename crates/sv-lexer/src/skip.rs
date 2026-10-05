// Tanggung jawab: lewati whitespace dan komentar.
use std::iter::Peekable;
use std::str::Chars;

/// Coba lewati satu unit trivia. Kembalikan true bila ada yang dilewati.
pub fn skip_trivia(
    chars: &mut Peekable<Chars>,
    line: &mut usize,
    col: &mut usize,
    byte_offset: &mut usize,
) -> bool {
    let ch = match chars.peek() {
        Some(&c) => c,
        None => return false,
    };

    // Spasi kosong.
    if ch.is_whitespace() {
        if ch == '\n' {
            *line += 1;
            *col = 1;
        } else {
            *col += 1;
        }
        *byte_offset += ch.len_utf8();
        chars.next();
        return true;
    }

    // Komentar baris //...
    if ch == '/' && chars.clone().nth(1) == Some('/') {
        *byte_offset += 2;
        *col += 2;
        chars.next();
        chars.next();
        while let Some(&c) = chars.peek() {
            if c == '\n' {
                break;
            }
            *byte_offset += c.len_utf8();
            *col += 1;
            chars.next();
        }
        return true;
    }

    // Komentar blok /* ... */.
    if ch == '/' && chars.clone().nth(1) == Some('*') {
        *byte_offset += 2;
        *col += 2;
        chars.next();
        chars.next();
        while let Some(&c) = chars.peek() {
            if c == '*' && chars.clone().nth(1) == Some('/') {
                *byte_offset += 2;
                *col += 2;
                chars.next();
                chars.next();
                break;
            }
            if c == '\n' {
                *line += 1;
                *col = 1;
            } else {
                *col += 1;
            }
            *byte_offset += c.len_utf8();
            chars.next();
        }
        return true;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lewati(sumber: &str) -> (usize, usize, usize) {
        let mut chars = sumber.chars().peekable();
        let mut baris = 1usize;
        let mut kolom = 1usize;
        let mut offset = 0usize;
        while skip_trivia(&mut chars, &mut baris, &mut kolom, &mut offset) {}
        (baris, kolom, offset)
    }

    #[test]
    fn spasi_dilewati() {
        let (baris, _, _) = lewati("   \nmodule");
        assert_eq!(baris, 2);
    }

    #[test]
    fn komentar_baris_dilewati() {
        let (_, _, offset) = lewati("// halo");
        assert_eq!(offset, "// halo".len());
    }

    #[test]
    fn komentar_blok_dilewati() {
        let (baris, _, _) = lewati("/* a\nb */");
        assert_eq!(baris, 2);
    }
}
