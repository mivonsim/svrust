// Tanggung jawab: parse sisi kiri assignment berupa sinyal dengan irisan opsional.
use sv_ast::lvalue::{Lvalue, Slice};
use sv_lexer::spanned::SpannedToken;
use sv_lexer::token::Token;

/// Parse LHS: `nama`, `nama[msb:lsb]`, `nama[bit]`, atau `nama[genvar]`.
/// Mengembalikan `(Lvalue, jumlah token yang dikonsumsi)`.
pub fn parse_lvalue(tokens: &[SpannedToken]) -> Result<(Lvalue, usize), String> {
    if tokens.is_empty() {
        return Err("expected LHS signal name".to_string());
    }
    let name = match &tokens[0].token {
        Token::Ident(n) => n.clone(),
        other => return Err(format!("expected LHS signal name, ditemukan {:?}", other)),
    };
    let span = tokens[0].span;

    // Tanpa kurung siku: seluruh sinyal jadi target.
    if tokens.get(1).map(|t| &t.token) != Some(&Token::LBracket) {
        return Ok((Lvalue::simple(name, span), 1));
    }

    // Kurung siku kosong / tak tertutup harus ditolak eksplisit. Tanpa cek ini
    // `parse_slice_index` sampai ke `parse_index` dan membocorkan pesan
    // "indeks harus literal, ditemukan RBracket".
    let closing =
        (2..=tokens.len()).find(|&i| tokens.get(i).map(|t| &t.token) == Some(&Token::RBracket));
    let Some(closing) = closing else {
        return Err("expected ']' closing LHS select".to_string());
    };
    if closing == 2 {
        return Err("kurung siku seleksi kosong `[]` belum didukung".to_string());
    }
    if tokens[2].token == Token::Colon {
        return Err("part-select dengan batas bawah kosong `[:lsb]` belum didukung".to_string());
    }
    if tokens.get(closing - 1).map(|t| &t.token) == Some(&Token::Colon) {
        return Err("part-select dengan batas atas kosong `[msb:]` belum didukung".to_string());
    }
    // Seleksi bertingkat pada LHS belum didukung untuk semua bentuk indeks,
    // bukan hanya yang beridentifier. Dulu `[1]` pada `y[3:0][1]` diterima
    // diam-diam lalu tokennya dibuang begitu saja.
    if tokens.get(closing + 1).map(|t| &t.token) == Some(&Token::LBracket) {
        return Err("indeks bertingkat pada LHS belum didukung".to_string());
    }
    // Indexed part-select `a[base +: width]` (LRM §7.8).
    if let Some(pos) = (3..closing).find(|&i| tokens[i].token == Token::Colon) {
        match tokens[pos - 1].token {
            Token::Plus | Token::Minus => {
                return Err("indexed part-select `a[base +: width]` belum didukung".to_string())
            }
            _ => {}
        }
    }

    // Bentuk `nama[genvar]` menyimpan nama genvar; nilainya baru diketahui
    // saat elaborasi loop generate. Isi `[...]` harus PERSIS satu token:
    // kalau tidak, `y[i-1]` klaim "sudah selesai" padahal `- 1` dibuang
    // diam-diam, dan penulisan terjadi pada bit yang salah (silently wrong).
    if let Token::Ident(genvar) = &tokens[2].token {
        if closing != 3 {
            // Ada titik dua → itu part-select variabel, bukan indeks genvar;
            // pesannya harus menyebut part-select supaya tidak menyesatkan.
            let apa = if tokens[2..closing].iter().any(|t| t.token == Token::Colon) {
                "part-select variabel pada LHS belum didukung"
            } else {
                "indeks genvar harus satu token"
            };
            return Err(format!(
                "{apa}, ditemukan `{}`",
                tokens[2..closing]
                    .iter()
                    .map(|t| format!("{:?}", t.token))
                    .collect::<Vec<_>>()
                    .join(" ")
            ));
        }
        let genvar = genvar.clone();
        return Ok((Lvalue::dengan_genvar(name, genvar, span), closing + 1));
    }

    let (slice, consumed) = parse_slice_index(&tokens[2..closing])?;
    // `parse_slice_index` hanya mengonsumsi 1 atau 3 token. Sisa token berarti
    // indeks rusak (`y[3 5]`); release build akan membuangnya diam-diam.
    if 2 + consumed != closing {
        return Err(format!(
            "sisa token tak dikenal pada indeks LHS: {:?}",
            tokens[2 + consumed..closing]
                .iter()
                .map(|t| &t.token)
                .collect::<Vec<_>>()
        ));
    }
    Ok((Lvalue::sliced(name, slice, span), closing + 1))
}

/// Parse isi `[ ... ]`: `msb:lsb` untuk part-select, `i` untuk bit-select.
/// Hanya literal yang diterima; indeks dinamis di LHS ditolak (LRM §7.8).
fn parse_slice_index(tokens: &[SpannedToken]) -> Result<(Slice, usize), String> {
    let msb = parse_index(tokens, 0, "msb")?;
    let is_part_select = tokens.get(1).map(|t| &t.token) == Some(&Token::Colon);
    if is_part_select {
        let lsb = parse_index(tokens, 2, "lsb")?;
        if lsb > msb {
            return Err(format!("part-select tidak valid: [{}:{}]", msb, lsb));
        }
        // msb, ':', lsb
        return Ok((Slice { msb, lsb }, 3));
    }
    Ok((Slice { msb, lsb: msb }, 1))
}

fn parse_index(tokens: &[SpannedToken], at: usize, posisi: &str) -> Result<u32, String> {
    match tokens.get(at).map(|t| &t.token) {
        Some(Token::Number(n)) => u32::try_from(*n).map_err(|_| "index melebihi u32".to_string()),
        Some(Token::BasedNumber { value, .. }) => {
            u32::try_from(*value).map_err(|_| "index melebihi u32".to_string())
        }
        // Pesannya menyebut posisi supaya batas part-select yang bukan literal
        // bisa dibedakan dari bit-select.
        Some(Token::Ident(n)) => Err(format!(
            "indeks {posisi} harus literal, ditemukan identifier '{}'",
            n
        )),
        other => Err(format!(
            "indeks {posisi} harus literal, ditemukan {:?}",
            other
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sv_lexer::lex;

    fn parse(src: &str) -> Lvalue {
        let tokens = lex(src).expect("lex");
        let (lhs, _) = parse_lvalue(&tokens).expect("parse");
        lhs
    }

    #[test]
    fn sinyal_polos_tanpa_irisan() {
        let lhs = parse("y");
        assert_eq!(lhs.name, "y");
        assert!(!lhs.is_sliced());
    }

    #[test]
    fn bit_select_satu_bit() {
        let lhs = parse("y[0]");
        assert!(lhs.is_sliced());
        assert_eq!(lhs.slice.unwrap(), Slice { msb: 0, lsb: 0 });
    }

    #[test]
    fn part_select_berentang() {
        let lhs = parse("y[7:4]");
        assert_eq!(lhs.slice.unwrap(), Slice { msb: 7, lsb: 4 });
    }

    #[test]
    fn lsb_lebih_besar_dari_msb_ditolak() {
        let tokens = lex("y[3:7]").unwrap();
        assert!(parse_lvalue(&tokens).is_err());
    }

    /// Pesan error LHS untuk bentuk indeks yang ditolak.
    fn pesan(src: &str) -> String {
        let tokens = lex(src).expect("lex");
        parse_lvalue(&tokens)
            .err()
            .unwrap_or_else(|| panic!("{src} seharusnya ditolak"))
    }

    // Bug: `y[3:0][1]` Accepted `Ok` dengan `[1]` dibuang diam-diam; penjaga
    // "indeks bertingkat" hanya ada di jalur genvar.
    #[test]
    fn seleksi_bertingkat_pada_lhs_ditolak() {
        for src in ["y[3:0][1]", "y[0][1]", "y[7:4][3:0]"] {
            assert!(
                pesan(src).contains("indeks bertingkat"),
                "{src}: {}",
                pesan(src)
            );
        }
    }

    // Bug: `y[i:0]` dan `y[W:3]` dilaporkan sebagai "indeks genvar" karena
    // cabang genvar menerima identifier apa pun di posisi itu.
    #[test]
    fn part_select_variabel_dilaporkan_sebagai_part_select() {
        for src in ["y[i:0]", "y[W:3]", "y[base:0]"] {
            let p = pesan(src);
            assert!(p.contains("part-select"), "{src} dapat pesan keliru: {p}");
            assert!(!p.contains("genvar"), "{src} salah disebut genvar: {p}");
        }
    }

    // Bug: bentuk kosong/terbuka membocorkan "indeks harus literal,
    // ditemukan Some(RBracket)".
    #[test]
    fn bentuk_kosong_dan_terbuka_pesan_jelas() {
        assert!(pesan("y[]").contains("kosong"), "{}", pesan("y[]"));
        assert!(pesan("y[3:]").contains("batas atas kosong"));
        assert!(pesan("y[:3]").contains("batas bawah kosong"));
        assert!(pesan("y[").contains("']'"), "{}", pesan("y["));
    }

    // Bug: `debug_assert_eq!(2 + consumed, closing)` PANIC untuk `y[3 5]` — isi
    //[`3`,`5`] punya 2 token tapi `parse_slice_index` hanya mengonsumsi 1. Di
    // release build assert mati sehingga token sisa dibuang diam-diam.
    #[test]
    fn isi_indeks_berlebihan_ditolak_bukan_panic() {
        for src in ["y[3 5]", "y[0 1]", "y[1 2 3]", "y[3 5 7 9]"] {
            let tokens = lex(src).expect("lex");
            let hasil = std::panic::catch_unwind(|| parse_lvalue(&tokens));
            let Ok(hasil) = hasil else {
                panic!("{src} PANIC, harus Err");
            };
            assert!(hasil.is_err(), "{src} harus ditolak");
        }
    }

    #[test]
    fn isi_indeks_berlebihan_pesan_sisa_token() {
        let p = pesan("y[3 5]");
        assert!(p.contains("sisa token"), "pesan: {p}");
    }

    // Bug: `y[i-1]` di LHS diklaim "sudah lengkap" padahal `- 1` dibuang,
    // sehingga penulisan terjadi pada bit yang salah tanpa pesan apa pun
    // (silent wrong answer). Cabang genvar harus menuntut isi persis 1 token.
    #[test]
    fn indeks_genvar_hanya_berupa_satu_token() {
        // Bentuk ini sengaja TIDAK memakai titik dua, supaya menguji penjaga
        // "satu token" dan bukan jalur part-select.
        for src in ["y[i-1]", "y[i*2]", "y[W+1]", "y[W-1]", "y[i+2]"] {
            let p = pesan(src);
            assert!(p.contains("satu token"), "{src} dapat pesan: {p}");
        }
    }

    #[test]
    fn indeks_genvar_tunggal_tetap_diterima() {
        for src in ["y[i]", "y[j]", "y[my_genvar]"] {
            let tokens = lex(src).expect("lex");
            parse_lvalue(&tokens).unwrap_or_else(|e| panic!("{src} harus sah: {e}"));
        }
    }

    #[test]
    fn indexed_part_select_pesan_jelas() {
        for src in ["y[b +: 4]", "y[b -: 4]"] {
            assert!(pesan(src).contains("indexed part-select"), "{src}");
        }
    }

    // Bentuk sah yang harus tetap jalan setelah penjaga baru ditambah.
    #[test]
    fn bentuk_sah_tetap_diterima() {
        for src in ["y[0]", "y[7:0]", "y[i]"] {
            let tokens = lex(src).expect("lex");
            parse_lvalue(&tokens).unwrap_or_else(|e| panic!("{src} seharusnya sah: {e}"));
        }
    }

    #[test]
    fn kurung_tidak_tertutup_ditolak() {
        let tokens = lex("y[7:4").unwrap();
        assert!(parse_lvalue(&tokens).is_err());
    }

    #[test]
    fn indeks_non_nama_ditolak() {
        // Indeks harus angka atau nama; token lain tetap ditolak.
        for src in ["y[]", "y[*]", "y[:0]"] {
            let tokens = lex(src).unwrap();
            assert!(parse_lvalue(&tokens).is_err(), "{}", src);
        }
    }

    #[test]
    fn indeks_nama_non_genvar_diterima_parser_ditolak_elaborator() {
        // Parser tidak bisa tahu `W` itu genvar atau bukan; penolakannya
        // terjadi saat elaborasi dengan pesan yang menyebut nama genvar.
        let lhs = parse("y[W]");
        assert_eq!(lhs.genvar_index.as_deref(), Some("W"));
    }

    #[test]
    fn irisan_bukan_nama_ditolak() {
        let tokens = lex("8'hFF[3:0]").unwrap();
        assert!(parse_lvalue(&tokens).is_err());
    }

    #[test]
    fn konsumsi_token_sesuai_panjang_irisan() {
        let tokens = lex("y[3:0] = 1").unwrap();
        let (_, consumed) = parse_lvalue(&tokens).expect("parse");
        // 'y', '[', '3', ':', '0', ']' = 6 token.
        assert_eq!(consumed, 6);
    }

    #[test]
    fn bit_select_konsumsi_empat_token() {
        let tokens = lex("y[3] = 1").unwrap();
        let (_, consumed) = parse_lvalue(&tokens).expect("parse");
        assert_eq!(consumed, 4);
    }

    // --- indeks genvar pada LHS (LRM §27.3) ---

    #[test]
    fn indeks_genvar_pada_lhs_diterima() {
        let lhs = parse("y[i]");
        assert_eq!(lhs.name, "y");
        assert_eq!(lhs.genvar_index.as_deref(), Some("i"));
        // Belum ada irisan konkret sampai elaborasi mengisinya.
        assert!(lhs.slice.is_none());
    }

    #[test]
    fn indeks_genvar_konsumsi_empat_token() {
        let tokens = lex("y[i] = 1'b1").unwrap();
        let (_, consumed) = parse_lvalue(&tokens).expect("parse");
        // 'y', '[', 'i', ']' = 4 token.
        assert_eq!(consumed, 4);
    }

    #[test]
    fn indeks_genvar_menghasilkan_lvalue_yang_bisa_dipakai() {
        // Token setelah `y[i]` harus '=' supaya caller lanjut ke RHS.
        let tokens = lex("y[i] = 1'b1").unwrap();
        let (lhs, consumed) = parse_lvalue(&tokens).expect("parse");
        assert_eq!(tokens[consumed].token, Token::Eq);
        assert_eq!(lhs.name, "y");
    }

    #[test]
    fn indeks_genvar_bertingkat_ditolak() {
        let tokens = lex("y[i][0]").unwrap();
        assert!(parse_lvalue(&tokens).is_err());
    }

    #[test]
    fn indeks_genvar_part_select_ditolak() {
        // Genvar selalu satu bit; `y[i:0]` tidak punya makna di sini.
        let tokens = lex("y[i:0]").unwrap();
        assert!(parse_lvalue(&tokens).is_err());
    }
}
