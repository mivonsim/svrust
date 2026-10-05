// Tanggung jawab: enum sisi edge pada event control testbench.
use sv_lexer::token::Token;

/// Sisi perubahan sinyal yang memicu event control (LRM §9.7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventEdge {
    /// `posedge` — naik dari 0 ke 1.
    Posedge,
    /// `negedge` — turun dari 1 ke 0.
    Negedge,
    /// `edge` — perubahan apa pun.
    AnyChange,
}

impl EventEdge {
    /// Petakan token event control ke enum; `None` berarti belum didukung.
    pub fn from_token(token: &Token) -> Option<Self> {
        match token {
            Token::Posedge => Some(EventEdge::Posedge),
            Token::Negedge => Some(EventEdge::Negedge),
            // LRM §9.7: `@(edge sig)` memicu pada perubahan apa pun.
            Token::Edge => Some(EventEdge::AnyChange),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn posedge_dipetakan() {
        assert_eq!(
            EventEdge::from_token(&Token::Posedge),
            Some(EventEdge::Posedge)
        );
    }

    #[test]
    fn negedge_dipetakan() {
        assert_eq!(
            EventEdge::from_token(&Token::Negedge),
            Some(EventEdge::Negedge)
        );
    }

    #[test]
    fn edge_dipetakan_sebagai_perubahan_apa_pun() {
        assert_eq!(
            EventEdge::from_token(&Token::Edge),
            Some(EventEdge::AnyChange)
        );
    }

    #[test]
    fn token_lain_tidak_dipetakan() {
        assert_eq!(
            EventEdge::from_token(&Token::Ident("clk".to_string())),
            None
        );
    }
}
