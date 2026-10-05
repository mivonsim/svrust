// Tanggung jawab: satu item event control (sisi edge + sinyal) pada `@(...)`.
use super::event_edge::EventEdge;
use sv_lexer::span::Span;

/// Satu pasangan sisi edge dan sinyal yang diawasi di dalam `@(...)`.
///
/// LRM §9.7 memungkinkan beberapa item dipisahkan `or`, misalnya
/// `@(posedge clk or negedge rst)`. Proses ditangguhkan sampai salah satu
/// item benar-benar terjadi.
///
/// `span` menunjuk token nama sinyal, bukan token `posedge`, supaya pesan
/// `undefined signal` menunjuk nama yang salah, bukan `@`-nya (AGENTS.md aturan 3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventItem {
    pub edge: EventEdge,
    pub signal: String,
    pub span: Span,
}

impl EventItem {
    /// Bentuk tunggal dengan span token sinyal aslinya. Tidak ada bentuk
    /// tanpa span: span yang salah membuat pesan `undefined signal` menunjuk
    /// posisi yang bukan nama sinyalnya.
    pub fn dengan_span(edge: EventEdge, signal: &str, span: Span) -> Self {
        Self {
            edge,
            signal: signal.to_string(),
            span,
        }
    }

    /// Bentuk baru dengan nama sinyal lain, span tetap mengikuti token asal.
    pub fn dengan_nama(&self, nama: impl Into<String>) -> Self {
        let mut hasil = self.clone();
        hasil.signal = nama.into();
        hasil
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn item_menyimpan_edge_sinyal_dan_span() {
        let span = Span::new(4, 7, 2, 5);
        let item = EventItem::dengan_span(EventEdge::Posedge, "clk", span);
        assert_eq!(item.edge, EventEdge::Posedge);
        assert_eq!(item.signal, "clk");
        assert_eq!(item.span, span);
    }

    #[test]
    fn dengan_nama_mempertahankan_span() {
        // Substitusi scope di elaborator mengganti nama tanpa boleh memindahkan
        // posisi sumbernya.
        let span = Span::new(13, 16, 2, 14);
        let item = EventItem::dengan_span(EventEdge::Posedge, "clk", span);
        let hasil = item.dengan_nama("u0_clk");
        assert_eq!(hasil.signal, "u0_clk");
        assert_eq!(hasil.span, span);
        assert_eq!(hasil.edge, EventEdge::Posedge);
    }

    #[test]
    fn dua_item_tidak_sama_memisahkan_nama_sinyal() {
        let items = [
            EventItem::dengan_span(EventEdge::Posedge, "clk", Span::dummy()),
            EventItem::dengan_span(EventEdge::Negedge, "rst", Span::dummy()),
        ];
        assert_eq!(items.len(), 2);
        assert_ne!(items[0].signal, items[1].signal);
    }
}
