// Tanggung jawab: variabel dan sinyal dalam IR.
use crate::datatype::DataType;
use crate::scope::ScopePath;
use std::rc::Rc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VarKind {
    /// Ditulis oleh continuous assignment.
    Net,
    /// Ditulis oleh procedural assignment.
    Variable,
    Port,
    Parameter,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VarDecl {
    pub name: String,
    pub path: ScopePath,
    pub data_type: DataType,
    pub kind: VarKind,
    pub signal_id: crate::expr::SignalId,
    pub initial: Option<crate::expr::Expr>,
}

impl VarDecl {
    pub fn new(
        name: impl Into<String>,
        scope: ScopePath,
        data_type: DataType,
        kind: VarKind,
        signal_id: crate::expr::SignalId,
    ) -> Self {
        Self {
            name: name.into(),
            path: scope,
            data_type,
            kind,
            signal_id,
            initial: None,
        }
    }

    pub fn qualified_name(&self) -> String {
        if self.path.is_root() {
            self.name.clone()
        } else {
            format!("{}.{}", self.path.as_string(), self.name)
        }
    }
}

#[derive(Debug, Default)]
pub struct SignalTable {
    entries: Vec<Rc<VarDecl>>,
}

impl SignalTable {
    pub fn insert(&mut self, decl: VarDecl) -> Rc<VarDecl> {
        let entry = Rc::new(decl);
        self.entries.push(Rc::clone(&entry));
        entry
    }

    pub fn get(&self, name: &str) -> Option<&Rc<VarDecl>> {
        self.entries.iter().find(|e| e.name == name)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Rc<VarDecl>> {
        self.entries.iter()
    }
}
