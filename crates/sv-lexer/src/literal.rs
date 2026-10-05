// Tanggung jawab: pemindaian literal angka SystemVerilog, termasuk based literal.
use crate::error::LexError;
use crate::skip::skip_trivia;
use crate::span::Span;
use crate::token::Token;
use std::iter::Peekable;
use std::str::Chars;

/// Basis penulisan digit pada based literal (LRM §5.7.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Basis {
    Binary,
    Octal,
    Decimal,
    Hex,
}

impl Basis {
    /// Petakan huruf basis (`b`/`o`/`d`/`h`, huruf besar juga sah).
    pub fn from_char(ch: char) -> Option<Self> {
        match ch {
            'b' | 'B' => Some(Basis::Binary),
            'o' | 'O' => Some(Basis::Octal),
            'd' | 'D' => Some(Basis::Decimal),
            'h' | 'H' => Some(Basis::Hex),
            _ => None,
        }
    }

    /// Nilai digit `ch` pada basis ini, atau None bila bukan digit sah.
    pub fn digit_value(self, ch: char) -> Option<u32> {
        let radix = match self {
            Basis::Binary => 2,
            Basis::Octal => 8,
            Basis::Decimal => 10,
            Basis::Hex => 16,
        };
        ch.to_digit(radix)
    }

    /// Jumlah bit yang diwakili satu digit pada basis ini.
    pub fn bits_per_digit(self) -> u32 {
        match self {
            Basis::Binary => 1,
            Basis::Octal => 3,
            // Basis desimal tidak punya representasi bit per digit.
            Basis::Decimal => 0,
            Basis::Hex => 4,
        }
    }
}

/// Klasifikasi digit `x`/`z`/`?` pada based literal (LRM §5.7.1).
///
/// Engine ini 2-state, jadi nilainya di-collapse ke 0; yang tetap disimpan
/// adalah penanda bit unknown agar `casez`/`casex` bisa memperlakukannya
/// sebagai wildcard.
pub fn is_unknown_digit(ch: char) -> bool {
    matches!(ch, 'x' | 'X' | 'z' | 'Z' | '?')
}

/// Lebar default based literal tanpa ukuran eksplisit (LRM §5.7.1).
pub const LEBAR_UNSIZED: u32 = 32;

/// Pindai literal angka; `ch` sudah dipastikan digit desimal atau tanda `'`.
pub fn scan_number(
    chars: &mut Peekable<Chars>,
    line: &mut usize,
    byte_offset: &mut usize,
    col: &mut usize,
    start_offset: usize,
    start_line: usize,
    start_col: usize,
) -> Result<Token, LexError> {
    let span = |byte_offset: &usize| Span::new(start_offset, *byte_offset, start_line, start_col);
    let adv = |chars: &mut Peekable<Chars>, byte_offset: &mut usize, col: &mut usize| {
        *byte_offset += 1;
        *col += 1;
        chars.next();
    };

    // Bagian sebelum tanda kutip adalah ukuran opsional, misal `8` pada `8'hFF`.
    let mut ukuran_teks = String::new();
    while let Some(&c) = chars.peek() {
        if c.is_ascii_digit() || c == '_' {
            if c != '_' {
                ukuran_teks.push(c);
            }
            adv(chars, byte_offset, col);
        } else {
            break;
        }
    }

    // Tanpa tanda kutip basis: literal desimal polos seperti `42`.
    if chars.peek() != Some(&'\'') {
        if ukuran_teks.is_empty() {
            return Err(LexError::new("invalid number", span(byte_offset)));
        }
        let value = ukuran_teks
            .parse::<u64>()
            .map_err(|_| LexError::new("invalid number", span(byte_offset)))?;
        return Ok(Token::Number(value));
    }

    adv(chars, byte_offset, col); // consume `'`

    // LRM §6.14: `16'(x)` adalah size cast — lebar eksplisit tanpa nama tipe.
    // Karakter setelah `'` adalah `(` pada bentuk ini, bukan basis digit, jadi
    // harus diperiksa sebelum basis. Spasi di antaranya legal (`16' (x)`),
    // jadi trivia dilewati lebih dulu; `skip_trivia` idempoten sehingga tidak
    // ada yang terlewat saat loop utama berjalan lagi.
    skip_trivia(chars, line, col, byte_offset);
    if chars.peek() == Some(&'(') {
        if ukuran_teks.is_empty() {
            return Err(LexError::new(
                "size cast butuh lebar eksplisit, contoh 16'(x)",
                span(byte_offset),
            ));
        }
        let width = ukuran_teks
            .replace('_', "")
            .parse::<u32>()
            .map_err(|_| LexError::new("ukuran size cast tidak valid", span(byte_offset)))?;
        if width == 0 {
            return Err(LexError::new(
                "lebar size cast harus > 0",
                span(byte_offset),
            ));
        }
        return Ok(Token::SizeCast(width));
    }

    // Tanda `s` opsional menandai literal bertanda (LRM §5.7.1).
    let signed = matches!(chars.peek(), Some('s') | Some('S'));
    if signed {
        adv(chars, byte_offset, col);
    }

    let basis = match chars.peek().copied().and_then(Basis::from_char) {
        Some(b) => b,
        None => {
            // Lebar eksplisit + karakter non-basis setelah `'` hampir selalu
            // size cast yang salah tulis (`16'a`), bukan literal. Pesan umum
            // "expected basis" menyesatkan di sana.
            if !ukuran_teks.is_empty() {
                return Err(LexError::new(
                    "setelah N' harus basis b/o/d/h untuk literal, atau '(' untuk size cast",
                    span(byte_offset),
                ));
            }
            return Err(LexError::new(
                "expected basis b, o, d or h after '",
                span(byte_offset),
            ));
        }
    };
    adv(chars, byte_offset, col);

    // Digit isi; `_` adalah separator yang diabaikan (LRM §5.7.1).
    let mut nilai: u64 = 0;
    let mut unknown_mask: u64 = 0;
    let mut zmask: u64 = 0;
    let mut jumlah_digit = 0usize;
    let mut overflow = false;
    // Digit paling kiri menentukan padding LRM §5.7.1: `x`/`z` di-padding
    // dengan dirinya sendiri, selain itu dengan nol.
    let mut digit_kiri_z = false;
    let mut digit_kiri_unknown = false;
    while let Some(&c) = chars.peek() {
        if c == '_' {
            adv(chars, byte_offset, col);
            continue;
        }
        let basis_nilai = basis_radix(basis);
        if is_unknown_digit(c) {
            // Nilai di-collapse ke 0, tetapi posisinya tetap harus dihitung:
            // digit unknown menggeser akumulator seperti digit biasa lalu
            // menandai bit barunya pada `unknown_mask`.
            nilai = nilai.saturating_mul(basis_nilai);
            let digit_mask = digit_unknown_mask(basis);
            unknown_mask = (unknown_mask.saturating_mul(basis_nilai)) | digit_mask;
            // LRM §5.7.1: `?` berarti `z` bila digit bernilai 0 atau 1 dan
            // `x` bila digitnya sudah `x`/`z`. Karena `nilai` di-collapse ke 0
            // di sini, `?` selalu diperlakukan sebagai `z` — sama seperti
            // penulisan eksplisit `z`. Digit `x` dan `z` eksplisit tetap
            // dibedakan.
            zmask = zmask.saturating_mul(basis_nilai);
            let ini_z = matches!(c, 'z' | 'Z' | '?');
            if ini_z {
                zmask |= digit_mask;
            }
            if jumlah_digit == 0 {
                digit_kiri_unknown = true;
                digit_kiri_z = ini_z;
            }
        } else if let Some(d) = basis.digit_value(c) {
            let _ = d;
            // Deteksi luap sebelum mengalikan agar tidak diam-diam terpotong.
            if nilai > u64::MAX / basis_nilai {
                overflow = true;
            }
            nilai = nilai.saturating_mul(basis_nilai).saturating_add(d as u64);
            // Bit unknown yang sudah lewat juga harus bergeser agar posisinya
            // tetap relatif terhadap digit yang sedang diproses.
            unknown_mask = unknown_mask.saturating_mul(basis_nilai);
            zmask = zmask.saturating_mul(basis_nilai);
        } else {
            break;
        }
        jumlah_digit += 1;
        adv(chars, byte_offset, col);
    }

    if jumlah_digit == 0 {
        return Err(LexError::new(
            "based literal tanpa digit",
            span(byte_offset),
        ));
    }
    // Digit yang tersisa setelah loop bukan digit sah untuk basis ini.
    if chars
        .peek()
        .is_some_and(|&c| c.is_alphanumeric() || c == '_')
    {
        return Err(LexError::new(
            "digit tidak valid untuk basis ini",
            span(byte_offset),
        ));
    }
    if overflow {
        return Err(LexError::new("literal meluap", span(byte_offset)));
    }

    // Desimal tidak punya digit unknown; tolak agar tidak mengarang masker.
    if unknown_mask != 0 && basis == Basis::Decimal {
        return Err(LexError::new(
            "literal desimal tidak boleh memuat x/z/?",
            span(byte_offset),
        ));
    }

    // Lebar explicit dipakai apa adanya; hanya tanpa ukuran yang default 32.
    let mut lebar = match ukuran_teks.is_empty() {
        true => LEBAR_UNSIZED,
        false => ukuran_teks
            .parse::<u32>()
            .map_err(|_| LexError::new("ukuran literal tidak valid", span(byte_offset)))?,
    };
    if lebar == 0 {
        return Err(LexError::new("ukuran literal harus > 0", span(byte_offset)));
    }
    // Nilai disimpan pada u64, jadi lebar di atas 64 dipotong ke 64.
    lebar = lebar.min(64);

    // LRM §5.7.1: bila ukuran lebih besar dari jumlah digit, angka dipad ke
    // kiri dengan `0` — kecuali bila digit paling kiri yang ditulis adalah
    // `x` atau `z`, maka dipad dengan `x`/`z` yang sama. Tanpa ini `16'hzz`
    // hanya jadi 8 bit `zz` dengan 8 bit nol di atasnya, padahal LRM
    // menyebutnya `zzzz`.
    // Padding hanya berlaku bila digit punya representasi bit tetap. Basis
    // desimal tidak punya (Tabel 5-22: `8'd200` bernilai 200), jadi tidak ada
    // yang perlu dipad.
    let per_digit = basis.bits_per_digit();
    // LRM §5.7.1: digit selalu menempati bit paling rendah dan NILAI tidak
    // pernah digeser — `16'hA5` bernilai 0x00A5, bukan 0xA500. Yang dipad hanya
    // bit DI ATAS digit yang benar-benar tertulis.
    //
    // Mengisi seluruh lebar dengan `u64::MAX` akan ikut menimpa digit yang sudah
    // ditulis: `16'hz3` jadi `zzzz` penuh, padahal LRM memberi
    // `zzzzzzzzzzzz0011` — padding `z`, tapi digit `0011` tetap utuh.
    let bit_tertulis = if per_digit > 0 {
        (jumlah_digit as u32).saturating_mul(per_digit).min(64)
    } else {
        0
    };
    if per_digit > 0 && jumlah_digit > 0 && lebar > bit_tertulis {
        // Digit tak diketahui di-pad dengan dirinya sendiri kalau digit paling
        // kiri yang ditulis adalah `x`/`z`, selain itu dengan nol.
        let pad = u64::MAX << bit_tertulis.min(63);
        if digit_kiri_unknown {
            unknown_mask |= truncate(pad, lebar);
            if digit_kiri_z {
                zmask |= truncate(pad, lebar);
            }
        }
    }

    Ok(Token::BasedNumber {
        value: truncate(nilai, lebar),
        width: lebar,
        signed,
        unknown_mask: truncate(unknown_mask, lebar),
        zmask: truncate(zmask, lebar),
    })
}

/// Masker bit yang diwakili satu digit `x`/`z`/`?` pada basis ini.
/// Pindai string literal `"..."` beserta escape dasarnya (LRM §5.7.3).
///
/// Karakter pemanggil `"` sudah dikonsumsi sebelum fungsi ini dipanggil.
pub fn scan_string(
    chars: &mut Peekable<Chars>,
    byte_offset: &mut usize,
    col: &mut usize,
    start_offset: usize,
    start_line: usize,
    start_col: usize,
) -> Result<Token, LexError> {
    let mut isi = String::new();
    loop {
        let Some(c) = chars.next() else {
            return Err(LexError::new(
                "unterminated string literal",
                Span::new(start_offset, *byte_offset, start_line, start_col),
            ));
        };
        *byte_offset += c.len_utf8();
        match c {
            '"' => break,
            // Baris baru di dalam string tidak diperbolehkan LRM; ia dianggap penanda akhir.
            '\n' => {
                return Err(LexError::new(
                    "newline di dalam string literal",
                    Span::new(start_offset, *byte_offset, start_line, start_col),
                ))
            }
            '\\' => {
                let Some(e) = chars.next() else {
                    return Err(LexError::new(
                        "escape tidak lengkap",
                        Span::new(start_offset, *byte_offset, start_line, start_col),
                    ));
                };
                *byte_offset += e.len_utf8();
                isi.push(match e {
                    'n' => '\n',
                    't' => '\t',
                    'r' => '\r',
                    '\\' => '\\',
                    '"' => '"',
                    lain => lain,
                });
            }
            lain => isi.push(lain),
        }
        *col += 1;
    }
    Ok(Token::StringLiteral(isi))
}

fn digit_unknown_mask(basis: Basis) -> u64 {
    let bits = basis.bits_per_digit();
    if bits == 0 || bits >= 64 {
        u64::MAX
    } else {
        (1u64 << bits) - 1
    }
}

/// Nilai basis sebagai pengali perpangkatan saat accumulated.
fn basis_radix(basis: Basis) -> u64 {
    match basis {
        Basis::Binary => 2,
        Basis::Octal => 8,
        Basis::Decimal => 10,
        Basis::Hex => 16,
    }
}

/// Masker nilai ke lebar target; lebar 64 berarti tanpa pemotongan.
fn truncate(value: u64, width: u32) -> u64 {
    if width >= 64 {
        return value;
    }
    value & ((1u64 << width) - 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lex;

    fn pindai(sumber: &str) -> Token {
        let tokens = lex(sumber).expect("lex gagal");
        assert_eq!(tokens.len(), 1, "harus satu token untuk {sumber:?}");
        tokens[0].token.clone()
    }

    #[test]
    fn desimal_polos_tetap_number() {
        assert_eq!(pindai("42"), Token::Number(42));
    }

    #[test]
    fn heksadesimal_berukuran() {
        assert_eq!(
            pindai("8'hFF"),
            Token::BasedNumber {
                value: 0xFF,
                width: 8,
                signed: false,
                unknown_mask: 0,
                zmask: 0,
            }
        );
    }

    #[test]
    fn heksadesimal_kecil_sah() {
        assert_eq!(
            pindai("8'hff"),
            Token::BasedNumber {
                value: 0xFF,
                width: 8,
                signed: false,
                unknown_mask: 0,
                zmask: 0,
            }
        );
    }

    #[test]
    fn biner_dengan_separator() {
        assert_eq!(
            pindai("8'b1010_1010"),
            Token::BasedNumber {
                value: 0xAA,
                width: 8,
                signed: false,
                unknown_mask: 0,
                zmask: 0,
            }
        );
    }

    #[test]
    fn desimal_berukuran() {
        assert_eq!(
            pindai("8'd200"),
            Token::BasedNumber {
                value: 200,
                width: 8,
                signed: false,
                unknown_mask: 0,
                zmask: 0,
            }
        );
    }

    #[test]
    fn oktal_tanpa_ukuran_default_32() {
        assert_eq!(
            pindai("'o17"),
            Token::BasedNumber {
                value: 15,
                width: 32,
                signed: false,
                unknown_mask: 0,
                zmask: 0,
            }
        );
    }

    #[test]
    fn heksa_panjang_melebar_implicit() {
        assert_eq!(
            pindai("16'hDEAD"),
            Token::BasedNumber {
                value: 0xDEAD,
                width: 16,
                signed: false,
                unknown_mask: 0,
                zmask: 0,
            }
        );
    }

    #[test]
    fn literal_bertanda() {
        assert_eq!(
            pindai("8'shFF"),
            Token::BasedNumber {
                value: 0xFF,
                width: 8,
                signed: true,
                unknown_mask: 0,
                zmask: 0,
            }
        );
    }

    #[test]
    fn nilai_dipotong_ke_lebar() {
        // 4'b11111 overflow terhadap lebar 4 -> 0b1111.
        assert_eq!(
            pindai("4'b11111"),
            Token::BasedNumber {
                value: 0xF,
                width: 4,
                signed: false,
                unknown_mask: 0,
                zmask: 0,
            }
        );
    }

    #[test]
    fn lebar_implicit_menaik_ke_64() {
        assert_eq!(
            pindai("128'h1"),
            Token::BasedNumber {
                value: 1,
                width: 64,
                signed: false,
                unknown_mask: 0,
                zmask: 0,
            }
        );
    }

    #[test]
    fn digit_x_ditandai_sebagai_unknown() {
        // Engine 2-state: nilainya 0, tapi bit unknown tetap dilacak agar
        // `casez`/`casex` bisa memperlakukannya sebagai wildcard.
        match pindai("8'b10x1") {
            Token::BasedNumber {
                value,
                unknown_mask,
                ..
            } => {
                // x berada di bit 1; nilainya di-collapse ke 0.
                assert_eq!(value, 0b1001);
                assert_eq!(unknown_mask, 0b0010);
            }
            other => panic!("expected based number, dapat {other:?}"),
        }
    }

    #[test]
    fn digit_question_mark_ditandai_unknown() {
        match pindai("4'b1?1?") {
            Token::BasedNumber {
                value,
                unknown_mask,
                ..
            } => {
                // Posisi ? adalah bit 2 dan bit 0.
                assert_eq!(value, 0b1010);
                assert_eq!(unknown_mask, 0b0101);
            }
            other => panic!("expected based number, dapat {other:?}"),
        }
    }

    #[test]
    fn digit_z_ditandai_unknown() {
        match pindai("4'bzzzz") {
            Token::BasedNumber {
                value,
                unknown_mask,
                ..
            } => {
                assert_eq!(value, 0);
                assert_eq!(unknown_mask, 0b1111);
            }
            other => panic!("expected based number, dapat {other:?}"),
        }
    }

    #[test]
    fn unknown_heksadesimal_menutup_empat_bit() {
        // Satu digit hex `x` menutup 4 bit.
        match pindai("8'hFx") {
            Token::BasedNumber {
                value,
                unknown_mask,
                width,
                ..
            } => {
                assert_eq!(value, 0xF0);
                assert_eq!(unknown_mask, 0x0F);
                assert_eq!(width, 8);
            }
            other => panic!("expected based number, dapat {other:?}"),
        }
    }

    #[test]
    fn unknown_oktal_menutup_tiga_bit() {
        match pindai("6'o7x") {
            Token::BasedNumber { unknown_mask, .. } => assert_eq!(unknown_mask, 0b111),
            other => panic!("expected based number, dapat {other:?}"),
        }
    }

    #[test]
    fn unknown_dipotong_ke_lebar() {
        // 2'bxx hanya mengisi 2 bit, jadi unknown_mask tepat 0b11.
        match pindai("2'bxx") {
            Token::BasedNumber { unknown_mask, .. } => assert_eq!(unknown_mask, 0b11),
            other => panic!("expected based number, dapat {other:?}"),
        }
        // 1'bx hanya satu digit => satu bit unknown, walaupun lebarnya 1.
        match pindai("1'bx") {
            Token::BasedNumber { unknown_mask, .. } => assert_eq!(unknown_mask, 0b1),
            other => panic!("expected based number, dapat {other:?}"),
        }
    }

    #[test]
    fn unknown_ditolak_pada_literal_desimal() {
        // LRM §5.7.1: basis desimal tidak punya digit unknown.
        assert!(lex("8'd1x").is_err());
    }

    #[test]
    fn digit_bukan_sah_lainnya_masih_ditolak() {
        assert!(lex("8'b102").is_err());
    }

    #[test]
    fn dasar_tanpa_basis_ditolak() {
        assert!(lex("8'q1").is_err());
    }

    #[test]
    fn basis_tanpa_digit_ditolak() {
        assert!(lex("8'h").is_err());
    }

    #[test]
    fn ukuran_nol_ditolak() {
        assert!(lex("0'h1").is_err());
    }

    #[test]
    fn basis_kirim_huruf_besar() {
        assert_eq!(
            pindai("8'HF0"),
            Token::BasedNumber {
                value: 0xF0,
                width: 8,
                signed: false,
                unknown_mask: 0,
                zmask: 0,
            }
        );
    }

    #[test]
    fn digit_setelah_literal_terpisah_token() {
        let tokens = lex("8'hFF 4'd1").unwrap();
        assert_eq!(tokens.len(), 2);
    }
}

#[cfg(test)]
mod string_tests {
    use super::*;
    use crate::lex;

    fn isi(src: &str) -> String {
        match &lex(src).expect("lex")[0].token {
            Token::StringLiteral(teks) => teks.clone(),
            lain => panic!("dapat {:?}", lain),
        }
    }

    #[test]
    fn string_polos_dibaca_utuh() {
        assert_eq!(isi("\"halo dunia\""), "halo dunia");
    }

    #[test]
    fn string_kosong_diterima() {
        assert_eq!(isi("\"\""), "");
    }

    #[test]
    fn escape_dasar_diperluas() {
        assert_eq!(isi(r#""a\nb""#), "a\nb");
        assert_eq!(isi(r#""a\tb""#), "a\tb");
        assert_eq!(isi(r#""a\"b""#), "a\"b");
        // Sumber SV berisi dua backslash menghasilkan satu backslash.
        assert_eq!(isi(r#""a\\b""#), "a\\b");
    }

    #[test]
    fn string_tidak_ditutup_ditolak() {
        assert!(lex("\"tidak ditutup").is_err());
    }
}
