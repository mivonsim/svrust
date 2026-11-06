// Tanggung jawab: substitusi pemanggilan makro (argumen, default, rekursi).
use crate::error::PreprocessError;

use super::Engine;

/// Batas kedalaman ekspansi makro bertingkat sebelum dianggap rekursi.
const MAKS_REKURSI: usize = 64;

impl Engine {
    /// Substitusi semua `` `NAME `` pada satu baris; lewati string literal.
    /// Function-like dipanggil dengan `(args)`; argumen disubstitusi sebagai
    /// token ke body, lalu hasilnya diekspansi rekursif (makro bersarang).
    pub(super) fn substitute(
        &self,
        line: &str,
        file: &str,
        line_no: usize,
    ) -> Result<String, PreprocessError> {
        let hasil = self.expand(line, file, line_no, 0)?;
        // Direktif kondisional di dalam body makro dievaluasi saat makro
        // dipakai (LRM §22.5.2), yaitu setelah substitusi baris ini.
        Ok(self.selesaikan_kondisional(&hasil))
    }

    fn expand(
        &self,
        text: &str,
        file: &str,
        line_no: usize,
        depth: usize,
    ) -> Result<String, PreprocessError> {
        if depth > MAKS_REKURSI {
            return Err(PreprocessError::at(
                "rekursi makro terlalu dalam",
                file,
                line_no,
            ));
        }
        let chars: Vec<char> = text.chars().collect();
        let mut out = String::new();
        let mut in_string = false;
        let mut i = 0usize;
        while i < chars.len() {
            let c = chars[i];
            if c == '\"' {
                in_string = !in_string;
                out.push(c);
                i += 1;
                continue;
            }
            if c == '`' && !in_string {
                // `` `" `` — kutip ter-escape di dalam body makro (LRM §22.5.1):
                // hasil ekspansi memuat kutip biasa, bukan backtick.
                if chars.get(i + 1) == Some(&'"') {
                    out.push('"');
                    i += 2;
                    continue;
                }
                let mut j = i + 1;
                while j < chars.len() && (chars[j].is_alphanumeric() || chars[j] == '_') {
                    j += 1;
                }
                let ident: String = chars[i + 1..j].iter().collect();
                // Makro bawaan LRM §22.5.1.
                if ident == "__FILE__" {
                    out.push('"');
                    out.push_str(file);
                    out.push('"');
                    i = j;
                    continue;
                }
                if ident == "__LINE__" {
                    out.push_str(&line_no.to_string());
                    i = j;
                    continue;
                }
                if let Some(mac) = self.macros.get(&ident) {
                    let mut body = mac.body.clone();
                    let mut next = j;
                    if !mac.params.is_empty() {
                        // Function-like: wajib ada (args) setelah nama (spasi diperbolehkan).
                        let mut k = j;
                        while k < chars.len() && chars[k].is_whitespace() {
                            k += 1;
                        }
                        if k >= chars.len() || chars[k] != '(' {
                            return Err(PreprocessError::at(
                                format!("pemanggilan makro `{ident} memerlukan argumen"),
                                file,
                                line_no,
                            ));
                        }
                        let (args, end) = parse_args(&chars, k, file, line_no)?;
                        body = bind_args(mac, &args, file, line_no)?;
                        next = end;
                    }
                    let expanded = self.expand(&body, file, line_no, depth + 1)?;
                    out.push_str(&expanded);
                    i = next;
                    continue;
                }
                out.push('`');
                i += 1;
                continue;
            }
            out.push(c);
            i += 1;
        }
        Ok(out)
    }
}

/// Ambil argumen pemanggilan makro mulai dari posisi `open` (harus '(').
/// Mengembalikan (daftar argumen mentah, indeks setelah ')' penutup).
/// Pemisah koma hanya di kedalaman 0 sehingga `{a,b}`, `(x,y)`, dan string
/// dengan koma di dalamnya tetap utuh sebagai satu argumen.
fn parse_args(
    chars: &[char],
    open: usize,
    file: &str,
    line: usize,
) -> Result<(Vec<String>, usize), PreprocessError> {
    let mut args: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut depth = 0usize;
    let mut in_string = false;
    // Lewati '(' pembuka: depth 0 = di dalam argumen teratas.
    let mut i = open + 1;
    while i < chars.len() {
        let c = chars[i];
        if in_string {
            cur.push(c);
            if c == '\\' && i + 1 < chars.len() {
                cur.push(chars[i + 1]);
                i += 2;
                continue;
            }
            if c == '"' {
                in_string = false;
            }
            i += 1;
            continue;
        }
        match c {
            '"' => {
                in_string = true;
                cur.push(c);
            }
            '(' | '[' | '{' => {
                depth += 1;
                cur.push(c);
            }
            ')' | ']' | '}' => {
                if depth == 0 && c == ')' {
                    args.push(cur.trim().to_string());
                    return Ok((args, i + 1));
                }
                depth = depth.saturating_sub(1);
                cur.push(c);
            }
            ',' if depth == 0 => {
                args.push(cur.trim().to_string());
                cur.clear();
            }
            _ => cur.push(c),
        }
        i += 1;
    }
    Err(PreprocessError::at(
        "argumen makro tanpa ')' penutup",
        file,
        line,
    ))
}

/// Ikat argumen aktual ke parameter formal: substitusi token di body.
/// Argumen kurang diizinkan bila parameter sisa punya nilai default;
/// argumen lebih adalah error.
fn bind_args(
    mac: &super::Macro,
    args: &[String],
    file: &str,
    line: usize,
) -> Result<String, PreprocessError> {
    if args.len() > mac.params.len() {
        return Err(PreprocessError::at(
            format!(
                "terlalu banyak argumen: diberi {}, makro meminta {}",
                args.len(),
                mac.params.len()
            ),
            file,
            line,
        ));
    }
    // Nilai final per parameter: argumen aktual, atau default.
    let params = &mac.params;
    let mut values: Vec<(&str, String)> = Vec::with_capacity(params.len());
    for (idx, p) in params.iter().enumerate() {
        if idx < args.len() {
            // Argumen eksplisit, bahkan bila kosong (`F(1, , 3)` → argumen ke-2
            // adalah string kosong, TIDAK memicu default). Default hanya
            // dipakai bila parameter sama sekali tidak ditulis (idx >= args).
            let given = args[idx].trim();
            values.push((&p.name, given.to_string()));
        } else {
            match &p.default {
                Some(d) => values.push((&p.name, d.clone())),
                None => return Err(missing_arg(&p.name, file, line)),
            }
        }
    }
    Ok(subst_tokens(&mac.body, &values))
}

fn missing_arg(name: &str, file: &str, line: usize) -> PreprocessError {
    PreprocessError::at(
        format!("argumen `{}` tidak diberikan dan tanpa default", name),
        file,
        line,
    )
}

/// Substitusi identifier yang cocok dengan parameter formal (whole-word)
/// oleh nilai argumen, di seluruh teks body termasuk di dalam string —
/// substitusi makro SV bersifat tekstual.
fn subst_tokens(body: &str, values: &[(&str, String)]) -> String {
    let chars: Vec<char> = body.chars().collect();
    let mut out = String::with_capacity(body.len());
    let mut i = 0usize;
    while i < chars.len() {
        // `` `` `` — penyambungan token (LRM §22.5.1): kedua sisi body ditempel
        // tanpa spasi, mis. `bit __name``_cond;`.
        if chars[i] == '`' && chars.get(i + 1) == Some(&'`') {
            while out.chars().last().is_some_and(|c| c.is_whitespace()) {
                out.pop();
            }
            i += 2;
            while i < chars.len() && chars[i].is_whitespace() {
                i += 1;
            }
            continue;
        }
        if chars[i].is_alphanumeric() || chars[i] == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            match values.iter().find(|(n, _)| *n == word) {
                Some((_, v)) => out.push_str(v),
                None => out.push_str(&word),
            }
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}
