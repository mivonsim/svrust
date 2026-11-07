// Tanggung jawab: menyimpan source dan memetakan offset menjadi lokasi baris/kolom.
use crate::diagnostic::SourceId;
use crate::Span;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Location {
    pub line: usize,
    pub column: usize,
}

#[derive(Debug, Clone)]
pub struct SourceFile {
    pub name: String,
    pub text: String,
    line_starts: Vec<usize>,
}

impl SourceFile {
    fn new(name: impl Into<String>, text: impl Into<String>) -> Self {
        let text = text.into();
        let mut line_starts = vec![0];
        for (index, byte) in text.bytes().enumerate() {
            if byte == b'\n' {
                line_starts.push(index + 1);
            }
        }
        Self {
            name: name.into(),
            text,
            line_starts,
        }
    }

    pub fn location(&self, offset: usize) -> Location {
        let offset = self.floor_char_boundary(offset.min(self.text.len()));
        let line_index = self.line_starts.partition_point(|start| *start <= offset) - 1;
        let line_start = self.line_starts[line_index];
        let column = self.text[line_start..offset].chars().count() + 1;
        Location {
            line: line_index + 1,
            column,
        }
    }

    pub fn line(&self, line: usize) -> Option<&str> {
        let start = *self.line_starts.get(line.checked_sub(1)?)?;
        let end = self
            .line_starts
            .get(line)
            .copied()
            .unwrap_or(self.text.len());
        Some(self.text[start..end].trim_end_matches(['\n', '\r']))
    }

    pub fn line_start(&self, line: usize) -> Option<usize> {
        self.line_starts.get(line.checked_sub(1)?).copied()
    }

    pub fn offset(&self, line: usize, column: usize) -> Option<usize> {
        if column == 0 {
            return None;
        }
        let line_text = self.line(line)?;
        let line_start = self.line_start(line)?;
        let byte_offset = line_text
            .char_indices()
            .nth(column - 1)
            .map(|(offset, _)| offset)
            .or_else(|| (line_text.chars().count() + 1 == column).then_some(line_text.len()))?;
        Some(line_start + byte_offset)
    }

    pub(crate) fn floor_char_boundary(&self, offset: usize) -> usize {
        let mut boundary = offset;
        while !self.text.is_char_boundary(boundary) {
            boundary -= 1;
        }
        boundary
    }
}

#[derive(Debug, Clone, Default)]
pub struct SourceMap {
    files: Vec<SourceFile>,
}

impl SourceMap {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_file(&mut self, name: impl Into<String>, text: impl Into<String>) -> SourceId {
        let id = SourceId(self.files.len());
        self.files.push(SourceFile::new(name, text));
        id
    }

    pub fn get(&self, id: SourceId) -> Option<&SourceFile> {
        self.files.get(id.0)
    }

    pub fn location(&self, span: Span) -> Option<Location> {
        self.get(span.source).map(|file| file.location(span.start))
    }

    pub fn span_at(&self, source: SourceId, line: usize, column: usize) -> Option<Span> {
        let offset = self.get(source)?.offset(line, column)?;
        Some(Span::empty(source, offset))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_offsets_to_one_based_line_and_column() {
        let file = SourceFile::new("test.sv", "alpha\nβeta\n");
        assert_eq!(file.location(0), Location { line: 1, column: 1 });
        assert_eq!(file.location(6), Location { line: 2, column: 1 });
        assert_eq!(file.location(8), Location { line: 2, column: 2 });
        assert_eq!(file.line(2), Some("βeta"));
        assert_eq!(file.offset(2, 2), Some(8));
        assert_eq!(file.offset(2, 5), Some(11));
        assert_eq!(file.offset(2, 6), None);
    }
}
