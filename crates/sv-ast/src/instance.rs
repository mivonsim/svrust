// Tanggung jawab: node AST untuk instansiasi module.
use crate::expression::Expr;
use serde::{Deserialize, Serialize};
use sv_lexer::span::Span;

/// Override parameter bernama, misal `.WIDTH(16)`.
///
/// Nilai disimpan sebagai ekspresi (bukan angka mentah) supaya referensi
/// parameter seperti `.W(.CLK_FREQ(FreqMHz))` dapat di-resolve saat elaborasi
/// dengan tabel parameter modul induk (LRM §23.10). Parser hanya mengisi
/// `Expr::Number` untuk literal; ekspresi yang merujuk parameter induk tetap
/// berupa `Expr::Ident` dan baru bernilai setelah evaluasi konstanta.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParamOverride {
    pub name: String,
    pub value: Expr,
    pub span: Span,
}

/// Indeks pada koneksi port, misal `[3]` atau `[7:4]`.
///
/// `Genvar` menyimpan nama genvar yang nilainya baru diketahui saat
/// elaborasi; bentuk ini muncul pada pola kanonik generate `.y(q[i])`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PortIndex {
    /// Bit-select, misal `q[3]`.
    Bit(u32),
    /// Part-select, misal `q[7:4]`.
    Range(u32, u32),
    /// Indeks genvar, misal `q[i]` di dalam loop generate.
    Genvar(String),
}

impl PortIndex {
    /// Lebar dalam bit.
    pub fn width(&self) -> u32 {
        match self {
            PortIndex::Bit(_) => 1,
            PortIndex::Range(msb, lsb) => msb - lsb + 1,
            // Indeks genvar selalu menunjuk satu bit.
            PortIndex::Genvar(_) => 1,
        }
    }
}

/// Sinyal yang tersambung ke port: bisa nama sinyal polos atau ekspresi
/// (misal concatenation `{a, b}`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PortSignal {
    Signal(String),
    Expr(Expr),
}

impl PortSignal {
    /// Nama sinyal bila ini `Signal(nama)`, `None` bila `Expr`.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            PortSignal::Signal(n) => Some(n),
            PortSignal::Expr(_) => None,
        }
    }
}

impl std::fmt::Display for PortSignal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PortSignal::Signal(n) => write!(f, "{n}"),
            PortSignal::Expr(e) => write!(f, "{e:?}"),
        }
    }
}

/// Koneksi port bernama, misal `.a(sig_a)` atau `.y(q[i])` atau `.a({b, c})`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortConn {
    pub port: String,
    pub signal: PortSignal,
    /// `None` berarti koneksi ke seluruh sinyal.
    pub index: Option<PortIndex>,
    pub span: Span,
}

/// Instansiasi satu submodule, misal `adder #(.WIDTH(16)) u0 (.a(x));`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Instance {
    pub module_name: String,
    pub inst_name: String,
    pub param_overrides: Vec<ParamOverride>,
    pub port_conns: Vec<PortConn>,
    pub span: Span,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bit_select_berlebar_satu() {
        assert_eq!(PortIndex::Bit(3).width(), 1);
    }

    #[test]
    fn part_select_berlebar_seluruh() {
        assert_eq!(PortIndex::Range(7, 4).width(), 4);
    }

    #[test]
    fn indeks_genvar_berlebar_satu() {
        assert_eq!(PortIndex::Genvar("i".to_string()).width(), 1);
    }
}
