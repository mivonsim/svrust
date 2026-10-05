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

struct Engine {
    macros: HashMap<String, Macro>,
    stack: Vec<Frame>,
    timescale: String,
    include_stack: Vec<PathBuf>,
    search_dirs: Vec<PathBuf>,
}

impl Engine {
    fn new(base_dir: Option<&Path>) -> Self {
        let mut search_dirs = Vec::new();
        if let Some(d) = base_dir {
            search_dirs.push(d.to_path_buf());
        }
        Self {
            macros: HashMap::new(),
            stack: Vec::new(),
            timescale: String::new(),
            include_stack: Vec::new(),
            search_dirs,
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
            let stripped = comment::strip_line(&mut in_block_comment, lines[i]);
            in_block_comment = stripped.continues;
            let trimmed = stripped.text.trim_start();

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
                                    let cont =
                                        comment::strip_line(&mut in_block_comment, lines[i]).text;
                                    let cont_trim = cont.trim_start();
                                    let ends_back = cont_trim.trim_end().ends_with('\\');
                                    let piece = if ends_back {
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
                            }
                            self.macros.insert(name, mac);
                        }
                    }
                    _ if !self.is_active() => {}
                    // `` `NAME `` di awal baris adalah pemanggilan makro, bukan direktif.
                    _ if self.is_macro_call(&word) => {
                        let expanded = self.substitute(&stripped.text, display_name, line_no)?;
                        out.push_str(&expanded);
                        out.push('\n');
                    }
                    _ => {
                        self.handle_other(&word, rest, display_name, line_no, &mut out)?;
                    }
                }
            } else if self.is_active() {
                let expanded = self.substitute(&stripped.text, display_name, line_no)?;
                out.push_str(&expanded);
                out.push('\n');
            }
            i += 1;
        }

        if !self.stack.is_empty() {
            return Err(PreprocessError::at(
                "`endif belum ditutup",
                display_name,
                lines.len(),
            ));
        }
        Ok(out)
    }

    /// True bila `word` adalah nama makro yang pernah didefinisikan.
    /// Dipakai untuk membedakan pemanggilan makro di awal baris vs direktif asing.
    fn is_macro_call(&self, word: &str) -> bool {
        self.macros.contains_key(word)
    }
}
