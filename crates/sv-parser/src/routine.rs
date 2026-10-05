// Tanggung jawab: parse deklarasi task/function beserta port list dan badan.
use sv_ast::combinational::CombinationalStatement;
use sv_ast::routine::{ArgDirection, RoutineArg, RoutineDecl, RoutineKind, TaskCall};
use sv_ast::width::WidthExpr;
use sv_lexer::span::Span;
use sv_lexer::spanned::SpannedToken;
use sv_lexer::token::Token;

/// Parse deklarasi `task ... endtask` atau `function ... endfunction`.
///
/// Bentuk yang didukung (LRM §13.3/§13.4):
/// ```text
/// task [automatic] name [( [arg {, arg}]* )] ; body endtask
/// function [automatic] [logic|integer|int] [signed] [[msb:lsb]] name
///          [( [arg {, arg}]* )] ; body endfunction
/// ```
/// Setiap argumen bertipe `input|output|inout [logic] [signed] [[msb:lsb]] name`.
pub fn parse_routine(tokens: &[SpannedToken]) -> Result<(RoutineDecl, usize), String> {
    let (kind, _) = match tokens.first().map(|t| &t.token) {
        Some(Token::Task) => (RoutineKind::Task, ()),
        Some(Token::Function) => (RoutineKind::Function, ()),
        other => return Err(format!("expected 'task' atau 'function', dapat {other:?}")),
    };
    let span = tokens[0].span;
    let mut idx = 1;

    // Keyword `automatic` diterima; body di-inline sehingga tidak ada
    // penyimpanan statis yang perlu dibedakan.
    let automatic = matches!(tokens.get(idx).map(|t| &t.token), Some(Token::Automatic));
    if automatic {
        idx += 1;
    }

    // Tipe balik hanya untuk function: `logic`, `integer`, `int`, `byte`,
    // `reg`, `bit`, dan seterusnya (LRM §6.16, Tabel 6-22).
    let mut lebar_atom = None;
    if matches!(kind, RoutineKind::Function) {
        lebar_atom = tokens
            .get(idx)
            .and_then(|t| sv_lexer::token::tipe::atom(&t.token));
        let bisa_mulai = tokens
            .get(idx)
            .is_some_and(|t| sv_lexer::token::tipe::bisa_mulai_deklarasi(&t.token));
        if bisa_mulai {
            idx += 1;
        }
    }

    let return_signed = lebar_atom.is_some_and(|(_, s)| s)
        || matches!(tokens.get(idx).map(|t| &t.token), Some(Token::Signed));
    if matches!(
        tokens.get(idx).map(|t| &t.token),
        Some(Token::Signed) | Some(Token::Unsigned)
    ) {
        idx += 1;
    }

    // Lebar nilai balik: `function [7:0] f`. Tanpa dimension berarti 1 bit,
    // kecuali tipe atom yang lebarnya sudah pasti (Tabel 6-22).
    let (lebar, consumed) = match lebar_atom {
        Some((w, _)) => (WidthExpr::Literal(w as usize), 0),
        None => super::dimension::parse_dimension(tokens, idx)?,
    };
    idx += consumed;

    // Nama subrutin.
    let name = match tokens.get(idx).map(|t| &t.token) {
        Some(Token::Ident(n)) => n.clone(),
        other => return Err(format!("expected nama subrutin, dapat {other:?}")),
    };
    idx += 1;

    // Port list opsional.
    let mut args = Vec::new();
    if tokens.get(idx).map(|t| &t.token) == Some(&Token::LParen) {
        idx += 1;
        while idx < tokens.len() && tokens[idx].token != Token::RParen {
            let (arg, consumed) = parse_arg(&tokens[idx..])?;
            args.push(arg);
            idx += consumed;
            if tokens.get(idx).map(|t| &t.token) == Some(&Token::Comma) {
                idx += 1;
            }
        }
        if tokens.get(idx).map(|t| &t.token) != Some(&Token::RParen) {
            return Err("expected ')' closing port list".to_string());
        }
        idx += 1;
    }

    if tokens.get(idx).map(|t| &t.token) != Some(&Token::Semi) {
        return Err("expected ';' setelah header subrutin".to_string());
    }
    idx += 1;

    let penutup = match kind {
        RoutineKind::Task => Token::Endtask,
        RoutineKind::Function => Token::Endfunction,
    };
    let (body, consumed) = super::combinational::parse_combinational_block(&tokens[idx..])?;
    idx += consumed;
    if tokens.get(idx).map(|t| &t.token) != Some(&penutup) {
        return Err(format!("expected {:?} menutup subrutin", penutup));
    }
    idx += 1;

    // Task tidak punya nilai balik, jadi lebar yang ditulis di header diabaikan.
    let return_width = match kind {
        RoutineKind::Task => None,
        RoutineKind::Function => Some(lebar),
    };

    Ok((
        RoutineDecl {
            kind,
            name,
            return_width,
            return_signed,
            args,
            body,
            automatic,
            span,
        },
        idx,
    ))
}

/// Parse satu argumen pada port list subrutin (LRM §13.3).
///
/// Bentuk: `[input|output|inout] [logic|reg|bit] [signed] [[msb:lsb]] name`.
fn parse_arg(tokens: &[SpannedToken]) -> Result<(RoutineArg, usize), String> {
    let mut idx = 0;
    let span = tokens.first().map(|t| t.span).unwrap_or_else(Span::dummy);
    let direction = match tokens.get(idx).map(|t| &t.token) {
        Some(Token::Input) => ArgDirection::Input,
        Some(Token::Output) => ArgDirection::Output,
        Some(Token::Inout) => ArgDirection::Inout,
        other => return Err(format!("expected arah argumen, dapat {other:?}")),
    };
    idx += 1;

    // Qualifier tipe opsional. Tipe atom sudah menentukan lebarnya sehingga
    // tidak boleh diikuti dimension (LRM §6.16, Tabel 6-22).
    let tipe_bulat = tokens
        .get(idx)
        .and_then(|t| sv_lexer::token::tipe::atom(&t.token));
    let bisa_mulai = tokens
        .get(idx)
        .is_some_and(|t| sv_lexer::token::tipe::bisa_mulai_deklarasi(&t.token));
    if bisa_mulai {
        idx += 1;
    }

    let signed = tipe_bulat.is_some_and(|(_, s)| s)
        || matches!(tokens.get(idx).map(|t| &t.token), Some(Token::Signed));
    if matches!(
        tokens.get(idx).map(|t| &t.token),
        Some(Token::Signed) | Some(Token::Unsigned)
    ) {
        idx += 1;
    }

    let (width, consumed) = match tipe_bulat {
        Some((w, _)) => (WidthExpr::Literal(w as usize), 0),
        None => super::dimension::parse_dimension(tokens, idx)?,
    };
    idx += consumed;

    let name = match tokens.get(idx).map(|t| &t.token) {
        Some(Token::Ident(n)) => n.clone(),
        other => return Err(format!("expected nama argumen, dapat {other:?}")),
    };
    let arg_span = tokens[idx].span;
    idx += 1;

    Ok((
        RoutineArg {
            name,
            direction,
            width,
            signed,
            span: if arg_span == Span::dummy() {
                span
            } else {
                arg_span
            },
        },
        idx,
    ))
}

/// Parse statement `nama(args);` sebagai panggilan task (LRM §13.3).
///
/// Hanya dipanggil bila token pertama identifier dan token kedua `(`.
pub fn parse_task_call(tokens: &[SpannedToken]) -> Result<(TaskCall, usize), String> {
    let span = tokens[0].span;
    let name = match &tokens[0].token {
        Token::Ident(n) => n.clone(),
        other => return Err(format!("expected nama task, dapat {other:?}")),
    };
    let mut idx = 1; // lewati identifier
    if tokens.get(idx).map(|t| &t.token) != Some(&Token::LParen) {
        return Err("expected '(' setelah nama task".to_string());
    }
    idx += 1;

    let mut args = Vec::new();
    while idx < tokens.len() && tokens[idx].token != Token::RParen {
        let (arg, consumed) = super::expression::parse_expression(&tokens[idx..])?;
        args.push(arg);
        idx += consumed;
        if tokens.get(idx).map(|t| &t.token) == Some(&Token::Comma) {
            idx += 1;
        }
    }
    if tokens.get(idx).map(|t| &t.token) != Some(&Token::RParen) {
        return Err("expected ')' closing argumen task".to_string());
    }
    idx += 1;

    if tokens.get(idx).map(|t| &t.token) != Some(&Token::Semi) {
        return Err("expected ';' setelah panggilan task".to_string());
    }
    idx += 1;

    Ok((TaskCall { name, args, span }, idx))
}

/// Parse statement `nama;` sebagai panggilan task tanpa argumen (LRM §13.3).
///
/// Bentuk tanpa kurung sah untuk task yang tidak punya argumen.
pub fn parse_task_call_tanpa_kurung(tokens: &[SpannedToken]) -> Result<(TaskCall, usize), String> {
    let span = tokens[0].span;
    let name = match &tokens[0].token {
        Token::Ident(n) => n.clone(),
        other => return Err(format!("expected nama task, dapat {other:?}")),
    };
    if tokens.get(1).map(|t| &t.token) != Some(&Token::Semi) {
        return Err("expected ';' setelah nama task".to_string());
    }
    Ok((
        TaskCall {
            name,
            args: Vec::new(),
            span,
        },
        2,
    ))
}

/// Parse `return;` atau `return expr;` (LRM §13.4).
pub fn parse_return(tokens: &[SpannedToken]) -> Result<(CombinationalStatement, usize), String> {
    let span = tokens[0].span;
    let mut idx = 1;
    let value = if tokens.get(idx).map(|t| &t.token) == Some(&Token::Semi) {
        None
    } else {
        let (expr, consumed) = super::expression::parse_expression(&tokens[idx..])?;
        idx += consumed;
        Some(expr)
    };
    if tokens.get(idx).map(|t| &t.token) != Some(&Token::Semi) {
        return Err("expected ';' setelah return".to_string());
    }
    idx += 1;
    Ok((CombinationalStatement::Return { value, span }, idx))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sv_lexer::lex;

    fn parse(src: &str) -> RoutineDecl {
        let tokens = lex(src).expect("lex");
        parse_routine(&tokens).expect("parse").0
    }

    #[test]
    fn function_tanpa_argumen_dan_lebar_sembilan() {
        let decl = parse("function [7:0] paritas; paritas = a; endfunction");
        assert_eq!(decl.kind, RoutineKind::Function);
        assert_eq!(decl.name, "paritas");
        assert_eq!(decl.return_width, Some(WidthExpr::Literal(8)));
        assert!(decl.args.is_empty());
    }

    #[test]
    fn function_dengan_port_list_ansi() {
        let src = "function [7:0] jumlah(input [7:0] a, input [7:0] b); \
                   jumlah = a + b; endfunction";
        let decl = parse(src);
        assert_eq!(decl.args.len(), 2);
        assert_eq!(decl.args[0].name, "a");
        assert_eq!(decl.args[0].width, WidthExpr::Literal(8));
        assert_eq!(decl.args[1].direction, ArgDirection::Input);
    }

    #[test]
    fn task_menyimpan_arah_output() {
        let src = "task isi(output reg [7:0] q, input [7:0] d); q = d; endtask";
        let decl = parse(src);
        assert_eq!(decl.kind, RoutineKind::Task);
        assert_eq!(decl.args[0].direction, ArgDirection::Output);
        assert_eq!(decl.args[1].direction, ArgDirection::Input);
        // Task tidak punya nilai balik meski header menuliskan dimension.
        assert!(decl.return_width.is_none());
    }

    #[test]
    fn automatic_diterima() {
        let decl = parse("task automatic t; endtask");
        assert!(decl.automatic);
    }

    #[test]
    fn integer_function_berlebar_tiga_puluh_dua() {
        let decl = parse("function integer f(input integer a); f = a; endfunction");
        assert_eq!(decl.return_width, Some(WidthExpr::Literal(32)));
        assert!(decl.return_signed);
    }

    #[test]
    fn badan_bisa_berisi_statement_lain() {
        let src = "function [7:0] f(input [7:0] a); if (a > 0) f = a; else f = 0; endfunction";
        let decl = parse(src);
        assert_eq!(decl.body.len(), 1);
    }

    #[test]
    fn endfunction_tidak_tertutup_ditolak() {
        let tokens = lex("function [7:0] f; f = 1;").expect("lex");
        assert!(parse_routine(&tokens).is_err());
    }

    #[test]
    fn port_list_tidak_tertutup_ditolak() {
        let tokens = lex("task t(input a; endtask").expect("lex");
        assert!(parse_routine(&tokens).is_err());
    }

    #[test]
    fn tanpa_nama_ditolak() {
        let tokens = lex("function [7:0]; endfunction").expect("lex");
        assert!(parse_routine(&tokens).is_err());
    }

    #[test]
    fn argumen_tanpa_arah_ditolak() {
        let tokens = lex("task t(a); endtask").expect("lex");
        assert!(parse_routine(&tokens).is_err());
    }

    #[test]
    fn panggilan_task_dengan_argumen() {
        let tokens = lex("isi(q, a + b);").expect("lex");
        let (call, consumed) = parse_task_call(&tokens).expect("parse");
        assert_eq!(call.name, "isi");
        assert_eq!(call.args.len(), 2);
        assert_eq!(consumed, tokens.len());
    }

    #[test]
    fn panggilan_task_tanpa_argumen() {
        let tokens = lex("tick();").expect("lex");
        let (call, _) = parse_task_call(&tokens).expect("parse");
        assert_eq!(call.name, "tick");
        assert!(call.args.is_empty());
    }

    #[test]
    fn panggilan_task_tanpa_semicolon_ditolak() {
        let tokens = lex("isi(q, a)").expect("lex");
        assert!(parse_task_call(&tokens).is_err());
    }

    #[test]
    fn panggilan_task_tanpa_kurung() {
        // LRM §13.3: task tanpa argumen boleh dipanggil `nama;`.
        let tokens = lex("tick;").expect("lex");
        let (call, consumed) = parse_task_call_tanpa_kurung(&tokens).expect("parse");
        assert_eq!(call.name, "tick");
        assert!(call.args.is_empty());
        assert_eq!(consumed, tokens.len());
    }

    #[test]
    fn panggilan_task_tanpa_kurung_tanpa_semicolon_ditolak() {
        let tokens = lex("tick").expect("lex");
        assert!(parse_task_call_tanpa_kurung(&tokens).is_err());
    }

    #[test]
    fn return_tanpa_nilai() {
        let tokens = lex("return;").expect("lex");
        let (stmt, _) = parse_return(&tokens).expect("parse");
        match stmt {
            CombinationalStatement::Return { value, .. } => assert!(value.is_none()),
            other => panic!("harus return, dapat {other:?}"),
        }
    }

    #[test]
    fn return_dengan_nilai() {
        let tokens = lex("return a + 1;").expect("lex");
        let (stmt, _) = parse_return(&tokens).expect("parse");
        match stmt {
            CombinationalStatement::Return { value, .. } => assert!(value.is_some()),
            other => panic!("harus return, dapat {other:?}"),
        }
    }
}
