// Tanggung jawab: buang komentar SystemVerilog sebelum direktif diproses.
/// Baris setelah komentar dibuang, dengan info blok komentar masih terbuka.
pub(super) struct Stripped {
    /// Isi baris tanpa bagian komentar.
    pub text: String,
    /// True bila blok `/* ... */` masih terbuka di akhir baris ini.
    pub continues: bool,
}

/// Buang komentar baris `//` dan blok `/* ... */` dari satu baris.
/// String literal dipertahankan apa adanya; `state` menyimpan status blok
/// komentar antar baris.
pub(super) fn strip_line(state: &mut bool, line: &str) -> Stripped {
    let chars: Vec<char> = line.chars().collect();
    let mut out = String::new();
    let mut i = 0usize;
    let mut in_string = false;

    while i < chars.len() {
        if *state {
            // Sedang di dalam blok komentar: cari penutup.
            if chars[i] == '*' && i + 1 < chars.len() && chars[i + 1] == '/' {
                *state = false;
                i += 2;
            } else {
                i += 1;
            }
            continue;
        }

        if in_string {
            out.push(chars[i]);
            if chars[i] == '\\' && i + 1 < chars.len() {
                out.push(chars[i + 1]);
                i += 2;
                continue;
            }
            if chars[i] == '"' {
                in_string = false;
            }
            i += 1;
            continue;
        }

        if chars[i] == '/' && i + 1 < chars.len() && chars[i + 1] == '/' {
            break; // sisa baris adalah komentar
        }
        if chars[i] == '/' && i + 1 < chars.len() && chars[i + 1] == '*' {
            *state = true;
            i += 2;
            continue;
        }
        if chars[i] == '"' {
            in_string = true;
            out.push(chars[i]);
            i += 1;
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }

    Stripped {
        text: out,
        continues: *state,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn komentar_baris_dibuang() {
        let mut state = false;
        let out = strip_line(&mut state, "wire a; // `ifdef SALAH");
        assert_eq!(out.text, "wire a; ");
        assert!(!out.continues);
    }

    #[test]
    fn komentar_blok_beribu_baris() {
        let mut state = false;
        let a = strip_line(&mut state, "a /* `define X");
        assert!(a.continues);
        let b = strip_line(&mut state, "Y 1 */ b");
        assert_eq!(b.text, " b");
        assert!(!b.continues);
    }

    #[test]
    fn string_literral_diperahankan() {
        let mut state = false;
        let out = strip_line(&mut state, "$display(\"a // b\"); // nyata");
        assert_eq!(out.text, "$display(\"a // b\"); ");
    }

    #[test]
    fn backtick_di_blok_komentar_diabaikan() {
        let mut state = false;
        let a = strip_line(&mut state, "/* `ifdef X");
        let b = strip_line(&mut state, "*/ wire w;");
        assert_eq!(b.text.trim(), "wire w;");
        let _ = a;
    }
}
