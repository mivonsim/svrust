// Tanggung jawab: penanganan direktif preprocessing (ifdef, define, include, timescale).
use crate::error::PreprocessError;
use std::path::PathBuf;

use super::{Engine, Frame};

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
                self.stack
                    .pop()
                    .ok_or_else(|| PreprocessError::at("`endif tanpa `ifdef", file, line))?;
            }
            _ => unreachable!(),
        }
        Ok(())
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
            "include" => {
                let path = self.include_arg(rest, file, line)?;
                self.do_include(&path, file, out)?;
            }
            "timescale" => {
                self.timescale = rest[word.len()..].trim().to_string();
            }
            // Direktif yang diterima namun tidak mengubah semantik di level ini.
            "default_nettype"
            | "resetall"
            | "celldefine"
            | "endcelldefine"
            | "nounconnected_drive"
            | "unconnected_drive"
            | "begin_keywords"
            | "end_keywords"
            | "line"
            | "pragma"
            | "default_decay_time" => {}
            other => {
                return Err(PreprocessError::at(
                    format!("direktif `{} tidak dikenal", other),
                    file,
                    line,
                ));
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
        let mut params = Vec::new();
        if cursor.starts_with('(') {
            let close = cursor
                .find(')')
                .ok_or_else(|| PreprocessError::at("`define argumen tanpa ')'", file, line))?;
            let inner = &cursor[1..close];
            for part in inner.split(',') {
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
            cursor = &cursor[close + 1..];
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
        let target = after
            .strip_prefix('\"')
            .and_then(|s| s.strip_suffix('\"'))
            .or_else(|| after.strip_prefix('<').and_then(|s| s.strip_suffix('>')));
        target
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .ok_or_else(|| PreprocessError::at("`include memerlukan nama file", file, line))
    }

    fn resolve_include(&self, rel: &str, file: &str) -> Result<PathBuf, PreprocessError> {
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
        Err(PreprocessError::at(
            format!("file include tidak ditemukan: {}", rel),
            file,
            0,
        ))
    }

    fn do_include(
        &mut self,
        rel: &str,
        file: &str,
        out: &mut String,
    ) -> Result<(), PreprocessError> {
        let resolved = self.resolve_include(rel, file)?;
        let canon = resolved
            .canonicalize()
            .map_err(|e| PreprocessError::at(format!("gagal resolve {}: {}", rel, e), file, 0))?;
        if self.include_stack.contains(&canon) {
            return Err(PreprocessError::at(
                format!("siklus `include terdeteksi untuk {}", rel),
                file,
                0,
            ));
        }
        let content = std::fs::read_to_string(&resolved)
            .map_err(|e| PreprocessError::at(format!("gagal baca {}: {}", rel, e), file, 0))?;
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
