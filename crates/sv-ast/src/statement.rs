// Tanggung jawab: node AST untuk statement procedural/continuous.
use crate::combinational::CombinationalStatement;
use crate::dpi::DpiCall;
use crate::expression::Expr;
use crate::lvalue::Lvalue;
use serde::{Deserialize, Serialize};
use sv_lexer::span::Span;

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    /// Blok `always @*` / `always` tanpa `@` (LRM §9.2.1): dievaluasi setiap
    /// perubahan sinyal dalam body (sensitivitas implisit).
    /// Blok `always @(sinyal)` level-sensitive: dievaluasi setiap kali salah
    /// satu sinyal dalam `events` berubah nilai.
    ///
    /// Jika `events` kosong, berarti sensitivitas implisit (`always` tanpa
    /// `@` atau `always @*`): dievaluasi setiap langkah.
    /// Jika `events` berisi sinyal (misal `always @(clk)`), proses baru
    /// dievaluasi bila salah satu sinyal itu berubah.
    AlwaysComb {
        events: Vec<crate::event_item::EventItem>,
        body: Vec<CombinationalStatement>,
        span: Span,
    },
    /// Proses WAKTU: `always` tanpa sensitivitas yang badannya penundaan,
    /// misal `always #5 clk = ~clk;` (LRM §9.2.1, §11.2).
    ///
    /// Bentuk ini BUKAN `always_comb`: tidak punya sensitivitas, dan
    /// penundaan di dalamnya menentukan kapan badan berjalan — bukan
    /// "setiap perubahan sinyal". Memperlakukannya sebagai proses
    /// combinational membuat jam simulasi bergerak setiap kali proses
    /// dievaluasi.
    AlwaysTimed {
        body: Vec<CombinationalStatement>,
        span: Span,
    },
    /// Blok `initial begin ... end` pada testbench (LRM §15.2).
    Initial {
        body: Vec<CombinationalStatement>,
        span: Span,
    },
    /// Blok `final begin ... end` pada testbench (LRM §15.2).
    ///
    /// Dieksekusi pada akhir simulasi, setelah semua proses selesai.
    Final {
        body: Vec<CombinationalStatement>,
        span: Span,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    /// Panggilan fungsi C lewat DPI-C import sebagai statement (LRM §35).
    ///
    /// Digunakan saat memanggil fungsi yang dideklarasi dengan
    /// `import "DPI-C" function ...;`.
    DpiCall(DpiCall),
    /// `return [expr];` di dalam blok sekuensial (LRM §13.4).
    Return {
        value: Option<Expr>,
        span: Span,
    },
    /// Loop `forever body` (LRM §12.8).
    ///
    /// Loop tak berujung; body wajib mengandung penundaan atau event control
    /// agar simulasi tidak hang.
    Forever {
        body: Vec<SequentialStatement>,
        span: Span,
    },
    /// Blok berlabel `nama: begin ... end` di dalam blok sekuensial (LRM §12.9).
    ///
    /// Nama blok menciptakan scope hierarki; konten blok disatukan ke body
    /// induk karena blok sekuensial tidak menambah statement runtime sendiri.
    Block {
        label: Option<String>,
        body: Vec<SequentialStatement>,
        span: Span,
    },
    /// Penetapans berkelanjutan prosedural `assign lhs = rhs;` di dalam blok
    /// prosedural (LRM §14).
    ProceduralAssign {
        lhs: Lvalue,
        rhs: Expr,
        span: Span,
    },
    /// Melepasi penetapans berkelanjutan prosedural `deassign lhs;` (LRM §14).
    Deassign {
        lhs: Lvalue,
        span: Span,
    },
}
