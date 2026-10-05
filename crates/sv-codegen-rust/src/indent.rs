// Tanggung jawab: generator indentasi untuk kode Rust keluaran.
pub const INDENT_WIDTH: usize = 4;

#[derive(Debug, Clone, Default)]
pub struct Indent {
    level: usize,
}

impl Indent {
    pub fn new() -> Self {
        Self { level: 0 }
    }

    pub fn child(&self) -> Self {
        Self {
            level: self.level + 1,
        }
    }

    pub fn push(&self, out: &mut String) {
        for _ in 0..self.level {
            out.push_str("    ");
        }
    }

    pub fn level(&self) -> usize {
        self.level
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_zero_writes_nothing() {
        let mut out = String::new();
        Indent::new().push(&mut out);
        assert_eq!(out, "");
    }

    #[test]
    fn child_adds_one_level() {
        let mut out = String::new();
        Indent::new().child().push(&mut out);
        assert_eq!(out, "    ");
    }

    #[test]
    fn two_levels_eight_spaces() {
        let mut out = String::new();
        Indent::new().child().child().push(&mut out);
        assert_eq!(out, "        ");
    }
}
