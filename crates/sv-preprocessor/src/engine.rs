// Tanggung jawab: driver preprocessing + substitusi makro SV.
use crate::error::PreprocessError;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

mod comment;
mod directives;
mod expand;

/// Hasil preprocessing: source terekspansi dan timescale yang dideklarasikan.
#[derive(Debug)]
pub struct Preprocessed {
    pub source: String,
    pub timescale: String,
}

/// Satu argumen formal makro function-like, dengan nilai default opsional.
#[derive(Debug, Clone)]
struct Param {
    /// Nama argumen formal.
    name: String,
    /// Nilai default (`define F(a, b=5)`); None bila wajib diisi pemanggil.
    default: Option<String>,
}

/// Definisi satu makro: object-like (tanpa argumen) atau function-like.
#[derive(Debug, Clone)]
struct Macro {
    /// Daftar argumen formal; kosong = object-like.
    params: Vec<Param>,
    /// Body teks; argumen formal disubstitusi sebagai token saat makro dipakai.
    body: String,
}

/// Frame kondisional untuk melacak nesting `ifdef.
struct Frame {
    active: bool,
    taken: bool,
    seen_else: bool,
    parent: bool,
}

pub fn preprocess(source: &str, base_dir: Option<&Path>) -> Result<Preprocessed, PreprocessError> {
    let mut engine = Engine::new(base_dir);
    let src = engine.run(source, "<input>")?;
    Ok(Preprocessed {
        source: src,
        timescale: engine.timescale,
    })
}

/// Preprocess dengan direktori pencarian include tambahan (untuk `--incdir`).
pub fn preprocess_with_dirs(
    source: &str,
    search_dirs: &[&Path],
    display_name: &str,
) -> Result<Preprocessed, PreprocessError> {
    let mut engine = Engine::new_with_dirs(search_dirs);
    let src = engine.run(source, display_name)?;
    Ok(Preprocessed {
        source: src,
        timescale: engine.timescale,
    })
}

struct Engine {
    macros: HashMap<String, Macro>,
    stack: Vec<Frame>,
    timescale: String,
    /// Nilai `default_nettype` yang berlaku (`None` = bawaan LRM = `wire`).
    /// Digunakan untuk mengklasifikasikan deklarasi net tanpa tipe eksplisit
    /// (LRM §6.5). `Some("none")` berarti deklarasi tanpa tipe ditolak.
    default_nettype: Option<String>,
    include_stack: Vec<PathBuf>,
    search_dirs: Vec<PathBuf>,
}

/// Hitung kedalaman kurung buka yang belum ditutup dalam `text`.
/// Digunakan untuk mendeteksi body `define` yang melintasi baris.
fn paren_depth(text: &str) -> usize {
    let mut depth = 0usize;
    for c in text.chars() {
        match c {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    depth
}

/// True bila semua pemanggilan makro function-like (`` `ident(...) ``) di
/// `teks` punya kurung seimbang, yaitu tidak memerlukan baris berikutnya
/// untuk menutup argumen. Digunakan agar pemanggilan makro multi-line
/// (umum di OpenTitan UVM/DV macros) bisa digabung sebelum diekspansi.
fn pemanggilan_makro_lengkap(teks: &str) -> bool {
    let chars: Vec<char> = teks.chars().collect();
    let mut i = 0usize;
    let mut in_string = false;
    while i < chars.len() {
        let c = chars[i];
        if in_string {
            if c == '\\' && i + 1 < chars.len() {
                i += 2;
                continue;
            }
            if c == '"' {
                in_string = false;
            }
            i += 1;
            continue;
        }
        if c == '"' {
            in_string = true;
            i += 1;
            continue;
        }
        if c == '`' {
            let mut k = i + 1;
            while k < chars.len() && (chars[k].is_alphanumeric() || chars[k] == '_') {
                k += 1;
            }
            // `ident(` — pemanggilan function-like.
            if k > i + 1 && k < chars.len() && chars[k] == '(' {
                let mut depth = 1u32;
                let mut m = k + 1;
                let mut selesai = false;
                let mut dalam_string = false;
                while m < chars.len() {
                    let cm = chars[m];
                    if dalam_string {
                        if cm == '\\' && m + 1 < chars.len() {
                            m += 2;
                            continue;
                        }
                        if cm == '"' {
                            dalam_string = false;
                        }
                        m += 1;
                        continue;
                    }
                    if cm == '"' {
                        dalam_string = true;
                        m += 1;
                        continue;
                    }
                    match cm {
                        '(' => depth += 1,
                        ')' => {
                            depth = depth.saturating_sub(1);
                            if depth == 0 {
                                selesai = true;
                                break;
                            }
                        }
                        _ => {}
                    }
                    m += 1;
                }
                if !selesai {
                    return false; // kurung belum seimbang, butuh baris berikutnya
                }
                i = m;
                continue;
            }
        }
        i += 1;
    }
    true
}

impl Engine {
    fn new(base_dir: Option<&Path>) -> Self {
        let mut search_dirs = Vec::new();
        if let Some(d) = base_dir {
            search_dirs.push(d.to_path_buf());
        }
        let search_dirs_refs: Vec<&Path> = search_dirs.iter().map(|p| p.as_path()).collect();
        Self::new_with_dirs(&search_dirs_refs)
    }

    fn new_with_dirs(search_dirs: &[&Path]) -> Self {
        Self {
            macros: HashMap::new(),
            stack: Vec::new(),
            timescale: String::new(),
            default_nettype: None,
            include_stack: Vec::new(),
            search_dirs: search_dirs.iter().map(|p| p.to_path_buf()).collect(),
        }
    }

    /// Baris aktif bila semua frame kondisional memilih cabang aktif.
    fn is_active(&self) -> bool {
        match self.stack.last() {
            Some(frame) => frame.active,
            None => true,
        }
    }

    fn run(&mut self, source: &str, display_name: &str) -> Result<String, PreprocessError> {
        let lines: Vec<&str> = source.split('\n').collect();
        let mut out = String::new();
        let mut in_block_comment = false;
        let mut i = 0usize;
        while i < lines.len() {
            let line_no = i + 1;
            // Komentar dibuang dulu: direktif di dalam komentar tidak boleh diproses.
            let first_stripped = comment::strip_line(&mut in_block_comment, lines[i]);
            in_block_comment = first_stripped.continues;
            let mut text = first_stripped.text.to_string();
            let mut j = i;
            while paren_depth(&text) > 0 && j + 1 < lines.len() {
                j += 1;
                let next_stripped = comment::strip_line(&mut in_block_comment, lines[j]);
                in_block_comment = next_stripped.continues;
                text.push('\n');
                text.push_str(&next_stripped.text);
            }
            let trimmed = text.trim_start();

            if let Some(rest) = trimmed.strip_prefix('`') {
                // Ident: prefix huruf/angka/underscore pertama, mis. `F(1,2) -> "F".
                let word: String = rest
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                match word.as_str() {
                    "ifdef" | "ifndef" | "else" | "elsif" | "endif" => {
                        self.handle_conditional(&word, rest, display_name, line_no)?;
                    }
                    "define" => {
                        if self.is_active() {
                            let (name, mut mac) = self.parse_define(rest, display_name, line_no)?;
                            if trimmed.trim_end().ends_with('\\') {
                                // Hapus backslash lanjutan dari body, lalu gabung baris berikutnya.
                                if mac.body.ends_with('\\') {
                                    mac.body.pop();
                                }
                                loop {
                                    i += 1;
                                    if i >= lines.len() {
                                        break;
                                    }
                                    // Cek continuation '\' pada baris asli
                                    // SEBELUM strip komentar, karena `\` yang
                                    // berada di dalam `//` komentar akan hilang
                                    // saat strip_line (misal `30, // komentar \`).
                                    let orig_ends_back = lines[i].trim_end().ends_with('\\');
                                    let cont =
                                        comment::strip_line(&mut in_block_comment, lines[i]).text;
                                    let cont_trim = cont.trim_start();
                                    let ends_back =
                                        orig_ends_back || cont_trim.trim_end().ends_with('\\');
                                    let piece = if orig_ends_back {
                                        // `\` ada di dalam komentar, sudah hilang;
                                        // gunakan isi konten penuh.
                                        cont_trim
                                    } else if ends_back {
                                        let e = cont_trim.trim_end().len();
                                        &cont_trim[..e - 1]
                                    } else {
                                        cont_trim
                                    };
                                    mac.body.push(' ');
                                    mac.body.push_str(piece);
                                    if !ends_back {
                                        break;
                                    }
                                }
                                // Baris lanjutan sudah dikonsumsi: jangan biarkan
                                // `i = j + 1` di akhir loop memprosesnya ulang
                                // sebagai direktif tingkat atas (backslash bocor).
                                j = i;
                            }
                            self.macros.insert(name, mac);
                        }
                    }
                    _ if !self.is_active() => {}
                    // `` `NAME `` di awal baris adalah pemanggilan makro, bukan direktif.
                    _ if self.is_macro_call(&word) => {
                        // Gabungkan baris berikutnya bila pemanggilan makro
                        // melintasi baris (kurung argumen belum seimbang),
                        // umum di OpenTitan UVM/DV macros.
                        let mut jj = j;
                        let mut gabungan = text.clone();
                        while !pemanggilan_makro_lengkap(&gabungan) && jj + 1 < lines.len() {
                            jj += 1;
                            gabungan.push('\n');
                            gabungan.push_str(lines[jj]);
                        }
                        let expanded = self.substitute(&gabungan, display_name, line_no)?;
                        out.push_str(&expanded);
                        out.push('\n');
                        i = jj + 1;
                        continue;
                    }
                    _ => {
                        self.handle_other(&word, rest, display_name, line_no, &mut out)?;
                    }
                }
            } else if self.is_active() {
                let expanded = self.substitute(&text, display_name, line_no)?;
                out.push_str(&expanded);
                out.push('\n');
            }
            i = j + 1;
        }

        Ok(out)
    }

    /// True bila `word` adalah nama makro yang pernah didefinisikan.
    /// Dipakai untuk membedakan pemanggilan makro di awal baris vs direktif asing.
    fn is_macro_call(&self, word: &str) -> bool {
        self.macros.contains_key(word)
    }
}
