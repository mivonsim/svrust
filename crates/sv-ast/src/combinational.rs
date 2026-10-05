// Tanggung jawab: node AST untuk statement di dalam always_comb.
use crate::expression::Expr;
use crate::lvalue::Lvalue;
use sv_lexer::span::Span;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaseKind {
    /// `case` — perbandingan bit eksak.
    Exact,
    /// `casez` — `z`/`?` pada label menjadi wildcard.
    Casez,
    /// `casex` — `x`/`z`/`?` pada label menjadi wildcard.
    Casex,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompoundOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
    /// `>>>=` — geser kanan aritmetik (LRM §11.4.10).
    Sar,
}

/// Pernyataan di dalam blok prosedural.
///
/// Selisih ukuran antarvarian berasal dari `Expr` yang cukup besar dan
/// disalin penuh pada tiap statement. Boxing akan menambah indirection pada
/// setiap akses, sedangkan AST adalah nilai milik sendiri yang segera
/// dibuang setelah dielaborasi, jadi dibiarkan apa adanya.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq)]
pub enum CombinationalStatement {
    /// Deklarasi variabel lokal di dalam blok prosedural (LRM §12.8).
    ///
    /// Eligator mendaftarkannya sebagai variable design lalu mengabaikan
    /// statement ini, jadi tidak ada efek runtime.
    Decl(crate::declaration::Declaration),
    BlockingAssign {
        lhs: Lvalue,
        rhs: Expr,
        span: Span,
    },
    /// Non-blocking assignment `q <= a;` (LRM §10.4).
    ///
    /// Sah di mana pun assignment biasa sah, termasuk di dalam `initial`
    /// testbench — LRM §15.2 tidak membatasi `<=` pada blok prosedural
    /// khusus. Tanpa variant ini `initial q <= 8'hAA;` ditolak parser.
    NonBlockingAssign {
        lhs: Lvalue,
        rhs: Expr,
        span: Span,
    },
    /// Compound assignment seperti `y += a` (LRM §11.3).
    /// Setara dengan `y = y <op> a` memakai nilai `y` sebelum assignment.
    CompoundAssign {
        lhs: Lvalue,
        op: CompoundOp,
        rhs: Expr,
        span: Span,
    },
    /// Loop `for (init; cond; step) body` (LRM §12.7.1).
    For {
        init: ForStep,
        condition: Expr,
        step: ForStep,
        body: Vec<CombinationalStatement>,
        span: Span,
    },
    /// Panggilan system task seperti `$display(...)` (LRM §20).
    SystemTask(crate::system_task::SystemTask),
    /// Penundaan `#n` sebelum statement berikutnya (LRM §11.2).
    ///
    /// Tubuh setelah `#n` disimpan terpisah agar delay bisa menggeser waktu
    /// simulasi tepat sebelum statement itu dijalankan.
    Delay {
        amount: Expr,
        /// Satuan waktu penulisan `#n`; `Ns` bila `#n` tanpa satuan.
        unit: crate::time_unit::TimeUnit,
        body: Box<CombinationalStatement>,
        span: Span,
    },
    /// Loop `repeat (count) body` (LRM §12.8.1).
    Repeat {
        count: Expr,
        body: Vec<CombinationalStatement>,
        span: Span,
    },
    /// Loop `while (cond) body` (LRM §12.7.2).
    While {
        condition: Expr,
        body: Vec<CombinationalStatement>,
        span: Span,
    },
    Case {
        selector: Expr,
        arms: Vec<CaseArm>,
        kind: CaseKind,
        span: Span,
    },
    IfElse {
        condition: Expr,
        then_branch: Vec<CombinationalStatement>,
        else_branch: Option<Vec<CombinationalStatement>>,
        span: Span,
    },
    Block {
        body: Vec<CombinationalStatement>,
        span: Span,
    },
    /// Tunggu perubahan sinyal `@(posedge clk)` lalu lanjut statement
    /// berikutnya (LRM §9.7).
    ///
    /// Tubuh kosong berarti hanya menunggu tanpa statement tambahan, seperti
    /// `@(posedge clk);` di dalam blok `initial`.
    EventControl {
        /// Satu atau beberapa item event; pemicu bila salah satu terjadi.
        events: Vec<crate::event_item::EventItem>,
        body: Vec<CombinationalStatement>,
        span: Span,
    },
    /// Panggilan task pada posisi statement (LRM §13.3).
    ///
    /// Eligator meng-inline badan task ke titik panggilan, sehingga node ini
    /// tidak pernah mencapai IR.
    TaskCall(crate::routine::TaskCall),
    /// `return [expr];` di dalam function/task (LRM §13.4).
    ///
    /// Di dalam function, nilai yang dibawa `value` menjadi nilai balik; di
    /// dalam task, `value` harus `None` karena task tidak mengembalikan nilai.
    Return {
        value: Option<Expr>,
        span: Span,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct CaseArm {
    pub labels: Vec<Expr>,
    pub is_default: bool,
    pub body: Vec<CombinationalStatement>,
    pub span: Span,
}

/// Satu langkah loop `for`: assignment ke variabel loop (LRM §12.7.1).
#[derive(Debug, Clone, PartialEq)]
pub struct ForStep {
    pub lhs: Lvalue,
    pub rhs: Expr,
    pub span: Span,
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::event_edge::EventEdge;
    use crate::event_item::EventItem;
    use sv_lexer::span::Span;

    #[test]
    fn event_control_menyimpan_item_edge_dan_sinyal() {
        let stmt = CombinationalStatement::EventControl {
            events: vec![EventItem::dengan_span(
                EventEdge::Posedge,
                "clk",
                Span::dummy(),
            )],
            body: Vec::new(),
            span: Span::dummy(),
        };
        match stmt {
            CombinationalStatement::EventControl { events, body, .. } => {
                assert_eq!(events.len(), 1);
                assert_eq!(events[0].edge, EventEdge::Posedge);
                assert_eq!(events[0].signal, "clk");
                assert!(body.is_empty());
            }
            lain => panic!("dapat {:?}", lain),
        }
    }

    #[test]
    fn event_control_bisa_membawa_tubuh() {
        let stmt = CombinationalStatement::EventControl {
            events: vec![EventItem::dengan_span(
                EventEdge::Negedge,
                "rst",
                Span::dummy(),
            )],
            body: vec![CombinationalStatement::Block {
                body: Vec::new(),
                span: Span::dummy(),
            }],
            span: Span::dummy(),
        };
        match stmt {
            CombinationalStatement::EventControl { body, .. } => {
                assert_eq!(body.len(), 1);
            }
            lain => panic!("dapat {:?}", lain),
        }
    }

    #[test]
    fn event_control_menyimpan_dua_item_gabungan() {
        let stmt = CombinationalStatement::EventControl {
            events: vec![
                EventItem::dengan_span(EventEdge::Posedge, "clk", Span::dummy()),
                EventItem::dengan_span(EventEdge::Negedge, "rst", Span::dummy()),
            ],
            body: Vec::new(),
            span: Span::dummy(),
        };
        match stmt {
            CombinationalStatement::EventControl { events, .. } => assert_eq!(events.len(), 2),
            lain => panic!("dapat {:?}", lain),
        }
    }
}
