// Tanggung jawab: node AST untuk region generate (LRM §27).
use crate::declaration::Declaration;
use crate::expression::Expr;
use crate::instance::Instance;
use crate::lvalue::Lvalue;
use crate::statement::Statement;
use sv_lexer::span::Span;

/// Region `generate ... endgenerate` beserta variabel genvar yang dipakai.
#[derive(Debug, Clone, PartialEq)]
pub struct GenerateRegion {
    pub genvars: Vec<String>,
    pub items: Vec<GenerateItem>,
    pub span: Span,
}

/// Satu item di dalam region generate.
#[derive(Debug, Clone, PartialEq)]
pub enum GenerateItem {
    /// `for (genvar = A; genvar < B; genvar = genvar + C) begin : label ... end`
    For(Box<ForGenerate>),
    /// `if (expr) begin : label ... end else begin : label ... end`
    If(Box<IfGenerate>),
    /// `case (expr) label: begin : nama ... end endcase`
    Case(Box<CaseGenerate>),
    /// Instansiasi module di dalam generate.
    Instance(Box<Instance>),
    /// Deklarasi net atau variabel di dalam generate.
    Decl(Vec<Declaration>),
    /// `assign` di dalam generate.
    Assign { lhs: Lvalue, rhs: Expr, span: Span },
    /// `always_ff`/`always_comb`/`initial` di dalam generate.
    Process(Box<Statement>),
}

/// Generate bersyarat (LRM §27.1).
///
/// Kondisi harus konstanta pada waktu elaborasi; cabang yang tidak diambil
/// tidak pernah dielaborasi sehingga sinyalnya tidak muncul di design.
#[derive(Debug, Clone, PartialEq)]
pub struct IfGenerate {
    pub condition: Expr,
    pub then_branch: Vec<GenerateItem>,
    /// `None` bila tidak ada `else`.
    pub else_branch: Option<Vec<GenerateItem>>,
    pub span: Span,
}

/// Satu lengan `case` di dalam generate (LRM §27.1).
#[derive(Debug, Clone, PartialEq)]
pub struct GenerateCaseArm {
    /// Label konstanta; boleh kosong bila `is_default`.
    pub labels: Vec<Expr>,
    pub body: Vec<GenerateItem>,
    pub is_default: bool,
    pub span: Span,
}

/// Generate `case` (LRM §27.1).
#[derive(Debug, Clone, PartialEq)]
pub struct CaseGenerate {
    pub selector: Expr,
    pub arms: Vec<GenerateCaseArm>,
    pub span: Span,
}

/// Loop generate dengan penghitung `genvar` (LRM §27.3).
#[derive(Debug, Clone, PartialEq)]
pub struct ForGenerate {
    pub var: String,
    /// Nilai awal genvar; wajib konstanta.
    pub init: u64,
    /// Nilai batas; di bandingkan sesuai `compare`.
    pub bound: u64,
    pub compare: CompareOp,
    /// Kenaikan per iterasi; negatif untuk iterasi menurun.
    pub step: i64,
    /// Label blok `begin : nama`; menentukan nama hierarki tiap iterasi.
    pub label: String,
    pub body: Vec<GenerateItem>,
    pub span: Span,
}

/// Operator perbandingan batas loop generate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompareOp {
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
}

impl CompareOp {
    /// Uji apakah nilai genvar saat ini masih memenuhi batas loop.
    pub fn holds(self, value: u64, bound: u64) -> bool {
        match self {
            CompareOp::Lt => value < bound,
            CompareOp::Le => value <= bound,
            CompareOp::Gt => value > bound,
            CompareOp::Ge => value >= bound,
            CompareOp::Eq => value == bound,
            CompareOp::Ne => value != bound,
        }
    }

    /// Apakah loop masih mendekati `bound` bila `step` diterapkan.
    ///
    /// `Lt`/`Le` butuh `step` positif dan `Gt`/`Ge` butuh `step` negatif.
    /// Step berlawanan arah membuat nilai genvar menjauh, sehingga loop tidak
    /// pernah selesai dan harus ditolak sebelum elaborasi. `Eq`/`Ne` juga
    /// ditolak karena batasnya tidak akan pernah tercapai.
    pub fn progresses(self, step: i64) -> bool {
        match self {
            CompareOp::Lt | CompareOp::Le => step > 0,
            CompareOp::Gt | CompareOp::Ge => step < 0,
            CompareOp::Eq | CompareOp::Ne => false,
        }
    }
}

#[cfg(test)]
mod sel_tests {
    use super::*;

    fn ident(nama: &str) -> Expr {
        Expr::ident(nama, Span::dummy())
    }

    #[test]
    fn if_generate_menyimpan_kedua_cabang() {
        let item = GenerateItem::If(Box::new(IfGenerate {
            condition: ident("P"),
            then_branch: Vec::new(),
            else_branch: Some(Vec::new()),
            span: Span::dummy(),
        }));
        match item {
            GenerateItem::If(g) => assert!(g.else_branch.is_some()),
            other => panic!("harus if-generate, dapat {:?}", other),
        }
    }

    #[test]
    fn case_generate_menyimpan_lengan_default() {
        let item = GenerateItem::Case(Box::new(CaseGenerate {
            selector: ident("SEL"),
            arms: vec![GenerateCaseArm {
                labels: Vec::new(),
                body: Vec::new(),
                is_default: true,
                span: Span::dummy(),
            }],
            span: Span::dummy(),
        }));
        match item {
            GenerateItem::Case(g) => assert!(g.arms[0].is_default),
            other => panic!("harus case-generate, dapat {:?}", other),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn perbandingan_batas_loop() {
        assert!(CompareOp::Lt.holds(3, 4));
        assert!(!CompareOp::Lt.holds(4, 4));
        assert!(CompareOp::Le.holds(4, 4));
        assert!(CompareOp::Gt.holds(5, 4));
        assert!(CompareOp::Ge.holds(4, 4));
        assert!(CompareOp::Eq.holds(4, 4));
        assert!(CompareOp::Ne.holds(5, 4));
    }

    #[test]
    fn batas_sama_tidak_pernah_maju() {
        // `for (i = 0; i == 4; i++)` tidak pernah selesai, jadi ditolak.
        assert!(!CompareOp::Eq.progresses(1));
        assert!(!CompareOp::Ne.progresses(1));
        assert!(CompareOp::Lt.progresses(1));
    }

    #[test]
    fn step_berlawanan_arah_ditolak() {
        // `for (i = 0; i < 4; i--)` menjauh dari batas dan tidak pernah selesai.
        assert!(!CompareOp::Lt.progresses(-1));
        assert!(!CompareOp::Le.progresses(-2));
        // `for (i = 8; i > 0; i++)` juga menjauh dari batas.
        assert!(!CompareOp::Gt.progresses(1));
        assert!(!CompareOp::Ge.progresses(3));
        // Arah yang benar tetap diterima.
        assert!(CompareOp::Gt.progresses(-1));
        assert!(CompareOp::Ge.progresses(-2));
    }

    #[test]
    fn step_nol_tidak_pernah_maju() {
        assert!(!CompareOp::Lt.progresses(0));
        assert!(!CompareOp::Gt.progresses(0));
    }
}
