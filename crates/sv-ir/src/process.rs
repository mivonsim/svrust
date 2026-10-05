// Tanggung jawab: definisi process dan edge sensitivity dalam IR.
use crate::expr::Expr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeSensitivity {
    Posedge,
    Negedge,
    AnyChange,
}

/// Satu item `@(posedge sig)` pada event control gabungan (LRM §9.7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EventItem {
    pub edge: EdgeSensitivity,
    /// Id sinyal yang diawasi; nilainya dibaca runtime tiap langkah.
    pub signal: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProcessKind {
    /// Combinational: re-evaluate saat input berubah.
    Combinational,
    /// Sequential: evaluate pada edge clock.
    Sequential,
    /// Latch: transparan saat enable aktif.
    Latch,
    /// Testbench: berjalan sekali pada waktu nol (LRM §15.2).
    Initial,
}

impl std::fmt::Display for ProcessKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProcessKind::Combinational => write!(f, "combinational"),
            ProcessKind::Sequential => write!(f, "sequential"),
            ProcessKind::Latch => write!(f, "latch"),
            ProcessKind::Initial => write!(f, "initial"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignStyle {
    /// `=` — immediate, terlihat dalam timestep yang sama.
    Blocking,
    /// `<=` — NBA, efek pada akhir timestep.
    NonBlocking,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SensitivityItem {
    pub signal: crate::expr::SignalId,
    pub edge: EdgeSensitivity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Slice {
    pub msb: u32,
    pub lsb: u32,
}

impl Slice {
    pub fn width(self) -> u32 {
        self.msb - self.lsb + 1
    }

    /// Masker dalam bentuk rentang, untuk di-emit codegen.
    ///
    /// Masker sebelumnya dihitung sebagai `u64` di crate ini, yang rapuh untuk
    /// dua alasan: array unpacked disimpan rata sehingga elemen ke-9 sudah
    /// punya `lsb >= 64` (build debug panic di `1u64 << 64`, release wrap
    /// diam-diam), dan `u64` tidak bisa membawa digit `X`/`Z`. Bit dan lebar
    /// sudah dibawa di `msb`/`lsb`, jadi bentuk rentang cukup.
    pub fn mask_range(self) -> (u32, u32) {
        (self.lsb, self.width())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Assignment {
    pub target: crate::expr::SignalId,
    /// `None` berarti seluruh sinyal; `Some` berarti hanya irisan bit.
    pub slice: Option<Slice>,
    pub value: Expr,
    pub style: AssignStyle,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Process {
    pub name: String,
    pub kind: ProcessKind,
    pub sensitivity: Vec<SensitivityItem>,
    pub body: Vec<Statement>,
    pub span: Span,
}

/// Statement IR untuk process sequential/combinational.
#[derive(Debug, Clone, PartialEq)]
pub enum Statement {
    Assign {
        assignment: Assignment,
        span: Span,
    },
    If {
        condition: Expr,
        then_branch: Vec<Statement>,
        else_branch: Vec<Statement>,
        span: Span,
    },
    Case {
        selector: Expr,
        arms: Vec<CaseArm>,
        kind: CaseKind,
        span: Span,
    },
    /// Loop `for` dengan inisialisasi, kondisi, dan langkah (LRM §12.7.1).
    For {
        init: Assignment,
        condition: Expr,
        step: Assignment,
        body: Vec<Statement>,
        span: Span,
    },
    /// Loop `repeat (count) body` (LRM §12.8.1).
    Repeat {
        count: Expr,
        body: Vec<Statement>,
        span: Span,
    },
    /// Loop `while (cond) body` (LRM §12.7.2).
    While {
        condition: Expr,
        body: Vec<Statement>,
        span: Span,
    },
    /// Panggilan system task seperti `$display(...)` (LRM §20).
    SystemTask {
        kind: crate::expr::SystemTaskKind,
        args: Vec<crate::expr::SystemArg>,
        /// Syarat pada `$monitor if (kondisi)`; `None` berarti tanpa syarat (LRM §20.2).
        condition: Option<Expr>,
        /// `timescale` modul pemanggil; dipakai `%t` (LRM §20.4 + §21.8).
        time_scale: crate::time_scale::TimeScale,
        span: Span,
    },
    /// Tunggu edge pada sinyal lalu jalankan body (LRM §9.7).
    ///
    /// Dipakai di blok `initial` testbench, bukan sebagai sensitivitas proses.
    /// Body berjalan bila salah satu `events` benar-benar terjadi.
    EventControl {
        /// Satu atau beberapa item event yang digabung dengan `or`.
        events: Vec<EventItem>,
        body: Vec<Statement>,
        span: Span,
    },
    /// Penundaan waktu simulasi `#n` sebelum statement body (LRM §11.2 / §15.2).
    Delay {
        amount: Expr,
        /// Satuan waktu `#n` agar codegen bisa mengonversinya (LRM §3.3).
        unit: crate::time_unit::TimeUnit,
        body: Box<Statement>,
        span: Span,
    },
    Block {
        body: Vec<Statement>,
        span: Span,
    },
    Noop {
        span: Span,
    },
}

/// Cara pencocokan label case terhadap selektor (LRM §12.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaseKind {
    /// `case` — seluruh bit harus sama persis.
    Exact,
    /// `casez` — `z`/`?` pada label adalah wildcard.
    Casez,
    /// `casex` — `x`/`z`/`?` pada label adalah wildcard.
    Casex,
}

impl CaseKind {
    /// True bila mode memakai wildcard pada label.
    pub fn is_wildcard(self) -> bool {
        matches!(self, CaseKind::Casez | CaseKind::Casex)
    }

    /// True bila digit `x` pada label juga wildcard (`casex`, bukan `casez`).
    pub fn is_casex(self) -> bool {
        matches!(self, CaseKind::Casex)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CaseArm {
    pub labels: Vec<Expr>,
    pub is_default: bool,
    pub body: Vec<Statement>,
}

/// Lokasi sumber untuk diagnostik.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Span {
    pub file_id: u32,
    pub line: u32,
    pub col: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slice_lebar_dihitung_dari_msb_lsb() {
        assert_eq!(Slice { msb: 7, lsb: 4 }.width(), 4);
        assert_eq!(Slice { msb: 3, lsb: 0 }.width(), 4);
        assert_eq!(Slice { msb: 2, lsb: 2 }.width(), 1);
    }

    #[test]
    fn slice_mask_range_mencakup_hanya_rentang_bit() {
        assert_eq!(Slice { msb: 7, lsb: 4 }.mask_range(), (4, 4));
        assert_eq!(Slice { msb: 3, lsb: 0 }.mask_range(), (0, 4));
        assert_eq!(Slice { msb: 0, lsb: 0 }.mask_range(), (0, 1));
    }

    #[test]
    fn slice_mask_range_untuk_satu_bit() {
        assert_eq!(Slice { msb: 5, lsb: 5 }.mask_range(), (5, 1));
    }

    #[test]
    fn slice_lebar_64_masker_penuh() {
        assert_eq!(Slice { msb: 63, lsb: 0 }.mask_range(), (0, 64));
    }

    /// BUG: masker dulu dihitung sebagai `u64`, jadi irisan pada posisi di atas
    /// bit 63 tidak bisa diwakili. Build debug panic di `1u64 << 64`, release
    /// wrap diam-diam sehingga tulisan mendarat di bit yang salah — ini yang
    /// terjadi pada array unpacked 16 elemen, pola yang diklaim didukung.
    #[test]
    fn slice_di_atas_bit_63_tetap_terwakili() {
        // Elemen ke-15 dari array 8-bit: bit 120..128.
        assert_eq!(Slice { msb: 127, lsb: 120 }.mask_range(), (120, 8));
    }
}
