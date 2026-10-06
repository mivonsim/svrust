// Tanggung jawab: parse always_ff dan always (proses sekuensial/combinational).
use sv_ast::event_edge::EventEdge;
use sv_ast::event_item::EventItem;
use sv_ast::statement::{SequentialStatement, Statement};
use sv_lexer::spanned::SpannedToken;
use sv_lexer::token::Token;

/// Parse `always_ff @(event list) body` (LRM §9.2.1).
///
/// LRM §9.7: daftar event boleh digabung `or`, misalnya
/// `@(posedge clk or posedge rst)` untuk reset asinkron.
pub fn parse_always_ff(tokens: &[SpannedToken]) -> Result<(Statement, usize), String> {
    if tokens.is_empty() || tokens[0].token != Token::AlwaysFf {
        return Err("expected 'always_ff'".to_string());
    }
    let span = tokens[0].span;
    let (events, idx) = parse_event_list(tokens, 1)?;
    parse_seq_body(tokens, idx, events, span)
}

/// Parse keyword `always` polos di body module (LRM §9.2.1).
///
/// Bentuk yang didukung:
/// - `always @(posedge clk ...)` — daftar edge, dipetakan ke proses
///   sekuensial seperti `always_ff`;
/// - `always @(a or b)`, `always @*`, `always @(*)`, dan `always` tanpa `@`
///   — sensitivity level/implisit, dipetakan ke `always_comb` karena
///   simulator mengevaluasi ulang proses combinational tiap langkah.
pub fn parse_always(tokens: &[SpannedToken]) -> Result<(Statement, usize), String> {
    if tokens.is_empty() || tokens[0].token != Token::Always {
        return Err("expected 'always'".to_string());
    }
    let span = tokens[0].span;
    let mut idx = 1;

    // Tanpa `@`: dua bentuk yang BERBEDA (LRM §9.2.1 + §11.2).
    //
    // - Body memuat `#delay` (misal `always #5 clk = ~clk;`) adalah proses
    //   WAKTU: tidak punya sensitivitas, badannya diulang terus-menerus, dan
    //   penundaannya menentukan kapan iterasi berikutnya berjalan.
    //   Memperlakukannya sebagai `always_comb` membuat penundaan dieksekusi
    //   setiap kali proses combinational dievaluasi — jam simulasi bergerak
    //   jauh sebelum stimulus, dan clock tidak pernah menghasilkan edge yang
    //   dibaca `always_ff` yang menunggu `@(posedge clk)`.
    // - Selain itu (`always begin a = b; end`) adalah sensitivity implisit,
    //   setara `always @*`. Catatan: iverilog 12.0 menolak bentuk tanpa
    //   timing control di elaborasi (`always process does not have any
    //   delay`); SVRust menerimanya sebagai sensitivitas implisit.
    if tokens.get(idx).map(|t| &t.token) != Some(&Token::At) {
        let (body, next) = super::always_comb::parse_comb_body(tokens, idx)?;
        if super::always_comb::mengandung_delay(&body) {
            return Ok((Statement::AlwaysTimed { body, span }, next));
        }
        return Ok((Statement::AlwaysComb { body, span }, next));
    }
    idx += 1; // '@'

    // `always @*` — tanda `*` tanpa kurung.
    if tokens.get(idx).map(|t| &t.token) == Some(&Token::Star) {
        let (body, next) = super::always_comb::parse_comb_body(tokens, idx + 1)?;
        return Ok((Statement::AlwaysComb { body, span }, next));
    }

    if tokens.get(idx).map(|t| &t.token) != Some(&Token::LParen) {
        return Err("expected '(', '@*', or event list after 'always @'".to_string());
    }

    // `always @(*)` — sensitivity implisit di dalam kurung.
    if tokens.get(idx + 1).map(|t| &t.token) == Some(&Token::Star) {
        if tokens.get(idx + 2).map(|t| &t.token) != Some(&Token::RParen) {
            return Err("expected ')' after '@(*'".to_string());
        }
        let (body, next) = super::always_comb::parse_comb_body(tokens, idx + 3)?;
        return Ok((Statement::AlwaysComb { body, span }, next));
    }

    // Item pertama menentukan: edge → sekuensial, nama sinyal → level (comb).
    let item_pertama = tokens
        .get(idx + 1)
        .map(|t| t.token.clone())
        .ok_or_else(|| "expected event list after '@('".to_string())?;
    if EventEdge::from_token(&item_pertama).is_some() {
        let (events, next) = super::event_control::parse_event_items(tokens, idx + 1)?;
        return parse_seq_body(tokens, next, events, span);
    }

    // Level-sensitive `always @(a or b)`: lewati seluruh daftar lalu parse
    // body sebagai combinational.
    let mut kedalaman = 0usize;
    let mut pos = idx;
    while pos < tokens.len() {
        match tokens[pos].token {
            Token::LParen => kedalaman += 1,
            Token::RParen => {
                kedalaman -= 1;
                if kedalaman == 0 {
                    pos += 1;
                    break;
                }
            }
            _ => {}
        }
        pos += 1;
    }
    if kedalaman != 0 {
        return Err("expected ')' closing event list".to_string());
    }
    let (body, next) = super::always_comb::parse_comb_body(tokens, pos)?;
    Ok((Statement::AlwaysComb { body, span }, next))
}

/// Parse `@(event list)` setelah keyword; `idx` menunjuk token sesudah keyword.
///
/// Mengembalikan daftar item event dan indeks token tepat setelah `)` penutup.
fn parse_event_list(
    tokens: &[SpannedToken],
    mut idx: usize,
) -> Result<(Vec<EventItem>, usize), String> {
    if tokens.get(idx).map(|t| &t.token) != Some(&Token::At) {
        return Err("expected '@' in event control".to_string());
    }
    idx += 1;
    if tokens.get(idx).map(|t| &t.token) != Some(&Token::LParen) {
        return Err("expected '(' after '@'".to_string());
    }
    super::event_control::parse_event_items(tokens, idx + 1)
}

/// Parse body sekuensial setelah event list: `;`, `begin...end`, atau satu statement.
fn parse_seq_body(
    tokens: &[SpannedToken],
    mut idx: usize,
    events: Vec<EventItem>,
    span: sv_lexer::span::Span,
) -> Result<(Statement, usize), String> {
    if idx >= tokens.len() {
        return Err("expected body".to_string());
    }

    // LRM §9.3: `always_ff @(posedge clk);` dengan body kosong sah — proses
    // tetap sensitif terhadap clock tetapi tidak menulis apa pun.
    if tokens[idx].token == Token::Semi {
        idx += 1;
        return Ok((
            Statement::AlwaysFf {
                events,
                body: Vec::new(),
                span,
            },
            idx,
        ));
    }

    if tokens[idx].token == Token::Begin {
        let (body, next) = parse_seq_block(tokens, idx)?;
        return Ok((Statement::AlwaysFf { events, body, span }, next));
    }

    let (stmt, consumed) = parse_seq_statement(&tokens[idx..])?;
    idx += consumed;
    Ok((
        Statement::AlwaysFf {
            events,
            body: vec![stmt],
            span,
        },
        idx,
    ))
}

/// Parse isi blok `begin...end` sekuensial; `idx` menunjuk token `begin`.
///
/// Mengembalikan isi blok dan indeks token tepat setelah `end` penutup.
fn parse_seq_block(
    tokens: &[SpannedToken],
    mut idx: usize,
) -> Result<(Vec<SequentialStatement>, usize), String> {
    if tokens.get(idx).map(|t| &t.token) != Some(&Token::Begin) {
        return Err("expected 'begin'".to_string());
    }
    idx += 1;
    let mut body = Vec::new();
    while idx < tokens.len() && tokens[idx].token != Token::End {
        // Deklarasi variabel lokal di dalam blok (LRM §12.8).
        let token = &tokens[idx].token;
        if sv_lexer::token::tipe::bisa_mulai_deklarasi(token) {
            let (decls, consumed) = super::declaration::parse_declaration(&tokens[idx..])?;
            for decl in decls {
                body.push(SequentialStatement::Decl(decl));
            }
            idx += consumed;
            continue;
        }
        let (stmt, consumed) = parse_seq_statement(&tokens[idx..])?;
        body.push(stmt);
        idx += consumed;
    }
    if idx >= tokens.len() {
        return Err("expected 'end'".to_string());
    }
    idx += 1; // skip 'end'
    Ok((body, idx))
}

/// Cabang `if`/`else`: blok `begin...end` atau tepat satu statement sekuensial.
fn parse_seq_branch(
    tokens: &[SpannedToken],
    idx: usize,
) -> Result<(Vec<SequentialStatement>, usize), String> {
    if tokens.get(idx).map(|t| &t.token) == Some(&Token::Begin) {
        return parse_seq_block(tokens, idx);
    }
    let (stmt, consumed) = parse_seq_statement(&tokens[idx..])?;
    Ok((vec![stmt], idx + consumed))
}

/// Parse `if (syarat) cabang [else cabang]` di dalam blok sekuensial (LRM §12.4).
fn parse_seq_if(tokens: &[SpannedToken]) -> Result<(SequentialStatement, usize), String> {
    let span = tokens[0].span;
    let mut idx = 1; // skip 'if'

    if tokens.get(idx).map(|t| &t.token) != Some(&Token::LParen) {
        return Err("expected '(' after if".to_string());
    }
    idx += 1;

    let (condition, consumed) = super::expression::parse_expression(&tokens[idx..])?;
    idx += consumed;

    if tokens.get(idx).map(|t| &t.token) != Some(&Token::RParen) {
        return Err("expected ')' after if condition".to_string());
    }
    idx += 1;

    let (then_branch, next) = parse_seq_branch(tokens, idx)?;
    idx = next;

    let mut else_branch = None;
    if tokens.get(idx).map(|t| &t.token) == Some(&Token::Else) {
        let (branch, next) = parse_seq_branch(tokens, idx + 1)?;
        else_branch = Some(branch);
        idx = next;
    }

    Ok((
        SequentialStatement::IfElse {
            condition,
            then_branch,
            else_branch,
            span,
        },
        idx,
    ))
}

/// Parse tepat satu statement sekuensial: `lhs <= rhs ;`.
fn parse_seq_statement(tokens: &[SpannedToken]) -> Result<(SequentialStatement, usize), String> {
    if tokens.is_empty() {
        return Err("expected statement".to_string());
    }
    // BUG-29: `++a;` pre-increment sebagai statement utuh.
    if matches!(tokens[0].token, Token::Incr | Token::Decr) {
        let naik = tokens[0].token == Token::Incr;
        let (lhs, mut idx) = super::lvalue::parse_lvalue(&tokens[1..])?;
        idx += 1;
        if tokens.get(idx).map(|t| &t.token) != Some(&Token::Semi) {
            return Err("expected ';' after increment".to_string());
        }
        idx += 1;
        let target = lhs.name.clone();
        let span = tokens[0].span;
        return Ok((
            SequentialStatement::BlockingAssign {
                rhs: sv_ast::expression::Expr::Binary {
                    op: if naik {
                        sv_ast::expression::BinaryOp::Add
                    } else {
                        sv_ast::expression::BinaryOp::Sub
                    },
                    lhs: Box::new(sv_ast::expression::Expr::ident(target, span)),
                    rhs: Box::new(sv_ast::expression::Expr::Number(1)),
                    span,
                },
                lhs,
                span,
            },
            idx,
        ));
    }
    // LRM §20: system task sah di dalam blok sekuensial, bukan hanya `initial`.
    if matches!(tokens[0].token, Token::SystemTask(_)) {
        let (task, consumed) = super::initial::parse_system_task_mentah(tokens)?;
        return Ok((SequentialStatement::SystemTask(task), consumed));
    }
    // LRM §13.4: `return [expr];` sah di dalam function yang dipanggil dari
    // blok sekuensial; di-inline oleh elaborator.
    if tokens[0].token == Token::Return {
        let (stmt, consumed) = super::routine::parse_return(tokens)?;
        let sv_ast::combinational::CombinationalStatement::Return { value, span } = stmt else {
            return Err("internal: parse_return harus menghasilkan Return".to_string());
        };
        return Ok((SequentialStatement::Return { value, span }, consumed));
    }
    // LRM §13.3: `nama(args);` adalah pemanggilan task, dibedakan dari
    // assignment lewat tanda kurung tepat setelah identifier.
    if matches!(tokens[0].token, Token::Ident(_))
        && tokens.get(1).map(|t| &t.token) == Some(&Token::LParen)
    {
        let (call, consumed) = super::routine::parse_task_call(tokens)?;
        return Ok((SequentialStatement::TaskCall(call), consumed));
    }

    if !matches!(tokens[0].token, Token::Ident(_) | Token::If) {
        return Err(format!(
            "unexpected token in always_ff body: {:?}",
            tokens[0].token
        ));
    }

    // LRM §12.4: `if (syarat) ... else ...` di dalam blok sekuensial.
    if tokens[0].token == Token::If {
        return parse_seq_if(tokens);
    }

    // LHS boleh sliced, misal `q[3:0] <= d[3:0];`.
    let (lhs, mut idx) = super::lvalue::parse_lvalue(tokens)?;

    // BUG-27: `a++;` sebagai statement sama dengan `a = a + 1`.
    if matches!(
        tokens.get(idx).map(|t| &t.token),
        Some(&Token::Incr) | Some(&Token::Decr)
    ) {
        let naik = tokens[idx].token == Token::Incr;
        let span = tokens[0].span;
        idx += 1;
        if tokens.get(idx).map(|t| &t.token) != Some(&Token::Semi) {
            return Err("expected ';' after increment".to_string());
        }
        idx += 1;
        let target = lhs.name.clone();
        return Ok((
            SequentialStatement::BlockingAssign {
                rhs: sv_ast::expression::Expr::Binary {
                    op: if naik {
                        sv_ast::expression::BinaryOp::Add
                    } else {
                        sv_ast::expression::BinaryOp::Sub
                    },
                    lhs: Box::new(sv_ast::expression::Expr::ident(target, span)),
                    rhs: Box::new(sv_ast::expression::Expr::Number(1)),
                    span,
                },
                lhs,
                span,
            },
            idx,
        ));
    }

    // LRM §10.3: blocking `=` sah di dalam blok sekuensial, hanya berbeda
    // waktu berlalunya dari nonblocking `<=`.
    let blocking = match tokens.get(idx).map(|t| &t.token) {
        Some(&Token::NonBlocking) => false,
        Some(&Token::Eq) => true,
        _ => return Err("expected '<=' or '=' in always_ff body".to_string()),
    };
    idx += 1;

    let (rhs, consumed) = super::expression::parse_expression(&tokens[idx..])?;
    idx += consumed;

    if idx >= tokens.len() || tokens[idx].token != Token::Semi {
        return Err("expected ';'".to_string());
    }
    idx += 1;

    let span = tokens[0].span;
    let statement = if blocking {
        SequentialStatement::BlockingAssign { lhs, rhs, span }
    } else {
        SequentialStatement::NonBlockingAssign { lhs, rhs, span }
    };
    Ok((statement, idx))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sv_lexer::lex;

    fn parse(src: &str) -> (Statement, usize) {
        let tokens = lex(src).expect("lex");
        parse_always_ff(&tokens).expect("parse")
    }

    fn parse_always(src: &str) -> (Statement, usize) {
        let tokens = lex(src).expect("lex");
        super::parse_always(&tokens).expect("parse")
    }

    #[test]
    fn always_ff_body_kosong_diterima() {
        // LRM §9.3: `always_ff @(posedge clk);` sah walau tidak menulis apa pun.
        let (stmt, _) = parse("always_ff @(posedge clk);");
        match stmt {
            Statement::AlwaysFf { events, body, .. } => {
                assert_eq!(events.len(), 1);
                assert_eq!(events[0].signal, "clk");
                assert_eq!(events[0].edge, EventEdge::Posedge);
                assert!(body.is_empty());
            }
            other => panic!("bukan always_ff: {:?}", other),
        }
    }

    #[test]
    fn always_ff_body_kosong_konsumsi_semicolon() {
        let tokens = lex("always_ff @(posedge clk);").expect("lex");
        let (_, consumed) = parse_always_ff(&tokens).expect("parse");
        assert_eq!(consumed, tokens.len());
    }

    #[test]
    fn always_ff_dengan_body_tetap_berfungsi() {
        // Regresi: bentuk yang punya body tidak boleh ikut berubah.
        let (stmt, _) = parse("always_ff @(posedge clk) q <= d;");
        match stmt {
            Statement::AlwaysFf { body, .. } => assert_eq!(body.len(), 1),
            other => panic!("bukan always_ff: {:?}", other),
        }
    }

    #[test]
    fn always_ff_body_kosong_tanpa_semicolon_menolak() {
        let tokens = lex("always_ff @(posedge clk)").expect("lex");
        assert!(parse_always_ff(&tokens).is_err());
    }

    // --- BUG-46: event control gabungan pada always/always_ff ---

    #[test]
    fn always_ff_event_gabungan_async_reset_diterima() {
        // LRM §9.7: `@(posedge clk or posedge rst)` untuk reset asinkron.
        let (stmt, _) = parse("always_ff @(posedge clk or posedge rst) q <= 8'd0;");
        match stmt {
            Statement::AlwaysFf { events, body, .. } => {
                assert_eq!(events.len(), 2);
                assert_eq!(events[0].signal, "clk");
                assert_eq!(events[0].edge, EventEdge::Posedge);
                assert_eq!(events[1].signal, "rst");
                assert_eq!(events[1].edge, EventEdge::Posedge);
                assert_eq!(body.len(), 1);
            }
            other => panic!("bukan always_ff: {:?}", other),
        }
    }

    #[test]
    fn always_ff_event_gabungan_negedge_disimpan() {
        let (stmt, _) = parse("always_ff @(posedge clk or negedge rst);");
        match stmt {
            Statement::AlwaysFf { events, .. } => {
                assert_eq!(events[1].edge, EventEdge::Negedge);
            }
            other => panic!("bukan always_ff: {:?}", other),
        }
    }

    #[test]
    fn always_ff_level_sensitive_ditolak() {
        // `always_ff` hanya sah dengan edge; nama sinyal polos ditolak jelas.
        let tokens = lex("always_ff @(clk) q <= d;").expect("lex");
        let err = parse_always_ff(&tokens).expect_err("harus gagal");
        assert!(err.contains("posedge/negedge/edge"), "pesan: {}", err);
    }

    // --- BUG-47: keyword `always` polos di body module ---

    #[test]
    fn always_dengan_edge_jadi_sekuensial() {
        let (stmt, _) = parse_always("always @(posedge clk) q <= q + 1;");
        match stmt {
            Statement::AlwaysFf { events, body, .. } => {
                assert_eq!(events.len(), 1);
                assert_eq!(events[0].signal, "clk");
                assert_eq!(body.len(), 1);
            }
            other => panic!("bukan always sekuensial: {:?}", other),
        }
    }

    #[test]
    fn always_event_gabungan_jadi_sekuensial() {
        let (stmt, _) = parse_always("always @(posedge clk or negedge rst) q <= d;");
        match stmt {
            Statement::AlwaysFf { events, .. } => assert_eq!(events.len(), 2),
            other => panic!("bukan always sekuensial: {:?}", other),
        }
    }

    #[test]
    fn always_level_sensitive_jadi_combinational() {
        // LRM §9.4.1: `always @(a or b)` level-sensitive setara always_comb
        // pada simulator yang mengevaluasi ulang tiap langkah.
        let (stmt, _) = parse_always("always @(a or b) y = a + b;");
        match stmt {
            Statement::AlwaysComb { body, .. } => assert_eq!(body.len(), 1),
            other => panic!("bukan always_comb: {:?}", other),
        }
    }

    #[test]
    fn always_star_jadi_combinational() {
        let (stmt, consumed) = parse_always("always @* begin y = a; end");
        match stmt {
            Statement::AlwaysComb { body, .. } => assert_eq!(body.len(), 1),
            other => panic!("bukan always_comb: {:?}", other),
        }
        let tokens = lex("always @* begin y = a; end").expect("lex");
        assert_eq!(consumed, tokens.len());
    }

    #[test]
    fn always_paren_star_jadi_combinational() {
        let (stmt, _) = parse_always("always @(*) y = a;");
        assert!(matches!(stmt, Statement::AlwaysComb { .. }));
    }

    #[test]
    fn always_tanpa_at_jadi_combinational() {
        let (stmt, _) = parse_always("always y = a;");
        assert!(matches!(stmt, Statement::AlwaysComb { .. }));
    }

    #[test]
    fn always_dengan_body_blok_sekuensial() {
        let (stmt, _) = parse_always("always @(posedge clk) begin q <= d; end");
        match stmt {
            Statement::AlwaysFf { body, .. } => assert_eq!(body.len(), 1),
            other => panic!("bukan always sekuensial: {:?}", other),
        }
    }

    // --- BUG-48: if/else di dalam blok sekuensial ---

    #[test]
    fn always_ff_if_else_diterima() {
        // LRM §12.4: bentuk klasik async reset.
        let (stmt, _) = parse(
            "always_ff @(posedge clk or posedge rst) if (rst) q <= 8'd0; else q <= q + 8'd1;",
        );
        match stmt {
            Statement::AlwaysFf { body, .. } => {
                assert_eq!(body.len(), 1);
                match &body[0] {
                    SequentialStatement::IfElse {
                        then_branch,
                        else_branch,
                        ..
                    } => {
                        assert_eq!(then_branch.len(), 1);
                        assert_eq!(else_branch.as_ref().map(|b| b.len()), Some(1));
                    }
                    other => panic!("bukan if/else: {:?}", other),
                }
            }
            other => panic!("bukan always_ff: {:?}", other),
        }
    }

    #[test]
    fn always_ff_if_dengan_blok_begin_end() {
        let (stmt, _) =
            parse("always_ff @(posedge clk) if (rst) begin q <= 8'd0; r <= 8'd1; end else q <= d;");
        match stmt {
            Statement::AlwaysFf { body, .. } => match &body[0] {
                SequentialStatement::IfElse {
                    then_branch,
                    else_branch,
                    ..
                } => {
                    assert_eq!(then_branch.len(), 2);
                    assert_eq!(else_branch.as_ref().map(|b| b.len()), Some(1));
                }
                other => panic!("bukan if/else: {:?}", other),
            },
            other => panic!("bukan always_ff: {:?}", other),
        }
    }

    #[test]
    fn always_ff_if_tanpa_else_diterima() {
        let (stmt, _) = parse("always_ff @(posedge clk) if (rst) q <= 8'd0;");
        match stmt {
            Statement::AlwaysFf { body, .. } => match &body[0] {
                SequentialStatement::IfElse { else_branch, .. } => {
                    assert!(else_branch.is_none());
                }
                other => panic!("bukan if/else: {:?}", other),
            },
            other => panic!("bukan always_ff: {:?}", other),
        }
    }

    #[test]
    fn always_ff_if_kondisi_tanpa_kurung_menolak() {
        let tokens = lex("always_ff @(posedge clk) if rst q <= 8'd0;").expect("lex");
        let err = parse_always_ff(&tokens).expect_err("harus gagal");
        assert!(err.contains("'('"), "pesan: {}", err);
    }
}
