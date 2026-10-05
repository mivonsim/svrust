// Tanggung jawab: node AST untuk statement procedural/continuous.
use crate::combinational::CombinationalStatement;
use crate::expression::Expr;
use crate::lvalue::Lvalue;
use sv_lexer::span::Span;

#[derive(Debug, Clone, PartialEq)]
pub enum Statement {
    ContinuousAssign {
        lhs: Lvalue,
        rhs: Expr,
        span: Span,
    },
    /// Blok `always_ff @(posedge clk ...)` (LRM §9.2.1).
    ///
    /// LRM §9.7: daftar event boleh digabung `or`, misalnya
    /// `@(posedge clk or posedge rst)` untuk reset asinkron; proses dievaluasi
    /// saat salah satu item terjadi.
    AlwaysFf {
        events: Vec<crate::event_item::EventItem>,
        body: Vec<SequentialStatement>,
        span: Span,
    },
    AlwaysComb {
        body: Vec<CombinationalStatement>,
        span: Span,
    },
    /// Blok `initial begin ... end` pada testbench (LRM §15.2).
    Initial {
        body: Vec<CombinationalStatement>,
        span: Span,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum SequentialStatement {
    NonBlockingAssign {
        lhs: Lvalue,
        rhs: Expr,
        span: Span,
    },
    /// Blocking assignment di dalam blok sekuensial — sah di SV (LRM §10.3),
    /// hanya berbeda waktuswipernya dari nonblocking.
    BlockingAssign {
        lhs: Lvalue,
        rhs: Expr,
        span: Span,
    },
    /// `if (syarat) ... else ...` di dalam blok `always_ff` (LRM §12.4).
    IfElse {
        condition: Expr,
        then_branch: Vec<SequentialStatement>,
        else_branch: Option<Vec<SequentialStatement>>,
        span: Span,
    },
    /// Deklarasi variabel lokal di dalam blok `always_ff` (LRM §12.8).
    Decl(crate::declaration::Declaration),
    /// Panggilan system task di dalam blok sekuensial (LRM §20), misal
    /// `$display(...)` atau `$finish` di dalam `always_ff`.
    ///
    /// Sah di SystemVerilog: LRM §20 tidak membatasi system task pada
    /// `initial` saja.
    SystemTask(crate::system_task::SystemTask),
    /// Panggilan task di dalam blok sekuensial (LRM §13.3), misal `sink(x);`
    /// di dalam `always_ff`.
    ///
    /// Eligator meng-inline badan task ke titik panggilan.
    TaskCall(crate::routine::TaskCall),
    /// `return [expr];` di dalam blok sekuensial (LRM §13.4).
    Return {
        value: Option<Expr>,
        span: Span,
    },
}
