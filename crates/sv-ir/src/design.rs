// Tanggung jawab: node design/elaboration dalam IR.
use crate::datatype::DataType;
use crate::expr::SignalId;
use crate::scope::ScopePath;
use crate::time_scale::TimeScale;
use crate::variable::VarDecl;
use std::collections::BTreeMap;
use std::rc::Rc;
use sv_ast::dpi::DpiDecl;

/// Satu dimensi unpacked: (first, size, descending).
pub type UnpackedElem = (u64, u64, bool);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortDirection {
    Input,
    Output,
    Inout,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PortInfo {
    pub name: String,
    pub direction: PortDirection,
    pub data_type: DataType,
}

#[derive(Debug, Clone, Default)]
pub struct InstanceInfo {
    pub module_name: String,
    pub scope: ScopePath,
    pub connections: BTreeMap<String, crate::expr::SignalId>,
}

#[derive(Debug, Clone, Default)]
pub struct Design {
    pub name: String,
    pub ports: Vec<PortInfo>,
    pub variables: Vec<Rc<VarDecl>>,
    pub processes: Vec<crate::process::Process>,
    pub instances: Vec<InstanceInfo>,
    /// `timescale` modul top (LRM §21.8).
    ///
    /// Dipakai untuk membulatkan `$time` ke presisi modul dan untuk header
    /// `$timescale` di VCD. `Option` karena `Design::new` tidak tahu apa pun
    /// tentang SV; elaborator yang mengisinya.
    pub time_scale: Option<TimeScale>,
    /// Dimensi unpacked per sinyal (LRM §7.8), untuk mengetahui offset elemen
    /// pada sinyal rata saat codegen. Setiap entri: (signal_id, daftar
    /// (first, size, descending) dari luar ke dalam).
    pub unpacked_dims: Vec<(SignalId, Vec<UnpackedElem>)>,
    /// Level hierarki `$dumpvars(n)` (LRM §23.2). `None` = semua hierarki,
    /// `Some(n)` = hanya level 0..n dari root.
    pub dump_level: Option<u32>,
    /// Deklarasi DPI-C `import` / `export "DPI-C" function ...` (LRM §35).
    /// Codegen menghasilkan blok `extern "C"` untuk tiap entri agar fungsi C
    /// dapat dipanggil dari generated Rust (pemanggilan nyata diserahkan ke
    /// runtime DPI-C di fase berikutnya).
    pub dpis: Vec<DpiDecl>,
}

impl Design {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Default::default()
        }
    }

    pub fn add_variable(&mut self, decl: VarDecl) {
        self.variables.push(Rc::new(decl));
    }

    pub fn find_variable(&self, name: &str) -> Option<&Rc<VarDecl>> {
        self.variables.iter().find(|v| v.name == name)
    }

    /// Tetapkan nilai awal waktu nol untuk sebuah signal.
    pub fn set_initial(&mut self, signal: crate::expr::SignalId, value: crate::expr::Expr) {
        if let Some(var) = self.variables.iter_mut().find(|v| v.signal_id == signal) {
            Rc::get_mut(var).expect("VarDecl belum di-share").initial = Some(value);
        }
    }

    /// Dimensi unpacked suatu sinyal, untuk mengetahui offset elemen pada
    /// sinyal rata saat codegen.
    pub fn unpacked_dims(&self, signal_id: SignalId) -> Option<&Vec<UnpackedElem>> {
        self.unpacked_dims
            .iter()
            .find(|(id, _)| *id == signal_id)
            .map(|(_, dims)| dims)
    }

    pub fn port_names(&self) -> Vec<&str> {
        self.ports.iter().map(|p| p.name.as_str()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::variable::VarKind;

    #[test]
    fn design_starts_empty() {
        let design = Design::new("top");
        assert_eq!(design.name, "top");
        assert!(design.variables.is_empty());
    }

    #[test]
    fn find_variable_by_name() {
        let mut design = Design::new("top");
        design.add_variable(VarDecl::new(
            "clk",
            ScopePath::root(),
            DataType::bit(),
            VarKind::Variable,
            0,
        ));
        assert!(design.find_variable("clk").is_some());
        assert!(design.find_variable("dne").is_none());
    }
}
