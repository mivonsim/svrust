// Tanggung jawab: parse statement di dalam always_comb.
use sv_ast::combinational::{CaseArm, CaseKind, CombinationalStatement, CompoundOp, ForStep};
use sv_ast::expression::{BinaryOp, Expr};
use sv_ast::lvalue::Lvalue;
use sv_lexer::span::Span;
use sv_lexer::spanned::SpannedToken;
use sv_lexer::token::Token;

/// Parse tepat satu statement combinational.
pub fn parse_combinational_statement(
    tokens: &[SpannedToken],
) -> Result<(CombinationalStatement, usize), String> {
    if tokens.is_empty() {
        return Err("expected combinational statement".to_string());
    }

    match &tokens[0].token {
        Token::Case => parse_case(tokens, CaseKind::Exact),
        Token::Casez => parse_case(tokens, CaseKind::Casez),
        Token::Casex => parse_case(tokens, CaseKind::Casex),
        Token::If => parse_if(tokens),
        Token::For => parse_for(tokens),
        Token::While => parse_while(tokens),
        Token::Repeat => parse_repeat(tokens),
        Token::SystemTask(_) => super::initial::parse_system_task(tokens),
        // LRM §11.2: `#n <stmt>` menunda statement berikut.
        Token::Hash => parse_delay(tokens),
        // LRM §9.7: `@(posedge clk)` menunggu edge pada statement testbench.
        Token::At => super::event_control::parse_event_control(tokens),
        // LRM §13.4: `return [expr];` keluar dari function/task.
        Token::Return => super::routine::parse_return(tokens),
        Token::Begin => {
            let (body, mut idx) = parse_combinational_block(&tokens[1..])?;
            if idx >= tokens[1..].len() || tokens[idx + 1].token != Token::End {
                return Err("expected 'end'".to_string());
            }
            idx += 1;
            Ok((
                CombinationalStatement::Block {
                    body,
                    span: tokens[0].span,
                },
                idx + 1,
            ))
        }
        Token::Incr | Token::Decr => {
            // BUG-29: `++a;` pre-increment sebagai statement utuh.
            let naik = tokens[0].token == Token::Incr;
            let (lhs, mut idx) = super::lvalue::parse_lvalue(&tokens[1..])?;
            idx += 1; // lvalue dibaca mulai token kedua
            if tokens.get(idx).map(|t| &t.token) != Some(&Token::Semi) {
                return Err("expected ';' after increment".to_string());
            }
            idx += 1;
            Ok((
                CombinationalStatement::CompoundAssign {
                    lhs,
                    op: if naik {
                        CompoundOp::Add
                    } else {
                        CompoundOp::Sub
                    },
                    rhs: Expr::Number(1),
                    span: tokens[0].span,
                },
                idx,
            ))
        }
        Token::Ident(_) => {
            let span = tokens[0].span;
            // LRM §13.3: `nama(args);` adalah pemanggilan task, dibedakan dari
            // assignment lewat tanda kurung tepat setelah identifier.
            if tokens.get(1).map(|t| &t.token) == Some(&Token::LParen) {
                let (call, consumed) = super::routine::parse_task_call(tokens)?;
                return Ok((CombinationalStatement::TaskCall(call), consumed));
            }
            // LRM §13.3: task tanpa argumen boleh dipanggil tanpa kurung,
            // yaitu `nama;`. Bentuk ini sah karena nama task sudah pasti
            // subroutine, bukan lvalue — penentuannya ada di elaborator.
            if tokens.get(1).map(|t| &t.token) == Some(&Token::Semi) {
                let (call, consumed) = super::routine::parse_task_call_tanpa_kurung(tokens)?;
                return Ok((CombinationalStatement::TaskCall(call), consumed));
            }
            // LHS boleh sliced, misal `y[3:0] = a;`.
            let (lhs, mut idx) = super::lvalue::parse_lvalue(tokens)?;

            // BUG-27: `a++;` sebagai statement sama dengan `a = a + 1`.
            // Karena nilai hasilnya dibuang, bentuk pre/post tidak berpengaruh.
            if matches!(
                tokens.get(idx).map(|t| &t.token),
                Some(&Token::Incr) | Some(&Token::Decr)
            ) {
                let naik = tokens[idx].token == Token::Incr;
                idx += 1;
                if tokens.get(idx).map(|t| &t.token) != Some(&Token::Semi) {
                    return Err("expected ';' after increment".to_string());
                }
                idx += 1;
                return Ok((
                    CombinationalStatement::CompoundAssign {
                        lhs,
                        op: if naik {
                            CompoundOp::Add
                        } else {
                            CompoundOp::Sub
                        },
                        rhs: Expr::Number(1),
                        span,
                    },
                    idx,
                ));
            }

            // Compound assignment: `y += a;` (LRM §11.3).
            if let Some(Token::CompoundAssign(op)) = tokens.get(idx).map(|t| &t.token) {
                let op = *op;
                idx += 1;
                let (rhs, consumed) = super::expression::parse_expression(&tokens[idx..])?;
                idx += consumed;
                if idx >= tokens.len() || tokens[idx].token != Token::Semi {
                    return Err("expected ';'".to_string());
                }
                idx += 1;
                return Ok((
                    CombinationalStatement::CompoundAssign {
                        lhs,
                        op: map_compound(op),
                        rhs,
                        span,
                    },
                    idx,
                ));
            }

            // LRM §10.4: `<=` adalah non-blocking assignment. Sah di mana pun
            // assignment biasa sah, termasuk di dalam `initial` testbench
            // (LRM §15.2 tidak membatasi `<=` pada blok prosedural khusus).
            let non_blocking = idx < tokens.len() && tokens[idx].token == Token::NonBlocking;
            if non_blocking {
                idx += 1;
            } else if idx >= tokens.len() || tokens[idx].token != Token::Eq {
                return Err("expected '=' atau '<='".to_string());
            } else {
                idx += 1;
            }

            let (rhs, consumed) = super::expression::parse_expression(&tokens[idx..])?;
            idx += consumed;

            // BUG-30: `y = a++;` — efek samping di ekspresi belum didukung.
            if matches!(
                tokens.get(idx).map(|t| &t.token),
                Some(&Token::Incr) | Some(&Token::Decr)
            ) {
                return Err("increment/decrement setelah ekspresi belum didukung; \
                     pakai statement terpisah `a++;`"
                    .to_string());
            }

            if idx >= tokens.len() || tokens[idx].token != Token::Semi {
                return Err("expected ';'".to_string());
            }
            idx += 1;

            Ok((
                if non_blocking {
                    CombinationalStatement::NonBlockingAssign { lhs, rhs, span }
                } else {
                    CombinationalStatement::BlockingAssign { lhs, rhs, span }
                },
                idx,
            ))
        }
        _ => Err(format!(
            "unexpected token in combinational statement: {:?}",
            tokens[0].token
        )),
    }
}

/// Petakan token compound operator ke AST.
fn map_compound(op: sv_lexer::token::CompoundOp) -> CompoundOp {
    use sv_lexer::token::CompoundOp as L;
    match op {
        L::Add => CompoundOp::Add,
        L::Sub => CompoundOp::Sub,
        L::Mul => CompoundOp::Mul,
        L::Div => CompoundOp::Div,
        L::Mod => CompoundOp::Mod,
        L::BitAnd => CompoundOp::BitAnd,
        L::BitOr => CompoundOp::BitOr,
        L::BitXor => CompoundOp::BitXor,
        L::Shl => CompoundOp::Shl,
        L::Shr => CompoundOp::Shr,
        L::Sar => CompoundOp::Sar,
    }
}

/// Parse `#n <stmt>`; satuan waktu opsional (`ns`, `us`, `ms`, `ps`, `fs`).
///
/// Satuan diabaikan karena runtime memakai satuan femtosecond sebagai basis
/// tunggal; hanya besaran nominal yang dipakai.
fn parse_delay(tokens: &[SpannedToken]) -> Result<(CombinationalStatement, usize), String> {
    let span = tokens[0].span;
    let mut idx = 1; // skip '#'

    let (amount, consumed) = super::expression::parse_expression(&tokens[idx..])?;
    idx += consumed;

    // Satuan waktu adalah identifier yang menempel langsung pada angka.
    // Identifier lain bukan satuan, jadi dibiarkan sebagai statement biasa.
    let mut unit = sv_ast::time_unit::TimeUnit::default_unit();
    if let Some(Token::Ident(satuan)) = tokens.get(idx).map(|t| &t.token) {
        if let Some(dikenal) = sv_ast::time_unit::TimeUnit::from_name(satuan) {
            unit = dikenal;
            idx += 1;
        }
    }

    // `#n;` tanpa statement bodybuilding diizinkan; delay tetap berlaku.
    let body = if tokens.get(idx).map(|t| &t.token) == Some(&Token::Semi) {
        idx += 1;
        CombinationalStatement::Block {
            body: Vec::new(),
            span,
        }
    } else {
        let (statement, consumed) = parse_combinational_statement(&tokens[idx..])?;
        idx += consumed;
        statement
    };

    Ok((
        CombinationalStatement::Delay {
            amount,
            unit,
            body: Box::new(body),
            span,
        },
        idx,
    ))
}

/// Parse `repeat (count) body`.
fn parse_repeat(tokens: &[SpannedToken]) -> Result<(CombinationalStatement, usize), String> {
    let span = tokens[0].span;
    let mut idx = 1; // skip 'repeat'

    if tokens.get(idx).map(|t| &t.token) != Some(&Token::LParen) {
        return Err("expected '(' after repeat".to_string());
    }
    idx += 1;

    let (count, consumed) = super::expression::parse_expression(&tokens[idx..])?;
    idx += consumed;
    if tokens.get(idx).map(|t| &t.token) != Some(&Token::RParen) {
        return Err("expected ')' closing repeat count".to_string());
    }
    idx += 1;

    if idx >= tokens.len() {
        return Err("expected repeat body".to_string());
    }
    let body = if tokens[idx].token == Token::Semi {
        idx += 1;
        Vec::new()
    } else {
        let (statement, consumed) = parse_combinational_statement(&tokens[idx..])?;
        idx += consumed;
        vec![statement]
    };

    Ok((CombinationalStatement::Repeat { count, body, span }, idx))
}

/// Parse `while (cond) body`.
fn parse_while(tokens: &[SpannedToken]) -> Result<(CombinationalStatement, usize), String> {
    let span = tokens[0].span;
    let mut idx = 1; // skip 'while'

    if tokens.get(idx).map(|t| &t.token) != Some(&Token::LParen) {
        return Err("expected '(' after while".to_string());
    }
    idx += 1;

    let (condition, consumed) = super::expression::parse_expression(&tokens[idx..])?;
    idx += consumed;
    if tokens.get(idx).map(|t| &t.token) != Some(&Token::RParen) {
        return Err("expected ')' closing while condition".to_string());
    }
    idx += 1;

    // Body boleh kosong dengan `;`, selain itu tepat satu statement.
    if idx >= tokens.len() {
        return Err("expected while body".to_string());
    }
    let body = if tokens[idx].token == Token::Semi {
        idx += 1;
        Vec::new()
    } else {
        let (statement, consumed) = parse_combinational_statement(&tokens[idx..])?;
        idx += consumed;
        vec![statement]
    };

    Ok((
        CombinationalStatement::While {
            condition,
            body,
            span,
        },
        idx,
    ))
}

/// Parse `for (init; cond; step) body`.
fn parse_for(tokens: &[SpannedToken]) -> Result<(CombinationalStatement, usize), String> {
    let span = tokens[0].span;
    let mut idx = 1; // skip 'for'

    if tokens.get(idx).map(|t| &t.token) != Some(&Token::LParen) {
        return Err("expected '(' after for".to_string());
    }
    idx += 1;

    let init = parse_for_step(tokens, idx)?;
    idx += init.1;
    if tokens.get(idx).map(|t| &t.token) != Some(&Token::Semi) {
        return Err("expected ';' after for init".to_string());
    }
    idx += 1;

    let (condition, consumed) = super::expression::parse_expression(&tokens[idx..])?;
    idx += consumed;
    if tokens.get(idx).map(|t| &t.token) != Some(&Token::Semi) {
        return Err("expected ';' after for condition".to_string());
    }
    idx += 1;

    let step = parse_for_step(tokens, idx)?;
    idx += step.1;
    if tokens.get(idx).map(|t| &t.token) != Some(&Token::RParen) {
        return Err("expected ')' closing for header".to_string());
    }
    idx += 1;

    // Body: `;` berarti loop kosong, selain itu tepat satu statement.
    if idx >= tokens.len() {
        return Err("expected for body".to_string());
    }
    let body = if tokens[idx].token == Token::Semi {
        idx += 1;
        Vec::new()
    } else {
        let (statement, consumed) = parse_combinational_statement(&tokens[idx..])?;
        idx += consumed;
        vec![statement]
    };

    Ok((
        CombinationalStatement::For {
            init: init.0,
            condition,
            step: step.0,
            body,
            span,
        },
        idx,
    ))
}

/// Parse satu langkah `for` tanpa titik koma.
///
/// Selain `nama = ekspresi`, langkah boleh berupa compound (`i += 2`) atau
/// increment (`i++`). Keduanya diurai menjadi bentuk `nama = nama <op> x`
/// karena `ForStep` hanya menyimpan satu assignment biasa.
fn parse_for_step(tokens: &[SpannedToken], start: usize) -> Result<(ForStep, usize), String> {
    let span = tokens[start].span;
    let (lhs, mut idx) = super::lvalue::parse_lvalue(&tokens[start..])?;

    let next = tokens.get(start + idx).map(|t| &t.token);
    match next {
        Some(&Token::Eq) => {
            idx += 1;
            let (rhs, consumed) = super::expression::parse_expression(&tokens[start + idx..])?;
            idx += consumed;
            Ok((ForStep { lhs, rhs, span }, idx))
        }
        // `i++` dan `i--` sama dengan `i = i + 1` dan `i = i - 1`.
        Some(&Token::Incr) | Some(&Token::Decr) => {
            let op = if next == Some(&Token::Incr) {
                BinaryOp::Add
            } else {
                BinaryOp::Sub
            };
            idx += 1;
            Ok((
                ForStep {
                    rhs: increment(&lhs, op, span),
                    lhs,
                    span,
                },
                idx,
            ))
        }
        // `i += 2` sama dengan `i = i + 2`.
        Some(&Token::CompoundAssign(_)) => {
            let compound = match next {
                Some(&Token::CompoundAssign(op)) => op,
                _ => unreachable!("pola sudah dipastikan CompoundAssign"),
            };
            // Token lexer lebih dulu dipetakan ke enum AST, lalu ke operator biner.
            let op = binary_op_dari_compound(map_compound(compound));
            idx += 1;
            let (rhs, consumed) = super::expression::parse_expression(&tokens[start + idx..])?;
            idx += consumed;
            let nilai = Expr::Binary {
                op,
                lhs: Box::new(Expr::ident(lhs.name.clone(), lhs.span)),
                rhs: Box::new(rhs),
                span,
            };
            Ok((
                ForStep {
                    lhs,
                    rhs: nilai,
                    span,
                },
                idx,
            ))
        }
        _ => Err("expected '=', compound, or '++' in for step".to_string()),
    }
}

/// Bangun `nama <op> 1` untuk langkah `i++` / `i--`.
fn increment(lhs: &Lvalue, op: BinaryOp, span: Span) -> Expr {
    Expr::Binary {
        op,
        lhs: Box::new(Expr::ident(lhs.name.clone(), lhs.span)),
        rhs: Box::new(Expr::Number(1)),
        span,
    }
}

/// Petakan operator compound assignment ke operator biner ekspresi.
fn binary_op_dari_compound(op: CompoundOp) -> BinaryOp {
    use CompoundOp as C;
    match op {
        C::Add => BinaryOp::Add,
        C::Sub => BinaryOp::Sub,
        C::Mul => BinaryOp::Mul,
        C::Div => BinaryOp::Div,
        C::Mod => BinaryOp::Mod,
        C::BitAnd => BinaryOp::And,
        C::BitOr => BinaryOp::Or,
        C::BitXor => BinaryOp::Xor,
        C::Shl => BinaryOp::Shl,
        C::Shr => BinaryOp::Shr,
        C::Sar => BinaryOp::Sar,
    }
}

/// Parse rangkaian statement hingga End / Endcase / Else (batas blok).
pub fn parse_combinational_block(
    tokens: &[SpannedToken],
) -> Result<(Vec<CombinationalStatement>, usize), String> {
    let mut body = Vec::new();
    let mut idx = 0;

    while idx < tokens.len() {
        match &tokens[idx].token {
            Token::End | Token::Endcase | Token::Else => break,
            // Penutup subrutin (LRM §13.3/§13.4) juga menjadi batas blok,
            // supaya badan task/function tidak menelan token penutupnya.
            Token::Endtask | Token::Endfunction => break,
            // Deklarasi variabel lokal di dalam blok (LRM §12.8).
            t if sv_lexer::token::tipe::bisa_mulai_deklarasi(t) => {
                let (decls, consumed) = super::declaration::parse_declaration(&tokens[idx..])?;
                for decl in decls {
                    body.push(CombinationalStatement::Decl(decl));
                }
                idx += consumed;
            }
            _ => {
                let (stmt, consumed) = parse_combinational_statement(&tokens[idx..])?;
                body.push(stmt);
                idx += consumed;
            }
        }
    }

    Ok((body, idx))
}

fn parse_case(
    tokens: &[SpannedToken],
    kind: CaseKind,
) -> Result<(CombinationalStatement, usize), String> {
    let span = tokens[0].span;
    let mut idx = 1; // skip 'case' | 'casez' | 'casex'

    if idx >= tokens.len() || tokens[idx].token != Token::LParen {
        return Err("expected '(' after case".to_string());
    }
    idx += 1;

    let (selector, consumed) = super::expression::parse_expression(&tokens[idx..])?;
    idx += consumed;

    if idx >= tokens.len() || tokens[idx].token != Token::RParen {
        return Err("expected ')'".to_string());
    }
    idx += 1;

    let mut arms = Vec::new();
    while idx < tokens.len() && tokens[idx].token != Token::Endcase {
        let arm_span = tokens[idx].span;

        if tokens[idx].token == Token::Default {
            idx += 1;
            if idx >= tokens.len() || tokens[idx].token != Token::Colon {
                return Err("expected ':' after default".to_string());
            }
            idx += 1;

            let (stmt, consumed) = parse_combinational_statement(&tokens[idx..])?;
            idx += consumed;

            arms.push(CaseArm {
                labels: Vec::new(),
                is_default: true,
                body: vec![stmt],
                span: arm_span,
            });
        } else {
            let mut labels = Vec::new();
            loop {
                let (label, consumed) = super::expression::parse_expression(&tokens[idx..])?;
                labels.push(label);
                idx += consumed;

                if idx < tokens.len() && tokens[idx].token == Token::Comma {
                    idx += 1;
                    continue;
                }
                break;
            }

            if idx >= tokens.len() || tokens[idx].token != Token::Colon {
                return Err("expected ':' after case label".to_string());
            }
            idx += 1;

            let (stmt, consumed) = parse_combinational_statement(&tokens[idx..])?;
            idx += consumed;

            arms.push(CaseArm {
                labels,
                is_default: false,
                body: vec![stmt],
                span: arm_span,
            });
        }
    }

    if idx >= tokens.len() {
        return Err("expected 'endcase'".to_string());
    }
    idx += 1; // skip 'endcase'

    Ok((
        CombinationalStatement::Case {
            selector,
            arms,
            kind,
            span,
        },
        idx,
    ))
}

fn parse_if(tokens: &[SpannedToken]) -> Result<(CombinationalStatement, usize), String> {
    let span = tokens[0].span;
    let mut idx = 1; // skip 'if'

    if idx >= tokens.len() || tokens[idx].token != Token::LParen {
        return Err("expected '(' after if".to_string());
    }
    idx += 1;

    let (condition, consumed) = super::expression::parse_expression(&tokens[idx..])?;
    idx += consumed;

    if idx >= tokens.len() || tokens[idx].token != Token::RParen {
        return Err("expected ')'".to_string());
    }
    idx += 1;

    let (then_branch, consumed) = parse_combinational_statement(&tokens[idx..])?;
    idx += consumed;

    let mut else_branch = None;
    if idx < tokens.len() && tokens[idx].token == Token::Else {
        idx += 1;
        let (stmt, consumed) = parse_combinational_statement(&tokens[idx..])?;
        idx += consumed;
        else_branch = Some(vec![stmt]);
    }

    Ok((
        CombinationalStatement::IfElse {
            condition,
            then_branch: vec![then_branch],
            else_branch,
            span,
        },
        idx,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(src: &str) -> CombinationalStatement {
        let tokens = sv_lexer::lex(src).expect("lex");
        let (stmt, _) = parse_combinational_statement(&tokens).expect("parse");
        stmt
    }

    /// Ambil langkah `for` (init atau step) dari statement hasil parse.
    fn langkah(stmt: &CombinationalStatement, mau_init: bool) -> &ForStep {
        match stmt {
            CombinationalStatement::For { init, step, .. } => {
                if mau_init {
                    init
                } else {
                    step
                }
            }
            other => panic!("bukan for: {:?}", other),
        }
    }

    #[test]
    fn langkah_increment_dipakai_ke_operator_tambah() {
        // BUG-20: `i++` diurai menjadi `i = i + 1`.
        let stmt = parse("for (i = 0; i < 5; i++) s = s + 1;");
        let step = langkah(&stmt, false);
        assert_eq!(step.lhs.name, "i");
        match &step.rhs {
            Expr::Binary { op, lhs, rhs, .. } => {
                assert_eq!(*op, BinaryOp::Add);
                assert!(matches!(**lhs, Expr::Ident { ref name, .. } if name == "i"));
                assert_eq!(**rhs, Expr::Number(1));
            }
            other => panic!("rhs tak terduga: {:?}", other),
        }
    }

    #[test]
    fn langkah_decrement_dipakai_ke_operator_kurang() {
        // BUG-20: `i--` diurai menjadi `i = i - 1`.
        let stmt = parse("for (i = 9; i > 0; i--) s = s + 1;");
        let step = langkah(&stmt, false);
        match &step.rhs {
            Expr::Binary { op, rhs, .. } => {
                assert_eq!(*op, BinaryOp::Sub);
                assert_eq!(**rhs, Expr::Number(1));
            }
            other => panic!("rhs tak terduga: {:?}", other),
        }
    }

    #[test]
    fn langkah_compound_dipakai_ke_ekspresi_lhs_sekarang() {
        // BUG-20: `i += 2` diurai menjadi `i = i + 2`, bukan `i = 2`.
        let stmt = parse("for (i = 0; i < 8; i += 2) s = s + 1;");
        let step = langkah(&stmt, false);
        match &step.rhs {
            Expr::Binary { op, lhs, rhs, .. } => {
                assert_eq!(*op, BinaryOp::Add);
                assert!(matches!(**lhs, Expr::Ident { ref name, .. } if name == "i"));
                assert_eq!(**rhs, Expr::Number(2));
            }
            other => panic!("rhs tak terduga: {:?}", other),
        }
    }

    #[test]
    fn langkah_compound_bagi_juga_didukung() {
        let stmt = parse("for (i = 8; i > 0; i /= 2) s = s + 1;");
        let step = langkah(&stmt, false);
        match &step.rhs {
            Expr::Binary { op, .. } => assert_eq!(*op, BinaryOp::Div),
            other => panic!("rhs tak terduga: {:?}", other),
        }
    }

    #[test]
    fn compound_geser_aritmetik_diparse_sebagai_operator_sar() {
        // LRM §11.4.10: `>>>=` harus menjadi `x = x >>> n`, bukan `x = x >> n`.
        let stmt = parse("q >>>= 2;");
        let keluar = format!("{stmt:?}");
        assert!(keluar.contains("Sar"), "dapat {keluar}");
        let step = parse("for (i = 8; i > 0; i >>>= 1) s = s + 1;");
        match &langkah(&step, false).rhs {
            Expr::Binary { op, .. } => assert_eq!(*op, BinaryOp::Sar),
            other => panic!("rhs tak terduga: {other:?}"),
        }
    }

    #[test]
    fn langkah_penugasan_biasa_tetap_berfungsi() {
        // Regresi: `i = i + 1` harus tetap diterima apa adanya.
        let stmt = parse("for (i = 0; i < 4; i = i + 1) s = s + 1;");
        let step = langkah(&stmt, false);
        match &step.rhs {
            Expr::Binary { op, .. } => assert_eq!(*op, BinaryOp::Add),
            other => panic!("rhs tak terduga: {:?}", other),
        }
    }

    #[test]
    fn langkah_increment_pada_init_juga_diterima() {
        // `i++` boleh muncul di init maupun step.
        let stmt = parse("for (i++; i < 5; i++) s = s + 1;");
        let init = langkah(&stmt, true);
        match &init.rhs {
            Expr::Binary { op, .. } => assert_eq!(*op, BinaryOp::Add),
            other => panic!("rhs tak terduga: {:?}", other),
        }
    }

    #[test]
    fn body_kosong_berhasil_diurai() {
        // BUG-21: `for (...);` berarti loop tanpa body.
        let stmt = parse("for (i = 0; i < 4; i = i + 1);");
        match &stmt {
            CombinationalStatement::For { body, .. } => assert!(body.is_empty()),
            other => panic!("bukan for: {:?}", other),
        }
    }

    #[test]
    fn body_berisi_bukan_kosong() {
        // Regresi: bodynyata tetap terurai satu statement.
        let stmt = parse("for (i = 0; i < 4; i = i + 1) s = s + 1;");
        match &stmt {
            CombinationalStatement::For { body, .. } => assert_eq!(body.len(), 1),
            other => panic!("bukan for: {:?}", other),
        }
    }

    #[test]
    fn delay_dengan_statement_badan() {
        // LRM §11.2: `#5 a = 1;` menunda statement yang mengikutinya.
        let stmt = parse("#5 a = 8'd1;");
        match &stmt {
            CombinationalStatement::Delay { amount, body, .. } => {
                assert_eq!(amount, &Expr::Number(5));
                assert!(matches!(
                    body.as_ref(),
                    CombinationalStatement::BlockingAssign { .. }
                ));
            }
            other => panic!("bukan delay: {:?}", other),
        }
    }

    #[test]
    fn delay_tanpa_statement_badan() {
        // `#10;` hanya menggeser waktu tanpa statement.
        let stmt = parse("#10;");
        match &stmt {
            CombinationalStatement::Delay { amount, body, .. } => {
                assert_eq!(amount, &Expr::Number(10));
                assert!(
                    matches!(body.as_ref(), CombinationalStatement::Block { body, .. } if body.is_empty())
                );
            }
            other => panic!("bukan delay: {:?}", other),
        }
    }

    #[test]
    fn delay_dengan_satuan_waktu_dilewati() {
        // Satuan `ns` dilewati tanpa error; hanya jumlah delay yang dipakai.
        let stmt = parse("#3ns a = 8'd1;");
        match &stmt {
            CombinationalStatement::Delay { amount, .. } => assert_eq!(amount, &Expr::Number(3)),
            other => panic!("bukan delay: {:?}", other),
        }
    }

    #[test]
    fn delay_dengan_amount_variabel() {
        let stmt = parse("#n a = 8'd1;");
        match &stmt {
            CombinationalStatement::Delay { amount, .. } => {
                assert!(matches!(amount, Expr::Ident { ref name, .. } if name == "n"));
            }
            other => panic!("bukan delay: {:?}", other),
        }
    }

    #[test]
    fn delay_membungkus_blok_penuh() {
        let stmt = parse("#1 begin a = 1; end");
        match &stmt {
            CombinationalStatement::Delay { body, .. } => {
                assert!(
                    matches!(body.as_ref(), CombinationalStatement::Block { body, .. } if body.len() == 1)
                );
            }
            other => panic!("bukan delay: {:?}", other),
        }
    }

    #[test]
    fn satuan_ps_mencatat_picosecond() {
        let stmt = parse("#1ps a = 8'd1;");
        match &stmt {
            CombinationalStatement::Delay { amount, unit, .. } => {
                assert_eq!(amount, &Expr::Number(1));
                assert_eq!(*unit, sv_ast::time_unit::TimeUnit::PicoSeconds);
            }
            other => panic!("bukan delay: {:?}", other),
        }
    }

    #[test]
    fn satuan_tanpa_nama_menggunakan_nanosecond() {
        let stmt = parse("#5 a = 8'd1;");
        match &stmt {
            CombinationalStatement::Delay { unit, .. } => {
                assert_eq!(*unit, sv_ast::time_unit::TimeUnit::NanoSeconds);
            }
            other => panic!("bukan delay: {:?}", other),
        }
    }

    #[test]
    fn setiap_satuan_waktu_dicatat() {
        for (teks, diharapkan) in [
            ("#1s", sv_ast::time_unit::TimeUnit::Seconds),
            ("#1ms", sv_ast::time_unit::TimeUnit::MilliSeconds),
            ("#1us", sv_ast::time_unit::TimeUnit::MicroSeconds),
            ("#1ns", sv_ast::time_unit::TimeUnit::NanoSeconds),
            ("#1ps", sv_ast::time_unit::TimeUnit::PicoSeconds),
            ("#1fs", sv_ast::time_unit::TimeUnit::FectoSeconds),
        ] {
            let stmt = parse(&format!("{} a = 8'd1;", teks));
            match &stmt {
                CombinationalStatement::Delay { unit, .. } => {
                    assert_eq!(*unit, diharapkan, "satuan pada {}", teks);
                }
                other => panic!("bukan delay: {:?}", other),
            }
        }
    }

    #[test]
    fn identifier_bukan_satuan_tidak_dikonsumsi_sebagai_satuan() {
        // `#5 step = ...` — `step` identifier biasa, bukan satuan waktu,
        // jadi harus jadi target statement biasa dan satuan tetap nanosecond.
        let tokens = sv_lexer::lex("#5 step = 8'd1;").unwrap();
        let (stmt, _) = parse_combinational_statement(&tokens).expect("parse");
        match &stmt {
            CombinationalStatement::Delay { unit, body, .. } => {
                assert_eq!(*unit, sv_ast::time_unit::TimeUnit::NanoSeconds);
                match body.as_ref() {
                    CombinationalStatement::BlockingAssign { lhs, .. } => {
                        assert_eq!(lhs.name, "step");
                    }
                    other => panic!("body harus assignment, dapat {:?}", other),
                }
            }
            other => panic!("bukan delay: {:?}", other),
        }
    }
}
