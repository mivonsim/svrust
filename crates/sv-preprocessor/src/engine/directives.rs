// Tanggung jawab: penanganan direktif preprocessing (ifdef, define, include, timescale).
use crate::error::PreprocessError;
use std::fs;
use std::path::{Path, PathBuf};

fn cari_rekursif(dir: &Path, nama: &str) -> Option<PathBuf> {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if let Some(found) = cari_rekursif(&path, nama) {
                    return Some(found);
                }
            } else if path.file_name().and_then(|n| n.to_str()) == Some(nama) {
                return Some(path);
            }
        }
    }
    None
}

use super::{Engine, Frame};

/// True bila frame kondisional teratas memilih cabang aktif.
fn aktif(frames: &[Frame]) -> bool {
    frames.last().map(|frame| frame.active).unwrap_or(true)
}

impl Engine {
    pub(super) fn handle_conditional(
        &mut self,
        word: &str,
        rest: &str,
        file: &str,
        line: usize,
    ) -> Result<(), PreprocessError> {
        match word {
            "ifdef" | "ifndef" => {
                let name = self.directive_arg(rest, word, file, line)?;
                let parent = self.is_active();
                let present = self.macros.contains_key(&name);
                let cond = if word == "ifdef" { present } else { !present };
                self.stack.push(Frame {
                    active: parent && cond,
                    taken: parent && cond,
                    seen_else: false,
                    parent,
                });
            }
            "elsif" => {
                let name = self.directive_arg(rest, word, file, line)?;
                let frame = self
                    .stack
                    .last_mut()
                    .ok_or_else(|| PreprocessError::at("`elsif tanpa `ifdef", file, line))?;
                if frame.seen_else {
                    return Err(PreprocessError::at("`elsif setelah `else", file, line));
                }
                let active = !frame.taken && frame.parent && self.macros.contains_key(&name);
                frame.active = active;
                if active {
                    frame.taken = true;
                }
            }
            "else" => {
                let frame = self
                    .stack
                    .last_mut()
                    .ok_or_else(|| PreprocessError::at("`else tanpa `ifdef", file, line))?;
                if frame.seen_else {
                    return Err(PreprocessError::at("`else ganda", file, line));
                }
                frame.seen_else = true;
                let active = !frame.taken && frame.parent;
                frame.active = active;
                if active {
                    frame.taken = true;
                }
            }
            "endif" => {
                // `endif tanpa `ifdef` yang cocok — lewati (bisa terjadi
                // bila makro bersyarat melintasi berkas include).
                let _ = self.stack.pop();
            }
            _ => unreachable!(),
        }
        Ok(())
    }

    /// Selesaikan `ifdef/`ifndef/`elsif/`else/`endif yang tersisa di teks
    /// hasil ekspansi makro.
    ///
    /// Body makro OpenTitan memuat direktif kondisional (mis. `ASSERT_ERROR`
    /// memilih `uvm_report_error` atau `$error`), dan LRM §22.5.2 menyatakan
    /// direktif di dalam body baru dievaluasi saat makro dipakai — bukan saat
    /// didefinisikan. Tanpa pass ini direktif mentah bocor ke parser dan
    /// memunculkan error pada baris yang salah. Direktif boleh berada di tengah
    /// baris, karena body makro digabung menjadi satu baris.
    pub(super) fn selesaikan_kondisional(&self, teks: &str) -> String {
        let chars: Vec<char> = teks.chars().collect();
        let mut out = String::with_capacity(teks.len());
        let mut frames: Vec<Frame> = Vec::new();
        let mut in_string = false;
        let mut i = 0usize;
        while i < chars.len() {
            let c = chars[i];
            if c == '"' {
                in_string = !in_string;
                if aktif(&frames) {
                    out.push(c);
                }
                i += 1;
                continue;
            }
            if c == '`' && !in_string {
                let mut j = i + 1;
                while j < chars.len() && (chars[j].is_alphanumeric() || chars[j] == '_') {
                    j += 1;
                }
                let word: String = chars[i + 1..j].iter().collect();
                match word.as_str() {
                    "ifdef" | "ifndef" | "elsif" => {
                        let mut k = j;
                        while k < chars.len() && chars[k].is_whitespace() {
                            k += 1;
                        }
                        let mut m = k;
                        while m < chars.len() && (chars[m].is_alphanumeric() || chars[m] == '_') {
                            m += 1;
                        }
                        let nama: String = chars[k..m].iter().collect();
                        let ada = !nama.is_empty() && self.macros.contains_key(&nama);
                        if word == "elsif" {
                            if let Some(frame) = frames.last_mut() {
                                let cond = !frame.taken && frame.parent && ada;
                                frame.active = cond;
                                if cond {
                                    frame.taken = true;
                                }
                            }
                        } else {
                            let parent = aktif(&frames);
                            let cond = if word == "ifdef" { ada } else { !ada };
                            frames.push(Frame {
                                active: parent && cond,
                                taken: parent && cond,
                                seen_else: false,
                                parent,
                            });
                        }
                        i = m;
                        continue;
                    }
                    "else" => {
                        if let Some(frame) = frames.last_mut() {
                            let cond = !frame.taken && frame.parent;
                            frame.active = cond;
                            frame.seen_else = true;
                            if cond {
                                frame.taken = true;
                            }
                        }
                        i = j;
                        continue;
                    }
                    "endif" => {
                        frames.pop();
                        i = j;
                        continue;
                    }
                    _ => {}
                }
            }
            if aktif(&frames) {
                out.push(c);
            }
            i += 1;
        }
        out
    }

    pub(super) fn handle_other(
        &mut self,
        word: &str,
        rest: &str,
        file: &str,
        line: usize,
        out: &mut String,
    ) -> Result<(), PreprocessError> {
        match word {
            "undef" => {
                let name = self.directive_arg(rest, word, file, line)?;
                self.macros.remove(&name);
            }
            "undefineall" => {
                self.macros.clear();
            }
            "default_nettype" => {
                let nilai = rest["default_nettype".len()..].trim();
                self.default_nettype = if nilai.is_empty() || nilai == "wire" {
                    None
                } else {
                    Some(nilai.to_string())
                };
            }
            "include" => {
                let after = rest["include".len()..].trim_start();
                // `` `include `MAKRO ``: nama file ditentukan oleh makro (biasanya
                // melalui `-D` dari command line). Expand makro bila dikenal;
                // bila tidak dikenal, skip include secara diam-diam (makro
                // biasanya didefinisikan di luar cakupan berkas yang diproses).
                if let Some(rest_bt) = after.strip_prefix('`') {
                    let nama: String = rest_bt
                        .chars()
                        .take_while(|c| c.is_alphanumeric() || *c == '_')
                        .collect();
                    if !nama.is_empty() {
                        let fname = self.macros.get(&nama).map(|mac| {
                            let fname = mac.body.trim();
                            fname
                                .strip_prefix('"')
                                .and_then(|s| s.strip_suffix('"'))
                                .unwrap_or(fname)
                                .to_string()
                        });
                        if let Some(fname) = fname {
                            self.do_include(&fname, file, line, out)?;
                        }
                        // makro tidak dikenal: skip
                    }
                } else {
                    let path = self.include_arg(rest, file, line)?;
                    self.do_include(&path, file, line, out)?;
                }
            }
            "timescale" => {
                self.timescale = rest[word.len()..].trim().to_string();
                // Direktif diteruskan ke output (dengan baris baru) supaya
                // lexer/parser bisa mengaitkannya dengan modul yang menyusul:
                // LRM §21.8 menyatakan `timescale` berlaku per modul, bukan
                // satu nilai global untuk seluruh berkas.
                out.push_str(&format!("`timescale {}\n", self.timescale));
            }
            // Direktif yang diterima namun tidak mengubah semantik di level ini.
            "resetall"
            | "celldefine"
            | "endcelldefine"
            | "nounconnected_drive"
            | "unconnected_drive"
            | "begin_keywords"
            | "end_keywords"
            | "line"
            | "pragma"
            | "default_decay_time" => {}
            // Direktif tak dikenal (mis. `ASSERT` dari OpenTitan) — lewatkan ke
            // output agar parser/lexer yang menanggungnya dapat menanganinya.
            other => {
                out.push('`');
                out.push_str(other);
                out.push_str(&rest[word.len()..]);
                out.push('\n');
            }
        }
        Ok(())
    }

    /// Parse `define NAME body` atau `define NAME(a, b = 5) body` (function-like).
    /// Parameter boleh punya nilai default setelah tanda `=`.
    pub(super) fn parse_define(
        &self,
        rest: &str,
        file: &str,
        line: usize,
    ) -> Result<(String, super::Macro), PreprocessError> {
        let after = rest["define".len()..].trim_start();
        // Nama makro: huruf awal + alnum/underscore.
        let name_end = after
            .find(|c: char| !(c.is_alphanumeric() || c == '_'))
            .unwrap_or(after.len());
        let name = after[..name_end].trim();
        if name.is_empty() {
            return Err(PreprocessError::at("`define memerlukan nama", file, line));
        }
        let mut cursor = &after[name_end..];

        // Opsional daftar argumen formal langsung setelah nama: (a, b = 5).
        let mut raw_params: Vec<String> = Vec::new();
        if cursor.starts_with('(') {
            // Cari `)` penutup dengan depth counting agar nested parens di
            // default (mis. `x #(y)` atau `x #(y, z)`) tidak ditanggap sebagai
            // penutup daftar parameter `define` terlalu awal.
            let mut depth = 1u32;
            let close = cursor[1..]
                .find(|c| match c {
                    '(' => {
                        depth += 1;
                        false
                    }
                    ')' => {
                        depth = depth.saturating_sub(1);
                        depth == 0
                    }
                    _ => false,
                })
                .ok_or_else(|| PreprocessError::at("`define argumen tanpa ')'", file, line))?;
            let close = 1 + close; // indeks dalam cursor
            let inner = &cursor[1..close];
            // Split inner oleh koma, tapi abaikan koma di dalam nested parens.
            let mut part_start = 0usize;
            let mut inner_depth = 0u32;
            for (i, c) in inner.char_indices() {
                match c {
                    '(' => inner_depth += 1,
                    ')' => inner_depth = inner_depth.saturating_sub(1),
                    ',' if inner_depth == 0 => {
                        raw_params.push(inner[part_start..i].to_string());
                        part_start = i + 1;
                    }
                    _ => {}
                }
            }
            let last = inner[part_start..].to_string();
            if !last.trim().is_empty() {
                raw_params.push(last);
            }
            cursor = &cursor[close + 1..];
        }
        // Konversi masing-masing bagian parameter: `nama` atau `nama = default`.
        let mut params: Vec<super::Param> = Vec::new();
        for part in raw_params {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            match part.split_once('=') {
                Some((n, d)) => params.push(super::Param {
                    name: n.trim().to_string(),
                    default: Some(d.trim().to_string()),
                }),
                None => params.push(super::Param {
                    name: part.to_string(),
                    default: None,
                }),
            }
        }

        let body = cursor.trim_start().to_string();
        Ok((name.to_string(), super::Macro { params, body }))
    }

    fn directive_arg(
        &self,
        rest: &str,
        word: &str,
        file: &str,
        line: usize,
    ) -> Result<String, PreprocessError> {
        let name = rest[word.len()..]
            .split_whitespace()
            .next()
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        name.ok_or_else(|| PreprocessError::at(format!("`{} memerlukan nama", word), file, line))
    }

    fn include_arg(&self, rest: &str, file: &str, line: usize) -> Result<String, PreprocessError> {
        let after = rest["include".len()..].trim_start();
        // Ambil token pertama (nama file dalam "..." atau <...>), abaikan komentar
        // atau spasi trailing pada baris yang sama, mis.
        // `include "chip_hier_macros.svh"  // TODO: Deprecate this.
        let target = after
            .split_whitespace()
            .next()
            .filter(|s| !s.is_empty())
            .and_then(|s| {
                s.strip_prefix('"')
                    .and_then(|s| s.strip_suffix('"'))
                    .or_else(|| s.strip_prefix('<').and_then(|s| s.strip_suffix('>')))
            });
        target
            .map(|s| s.to_string())
            .ok_or_else(|| PreprocessError::at("`include memerlukan nama file", file, line))
    }

    fn resolve_include(
        &self,
        rel: &str,
        file: &str,
        line: usize,
    ) -> Result<PathBuf, PreprocessError> {
        let direct = PathBuf::from(rel);
        if direct.is_file() {
            return Ok(direct);
        }
        for dir in &self.search_dirs {
            let candidate = dir.join(rel);
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
        // Fallback: cari rekursif di search_dirs (berguna untuk OpenTitan yang
        // menyimpan include di direktori berbeda tanpa +incdir+ eksplisit).
        let nama = Path::new(rel)
            .file_name()
            .map(|n| n.to_string_lossy().to_string());
        if let Some(nama) = nama {
            for dir in &self.search_dirs {
                if let Some(found) = cari_rekursif(dir, &nama) {
                    return Ok(found);
                }
            }
            if let Some(found) = cari_rekursif(Path::new("."), &nama) {
                return Ok(found);
            }
        }
        Err(PreprocessError::at(
            format!("file include tidak ditemukan: {}", rel),
            file,
            line,
        ))
    }

    fn do_include(
        &mut self,
        rel: &str,
        file: &str,
        line: usize,
        out: &mut String,
    ) -> Result<(), PreprocessError> {
        let resolved = self.resolve_include(rel, file, line)?;
        let canon = resolved.canonicalize().map_err(|e| {
            PreprocessError::at(format!("gagal resolve {}: {}", rel, e), file, line)
        })?;
        if self.include_stack.contains(&canon) {
            return Err(PreprocessError::at(
                format!("siklus `include terdeteksi untuk {}", rel),
                file,
                line,
            ));
        }
        let content = std::fs::read_to_string(&resolved)
            .map_err(|e| PreprocessError::at(format!("gagal baca {}: {}", rel, e), file, line))?;
        self.include_stack.push(canon);
        if let Some(parent) = resolved.parent() {
            self.search_dirs.push(parent.to_path_buf());
        }
        let text = self.run(&content, &resolved.display().to_string())?;
        self.include_stack.pop();
        out.push_str(&text);
        if !text.ends_with('\n') {
            out.push('\n');
        }
        Ok(())
    }
}
