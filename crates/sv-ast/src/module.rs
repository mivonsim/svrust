// Tanggung jawab: node AST untuk module SystemVerilog.
use serde::{Deserialize, Serialize};
use crate::declaration::Declaration;
use crate::generate::GenerateRegion;
use crate::instance::Instance;
use crate::port::Port;
use crate::statement::Statement;
use crate::width::ParamDecl;
use sv_lexer::span::Span;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Module {
    pub name: String,
    pub span: Span,
    pub params: Vec<ParamDecl>,
    pub ports: Vec<Port>,
    pub declarations: Vec<Declaration>,
    pub statements: Vec<Statement>,
    pub instances: Vec<Instance>,
    /// Deklarasi task/function di body modul (LRM §13.3/§13.4).
    pub routines: Vec<crate::routine::RoutineDecl>,
    /// Deklarasi `typedef` di body modul (LRM §8.20 dan §6.7).
    pub typedefs: Vec<crate::typedef::TypeDefDecl>,
    /// Region `generate ... endgenerate` (LRM §27.1).
    pub generates: Vec<GenerateRegion>,
    /// Genvar yang dideklarasikan terpisah di body modul, misal `genvar i, j;`
    /// (LRM §27.3). Bentuk ini setara dengan menulis `for (genvar i = ...)`
    /// di dalam loop, dan dipakai region generate untuk mengikat variabelnya.
    pub genvars: Vec<String>,
    /// Deklarasi `localparam` di body modul (LRM §6.20).
    ///
    /// Dipisah dari `params` karena `localparam` tidak boleh di-override saat
    /// instansiasi — mencampurnya ke daftar parameter port akan membuat
    /// `#(.NAMA(...))` diam-diam diterima untuk konstanta yang memang harus
    /// terkunci.
    pub localparams: Vec<crate::width::LocalParamDecl>,
    /// Deklarasi DPI-C `import` / `export "DPI-C" function ...` di body modul
    /// (LRM §35). Masih pada tingkat deklarasi: parser menangkap sintaks, sementara
    /// pemanggilan fungsi C nyata dan penanganan export diserahkan ke fase
    /// codegen/runtime DPI (nanti).
    pub dpis: Vec<crate::dpi::DpiDecl>,
    /// `timescale` yang berlaku bagi modul ini (LRM §21.8).
    ///
    /// Direktif berlaku untuk modul-modul yang menyusulnya sampai ada
    /// `timescale` berikutnya, jadi nilainya per modul, bukan global.
    pub time_scale: crate::time_scale::TimeScale,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modul_kosong_tanpa_generate() {
        let m = Module {
            name: "m".to_string(),
            span: Span::dummy(),
            params: Vec::new(),
            ports: Vec::new(),
            declarations: Vec::new(),
            statements: Vec::new(),
            instances: Vec::new(),
            routines: Vec::new(),
            typedefs: Vec::new(),
            generates: Vec::new(),
            genvars: Vec::new(),
            localparams: Vec::new(),
            dpis: Vec::new(),
            time_scale: crate::time_scale::TimeScale::default(),
        };
        assert!(m.generates.is_empty());
        assert!(m.genvars.is_empty());
        assert!(m.routines.is_empty());
        assert!(m.typedefs.is_empty());
        assert!(m.localparams.is_empty());
        assert!(m.dpis.is_empty());
    }
}
