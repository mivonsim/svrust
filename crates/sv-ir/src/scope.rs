// Tanggung jawab: definisi scope (nama + hierarki) dalam IR.
use std::collections::BTreeMap;
use std::rc::Rc;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ScopePath {
    pub segments: Vec<String>,
}

impl ScopePath {
    pub fn root() -> Self {
        Self {
            segments: Vec::new(),
        }
    }

    pub fn child(&self, name: impl Into<String>) -> Self {
        let mut segments = self.segments.clone();
        segments.push(name.into());
        Self { segments }
    }

    pub fn is_root(&self) -> bool {
        self.segments.is_empty()
    }

    pub fn leaf(&self) -> Option<&str> {
        self.segments.last().map(|s| s.as_str())
    }

    pub fn as_string(&self) -> String {
        self.segments.join("/")
    }
}

#[derive(Debug, Clone, Default)]
pub struct ScopeTree {
    pub children: BTreeMap<String, Rc<ScopeTree>>,
}
