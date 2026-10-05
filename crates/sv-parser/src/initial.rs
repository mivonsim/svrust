// Tanggung jawab: parse blok initial dan system task untuk testbench.
use sv_ast::statement::Statement;
use sv_ast::system_task::{SystemArg, SystemTask, SystemTaskKind};
use sv_lexer::spanned::SpannedToken;
use sv_lexer::token::Token;

/// Parse `initial begin ... end` atau `initial <statement>`.
pub fn parse_initial(tokens: &[SpannedToken]) -> Result<(Statement, usize), String> {
    if tokens.is_empty() || tokens[0].token != Token::Initial {
        return Err("expected 'initial'".to_string());
    }
    let span = tokens[0].span;
    let mut idx = 1;

    if idx >= tokens.len() {
        return Err("expected initial body".to_string());
    }

    let body = if tokens[idx].token == Token::Begin {
        idx += 1;
        let (stmts, consumed) = super::combinational::parse_combinational_block(&tokens[idx..])?;
        idx += consumed;
        if idx >= tokens.len() || tokens[idx].token != Token::End {
            return Err("expected 'end'".to_string());
        }
        idx += 1;
        stmts
    } else {
        let (stmt, consumed) = super::combinational::parse_combinational_statement(&tokens[idx..])?;
        idx += consumed;
        vec![stmt]
    };

    Ok((Statement::Initial { body, span }, idx))
}

/// Parse statement system task seperti `$display(...)` atau `$finish;`.
///
/// LRM §20 tidak membatasi system task pada `initial` saja; jalur `always_ff`
/// memakai `parse_system_task_mentah` lalu membungkusnya sebagai
/// `SequentialStatement::SystemTask`.
pub fn parse_system_task(
    tokens: &[SpannedToken],
) -> Result<(sv_ast::combinational::CombinationalStatement, usize), String> {
    let (task, consumed) = parse_system_task_mentah(tokens)?;
    Ok((
        sv_ast::combinational::CombinationalStatement::SystemTask(task),
        consumed,
    ))
}

/// Versi `parse_system_task` yang mengembalikan node `SystemTask` apa adanya,
/// tanpa membungkusnya ke `CombinationalStatement`.
pub(crate) fn parse_system_task_mentah(
    tokens: &[SpannedToken],
) -> Result<(SystemTask, usize), String> {
    if tokens.is_empty() {
        return Err("expected system task".to_string());
    }
    let span = tokens[0].span;
    let name = match &tokens[0].token {
        Token::SystemTask(n) => n.clone(),
        _ => return Err("expected system task".to_string()),
    };
    let kind = SystemTaskKind::from_name(&name)
        .ok_or_else(|| format!("system task '${}' belum didukung", name))?;

    let mut idx = 1;
    let mut args = Vec::new();
    // `$monitor if (kondisi) args;` — syarat opsional (LRM §20.2).
    let mut condition = None;

    // Bentuk LRM menempatkan `if` sebelum argumen.
    if tokens.get(idx).map(|t| &t.token) == Some(&Token::If) {
        let (expr, consumed) = parse_kondisi(tokens, idx)?;
        condition = Some(expr);
        idx += consumed;
    }

    // `$finish` boleh ditulis tanpa tanda kurung.
    if tokens.get(idx).map(|t| &t.token) == Some(&Token::LParen) {
        idx += 1;
        while idx < tokens.len() && tokens[idx].token != Token::RParen {
            // Koma pemisah dilewati sebelum argumen berikutnya dibaca.
            if tokens[idx].token == Token::Comma {
                idx += 1;
                continue;
            }
            match &tokens[idx].token {
                Token::StringLiteral(teks) => args.push(SystemArg::Format(teks.clone())),
                _ => {
                    let (expr, consumed) = super::expression::parse_expression(&tokens[idx..])?;
                    idx += consumed;
                    args.push(SystemArg::Value(expr));
                    continue;
                }
            }
            idx += 1;
        }
        if idx >= tokens.len() {
            return Err(format!("expected ')' closing ${}", name));
        }
        idx += 1;
    }

    // Bentuk lain menuliskan syarat setelah argumen; keduanya diterima.
    if condition.is_none() && tokens.get(idx).map(|t| &t.token) == Some(&Token::If) {
        let (expr, consumed) = parse_kondisi(tokens, idx)?;
        condition = Some(expr);
        idx += consumed;
    }

    // Setelah syarat, argumen boleh ditulis tanpa kurung seperti pada
    // `$monitor if (c) "a=%d", a;` (LRM §20.2).
    if condition.is_some() && tokens.get(idx).map(|t| &t.token) != Some(&Token::Semi) {
        loop {
            match tokens.get(idx).map(|t| &t.token) {
                None => return Err(format!("expected ';' after ${}", name)),
                Some(Token::Semi) => break,
                Some(Token::Comma) => {
                    idx += 1;
                    continue;
                }
                Some(Token::StringLiteral(teks)) => {
                    args.push(SystemArg::Format(teks.clone()));
                    idx += 1;
                }
                Some(_) => {
                    let (expr, consumed) = super::expression::parse_expression(&tokens[idx..])?;
                    idx += consumed;
                    args.push(SystemArg::Value(expr));
                }
            }
        }
    }

    if tokens.get(idx).map(|t| &t.token) != Some(&Token::Semi) {
        return Err(format!("expected ';' after ${}", name));
    }
    idx += 1;

    Ok((
        SystemTask {
            kind,
            args,
            condition,
            // `timescale` modul belum diketahui saat parsing;
            // `sv_ast::delay_unit::terapkan_module` yang mengisinya.
            time_scale: sv_ast::time_scale::TimeScale::default(),
            span,
        },
        idx,
    ))
}

/// Parse `if (ekspresi)` sebagai syarat system task; posisi `idx` menunjuk `if`.
fn parse_kondisi(
    tokens: &[SpannedToken],
    idx: usize,
) -> Result<(sv_ast::expression::Expr, usize), String> {
    let mut pos = idx + 1;
    if tokens.get(pos).map(|t| &t.token) != Some(&Token::LParen) {
        return Err("expected '(' after 'if' in system task".to_string());
    }
    pos += 1;
    let (expr, consumed) = super::expression::parse_expression(&tokens[pos..])?;
    pos += consumed;
    if tokens.get(pos).map(|t| &t.token) != Some(&Token::RParen) {
        return Err("expected ')' closing 'if' condition".to_string());
    }
    Ok((expr, pos + 1 - idx))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sv_lexer::lex;

    fn parse_task(src: &str) -> sv_ast::combinational::CombinationalStatement {
        let tokens = lex(src).expect("lex");
        let (stmt, _) = parse_system_task(&tokens).expect("parse");
        stmt
    }

    #[test]
    fn display_dengan_string_saja() {
        match parse_task("$display(\"halo\");") {
            sv_ast::combinational::CombinationalStatement::SystemTask(t) => {
                assert_eq!(t.kind, SystemTaskKind::Display);
                assert_eq!(t.args.len(), 1);
                assert_eq!(t.args[0], SystemArg::Format("halo".to_string()));
            }
            lain => panic!("dapat {:?}", lain),
        }
    }

    #[test]
    fn display_mencampur_format_dengan_ekspresi() {
        match parse_task("$display(\"n=%d\", a);") {
            sv_ast::combinational::CombinationalStatement::SystemTask(t) => {
                assert_eq!(t.args.len(), 2);
                assert!(matches!(t.args[0], SystemArg::Format(_)));
                assert!(matches!(t.args[1], SystemArg::Value(_)));
            }
            lain => panic!("dapat {:?}", lain),
        }
    }

    #[test]
    fn display_menerima_banyak_argumen_nilai() {
        match parse_task("$display(\"a=%d b=%d\", x, y);") {
            sv_ast::combinational::CombinationalStatement::SystemTask(t) => {
                assert_eq!(t.args.len(), 3);
                assert!(matches!(t.args[1], SystemArg::Value(_)));
                assert!(matches!(t.args[2], SystemArg::Value(_)));
            }
            lain => panic!("dapat {:?}", lain),
        }
    }

    #[test]
    fn finish_tanpa_kurung_diterima() {
        match parse_task("$finish;") {
            sv_ast::combinational::CombinationalStatement::SystemTask(t) => {
                assert_eq!(t.kind, SystemTaskKind::Finish);
                assert!(t.args.is_empty());
            }
            lain => panic!("dapat {:?}", lain),
        }
    }

    #[test]
    fn system_task_tidak_dikenal_memberi_pesan_jelas() {
        // `$monitor` kini didukung, jadi contoh memakai task yang belum.
        let tokens = lex("$writeln(a);").expect("lex");
        let err = parse_system_task(&tokens).expect_err("harus gagal");
        assert!(
            err.contains("writeln"),
            "pesan harus menyebut nama: {}",
            err
        );
    }

    #[test]
    fn monitor_diparse_dengan_argumen() {
        match parse_task("$monitor(\"a=%d\", a);") {
            sv_ast::combinational::CombinationalStatement::SystemTask(t) => {
                assert_eq!(t.kind, SystemTaskKind::Monitor);
                assert_eq!(t.args.len(), 2);
            }
            lain => panic!("dapat {:?}", lain),
        }
    }

    #[test]
    fn monitor_bisa_punya_syarat_sebelum_argumen() {
        match parse_task("$monitor if (a > b) \"a=%d\", a;") {
            sv_ast::combinational::CombinationalStatement::SystemTask(t) => {
                assert_eq!(t.kind, SystemTaskKind::Monitor);
                assert!(t.condition.is_some());
                assert_eq!(t.args.len(), 2);
            }
            lain => panic!("dapat {:?}", lain),
        }
    }

    #[test]
    fn monitor_bisa_punya_syarat_setelah_argumen() {
        match parse_task("$monitor(\"a=%d\", a) if (a > b);") {
            sv_ast::combinational::CombinationalStatement::SystemTask(t) => {
                assert_eq!(t.kind, SystemTaskKind::Monitor);
                assert!(t.condition.is_some());
            }
            lain => panic!("dapat {:?}", lain),
        }
    }

    #[test]
    fn display_tanpa_syarat_memakai_nilai_kosong() {
        match parse_task("$display(\"halo\");") {
            sv_ast::combinational::CombinationalStatement::SystemTask(t) => {
                assert!(t.condition.is_none());
            }
            lain => panic!("dapat {:?}", lain),
        }
    }

    #[test]
    fn syarat_tanpa_kurung_kurawal_ditolak() {
        let tokens = lex("$monitor if a \"x=%d\", a;").expect("lex");
        let err = parse_system_task(&tokens).expect_err("harus gagal");
        assert!(
            err.contains("'('"),
            "pesan harus menyebut kurung buka: {}",
            err
        );
    }

    #[test]
    fn dumpfile_membawa_nama_berkas() {
        match parse_task("$dumpfile(\"sim.vcd\");") {
            sv_ast::combinational::CombinationalStatement::SystemTask(t) => {
                assert_eq!(t.kind, SystemTaskKind::DumpFile);
                assert_eq!(t.args, vec![SystemArg::Format("sim.vcd".to_string())]);
                assert!(t.condition.is_none());
            }
            lain => panic!("dapat {:?}", lain),
        }
    }

    #[test]
    fn dumpvars_tanpa_kurung_diterima() {
        match parse_task("$dumpvars;") {
            sv_ast::combinational::CombinationalStatement::SystemTask(t) => {
                assert_eq!(t.kind, SystemTaskKind::DumpVars);
                assert!(t.args.is_empty());
            }
            lain => panic!("dapat {:?}", lain),
        }
    }

    #[test]
    fn dumpvars_dengan_kurung_kosong_diterima() {
        match parse_task("$dumpvars();") {
            sv_ast::combinational::CombinationalStatement::SystemTask(t) => {
                assert_eq!(t.kind, SystemTaskKind::DumpVars);
                assert!(t.args.is_empty());
            }
            lain => panic!("dapat {:?}", lain),
        }
    }

    #[test]
    fn monitor_tanpa_argumen_diterima() {
        match parse_task("$monitor;") {
            sv_ast::combinational::CombinationalStatement::SystemTask(t) => {
                assert_eq!(t.kind, SystemTaskKind::Monitor);
                assert!(t.args.is_empty());
            }
            lain => panic!("dapat {:?}", lain),
        }
    }

    #[test]
    fn initial_blok_diparse() {
        let tokens = lex("initial begin y = 8'd1; end").expect("lex");
        let (stmt, _) = parse_initial(&tokens).expect("parse");
        match stmt {
            Statement::Initial { body, .. } => assert_eq!(body.len(), 1),
            lain => panic!("dapat {:?}", lain),
        }
    }
}
