// Tanggung jawab: node AST untuk instansiasi module.
use sv_lexer::span::Span;

/// Override parameter bernama, misal `.WIDTH(16)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParamOverride {
    pub name: String,
    pub value: u64,
}

/// Indeks pada koneksi port, misal `[3]` atau `[7:4]`.
///
/// `Genvar` menyimpan nama genvar yang nilainya baru diketahui saat
/// elaborasi; bentuk ini muncul pada pola kanonik generate `.y(q[i])`.
#[derive(Debug, Clone, PartialEq, Eq)]
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

/// Koneksi port bernama, misal `.a(sig_a)` atau `.y(q[i])`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortConn {
    pub port: String,
    pub signal: String,
    /// `None` berarti koneksi ke seluruh sinyal.
    pub index: Option<PortIndex>,
    pub span: Span,
}

/// Instansiasi satu submodule, misal `adder #(.WIDTH(16)) u0 (.a(x));`.
#[derive(Debug, Clone, PartialEq, Eq)]
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
