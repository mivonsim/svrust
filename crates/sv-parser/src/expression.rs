// Tanggung jawab: parse expression dengan presedence operator LRM.
use sv_ast::expression::{BinaryOp, Expr, UnaryOp};
use sv_lexer::spanned::SpannedToken;
use sv_lexer::token::Token;

/// Parse ekspresi penuh; rantai kiri-assosiatif per level presedence.
pub fn parse_expression(tokens: &[SpannedToken]) -> Result<(Expr, usize), String> {
    if tokens.is_empty() {
        return Err("empty expression".to_string());
    }
    // BUG-30: bentuk prefix `++a` punya efek samping dan bisa dievaluasi
    // berulang pada proses combinational, jadi belum didukung. Hanya posisi
    // awal yang diperiksa, karena slice token menyisipkan sisa statement
    // (misal langkah `i++` pada header `for`).
    if matches!(tokens[0].token, Token::Incr | Token::Decr) {
        return Err("increment/decrement prefix dalam ekspresi belum didukung; \
             pakai statement terpisah `a++;`"
            .to_string());
    }
    let (mut expr, mut idx) = parse_level(tokens, 1)?;

    // Operator kondisional paling lemah: `c ? a : b` (LRM §11.4.11).
    if idx < tokens.len() && tokens[idx].token == Token::Question {
        let span = tokens[idx].span;
        let (when_true, consumed) = parse_expression(&tokens[idx + 1..])?;
        idx += 1 + consumed;
        if idx >= tokens.len() || tokens[idx].token != Token::Colon {
            return Err("expected ':' in ternary".to_string());
        }
        idx += 1;
        let (when_false, consumed) = parse_expression(&tokens[idx..])?;
        idx += consumed;
        expr = Expr::Ternary {
            condition: Box::new(expr),
            when_true: Box::new(when_true),
            when_false: Box::new(when_false),
            span,
        };
    }
    Ok((expr, idx))
}

/// Level rendah = kalah kuat. 1: `||` ... 9: `+ -`.
fn parse_level(tokens: &[SpannedToken], level: u8) -> Result<(Expr, usize), String> {
    if level > 9 {
        return parse_unary(tokens);
    }
    let (mut lhs, mut idx) = parse_level(tokens, level + 1)?;
    loop {
        if idx >= tokens.len() {
            break;
        }
        let op = match op_pada_level(&tokens[idx].token, level) {
            Some(op) => op,
            None => break,
        };
        let span = tokens[idx].span;
        let (rhs, makan) = parse_level(&tokens[idx + 1..], level + 1)?;
        idx += 1 + makan;
        lhs = Expr::Binary {
            op,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
            span,
        };
    }
    Ok((lhs, idx))
}

/// Petakan token ke operator bila cocok dengan level presedence.
fn op_pada_level(token: &Token, level: u8) -> Option<BinaryOp> {
    match (token, level) {
        (Token::OrOr, 1) => Some(BinaryOp::LogOr),
        (Token::AndAnd, 2) => Some(BinaryOp::LogAnd),
        (Token::EqEq, 3) => Some(BinaryOp::Eq),
        (Token::BangEq, 3) => Some(BinaryOp::NotEq),
        (Token::Lt, 4) => Some(BinaryOp::Lt),
        (Token::Gt, 4) => Some(BinaryOp::Gt),
        // `<=` di konteks ekspresi = less-equal (LRM §11.4).
        (Token::NonBlocking, 4) => Some(BinaryOp::Le),
        (Token::Ge, 4) => Some(BinaryOp::Ge),
        (Token::Or, 5) => Some(BinaryOp::Or),
        (Token::Xor, 6) => Some(BinaryOp::Xor),
        (Token::And, 7) => Some(BinaryOp::And),
        (Token::Shl, 8) => Some(BinaryOp::Shl),
        (Token::Shr, 8) => Some(BinaryOp::Shr),
        // LRM §11.4.10: `>>>` sama presedensinya dengan `>>`.
        (Token::Sar, 8) => Some(BinaryOp::Sar),
        (Token::Plus, 9) => Some(BinaryOp::Add),
        (Token::Minus, 9) => Some(BinaryOp::Sub),
        (Token::Star, 9) => Some(BinaryOp::Mul),
        // LRM §11.4: `*`, `/`, `%` punya presedence yang sama.
        (Token::Slash, 9) => Some(BinaryOp::Div),
        (Token::Percent, 9) => Some(BinaryOp::Mod),
        _ => None,
    }
}

/// Operator unary dan reduction, semuanya paling kuat (LRM §11.4).
fn parse_unary(tokens: &[SpannedToken]) -> Result<(Expr, usize), String> {
    if tokens.is_empty() {
        return Err("expected unary expression".to_string());
    }
    let span = tokens[0].span;

    // `~&`, `~|`, `~^` dua karakter harus dicek sebelum token tunggal.
    if let Some(op) = reduction_ganda(&tokens[0].token, tokens.get(1).map(|t| &t.token)) {
        // Kedua token operator sudah dibaca, jadi operand mulai di posisi 2.
        let (operand, consumed) = parse_unary(&tokens[2..])?;
        return Ok((
            Expr::Unary {
                op,
                operand: Box::new(operand),
                span,
            },
            2 + consumed,
        ));
    }

    let op = match &tokens[0].token {
        Token::Tilde => Some(UnaryOp::BitNot),
        Token::Bang => Some(UnaryOp::LogNot),
        Token::Minus => Some(UnaryOp::BitNeg),
        Token::Plus => None, // unary plus: identitas, cukup dilewati
        Token::And => Some(UnaryOp::RedAnd),
        Token::Or => Some(UnaryOp::RedOr),
        Token::Xor => Some(UnaryOp::RedXor),
        _ => None,
    };

    match op {
        Some(op) => {
            let (operand, consumed) = parse_unary(&tokens[1..])?;
            Ok((
                Expr::Unary {
                    op,
                    operand: Box::new(operand),
                    span,
                },
                1 + consumed,
            ))
        }
        None if tokens[0].token == Token::Plus => {
            let (operand, consumed) = parse_unary(&tokens[1..])?;
            Ok((operand, 1 + consumed))
        }
        None => parse_postfix(tokens),
    }
}

/// Reduksi dua karakter: `~&`, `~|`, `~^` (LRM §11.4.9).
fn reduction_ganda(first: &Token, next: Option<&Token>) -> Option<UnaryOp> {
    if first != &Token::Tilde {
        return None;
    }
    match next {
        Some(Token::And) => Some(UnaryOp::RedNand),
        Some(Token::Or) => Some(UnaryOp::RedNor),
        Some(Token::Xor) => Some(UnaryOp::RedXnor),
        _ => None,
    }
}

/// Primary ditambah suffix bit-select/part-select: `a[3:0]`, `a[0]`.
fn parse_postfix(tokens: &[SpannedToken]) -> Result<(Expr, usize), String> {
    let (mut base, mut idx) = parse_primary(tokens)?;

    // LRM §13.4: `nama(argumen, ...)` adalah pemanggilan function. Sleeve
    // sequel setelah tanda kurung, misal `f(a)[3:0]`.
    if let (Token::Ident(_), Some(&Token::LParen)) =
        (&tokens[0].token, tokens.get(1).map(|t| &t.token))
    {
        let (call, consumed) = parse_function_call(tokens)?;
        base = call;
        idx = consumed;
    }

    // Rangkaian seleksi boleh bertumpuk, misal `mem[3][2:0]`.
    while idx < tokens.len() && tokens[idx].token == Token::LBracket {
        let span = tokens[idx].span;
        let (indeks, consumed) = parse_select_index(&tokens[idx + 1..])?;
        idx += 1 + consumed;
        if idx >= tokens.len() || tokens[idx].token != Token::RBracket {
            return Err("expected ']' closing select".to_string());
        }
        idx += 1;
        base = match indeks {
            Indeks::Konstanta(msb, lsb) => Expr::Select {
                base: Box::new(base),
                msb,
                lsb,
                span,
            },
            Indeks::Dinamis(index) => Expr::IndexDynamic {
                base: Box::new(base),
                index: Box::new(index),
                span,
            },
        };
    }
    Ok((base, idx))
}

/// Parse type cast ke tipe bertipe `nama_tipe'(ekspresi)` (LRM §6.14).
///
/// Token pertama sudah dipastikan identifier dan token kedua `Token::Cast`.
fn parse_cast(tokens: &[SpannedToken]) -> Result<(Expr, usize), String> {
    let span = tokens[0].span;
    let type_name = match &tokens[0].token {
        Token::Ident(n) => n.clone(),
        other => return Err(format!("expected nama tipe pada cast, ditemukan {other:?}")),
    };
    parse_cast_body(type_name, tokens, span)
}

/// Parse `nama_tipe'(ekspresi)` setelah nama tipenya sudah diketahui.
fn parse_cast_body(
    type_name: String,
    tokens: &[SpannedToken],
    span: sv_lexer::span::Span,
) -> Result<(Expr, usize), String> {
    let mut idx = 2; // lewati identifier dan tanda `'`
    if tokens.get(idx).map(|t| &t.token) != Some(&Token::LParen) {
        return Err("expected '(' setelah tanda cast".to_string());
    }
    idx += 1;
    let (operand, consumed) = parse_expression(&tokens[idx..])?;
    idx += consumed;
    if tokens.get(idx).map(|t| &t.token) != Some(&Token::RParen) {
        return Err("expected ')' menutup cast".to_string());
    }
    idx += 1;
    Ok((make_cast(type_name, Box::new(operand), span), idx))
}

/// Bangun node cast sesuai jenis tipenya.
///
/// Nama tipe bawaan menghasilkan `BuiltinCast` yang sudah tahu lebarnya, dan
/// `None` menghasilkan `SizeCast` yang hanya mengubah lebar. Keduanya dipisah
/// karena perlakuan signedness-nya berbeda: tipe bawaan punya signedness
/// sendiri, sedangkan size cast mengikuti operand (LRM §6.14).
fn make_cast(type_name: String, operand: Box<Expr>, span: sv_lexer::span::Span) -> Expr {
    match tipe_bawaan(&type_name) {
        Some((width, signed)) => Expr::BuiltinCast {
            type_name,
            width,
            signed,
            operand,
            span,
        },
        None => Expr::Cast {
            type_name,
            operand,
            span,
        },
    }
}

/// Lebar dan signedness tipe bawaan SystemVerilog (LRM §6.16, Tabel 6-22).
///
/// `logic` dan `bit` tanpa `[msb:lsb]` adalah vektor 1 bit. Tabel ini diverifikasi
/// terhadap iverilog 12.0: `byte'(8'hA5)` dicetak `%0d` sebagai `-91` (jadi
/// bertanda), sedangkan `time'(-1)` sebagai 18446744073709551615 (unsigned).
fn tipe_bawaan(nama: &str) -> Option<(u32, bool)> {
    Some(match nama {
        // Bertanda: `byte`, `shortint`, `int`, `longint`, `integer`.
        // Tanpa tanda: `bit`, `logic`, `time`.
        "bit" => (1, false),
        "logic" => (1, false),
        "byte" => (8, true),
        "shortint" => (16, true),
        "int" => (32, true),
        "longint" => (64, true),
        "integer" => (32, true),
        "time" => (64, false),
        _ => return None,
    })
}

/// Nama sumber dari token tipe bawaan, untuk pesan dan node AST.
fn nama_tipe_bawaan(token: &Token) -> &'static str {
    match token {
        Token::Bit => "bit",
        Token::Logic => "logic",
        Token::Byte => "byte",
        Token::ShortInt => "shortint",
        Token::Int => "int",
        Token::LongInt => "longint",
        Token::Integer => "integer",
        Token::Time => "time",
        other => unreachable!("nama_tipe_bawaan dipanggil dengan {other:?}"),
    }
}

/// Parse pemanggilan function `nama(arg, ...)` (LRM §13.4).
fn parse_function_call(tokens: &[SpannedToken]) -> Result<(Expr, usize), String> {
    let span = tokens[0].span;
    let name = match &tokens[0].token {
        Token::Ident(n) => n.clone(),
        other => return Err(format!("expected nama function, ditemukan {other:?}")),
    };
    let mut idx = 1; // lewati identifier
    if tokens.get(idx).map(|t| &t.token) != Some(&Token::LParen) {
        return Err("expected '(' setelah nama function".to_string());
    }
    idx += 1;

    let mut args = Vec::new();
    while idx < tokens.len() && tokens[idx].token != Token::RParen {
        let (arg, consumed) = parse_expression(&tokens[idx..])?;
        args.push(arg);
        idx += consumed;
        if tokens.get(idx).map(|t| &t.token) == Some(&Token::Comma) {
            idx += 1;
        }
    }
    if tokens.get(idx).map(|t| &t.token) != Some(&Token::RParen) {
        return Err("expected ')' closing argumen function".to_string());
    }
    idx += 1;

    Ok((Expr::FunctionCall { name, args, span }, idx))
}

/// Bentuk hasil parsing `[ ... ]` (LRM §7.8).
enum Indeks {
    /// `[N]` atau `[MSB:LSB]` dengan kedua batas konstanta.
    Konstanta(u32, u32),
    /// `[expr]` dengan `expr` non-konstanta; hanya untuk baca.
    Dinamis(Expr),
}

/// Parse isi `[ ... ]` setelah kurung siku pertama sudah dimakan.
///
/// Nilai balik `consumed` menghitung token indeks saja, belum `]`. Isi dibaca
/// sebagai satu ekspresi dulu lalu bentuknya ditentukan dari token penutupnya,
/// sehingga batas part-select boleh ekspresi apa pun — bukan hanya literal di
/// posisi kedua. Kalau tidak begitu, `a[W-1:0]` (idiom RTL paling umum) salah
/// rute ke jalur indeks dinamis dan gagal dengan pesan "expected ']'".
fn parse_select_index(tokens: &[SpannedToken]) -> Result<(Indeks, usize), String> {
    let closing = let_span(tokens).ok_or_else(|| "expected ']' closing select".to_string())?;

    // Bentuk kosong dan batas kosong ditolak lebih dulu agar pesan errornya
    // menyebut fitur yang ditolak, bukan error internal dari `parse_expression`.
    if tokens[0].token == Token::RBracket {
        return Err("kurung siku seleksi kosong `[]` belum didukung".to_string());
    }
    if tokens[0].token == Token::Colon {
        return Err("part-select dengan batas bawah kosong `[:lsb]` belum didukung".to_string());
    }

    // Wildcard indeks `a[*]` hanya sah di `$sampled`/signature argument (LRM §20.11),
    // bukan di seleksi biasa.
    if tokens[0].token == Token::Star {
        return Err("wildcard indeks `a[*]` belum didukung di seleksi".to_string());
    }
    // Indexed part-select `[base +: width]` (LRM §7.8) dikenali dari token
    // sebelum parsing: kalau ditunggu, ekspresi `b +` lalu `:` menggagalkan
    // parse dengan pesan "expected identifier or number" yang tidak menjelaskan.
    if let Some(pos) = (1..closing).find(|&i| tokens[i].token == Token::Colon) {
        let arah = match tokens[pos - 1].token {
            Token::Plus => Some("+"),
            Token::Minus => Some("-"),
            _ => None,
        };
        if let Some(arah) = arah {
            return Err(format!(
                "indexed part-select `a[base {arah}: width]` belum didukung"
            ));
        }
    }

    let (expr, consumed) = parse_expression(&tokens[..closing])?;
    let setelah = tokens.get(consumed).map(|t| &t.token);

    match setelah {
        None => Err("expected ']' closing select".to_string()),
        // Part-select `[msb:lsb]`: kedua batas harus konstanta karena helper
        // runtime-nya hanya menangani indeks tunggal.
        Some(Token::Colon) => {
            if consumed + 1 >= closing {
                return Err(
                    "part-select dengan batas atas kosong `[msb:]` belum didukung".to_string(),
                );
            }
            let msb = batas_part_select_konstan(&expr, "atas")?;
            let (lsb_expr, lsb_consumed) = parse_expression(&tokens[consumed + 1..closing])?;
            if consumed + 1 + lsb_consumed != closing {
                return Err(format!(
                    "sisa token tak dikenal pada part-select: {:?}",
                    tokens[consumed + 1 + lsb_consumed..closing]
                        .iter()
                        .map(|t| &t.token)
                        .collect::<Vec<_>>()
                ));
            }
            let lsb = batas_part_select_konstan(&lsb_expr, "bawah")?;
            Ok((Indeks::Konstanta(msb, lsb), closing))
        }
        Some(Token::RBracket) => {
            // Bentuk tunggal: literal berarti select konstan, selain itu dinamis.
            match indeks_konstan(&expr) {
                Some(nilai) => Ok((Indeks::Konstanta(nilai, nilai), consumed)),
                None => Ok((Indeks::Dinamis(expr), consumed)),
            }
        }
        Some(other) => Err(format!("expected ']' closing select, ditemukan {other:?}")),
    }
}

/// Batas part-select harus literal; helper runtime hanya bisa menghitung irisan
/// statis. Pesannya menyebut posisi batas supaya dua sisi bisa dibedakan.
fn batas_part_select_konstan(expr: &Expr, posisi: &str) -> Result<u32, String> {
    indeks_konstan(expr).ok_or_else(|| {
        format!(
            "part-select dengan batas {posisi} dinamis belum didukung; \
             helper runtime hanya bisa menghitung irisan statis, \
             misal `a[7:0]`"
        )
    })
}

/// Nilai indeks harus literal non-negatif dan diketahui; indeks parametrik belum
/// didukung. Literal `x`/`z` (`1'bx`) dianggap tidak diketahui, bukan 0 —
/// kalau tidak, `a[1'bx]` diam-diam memilih bit 0.
fn indeks_konstan(expr: &Expr) -> Option<u32> {
    match expr {
        Expr::Number(n) => u32::try_from(*n).ok(),
        Expr::Sized {
            value,
            unknown_mask,
            zmask,
            ..
        } => {
            if *unknown_mask != 0 || *zmask != 0 {
                return None;
            }
            u32::try_from(*value).ok()
        }
        _ => None,
    }
}

/// Cari posisi `]` yang menutup `[` pembuka, memperhitungkan kurung siku
/// bersarang. Tanpa hitungan kedalaman, `a[b[0]]` terpotong di `]` milik `b[0]`
/// dan ekspresi indeksnya gagal dipecah.
///
/// Slice yang diberikan **sudah tanpa** `[` pembukanya (sudah dimakan pemanggil),
/// jadi kedalaman mulai dari 1.
fn let_span(tokens: &[SpannedToken]) -> Option<usize> {
    let mut depth = 1usize;
    for (i, t) in tokens.iter().enumerate() {
        match t.token {
            Token::LBracket => depth += 1,
            Token::RBracket => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

fn parse_primary(tokens: &[SpannedToken]) -> Result<(Expr, usize), String> {
    if tokens.is_empty() {
        return Err("expected primary expression".to_string());
    }

    match &tokens[0].token {
        Token::Ident(n) => {
            // LRM §6.14: `nama_tipe'(ekspresi)` adalah type cast, bukan
            // pemanggilan function. Keduanya diawali identifier dan `(`,
            // jadi pembedanya adalah tanda `'` di antaranya.
            if matches!(tokens.get(1).map(|t| &t.token), Some(&Token::Cast)) {
                let (cast, consumed) = parse_cast(tokens)?;
                return Ok((cast, consumed));
            }
            Ok((Expr::ident(n.clone(), tokens[0].span), 1))
        }
        // LRM §6.14: nama tipe bawaan adalah keyword, jadi tidak pernah
        // sampai sebagai `Token::Ident`. Tanpa cabang ini `integer'(x)` —
        // bentuk yang paling sering dipakai setelah typedef — gagal di lexer
        // karena `'` dibaca sebagai awal based literal yang basis-nya `(`.
        Token::Logic
        | Token::Bit
        | Token::Integer
        | Token::Int
        | Token::Byte
        | Token::ShortInt
        | Token::LongInt
        | Token::Time
            if matches!(tokens.get(1).map(|t| &t.token), Some(&Token::Cast)) =>
        {
            let span = tokens[0].span;
            let nama = nama_tipe_bawaan(&tokens[0].token);
            parse_cast_body(nama.to_string(), tokens, span)
        }
        // LRM §6.14: `16'(x)` adalah size cast — lebar eksplisit tanpa nama.
        // Signedness-nya ikut operand, jadi baru bisa ditentukan saat lowering.
        Token::SizeCast(width) => {
            let span = tokens[0].span;
            if tokens.get(1).map(|t| &t.token) != Some(&Token::LParen) {
                return Err(format!(
                    "size cast {width}' butuh operand dalam kurung, contoh {width}'(x)"
                ));
            }
            let mut idx = 2;
            let (operand, consumed) = parse_expression(&tokens[idx..])?;
            idx += consumed;
            if tokens.get(idx).map(|t| &t.token) != Some(&Token::RParen) {
                return Err("expected ')' menutup size cast".to_string());
            }
            idx += 1;
            Ok((
                Expr::SizeCast {
                    width: *width,
                    operand: Box::new(operand),
                    span,
                },
                idx,
            ))
        }
        // LRM §6.14: `signed'(x)` dan `unsigned'(x)` hanya mengubah tanda.
        Token::Signed | Token::Unsigned
            if matches!(tokens.get(1).map(|t| &t.token), Some(&Token::Cast)) =>
        {
            let signed = tokens[0].token == Token::Signed;
            let span = tokens[0].span;
            let mut idx = 2; // lewati keyword dan tanda `'`()
            if tokens.get(idx).map(|t| &t.token) != Some(&Token::LParen) {
                return Err("expected '(' setelah signed'".to_string());
            }
            idx += 1;
            let (operand, consumed) = parse_expression(&tokens[idx..])?;
            idx += consumed;
            if tokens.get(idx).map(|t| &t.token) != Some(&Token::RParen) {
                return Err("expected ')' menutup sign cast".to_string());
            }
            idx += 1;
            Ok((
                Expr::SignCast {
                    signed,
                    operand: Box::new(operand),
                    span,
                },
                idx,
            ))
        }
        // LRM §6.14: `'` tanpa lebar eksplisit bukan size cast yang sah, dan
        // tanpa `s`/`b`/`o`/`d`/`h` juga bukan literal. Pesannya menyebut
        // bentuk yang benar.
        Token::Cast => {
            Err("size cast butuh lebar eksplisit, contoh 16'(x), bukan '(x)".to_string())
        }
        Token::Number(n) => Ok((Expr::Number(*n), 1)),
        Token::BasedNumber {
            value,
            width,
            signed,
            unknown_mask,
            zmask,
        } => Ok((
            Expr::Sized {
                value: *value,
                width: *width,
                signed: *signed,
                unknown_mask: *unknown_mask,
                zmask: *zmask,
            },
            1,
        )),
        Token::LParen => {
            let (inner, mut consumed) = parse_expression(&tokens[1..])?;
            consumed += 1;
            if consumed >= tokens.len() || tokens[consumed].token != Token::RParen {
                return Err("expected ')'".to_string());
            }
            Ok((inner, consumed + 1))
        }
        Token::LBrace => parse_brace(tokens),
        // LRM §20: system function `$time` bernilai waktu simulasi berjalan.
        // LRM §11.4.7 / §20: `$signed(x)` dan `$unsigned(x)` identik dengan
        // `signed'(x)` / `unsigned'(x)` pada LRM §6.14 — hanya mengubah
        // signedness, lebar tetap. Nama keduanya keyword, jadi muncul sebagai
        // `SystemTask` dan bukan `Ident`.
        Token::SystemTask(nama) if matches!(nama.as_str(), "signed" | "unsigned") => {
            let span = tokens[0].span;
            if tokens.get(1).map(|t| &t.token) != Some(&Token::LParen) {
                return Err(format!("${nama} harus diikuti ekspresi dalam kurung"));
            }
            let (operand, consumed) = parse_expression(&tokens[2..])?;
            let mut idx = 2 + consumed;
            if tokens.get(idx).map(|t| &t.token) != Some(&Token::RParen) {
                return Err(format!("expected ')' menutup ${nama}"));
            }
            idx += 1;
            Ok((
                Expr::SignCast {
                    signed: nama == "signed",
                    operand: Box::new(operand),
                    span,
                },
                idx,
            ))
        }
        Token::SystemTask(nama) if nama == "time" => Ok((
            Expr::SystemTime {
                // `timescale` modul belum diketahui saat parsing;
                // `sv_ast::delay_unit::terapkan_module` yang mengisinya.
                unit: sv_ast::time_unit::TimeUnit::default_unit(),
                span: tokens[0].span,
            },
            1,
        )),
        // LRM §20: `$bits(x)` adalah system function yang mengembalikan
        // lebar bit operandnya sebagai konstanta.
        Token::SystemTask(nama) if nama == "bits" => {
            let span = tokens[0].span;
            let mut idx = 1;
            if tokens.get(idx).map(|t| &t.token) != Some(&Token::LParen) {
                return Err("expected '(' setelah $bits".to_string());
            }
            idx += 1;
            let (operand, consumed) = parse_expression(&tokens[idx..])?;
            idx += consumed;
            if tokens.get(idx).map(|t| &t.token) != Some(&Token::RParen) {
                return Err("expected ')' menutup argumen $bits".to_string());
            }
            idx += 1;
            Ok((
                Expr::Bits {
                    operand: Box::new(operand),
                    span,
                },
                idx,
            ))
        }
        _ => Err(format!(
            "expected identifier or number, ditemukan {:?}",
            tokens[0].token
        )),
    }
}

/// Parse `{...}`: replikasi `{N{expr}}` atau concatenation `{a, b, ...}`.
fn parse_brace(tokens: &[SpannedToken]) -> Result<(Expr, usize), String> {
    let span = tokens[0].span;

    // LRM §11.8.1 vs §11.8.2: `{ a, b }` (concatenation) dan
    // `{ n { x } }` (replication) sama-sama diawali kurung kurawal, dan
    // keduanya bisa muncul di ekspresi yang sama — `{ {2{a}}, b }` adalah
    // concatenation dengan item pertamanya replication.
    //
    // Karena itu replication dicoba lebih dulu (count boleh ekspresi konstanta
    // seperti `{WIDTH{1'b1}}` dengan `WIDTH` parameter modul, idiom RTL paling
    // umum), lalu kalau gagal dikembalikan ke jalur concatenation.
    match try_parse_replication(tokens, span) {
        Ok(hasil) => Ok(hasil),
        Err(e) => match try_parse_concatenation(tokens, span) {
            Ok(hasil) => Ok(hasil),
            // Kedua jalur gagal. Pesan dari replication lebih informatif saat
            // bentuknya memangMirip replication — `{2{a, 1'b0}}` tidak sah SV
            // dan iverilog juga menolaknya.
            Err(_) => Err(e),
        },
    }
}

/// Coba parse `{ count { value } }` (LRM §11.8.2).
fn try_parse_replication(
    tokens: &[SpannedToken],
    span: sv_lexer::span::Span,
) -> Result<(Expr, usize), String> {
    // Count boleh ekspresi apa pun; setelahnya harus ada `{` yang menandai awal
    // operand. Ini yang membedakan `{n{x}}` dari `{a, b}`.
    let (_, panjang_count) = parse_expression(&tokens[1..])?;
    let nilai_mulai = 1 + panjang_count;
    if tokens.get(nilai_mulai).map(|t| &t.token) != Some(&Token::LBrace) {
        return Err("bukan replication: tidak ada '{' setelah count".to_string());
    }
    // Count diparse sebagai ekspresi penuh supaya identifier parameter diterima;
    // nilainya di-resolve elaborator lewat tabel parameter.
    let (count, _) = parse_expression(&tokens[1..])?;
    let (value, mut consumed) = parse_expression(&tokens[nilai_mulai + 1..])?;
    // LRM §11.8.2: bentuknya `{ constant_expression { concatenation } }` —
    // isinya concatenation, bukan replication lagi. `{2{2{a}}}` karena itu
    // tidak sah; iverilog juga menolaknya dengan `syntax error`.
    if matches!(value, Expr::Replicate { .. }) {
        return Err("replication di dalam replication tidak diizinkan".to_string());
    }
    // '{', count, '{' lalu operand.
    consumed += nilai_mulai + 1;
    if tokens.get(consumed).map(|t| &t.token) != Some(&Token::RBrace) {
        return Err("expected '}' closing replication".to_string());
    }
    consumed += 1;
    if tokens.get(consumed).map(|t| &t.token) != Some(&Token::RBrace) {
        return Err("expected '}' closing replication".to_string());
    }
    Ok((
        Expr::Replicate {
            count: Box::new(count),
            value: Box::new(value),
            span,
        },
        consumed + 1,
    ))
}

/// Coba parse `{ a, b, ... }` (LRM §11.8.1).
fn try_parse_concatenation(
    tokens: &[SpannedToken],
    span: sv_lexer::span::Span,
) -> Result<(Expr, usize), String> {
    // Concatenation: satu atau lebih item **dipisah koma** (LRM §11.8.1).
    let mut items = Vec::new();
    let mut idx = 1;
    loop {
        if idx >= tokens.len() {
            return Err("expected '}' closing concatenation".to_string());
        }
        if tokens[idx].token == Token::RBrace {
            break;
        }
        let (item, consumed) = parse_expression(&tokens[idx..])?;
        items.push(item);
        idx += consumed;
        if idx >= tokens.len() {
            return Err("expected '}' closing concatenation".to_string());
        }
        match &tokens[idx].token {
            Token::RBrace => break,
            // Koma wajib di antara item. Tanpa pemeriksaan ini `{2{2{a}}}` —
            // replication lapsan yang tidak sah — diterima diam-diam sebagai
            // concatenation dengan dua item dan menghasilkan nilai yang salah.
            Token::Comma => idx += 1,
            other => {
                return Err(format!(
                    "expected koma atau penutup kurung kurawal pada concatenation, \
                     ditemukan {other:?}"
                ))
            }
        }
    }
    if items.is_empty() {
        return Err("concatenation kosong tidak sah".to_string());
    }
    idx += 1; // '}'
    Ok((Expr::Concat { items, span }, idx))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sv_lexer::lex;

    fn parse(src: &str) -> Expr {
        let tokens = lex(src).expect("lex");
        let (expr, _) = parse_expression(&tokens).expect("parse");
        expr
    }

    /// Nama identifier, mengabaikan span supaya assertion tidak rapuh.
    /// Span diuji terpisah oleh `ident_menyimpan_span_token`.
    pub(super) fn nama(expr: &Expr) -> &str {
        match expr {
            Expr::Ident { name, .. } => name,
            other => panic!("harus identifier, dapat {other:?}"),
        }
    }

    #[test]
    fn parse_number() {
        assert_eq!(parse("42"), Expr::Number(42));
    }

    #[test]
    fn parse_binary() {
        match parse("a & b") {
            Expr::Binary { op, lhs, rhs, .. } => {
                assert_eq!(op, BinaryOp::And);
                assert_eq!(nama(&lhs), "a");
                assert_eq!(nama(&rhs), "b");
            }
            _ => panic!("expected binary expression"),
        }
    }

    #[test]
    fn tambah_kalah_dari_and() {
        // `a & b + c` = `a & (b + c)` karena `+` lebih kuat.
        match parse("a & b + c") {
            Expr::Binary { op, rhs, .. } => {
                assert_eq!(op, BinaryOp::And);
                assert!(matches!(
                    *rhs,
                    Expr::Binary {
                        op: BinaryOp::Add,
                        ..
                    }
                ));
            }
            _ => panic!("expected binary expression"),
        }
    }

    #[test]
    fn rantai_kiri() {
        // `a & b | c` = `(a & b) | c`.
        match parse("a & b | c") {
            Expr::Binary { op, lhs, .. } => {
                assert_eq!(op, BinaryOp::Or);
                assert!(matches!(
                    *lhs,
                    Expr::Binary {
                        op: BinaryOp::And,
                        ..
                    }
                ));
            }
            _ => panic!("expected binary expression"),
        }
    }

    #[test]
    fn banding_dan_geser() {
        assert!(matches!(
            parse("a <= b"),
            Expr::Binary {
                op: BinaryOp::Le,
                ..
            }
        ));
        assert!(matches!(
            parse("a << b"),
            Expr::Binary {
                op: BinaryOp::Shl,
                ..
            }
        ));
        assert!(matches!(
            parse("a == b && c"),
            Expr::Binary {
                op: BinaryOp::LogAnd,
                ..
            }
        ));
    }

    #[test]
    fn geser_aritmetik_diparse_terpisah_dari_logis() {
        // LRM §11.4.10: `>>>` mengisi bit sign, jadi tidak boleh menyatu
        // dengan `>>`. `<<<` identik dengan `<<`.
        assert!(matches!(
            parse("a >>> b"),
            Expr::Binary {
                op: BinaryOp::Sar,
                ..
            }
        ));
        assert!(matches!(
            parse("a <<< b"),
            Expr::Binary {
                op: BinaryOp::Shl,
                ..
            }
        ));
        // Presedensi sama dengan `>>`: `a >>> b + 1` = `a >>> (b + 1)`.
        assert!(matches!(
            parse("a >>> b + 1"),
            Expr::Binary {
                op: BinaryOp::Sar,
                ..
            }
        ));
    }

    #[test]
    fn parse_parenthesized() {
        assert_eq!(nama(&parse("(opcode)")), "opcode");
    }

    // BUG: `AstExpr::Ident` tidak menyimpan span token, sehingga elaborator
    // melaporkan `undefined signal` pada posisi dummy `line 1 col 1` alih-alih
    // menunjuk identifier yang benar (AGENTS.md aturan 3).
    #[test]
    fn ident_menyimpan_span_token() {
        let tokens = lex("  a & b").expect("lex");
        let (expr, _) = parse_expression(&tokens).expect("parse");
        match expr {
            Expr::Binary { lhs, rhs, .. } => {
                let Expr::Ident { span: sl, .. } = *lhs else {
                    panic!("lhs harus identifier");
                };
                let Expr::Ident { span: sr, .. } = *rhs else {
                    panic!("rhs harus identifier");
                };
                let asli_a = tokens
                    .iter()
                    .find(|t| t.token == sv_lexer::token::Token::Ident("a".into()))
                    .expect("token a");
                let asli_b = tokens
                    .iter()
                    .find(|t| t.token == sv_lexer::token::Token::Ident("b".into()))
                    .expect("token b");
                assert_eq!(sl, asli_a.span, "span `a` harus mengikuti tokennya");
                assert_eq!(sr, asli_b.span, "span `b` harus mengikuti tokennya");
                assert_ne!(sl, sv_lexer::span::Span::dummy());
            }
            other => panic!("harus binary, dapat {other:?}"),
        }
    }

    // --- sized literal (BUG-1) ---

    #[test]
    fn sized_literal_astar_memakai_lebar() {
        assert_eq!(
            parse("8'hFF"),
            Expr::Sized {
                value: 0xFF,
                width: 8,
                signed: false,
                unknown_mask: 0,
                zmask: 0,
            }
        );
    }

    #[test]
    fn sized_literal_dalam_ekspresi() {
        match parse("a == 4'd5") {
            Expr::Binary { rhs, .. } => assert_eq!(
                *rhs,
                Expr::Sized {
                    value: 5,
                    width: 4,
                    signed: false,
                    unknown_mask: 0,
                    zmask: 0,
                }
            ),
            _ => panic!("expected binary expression"),
        }
    }

    #[test]
    fn sized_literal_sebagai_case_label() {
        let tokens = lex("8'b1010").unwrap();
        let (label, consumed) = parse_expression(&tokens).unwrap();
        assert_eq!(consumed, 1);
        assert!(matches!(label, Expr::Sized { width: 8, .. }));
    }

    // --- unary & reduction (BUG-3, BUG-4) ---

    #[test]
    fn unary_bitwise_not() {
        match parse("~a") {
            Expr::Unary { op, operand, .. } => {
                assert_eq!(op, UnaryOp::BitNot);
                assert_eq!(nama(&operand), "a");
            }
            _ => panic!("expected unary expression"),
        }
    }

    #[test]
    fn unary_logical_not_dan_negasi() {
        assert!(matches!(
            parse("!a"),
            Expr::Unary {
                op: UnaryOp::LogNot,
                ..
            }
        ));
        assert!(matches!(
            parse("-a"),
            Expr::Unary {
                op: UnaryOp::BitNeg,
                ..
            }
        ));
    }

    #[test]
    fn unary_plus_adalah_identitas() {
        assert_eq!(nama(&parse("+a")), "a");
    }

    #[test]
    fn reduction_satu_karakter() {
        for (src, expected) in [
            ("&a", UnaryOp::RedAnd),
            ("|a", UnaryOp::RedOr),
            ("^a", UnaryOp::RedXor),
        ] {
            match parse(src) {
                Expr::Unary { op, .. } => assert_eq!(op, expected, "sumber {src}"),
                other => panic!("sumber {src} menghasilkan {other:?}"),
            }
        }
    }

    #[test]
    fn reduction_dua_karakter() {
        for (src, expected) in [
            ("~&a", UnaryOp::RedNand),
            ("~|a", UnaryOp::RedNor),
            ("~^a", UnaryOp::RedXnor),
        ] {
            match parse(src) {
                Expr::Unary { op, .. } => assert_eq!(op, expected, "sumber {src}"),
                other => panic!("sumber {src} menghasilkan {other:?}"),
            }
        }
    }

    #[test]
    fn reduction_dua_karakter_bukan_nesting() {
        // `~^a` adalah satu operator XNOR atas `a`, bukan `~(^a)`.
        match parse("~^a") {
            Expr::Unary { op, operand, .. } => {
                assert_eq!(op, UnaryOp::RedXnor);
                assert_eq!(nama(&operand), "a");
            }
            other => panic!("sumber menghasilkan {other:?}"),
        }
    }

    #[test]
    fn reduction_dua_karakter_konsumsi_dua_token() {
        // Bila hanya satu token yang dilewati, `4` akan tertinggal dan `a`
        // tak akan ikut ter konsumsi sebagai operand.
        let tokens = lex("~^4'b0111").unwrap();
        let (expr, consumed) = parse_expression(&tokens).unwrap();
        assert_eq!(consumed, tokens.len(), "harus mengonsumsi semua token");
        match expr {
            Expr::Unary { op, operand, .. } => {
                assert_eq!(op, UnaryOp::RedXnor);
                assert!(matches!(
                    *operand,
                    Expr::Sized {
                        value: 7,
                        width: 4,
                        ..
                    }
                ));
            }
            other => panic!("sumber menghasilkan {other:?}"),
        }
    }

    #[test]
    fn unary_kuat_lebih_dari_binary() {
        // `~a & b` = `(~a) & b`.
        match parse("~a & b") {
            Expr::Binary { op, lhs, .. } => {
                assert_eq!(op, BinaryOp::And);
                assert!(matches!(
                    *lhs,
                    Expr::Unary {
                        op: UnaryOp::BitNot,
                        ..
                    }
                ));
            }
            _ => panic!("expected binary expression"),
        }
    }

    #[test]
    fn tilde_bukan_reduksi_bila_tanpa_operan_bitwise() {
        // `~a` harus bitwise not, bukan reduction NAND.
        match parse("~a") {
            Expr::Unary { op, .. } => assert_eq!(op, UnaryOp::BitNot),
            _ => panic!("expected unary expression"),
        }
    }

    // --- ternary (BUG-5) ---

    #[test]
    fn ternary_kodusial_kecil() {
        match parse("a ? 8'hFF : 8'h00") {
            Expr::Ternary {
                condition,
                when_true,
                when_false,
                ..
            } => {
                assert_eq!(nama(&condition), "a");
                assert!(matches!(*when_true, Expr::Sized { value: 0xFF, .. }));
                assert!(matches!(*when_false, Expr::Sized { value: 0x00, .. }));
            }
            other => panic!("expected ternary, dapat {other:?}"),
        }
    }

    #[test]
    fn ternary_kalah_dari_binary() {
        // `a + b ? c : d` harus parse `(a + b) ? c : d`.
        match parse("a + b ? c : d") {
            Expr::Ternary { condition, .. } => assert!(matches!(
                *condition,
                Expr::Binary {
                    op: BinaryOp::Add,
                    ..
                }
            )),
            other => panic!("expected ternary, dapat {other:?}"),
        }
    }

    #[test]
    fn ternary_nested_sebagai_cabang() {
        match parse("a ? b ? 1 : 2 : 3") {
            Expr::Ternary { when_true, .. } => assert!(matches!(*when_true, Expr::Ternary { .. })),
            other => panic!("expected nested ternary, dapat {other:?}"),
        }
    }

    // --- select (BUG-6) ---

    #[test]
    fn bit_select_msb_sama_lsb() {
        match parse("a[0]") {
            Expr::Select { base, msb, lsb, .. } => {
                assert_eq!(nama(&base), "a");
                assert_eq!((msb, lsb), (0, 0));
            }
            other => panic!("expected select, dapat {other:?}"),
        }
    }

    #[test]
    fn part_select_msb_dan_lsb() {
        match parse("a[7:4]") {
            Expr::Select { msb, lsb, .. } => assert_eq!((msb, lsb), (7, 4)),
            other => panic!("expected select, dapat {other:?}"),
        }
    }

    #[test]
    fn select_dalam_ekspresi_binary() {
        match parse("a[3:0] + b") {
            Expr::Binary { op, lhs, .. } => {
                assert_eq!(op, BinaryOp::Add);
                assert!(matches!(*lhs, Expr::Select { .. }));
            }
            other => panic!("expected binary, dapat {other:?}"),
        }
    }

    #[test]
    fn select_bertumpuk_konsumsi_semua() {
        let tokens = lex("a[3:0][1]").unwrap();
        let (_, consumed) = parse_expression(&tokens).unwrap();
        assert_eq!(consumed, tokens.len());
    }

    #[test]
    fn indeks_non_literal_jadi_indeks_dinamis() {
        // LRM §7.8: `a[i]` dengan `i` non-konstanta sah untuk dibaca. Dulu
        // parser menolaknya dengan "indeks harus literal", yang membuat
        // register file dengan indeks variabel mustahil dipakai.
        let tokens = lex("a[W]").unwrap();
        let (expr, _) = parse_expression(&tokens).expect("parse");
        match expr {
            Expr::IndexDynamic { base, index, .. } => {
                assert!(matches!(*base, Expr::Ident { .. }));
                assert!(matches!(*index, Expr::Ident { .. }));
            }
            other => panic!("expected IndexDynamic, dapat {other:?}"),
        }
    }

    #[test]
    fn indeks_selalu_konstan_tetap_select() {
        // Bentuk konstanta harus tetap `Select` supaya irisan statis dan
        // pemetaan array unpacked di elaborator tetap terpakai.
        let tokens = lex("a[3]").unwrap();
        let (expr, _) = parse_expression(&tokens).expect("parse");
        assert!(matches!(expr, Expr::Select { msb: 3, lsb: 3, .. }));
    }

    #[test]
    fn part_select_dengan_batas_dinamis_ditolak() {
        // Helper runtime hanya menangani indeks tunggal, jadi bentuk `a[hi:3]`
        // ditolak eksplisit — bukan diam-diam jadi bit-select.
        let tokens = lex("a[W:3]").unwrap();
        let err = parse_expression(&tokens).expect_err("batas dinamis ditolak");
        assert!(err.contains("batas atas dinamis"), "pesan: {err}");
    }

    // Bug: `a[` membuat `parse_select_index` dipanggil dengan slice kosong lalu
    // `&tokens[0..1]` panic. `parse_expression` adalah API publik, jadi slice
    // terpotong harus jadi error, bukan crash.
    #[test]
    fn kurung_siku_terbuka_tidak_panic() {
        for src in ["a[", "f(a[", "a[3", "a[b"] {
            let tokens = lex(src).unwrap();
            assert!(
                parse_expression(&tokens).is_err(),
                "{src} harus error, bukan panic"
            );
        }
    }

    // Bug: `a[W-1:0]` — idiom RTL paling umum — salah rute ke jalur indeks
    // dinamis karena titik dua dideteksi hanya di posisi token kedua, lalu
    // gagal dengan pesan "expected ']'" yang tidak masuk akal.
    #[test]
    fn part_select_dengan_batas_ekspresi_pesan_jelas() {
        for src in [
            "a[W-1:0]",
            "a[N-1:0]",
            "a[i+1:j+1]",
            "a[-1:0]",
            "a[3:0][W-1:0]",
        ] {
            let tokens = lex(src).unwrap();
            let err =
                parse_expression(&tokens).expect_err("batas ekspresi tanpa dukungan harus error");
            assert!(
                err.contains("part-select") && err.contains("dinamis"),
                "{src} dapat pesan tak berguna: {err}"
            );
        }
    }

    // Bug: `a[3:]` didiagnosis "batas dinamis" padahal batasnya kosong, dan
    // `a[:3]`/`a[]` membocorkan error internal `parse_expression`.
    #[test]
    fn batas_kosong_didiagnosis_sebagai_batas_kosong() {
        for (src, marinade) in [
            ("a[3:]", "batas atas kosong"),
            ("a[:3]", "batas bawah kosong"),
            ("a[:]", "batas bawah kosong"),
            ("a[]", "kosong"),
        ] {
            let tokens = lex(src).unwrap();
            let err = parse_expression(&tokens).expect_err("{src} harus error");
            assert!(err.contains(marinade), "{src} dapat pesan salah: {err}");
        }
    }

    // Bug: `a[b +: 4]` / `a[b -: 4]` (indexed part-select, LRM §7.8) dan `a[*]`
    // bocorkan error internal lexer/parser, bukan "belum didukung".
    #[test]
    fn bentuk_indeks_lain_didiagnosis_sebagai_belum_dukung() {
        for src in ["a[b +: 4]", "a[b -: 4]", "a[7 +: 4]", "a[*]"] {
            let tokens = lex(src).unwrap();
            let err = parse_expression(&tokens).expect_err("{src} harus error");
            assert!(
                err.contains("belum didukung"),
                "{src} dapat pesan tak berguna: {err}"
            );
        }
    }

    // Part-select konstan asli harus tetap jalan setelah parse_select_index
    // ditulis ulang berbasis pemindaian sampai `]`.
    #[test]
    fn part_select_konstan_tetap_diparse_setelah_rewrite() {
        for (src, msb, lsb) in [
            ("a[3:0]", 3, 0),
            ("a[7:4]", 7, 4),
            ("a[8'hF:8'h0]", 15, 0),
            ("a[3:0][1]", 3, 0),
        ] {
            let tokens = lex(src).unwrap();
            let (expr, _) = parse_expression(&tokens).unwrap_or_else(|e| panic!("{src}: {e}"));
            // Bentuk bertumpuk: sel yang pertama yang ada `Select`/`IndexDynamic`.
            let found = contains_select(&expr, msb, lsb);
            assert!(found, "{src} tidak menghasilkan Select({msb},{lsb})");
        }
    }

    ///Telusuri pohon expr mencari `Select` dengan batas tertentu (nested select).
    fn contains_select(expr: &Expr, msb: u32, lsb: u32) -> bool {
        match expr {
            Expr::Select {
                base,
                msb: m,
                lsb: l,
                ..
            } => (*m == msb && *l == lsb) || contains_select(base, msb, lsb),
            Expr::IndexDynamic { base, .. } => contains_select(base, msb, lsb),
            Expr::Binary { lhs, rhs, .. } => {
                contains_select(lhs, msb, lsb) || contains_select(rhs, msb, lsb)
            }
            Expr::Concat { items, .. } => items.iter().any(|i| contains_select(i, msb, lsb)),
            _ => false,
        }
    }

    // Bug: `closing` dicari sebagai `]` PERTAMA tanpa memperhitungkan kedalaman,
    // sehingga `a[b[0]]` terpotong di `]` milik `b[0]` dan gagal dengan pesan
    // "expected ']' closing select" — bentuk SV yang sah.
    #[test]
    fn kurung_siku_bersarang_dalam_indeks_diterima() {
        for src in ["a[b[0]]", "a[b[0]][1]", "a[b[1]][2:0]"] {
            let tokens = lex(src).unwrap();
            parse_expression(&tokens).unwrap_or_else(|e| panic!("{src} harus sah: {e}"));
        }
    }

    #[test]
    fn indeks_dengan_panggilan_fungsi_yang_mengindeks_sah() {
        let tokens = lex("a[f(b[3])]").unwrap();
        let (expr, _) = parse_expression(&tokens).expect("a[f(b[3])] harus sah");
        match expr {
            Expr::IndexDynamic { base, index, .. } => {
                assert!(matches!(*base, Expr::Ident { .. }), "base: {base:?}");
                assert!(
                    matches!(*index, Expr::FunctionCall { .. }),
                    "index harus FunctionCall: {index:?}"
                );
            }
            other => panic!("expected IndexDynamic, dapat {other:?}"),
        }
    }

    // Bug: `a[` (kurung tak tertutup) melaporkan "seleksi kosong `[]`" karena
    // `closing` jatuh ke `tokens.len()`. Dua kondisi berbeda butuh pesan beda.
    #[test]
    fn kurung_siku_terbuka_bukan_seleksi_kosong() {
        for src in ["a[", "f(a[", "a[b"] {
            let tokens = lex(src).unwrap();
            let err = parse_expression(&tokens).expect_err("{src} harus error");
            assert!(
                err.contains("']'"),
                "{src} salah dilaporkan sebagai seleksi kosong: {err}"
            );
        }
    }

    // Bug: `a[1'bx]` dilipat jadi `Select` dengan indeks 0 karena
    // `indeks_konstan` mengabaikan `unknown_mask`/`zmask`, sehingga bit 0
    // terpilih diam-diam. Indeks tak diketahui harus lewat jalur runtime
    // (bukan irisan statis) supaya LRM §7.8 bisa memberi `X`.
    #[test]
    fn indeks_dengan_literal_x_bukan_select_konstan() {
        for src in ["a[1'bx]", "a[4'bx]", "a[2'bz]", "a[1'bz]"] {
            let tokens = lex(src).unwrap();
            let (expr, _) = parse_expression(&tokens).unwrap_or_else(|e| panic!("{src}: {e}"));
            assert!(
                matches!(expr, Expr::IndexDynamic { .. }),
                "{src} harus jadi IndexDynamic, dapat {expr:?}"
            );
        }
    }

    // Part-select dengan batas `x` juga bukan irisan statis.
    #[test]
    fn part_select_dengan_batas_x_bukan_select_konstan() {
        let tokens = lex("a[3:1'bx]").unwrap();
        assert!(parse_expression(&tokens).is_err());
    }

    #[test]
    fn select_tidak_tertutup_ditolak() {
        let tokens = lex("a[3:0").unwrap();
        assert!(parse_expression(&tokens).is_err());
    }
}

#[cfg(test)]
mod brace_tests {
    use super::tests::nama;
    use super::*;
    use sv_lexer::lex;

    fn parse(src: &str) -> Expr {
        let tokens = lex(src).expect("lex");
        let (expr, _) = parse_expression(&tokens).expect("parse");
        expr
    }

    #[test]
    fn concat_dua_item() {
        match parse("{a, b}") {
            Expr::Concat { items, .. } => {
                assert_eq!(items.len(), 2);
                assert_eq!(nama(&items[0]), "a");
                assert_eq!(nama(&items[1]), "b");
            }
            other => panic!("expected concat, dapat {other:?}"),
        }
    }

    #[test]
    fn concat_tiga_item_dengan_literal() {
        match parse("{a, 2'b10, b}") {
            Expr::Concat { items, .. } => {
                assert_eq!(items.len(), 3);
                assert!(matches!(items[1], Expr::Sized { width: 2, .. }));
            }
            other => panic!("expected concat, dapat {other:?}"),
        }
    }

    #[test]
    fn concat_dalam_ekspresi_binary() {
        match parse("{a, b} + 1") {
            Expr::Binary { op, lhs, .. } => {
                assert_eq!(op, BinaryOp::Add);
                assert!(matches!(*lhs, Expr::Concat { .. }));
            }
            other => panic!("expected binary, dapat {other:?}"),
        }
    }

    #[test]
    fn replikasi_sekali() {
        match parse("{2{a}}") {
            Expr::Replicate { count, value, .. } => {
                // Count kini ekspresi, bukan literal: `{W{1'b1}}` harus bisa
                // diparse dan nilainya di-resolve elaborator.
                assert!(matches!(*count, Expr::Number(2)), "count: {count:?}");
                assert_eq!(nama(&value), "a");
            }
            other => panic!("expected replicate, dapat {other:?}"),
        }
    }

    #[test]
    fn replikasi_dengan_literal_berukuran() {
        match parse("{4{8'hFF}}") {
            Expr::Replicate { count, value, .. } => {
                assert!(matches!(*count, Expr::Number(4)), "count: {count:?}");
                assert!(matches!(*value, Expr::Sized { width: 8, .. }));
            }
            other => panic!("expected replicate, dapat {other:?}"),
        }
    }

    #[test]
    fn replikasi_dengan_count_berupa_identifier() {
        // BUG-9: `{W{1'b1}}` dengan `W` parameter modul adalah idiom RTL paling
        // umum. Dulu `W` tidak dikenali sebagai count sehingga `{W, 1'b1}`
        // dibaca sebagai concatenation — nilai yang salah tanpa error apa pun.
        match parse("{W{1'b1}}") {
            Expr::Replicate { count, value, .. } => {
                assert_eq!(nama(&count), "W");
                // Operandnya literal sized, bukan identifier.
                assert!(
                    matches!(*value, Expr::Sized { width: 1, .. }),
                    "operand: {value:?}"
                );
            }
            other => panic!("expected replicate, dapat {other:?}"),
        }
    }

    #[test]
    fn replikasi_dengan_count_berupa_ekspresi() {
        // Count boleh ekspresi konstanta apa pun (LRM §11.8.2).
        match parse("{W*2{1'b0}}") {
            Expr::Replicate { count, .. } => assert!(matches!(*count, Expr::Binary { .. })),
            other => panic!("expected replicate, dapat {other:?}"),
        }
    }

    #[test]
    fn concatenation_dengan_tanda_kurawal_tidak_berubah() {
        // `{a, b}` tetap concatenation — `find_lbrace` tidak boleh salah
        // menganggapnya replikasi.
        match parse("{a, b}") {
            Expr::Concat { items, .. } => assert_eq!(items.len(), 2),
            other => panic!("expected concat, dapat {other:?}"),
        }
    }

    #[test]
    fn replikasi_konsumsi_seluruh_token() {
        let tokens = lex("{3{2'b10}}").unwrap();
        let (_, consumed) = parse_expression(&tokens).unwrap();
        assert_eq!(consumed, tokens.len());
    }

    #[test]
    fn concat_konsumsi_seluruh_token() {
        let tokens = lex("{a, b, c}").unwrap();
        let (_, consumed) = parse_expression(&tokens).unwrap();
        assert_eq!(consumed, tokens.len());
    }

    #[test]
    fn concat_kosong_ditolak() {
        let tokens = lex("{}").unwrap();
        assert!(parse_expression(&tokens).is_err());
    }

    #[test]
    fn concat_tidak_tertutup_ditolak() {
        let tokens = lex("{a, b").unwrap();
        assert!(parse_expression(&tokens).is_err());
    }

    #[test]
    fn replikasi_nol_diterima_parser_ditolak_elaborator() {
        // Count kini ekspresi, jadi `0` tidak bisa diperiksa di parser lagi —
        // `0{a}` bisa saja bernilai nol setelah parameter di-resolve.
        // Pemeriksaan nihil ada di elaborator (lihat `assign_conv_test`/
        // test elaborator untuk rejection-nya).
        let tokens = lex("{0{a}}").unwrap();
        let (expr, _) = parse_expression(&tokens).expect("parser menerima count 0");
        assert!(matches!(expr, Expr::Replicate { .. }));
    }

    #[test]
    fn time_diparse_sebagai_ekspresi() {
        // LRM §20: `$time` system function yang bernilai waktu simulasi.
        let tokens = lex("$time").unwrap();
        let (expr, _) = parse_expression(&tokens).expect("parse");
        assert!(matches!(expr, Expr::SystemTime { .. }));
    }

    #[test]
    fn time_bisa_dijumlahkan_dengan_konstanta() {
        let tokens = lex("$time + 5").unwrap();
        let (expr, _) = parse_expression(&tokens).expect("parse");
        match expr {
            Expr::Binary { lhs, rhs, .. } => {
                assert!(matches!(*lhs, Expr::SystemTime { .. }));
                assert_eq!(*rhs, Expr::Number(5));
            }
            other => panic!("dapat {:?}", other),
        }
    }
}
