// Tanggung jawab: parse event control `@(posedge clk)` sebagai statement.
use sv_ast::combinational::CombinationalStatement;
use sv_ast::event_edge::EventEdge;
use sv_ast::event_item::EventItem;
use sv_lexer::spanned::SpannedToken;
use sv_lexer::token::Token;

/// Parse `@(posedge clk)`, `@(negedge rst)`, atau `@(edge sig)`.
///
/// LRM §9.7: beberapa item boleh digabung dengan `or`, misalnya
/// `@(posedge clk or negedge rst)`. Proses ditangguhkan sampai salah satu item
/// benar-benar terjadi.
///
/// Setelah tanda kurung boleh ada statement (bisa `begin...end` atau satu
/// statement biasa). `@(posedge clk);` tanpa statement berarti hanya menunggu,
/// yang umum di dalam blok `initial` testbench.
pub fn parse_event_control(
    tokens: &[SpannedToken],
) -> Result<(CombinationalStatement, usize), String> {
    let span = tokens[0].span;
    let mut idx = 1;
    if tokens.get(idx).map(|t| &t.token) != Some(&Token::LParen) {
        return Err("expected '(' after '@'".to_string());
    }
    idx += 1;

    let (events, next) = parse_event_items(tokens, idx)?;
    idx = next;

    // `@(posedge clk);` — hanya menunggu, tanpa statement tambahan.
    if tokens.get(idx).map(|t| &t.token) == Some(&Token::Semi) {
        idx += 1;
        return Ok((
            CombinationalStatement::EventControl {
                events,
                body: Vec::new(),
                span,
            },
            idx,
        ));
    }

    let (body, consumed) = if tokens.get(idx).map(|t| &t.token) == Some(&Token::Begin) {
        let (stmts, dipakai) = super::combinational::parse_combinational_block(&tokens[idx + 1..])?;
        let mut pos = idx + 1 + dipakai;
        if tokens.get(pos).map(|t| &t.token) != Some(&Token::End) {
            return Err("expected 'end' closing event control body".to_string());
        }
        pos += 1;
        (stmts, pos)
    } else {
        let (stmt, dipakai) = super::combinational::parse_combinational_statement(&tokens[idx..])?;
        (vec![stmt], idx + dipakai)
    };

    Ok((
        CombinationalStatement::EventControl { events, body, span },
        consumed,
    ))
}

/// Parse daftar item event di dalam kurung `@(...)`, diawali tepat setelah `(`.
///
/// LRM §9.7: item dipisah kata `or`, misalnya `posedge clk or posedge rst`.
/// Mengembalikan daftar item dan indeks token tepat setelah `)` penutup.
/// Dipakai oleh `parse_event_control` dan parser `always`/`always_ff` supaya
/// aturan event list hanya ditulis sekali.
pub(super) fn parse_event_items(
    tokens: &[SpannedToken],
    mut idx: usize,
) -> Result<(Vec<EventItem>, usize), String> {
    let mut events: Vec<EventItem> = Vec::new();
    loop {
        let edge_token = tokens
            .get(idx)
            .map(|t| t.token.clone())
            .ok_or_else(|| "expected edge after '@('".to_string())?;
        let edge = EventEdge::from_token(&edge_token).ok_or_else(|| {
            format!(
                "event control hanya mendukung posedge/negedge/edge, dapat {:?}",
                edge_token
            )
        })?;
        idx += 1;

        let signal_token = tokens
            .get(idx)
            .ok_or_else(|| "expected signal name in event control".to_string())?;
        let signal = match &signal_token.token {
            Token::Ident(nama) => nama.clone(),
            _ => return Err("expected signal name in event control".to_string()),
        };
        // Span diambil dari token sinyal, bukan dari `posedge`, supaya pesan
        // `undefined signal` menunjuk nama yang salah.
        let signal_span = signal_token.span;
        idx += 1;
        events.push(EventItem::dengan_span(edge, &signal, signal_span));

        // LRM §9.7: item berikutnya dipisahkan kata `or`.
        match tokens.get(idx).map(|t| &t.token) {
            Some(Token::Or) => idx += 1,
            Some(Token::RParen) => {
                idx += 1;
                break;
            }
            _ => return Err("expected 'or' or ')' in event control".to_string()),
        }
    }
    Ok((events, idx))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sv_lexer::lex;

    fn parse(src: &str) -> (CombinationalStatement, usize) {
        let tokens = lex(src).expect("lex");
        parse_event_control(&tokens).expect("parse")
    }

    fn items(src: &str) -> Vec<EventItem> {
        match parse(src).0 {
            CombinationalStatement::EventControl { events, .. } => events,
            lain => panic!("dapat {:?}", lain),
        }
    }

    #[test]
    fn posedge_tanpa_statement_hanya_menunggu() {
        let (stmt, consumed) = parse("@(posedge clk);");
        match stmt {
            CombinationalStatement::EventControl { events, body, .. } => {
                assert_eq!(events.len(), 1);
                assert_eq!(events[0].edge, EventEdge::Posedge);
                assert_eq!(events[0].signal, "clk");
                assert!(body.is_empty());
            }
            lain => panic!("dapat {:?}", lain),
        }
        // `@ ( posedge clk ) ;` ada 6 token dan keenamnya dikonsumsi.
        assert_eq!(consumed, 6);
    }

    #[test]
    fn negedge_dipetakan() {
        let items = items("@(negedge rst);");
        assert_eq!(items[0].edge, EventEdge::Negedge);
        assert_eq!(items[0].signal, "rst");
    }

    #[test]
    fn edge_dipetakan_sebagai_perubahan_apa_pun() {
        let items = items("@(edge sig);");
        assert_eq!(items[0].edge, EventEdge::AnyChange);
    }

    #[test]
    fn event_control_dengan_statement_tunggal() {
        match parse("@(posedge clk) x = 8'd1;").0 {
            CombinationalStatement::EventControl { body, .. } => assert_eq!(body.len(), 1),
            lain => panic!("dapat {:?}", lain),
        }
    }

    #[test]
    fn event_control_dengan_blok_begin_end() {
        match parse("@(posedge clk) begin x = 8'd1; y = 8'd2; end").0 {
            CombinationalStatement::EventControl { body, .. } => assert_eq!(body.len(), 2),
            lain => panic!("dapat {:?}", lain),
        }
    }

    #[test]
    fn dua_item_dipisah_or_keduanya_tersimpan() {
        let items = items("@(posedge clk or negedge rst);");
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].signal, "clk");
        assert_eq!(items[1].edge, EventEdge::Negedge);
        assert_eq!(items[1].signal, "rst");
    }

    #[test]
    fn tiga_item_dipisah_or_keduanya_tersimpan() {
        let items = items("@(posedge clk or negedge rst or edge sig);");
        assert_eq!(items.len(), 3);
        assert_eq!(items[2].edge, EventEdge::AnyChange);
    }

    #[test]
    fn pemisah_selain_or_ditolak() {
        let tokens = lex("@(posedge clk and negedge rst);").expect("lex");
        let err = parse_event_control(&tokens).expect_err("harus gagal");
        assert!(err.contains("'or'"), "pesan: {}", err);
    }

    #[test]
    fn tanpa_kurung_buka_ditolak() {
        let tokens = lex("@posedge clk);").expect("lex");
        let err = parse_event_control(&tokens).expect_err("harus gagal");
        assert!(err.contains("'('"), "pesan: {}", err);
    }

    #[test]
    fn edge_tidak_dikenal_memberi_pesan_jelas() {
        let tokens = lex("@(both clk);").expect("lex");
        let err = parse_event_control(&tokens).expect_err("harus gagal");
        assert!(err.contains("posedge/negedge/edge"), "pesan: {}", err);
    }

    #[test]
    fn sinyal_bukan_nama_memberi_pesan_jelas() {
        let tokens = lex("@(posedge 8'd1);").expect("lex");
        let err = parse_event_control(&tokens).expect_err("harus gagal");
        assert!(err.contains("signal name"), "pesan: {}", err);
    }
}
