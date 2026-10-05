// Tanggung jawab: parse region `generate`/`endgenerate` (LRM §27).
use sv_ast::generate::{
    CaseGenerate, CompareOp, ForGenerate, GenerateCaseArm, GenerateItem, GenerateRegion, IfGenerate,
};
use sv_lexer::spanned::SpannedToken;
use sv_lexer::token::Token;

/// Parse satu region `generate ... endgenerate` mulai pada `start`.
///
/// Mengembalikan (region, jumlah token yang dikonsumsi dari `start`).
/// Genvar yang dipakai tiap loop for-generate dicatat di `genvars`.
///
/// `genvar_luar` berisi nama genvar yang sudah dideklarasikan di body modul
/// (`genvar i, j;`, LRM §27.3). Nama di situ boleh dipakai sebagai variabel
/// loop tanpa mengulang keyword `genvar`; nama lain harus memakai bentuk
/// inline `for (genvar k = ...)`.
///
/// Nilai balik sengaja relatif supaya pemanggil cukup `idx += hasil`.
pub fn parse_generate_region(
    tokens: &[SpannedToken],
    start: usize,
    genvar_luar: &[String],
) -> Result<(GenerateRegion, usize), String> {
    if tokens.get(start).map(|t| &t.token) != Some(&Token::Generate) {
        return Err("expected 'generate'".to_string());
    }
    let span = tokens[start].span;
    let mut idx = start + 1;

    let mut genvars: Vec<String> = genvar_luar.to_vec();
    let mut items: Vec<GenerateItem> = Vec::new();

    while tokens.get(idx).map(|t| &t.token) != Some(&Token::Endgenerate) {
        if idx >= tokens.len() {
            return Err("expected 'endgenerate'".to_string());
        }
        let (item, baru) = parse_generate_item(tokens, idx, &mut genvars, genvar_luar)?;
        items.push(item);
        idx = baru;
    }
    // Lewati 'endgenerate'.
    idx += 1;

    Ok((
        GenerateRegion {
            genvars,
            items,
            span,
        },
        idx - start,
    ))
}

/// Parse satu item di dalam region generate.
///
/// Mengembalikan indeks token pertama setelah item (absolut terhadap
/// `tokens`), supaya pemanggil bisa langsung mem advancements `idx`.
fn parse_generate_item(
    tokens: &[SpannedToken],
    start: usize,
    genvars: &mut Vec<String>,
    genvar_luar: &[String],
) -> Result<(GenerateItem, usize), String> {
    let token = &tokens[start].token;
    // LRM §27.3: loop generate dinyatakan dengan `for` dan genvar.
    if *token == Token::For {
        let (loop_generate, baru) = parse_for_generate(tokens, start, genvar_luar)?;
        // Genvar boleh datang dari keyword inline, dari deklarasi body modul,
        // atau dari loop sebelumnya. Selain itu variabelnya bukan genvar.
        // Bentuk inline `for (genvar i = ...)` menaruh keyword setelah `(`,
        // jadi posisinya `start + 2`, bukan `start + 1`.
        let inline = tokens.get(start + 2).map(|t| &t.token) == Some(&Token::Genvar);
        let dikenal = inline
            || genvar_luar.contains(&loop_generate.var)
            || genvars.contains(&loop_generate.var);
        if !dikenal {
            return Err(format!(
                "genvar '{}' belum dideklarasikan; tulis `genvar {};` di body modul \
                 atau pakai bentuk inline `for (genvar {} = ...)`",
                loop_generate.var, loop_generate.var, loop_generate.var
            ));
        }
        if !genvars.contains(&loop_generate.var) {
            genvars.push(loop_generate.var.clone());
        }
        return Ok((GenerateItem::For(Box::new(loop_generate)), baru));
    }

    // LRM §27.1: generate bersyarat dan `case`.
    // `parse_if_generate`/`parse_case_generate` mengembalikan jumlah token
    // relatif terhadap `start`, jadi harus ditambah `start` agar konsisten
    // dengan item lain yang memakai indeks absolut.
    if *token == Token::If {
        let (if_gen, consumed) = parse_if_generate(tokens, start, genvar_luar)?;
        return Ok((GenerateItem::If(Box::new(if_gen)), start + consumed));
    }
    if *token == Token::Case {
        let (case_gen, consumed) = parse_case_generate(tokens, start, genvar_luar)?;
        return Ok((GenerateItem::Case(Box::new(case_gen)), start + consumed));
    }

    if is_decl_type(token) {
        let (decls, consumed) = super::declaration::parse_declaration(&tokens[start..])?;
        return Ok((GenerateItem::Decl(decls), start + consumed));
    }

    if *token == Token::Assign {
        let (stmt, consumed) = super::statement::parse_continuous_assign(&tokens[start..])?;
        // `assign` disimpan sebagai Statement; di dalam generate LHS dan RHS
        // diambil keluar supaya bisa dielaborasi per iterasi.
        if let sv_ast::statement::Statement::ContinuousAssign { lhs, rhs, span } = stmt {
            return Ok((GenerateItem::Assign { lhs, rhs, span }, start + consumed));
        }
        return Err("expected continuous assign".to_string());
    }

    if let Token::Ident(_) = token {
        let (inst, consumed) = super::instance::parse_instance(&tokens[start..])?;
        return Ok((GenerateItem::Instance(Box::new(inst)), start + consumed));
    }

    let (stmt, consumed) = parse_process(tokens, start)?;
    Ok((GenerateItem::Process(Box::new(stmt)), start + consumed))
}

/// Parse proses di dalam region generate (LRM §27.1).
///
/// Mengembalikan jumlah token yang dikonsumsi relatif terhadap `start`,
/// konsisten dengan `parse_if_generate` dan `parse_case_generate`.
fn parse_process(
    tokens: &[SpannedToken],
    start: usize,
) -> Result<(sv_ast::statement::Statement, usize), String> {
    let (stmt, consumed) = match &tokens[start].token {
        Token::AlwaysFf => super::always_ff::parse_always_ff(&tokens[start..])?,
        Token::Always => super::always_ff::parse_always(&tokens[start..])?,
        Token::AlwaysComb => super::always_comb::parse_always_comb(&tokens[start..])?,
        Token::Initial => super::initial::parse_initial(&tokens[start..])?,
        other => {
            return Err(format!("unexpected token in generate region: {:?}", other));
        }
    };
    Ok((stmt, consumed))
}

/// Parse `if (expr) begin : label ... end [else begin : label ... end]`.
fn parse_if_generate(
    tokens: &[SpannedToken],
    start: usize,
    genvar_luar: &[String],
) -> Result<(IfGenerate, usize), String> {
    let span = tokens[start].span;
    let mut idx = start + 1;
    expect(tokens, idx, Token::LParen, "expected '(' pada if-generate")?;
    idx += 1;
    let (condition, consumed) = super::expression::parse_expression(&tokens[idx..])?;
    idx += consumed;
    expect(tokens, idx, Token::RParen, "expected ')' pada if-generate")?;
    idx += 1;

    let (_then_label, then_branch, baru) = parse_generate_body(tokens, idx, genvar_luar)?;
    idx = baru;

    // `else` opsional.
    let mut else_branch = None;
    if tokens.get(idx).map(|t| &t.token) == Some(&Token::Else) {
        let (_else_label, body, baru) = parse_generate_body(tokens, idx + 1, genvar_luar)?;
        idx = baru;
        else_branch = Some(body);
    }

    Ok((
        IfGenerate {
            condition,
            then_branch,
            else_branch,
            span,
        },
        idx - start,
    ))
}

/// Parse `case (expr) label: begin : nama ... end default: ... endcase`.
fn parse_case_generate(
    tokens: &[SpannedToken],
    start: usize,
    genvar_luar: &[String],
) -> Result<(CaseGenerate, usize), String> {
    let span = tokens[start].span;
    let mut idx = start + 1;
    expect(
        tokens,
        idx,
        Token::LParen,
        "expected '(' pada case-generate",
    )?;
    idx += 1;
    let (selector, consumed) = super::expression::parse_expression(&tokens[idx..])?;
    idx += consumed;
    expect(
        tokens,
        idx,
        Token::RParen,
        "expected ')' pada case-generate",
    )?;
    idx += 1;

    let mut arms: Vec<GenerateCaseArm> = Vec::new();
    let mut default_ada = false;
    while tokens.get(idx).map(|t| &t.token) != Some(&Token::Endcase) {
        if idx >= tokens.len() {
            return Err("expected 'endcase'".to_string());
        }
        let arm_span = tokens[idx].span;
        let mut labels = Vec::new();
        let is_default = match &tokens[idx].token {
            Token::Default => {
                default_ada = true;
                idx += 1;
                true
            }
            _ => {
                loop {
                    let (label, consumed) = super::expression::parse_expression(&tokens[idx..])?;
                    labels.push(label);
                    idx += consumed;
                    match tokens.get(idx).map(|t| &t.token) {
                        Some(Token::Comma) => idx += 1,
                        _ => break,
                    }
                }
                false
            }
        };
        expect(
            tokens,
            idx,
            Token::Colon,
            "expected ':' pada lengan case-generate",
        )?;
        idx += 1;
        let (_label, body, baru) = parse_generate_body(tokens, idx, genvar_luar)?;
        idx = baru;
        arms.push(GenerateCaseArm {
            labels,
            body,
            is_default,
            span: arm_span,
        });
    }
    idx += 1; // lewati 'endcase'

    // LRM §27.1: case-generate tanpa `default` tidak mencoversi semua nilai,
    // jadi batasnya ambigu saat elaborasi.
    if !default_ada {
        return Err("case-generate wajib punya lengan `default`".to_string());
    }

    Ok((
        CaseGenerate {
            selector,
            arms,
            span,
        },
        idx - start,
    ))
}

/// Parse loop generate `for (genvar = A; genvar < B; genvar = genvar + C)`.
///
/// Mengembalikan indeks token pertama sesudah loop (absolut). Batas dan
/// langkah wajib konstanta supaya jumlah iterasi diketahui saat elaborasi;
/// loop yang tidak pernah mendekati batas ditolak di sini.
fn parse_for_generate(
    tokens: &[SpannedToken],
    start: usize,
    genvar_luar: &[String],
) -> Result<(ForGenerate, usize), String> {
    let span = tokens[start].span;
    let mut idx = start + 1;
    expect(
        tokens,
        idx,
        Token::LParen,
        "expected '(' pada loop generate",
    )?;
    idx += 1;

    // Semua helper di bawah mengembalikan indeks token berikutnya secara
    // absolut, sehingga `idx` langsung disetel, bukan dijumlahkan.
    let (var, init, baru) = parse_genvar_init(tokens, idx)?;
    idx = baru;
    expect(tokens, idx, Token::Semi, "expected ';' pada loop generate")?;
    idx += 1;

    let (compare, bound, baru) = parse_generate_condition(tokens, idx)?;
    idx = baru;
    expect(tokens, idx, Token::Semi, "expected ';' pada loop generate")?;
    idx += 1;

    let (step, baru) = parse_genvar_step(tokens, idx, &var)?;
    idx = baru;
    expect(
        tokens,
        idx,
        Token::RParen,
        "expected ')' pada loop generate",
    )?;
    idx += 1;

    // LRM §27.3: loop yang tidak pernah mendekati batas menggantung elaborasi,
    // jadi ditolak sebelum IR terbentuk.
    if !compare.progresses(step) {
        return Err(format!(
            "loop generate tidak pernah selesai: '{}' dengan langkah {}",
            compare_name(compare),
            step
        ));
    }

    let (label, body, baru) = parse_generate_body(tokens, idx, genvar_luar)?;
    idx = baru;

    Ok((
        ForGenerate {
            var,
            init,
            bound,
            compare,
            step,
            label,
            body,
            span,
        },
        idx,
    ))
}

/// Parse inisialisasi `genvar = A`; A wajib konstanta.
///
/// Keyword `genvar` sebelum nama variabel diterima karena itulah bentuk
/// kanonik LRM §27.3, dan bentuk tanpa keyword juga sah.
fn parse_genvar_init(
    tokens: &[SpannedToken],
    start: usize,
) -> Result<(String, u64, usize), String> {
    let mut idx = start;
    if tokens.get(idx).map(|t| &t.token) == Some(&Token::Genvar) {
        idx += 1;
    }
    let var = match tokens.get(idx).map(|t| &t.token) {
        Some(Token::Ident(nama)) => nama.clone(),
        _ => return Err("expected genvar pada inisialisasi loop generate".to_string()),
    };
    idx += 1;
    expect(
        tokens,
        idx,
        Token::Eq,
        "expected '=' pada inisialisasi loop generate",
    )?;
    idx += 1;
    let init = read_constant(tokens, idx, "nilai awal genvar")?;
    Ok((var, init, idx + 1))
}

/// Parse syarat `genvar <op> B` menjadi (op, batas).
///
/// `<=` dibaca lexer sebagai `NonBlocking`, jadi keduanya dipetakan ke `Le`.
fn parse_generate_condition(
    tokens: &[SpannedToken],
    start: usize,
) -> Result<(CompareOp, u64, usize), String> {
    if !matches!(tokens.get(start).map(|t| &t.token), Some(Token::Ident(_))) {
        return Err("expected genvar pada syarat loop generate".to_string());
    }
    let mut idx = start + 1;
    let compare = match tokens.get(idx).map(|t| &t.token) {
        Some(Token::Lt) => CompareOp::Lt,
        Some(Token::Gt) => CompareOp::Gt,
        Some(Token::Ge) => CompareOp::Ge,
        Some(Token::EqEq) => CompareOp::Eq,
        Some(Token::BangEq) => CompareOp::Ne,
        Some(Token::NonBlocking) => CompareOp::Le,
        other => {
            return Err(format!(
                "operator pembatas loop generate tidak didukung: {:?}",
                other
            ))
        }
    };
    idx += 1;
    let bound = read_constant(tokens, idx, "batas loop generate")?;
    Ok((compare, bound, idx + 1))
}

/// Parse langkah `genvar = genvar + C`, `genvar++`, atau `genvar--`.
///
/// Nilai awal genvar tidak boleh menjadi sumber langkah; hanya genvar itu
/// sendiri atau konstanta yang diterima (LRM §27.3).
fn parse_genvar_step(
    tokens: &[SpannedToken],
    start: usize,
    var: &str,
) -> Result<(i64, usize), String> {
    match tokens.get(start).map(|t| &t.token) {
        Some(Token::Ident(nama)) if nama == var => {}
        _ => return Err("expected genvar pada langkah loop generate".to_string()),
    }
    let mut idx = start + 1;

    // Bentuk `genvar++` / `genvar--` (LRM §12.5).
    match tokens.get(idx).map(|t| &t.token) {
        Some(Token::Incr) => return Ok((1, idx + 1)),
        Some(Token::Decr) => return Ok((-1, idx + 1)),
        _ => {}
    }

    expect(
        tokens,
        idx,
        Token::Eq,
        "expected '=' pada langkah loop generate",
    )?;
    idx += 1;

    // KIRI boleh `genvar` atau konstanta bulat.
    let kiri_konstan = match tokens.get(idx).map(|t| &t.token) {
        Some(Token::Ident(nama)) if nama == var => None,
        Some(Token::Number(n)) => Some(*n as i64),
        _ => return Err("langkah loop generate harus konstanta".to_string()),
    };
    idx += 1;
    if let Some(nilai) = kiri_konstan {
        return Ok((nilai, idx));
    }

    let arah = match tokens.get(idx).map(|t| &t.token) {
        Some(Token::Plus) => 1i64,
        Some(Token::Minus) => -1i64,
        _ => return Err("expected '+' atau '-' pada langkah loop generate".to_string()),
    };
    idx += 1;

    // KANAN boleh konstanta atau `genvar` yang berarti 1.
    let besar: i64 = match tokens.get(idx).map(|t| &t.token) {
        Some(Token::Number(n)) => *n as i64,
        Some(Token::Ident(nama)) if nama == var => 1,
        _ => return Err("expected konstanta pada langkah loop generate".to_string()),
    };
    idx += 1;
    Ok((arah * besar, idx))
}

/// Parse badan loop generate: `begin : label ... end` atau satu item langsung.
///
/// Tanpa label, nama hierarki setiap iterasi tidak dapat dibedakan, jadi
/// bentuk `begin` wajib berlabel (LRM §27.3).
fn parse_generate_body(
    tokens: &[SpannedToken],
    start: usize,
    genvar_luar: &[String],
) -> Result<(String, Vec<GenerateItem>, usize), String> {
    if tokens.get(start).map(|t| &t.token) != Some(&Token::Begin) {
        let mut genvars = Vec::new();
        let (item, setelah) = parse_generate_item(tokens, start, &mut genvars, genvar_luar)?;
        return Ok((String::new(), vec![item], setelah));
    }

    let mut idx = start + 1;
    let mut label = String::new();
    if tokens.get(idx).map(|t| &t.token) == Some(&Token::Colon) {
        idx += 1;
        match tokens.get(idx).map(|t| &t.token) {
            Some(Token::Ident(nama)) => {
                label = nama.clone();
                idx += 1;
            }
            _ => return Err("expected label setelah ':' pada blok generate".to_string()),
        }
    }
    if label.is_empty() {
        return Err("blok generate wajib berlabel, mis. `begin : g`".to_string());
    }

    let mut genvars = genvar_luar.to_vec();
    let mut items = Vec::new();
    while tokens.get(idx).map(|t| &t.token) != Some(&Token::End) {
        if idx >= tokens.len() {
            return Err("expected 'end' pada blok generate".to_string());
        }
        let (item, baru) = parse_generate_item(tokens, idx, &mut genvars, genvar_luar)?;
        items.push(item);
        idx = baru;
    }
    idx += 1; // lewati 'end'

    Ok((label, items, idx))
}

/// Baca konstanta bulat pada `idx` dengan pesan error yang spesifik.
fn read_constant(tokens: &[SpannedToken], idx: usize, apa: &str) -> Result<u64, String> {
    match tokens.get(idx).map(|t| &t.token) {
        Some(Token::Number(n)) => Ok(*n),
        _ => Err(format!("{} harus konstanta", apa)),
    }
}

/// Pastikan token pada `idx` sama dengan yang diharapkan.
fn expect(tokens: &[SpannedToken], idx: usize, token: Token, pesan: &str) -> Result<(), String> {
    match tokens.get(idx).map(|t| &t.token) {
        Some(ada) if *ada == token => Ok(()),
        _ => Err(pesan.to_string()),
    }
}

/// Token qualifier tipe net/variabel yang boleh mendahului deklarasi.
fn is_decl_type(token: &Token) -> bool {
    sv_lexer::token::tipe::bisa_mulai_deklarasi(token)
}

/// Nama operator untuk pesan error.
fn compare_name(op: CompareOp) -> &'static str {
    match op {
        CompareOp::Lt => "<",
        CompareOp::Le => "<=",
        CompareOp::Gt => ">",
        CompareOp::Ge => ">=",
        CompareOp::Eq => "==",
        CompareOp::Ne => "!=",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sv_lexer::lex;

    fn region(src: &str) -> GenerateRegion {
        let tokens = lex(src).unwrap();
        let (r, _) = parse_generate_region(&tokens, 0, &[]).expect("parse generate");
        r
    }

    /// Parse region dengan daftar genvar yang dideklarasikan di body modul.
    fn region_dengan_genvar(src: &str, genvar_luar: &[&str]) -> GenerateRegion {
        let tokens = lex(src).unwrap();
        let daftar: Vec<String> = genvar_luar.iter().map(|s| s.to_string()).collect();
        let (r, _) = parse_generate_region(&tokens, 0, &daftar).expect("parse generate");
        r
    }

    #[test]
    fn region_kosong_diterima() {
        let r = region("generate endgenerate");
        assert!(r.items.is_empty());
        assert!(r.genvars.is_empty());
    }

    #[test]
    fn for_generate_dasar() {
        let r = region(
            "generate for (genvar g = 0; g < 4; g = g + 1) begin : blk \
             logic [7:0] x; end endgenerate",
        );
        assert_eq!(r.genvars, vec!["g".to_string()]);
        match &r.items[0] {
            GenerateItem::For(f) => {
                assert_eq!(f.var, "g");
                assert_eq!(f.init, 0);
                assert_eq!(f.bound, 4);
                assert_eq!(f.compare, CompareOp::Lt);
                assert_eq!(f.step, 1);
                assert_eq!(f.label, "blk");
                assert!(matches!(f.body[0], GenerateItem::Decl(_)));
            }
            other => panic!("harus for-generate, dapat {:?}", other),
        }
    }

    #[test]
    fn langkah_genvar_increment_decrement() {
        let naik = region("generate for (genvar g = 0; g < 4; g++) begin : a end endgenerate");
        match &naik.items[0] {
            GenerateItem::For(f) => assert_eq!(f.step, 1),
            other => panic!("harus for, dapat {:?}", other),
        }
        let turun = region("generate for (genvar g = 4; g > 0; g--) begin : a end endgenerate");
        match &turun.items[0] {
            GenerateItem::For(f) => {
                assert_eq!(f.step, -1);
                assert_eq!(f.compare, CompareOp::Gt);
            }
            other => panic!("harus for, dapat {:?}", other),
        }
    }

    #[test]
    fn langkah_negatif_ditolak() {
        // `for (g = 0; g < 4; g--)` menjauh dari batas dan tidak pernah selesai.
        let tokens = lex("generate for (g = 0; g < 4; g--) begin : a end endgenerate").unwrap();
        let err = parse_generate_region(&tokens, 0, &[]).unwrap_err();
        assert!(err.contains("tidak pernah selesai"), "{}", err);
    }

    #[test]
    fn langkah_nol_ditolak() {
        let tokens = lex("generate for (g = 0; g < 4; g = g) begin : a end endgenerate").unwrap();
        assert!(parse_generate_region(&tokens, 0, &[]).is_err());
    }

    #[test]
    fn batas_konstanta_wajib() {
        let tokens =
            lex("generate for (g = 0; g < W; g = g + 1) begin : a end endgenerate").unwrap();
        let err = parse_generate_region(&tokens, 0, &[]).unwrap_err();
        assert!(err.contains("konstanta"), "{}", err);
    }

    #[test]
    fn nilai_awal_konstanta_wajib() {
        let tokens =
            lex("generate for (g = j; g < 4; g = g + 1) begin : a end endgenerate").unwrap();
        let err = parse_generate_region(&tokens, 0, &[]).unwrap_err();
        assert!(err.contains("konstanta"), "{}", err);
    }

    #[test]
    fn blk_tanpa_label_ditolak() {
        let tokens = lex("generate for (g = 0; g < 4; g = g + 1) begin end endgenerate").unwrap();
        let err = parse_generate_region(&tokens, 0, &[]).unwrap_err();
        assert!(err.contains("label"), "{}", err);
    }

    #[test]
    fn instans_di_dalam_generate() {
        let r = region("generate child c(); endgenerate");
        assert!(matches!(r.items[0], GenerateItem::Instance(_)));
    }

    // Proses di dalam generate ditolak eksplisit supaya elaborator tidak
    // menghasilkan design yang salah secara semantik tanpa error.
    #[test]
    fn genvar_terdeklarasi_di_body_modul_dipakai_apa_adanya() {
        // LRM §27.3: `for (i = ...)` sah bila `i` sudah dideklarasikan
        // `genvar i;` di body modul.
        let r = region_dengan_genvar(
            "generate for (i = 0; i < 4; i = i + 1) begin : g assign y[i] = din; end endgenerate",
            &["i"],
        );
        match &r.items[0] {
            GenerateItem::For(f) => assert_eq!(f.var, "i"),
            other => panic!("harus for, dapat {other:?}"),
        }
        assert!(r.genvars.contains(&"i".to_string()));
    }

    #[test]
    fn satu_genvar_dipakai_dua_loop_diterima() {
        // LRM §27.3: genvar boleh dipakai ulang; nilainya di-reset tiap loop.
        let r = region_dengan_genvar(
            "generate \
               for (i = 0; i < 4; i = i + 1) begin : g assign y[i] = din; end \
               for (i = 0; i < 2; i = i + 1) begin : h assign z[i] = din; end \
             endgenerate",
            &["i"],
        );
        let jumlah_for = r
            .items
            .iter()
            .filter(|i| matches!(i, GenerateItem::For(_)))
            .count();
        assert_eq!(jumlah_for, 2);
        // Nama genvar tidak diduplikasi meski dipakai dua kali.
        let n = r.genvars.iter().filter(|n| *n == "i").count();
        assert_eq!(n, 1, "genvar harus tercatat satu kali: {:?}", r.genvars);
    }

    #[test]
    fn loop_var_tanpa_deklarasi_ditolak() {
        // `k` bukan genvar dan tidak ada `genvar k;` di body modul.
        let tokens = lex(
            "generate for (k = 0; k < 4; k = k + 1) begin : g assign y[k] = din; end endgenerate",
        )
        .unwrap();
        let err = parse_generate_region(&tokens, 0, &["i".to_string()]).unwrap_err();
        assert!(err.contains("belum dideklarasikan"), "pesan: {err}");
        assert!(err.contains('k'), "pesan tak menyebut nama: {err}");
    }

    #[test]
    fn loop_var_tanpa_kolom_apapun_ditolak() {
        // Tanpa deklarasi modul maupun keyword inline, loop tidak sah.
        let tokens = lex(
            "generate for (q = 0; q < 4; q = q + 1) begin : g assign y[q] = din; end endgenerate",
        )
        .unwrap();
        assert!(parse_generate_region(&tokens, 0, &[]).is_err());
    }

    #[test]
    fn loop_var_dari_modul_tetap_dipakai_di_if_generate_tersarang() {
        // Genvar yang dideklarasikan di modul harus terlihat di body
        // `if`/`case`/`for` di dalam region.
        let r = region_dengan_genvar(
            "generate \
               for (i = 0; i < 2; i = i + 1) begin : g \
                 if (i == 0) assign y[0] = din; \
                 else assign y[1] = din; \
               end \
             endgenerate",
            &["i"],
        );
        match &r.items[0] {
            GenerateItem::For(f) => assert!(matches!(f.body[0], GenerateItem::If(_))),
            other => panic!("harus for, dapat {other:?}"),
        }
    }

    #[test]
    fn proses_di_dalam_generate_kini_diterima() {
        for src in [
            "generate for (genvar g = 0; g < 4; g = g + 1) begin : a always_ff @(posedge clk) x = 1'b0; end endgenerate",
            "generate for (genvar g = 0; g < 4; g = g + 1) begin : a always_comb x = 1'b0; end endgenerate",
            "generate for (genvar g = 0; g < 4; g = g + 1) begin : a initial x = 1'b0; end endgenerate",
        ] {
            let tokens = lex(src).unwrap();
            let (region, _) = parse_generate_region(&tokens, 0, &[]).expect(src);
            match &region.items[0] {
                GenerateItem::For(f) => assert!(
                    matches!(f.body[0], GenerateItem::Process(_)),
                    "{}",
                    src
                ),
                other => panic!("harus for-generate, dapat {:?}", other),
            }
        }
    }

    #[test]
    fn endgenerate_hilang_ditolak() {
        let tokens = lex("generate for (g = 0; g < 4; g = g + 1) begin : a end").unwrap();
        assert!(parse_generate_region(&tokens, 0, &[]).is_err());
    }

    // --- LRM §27.1: if-generate dan case-generate ---

    #[test]
    fn if_generate_dasar_diparse() {
        let r = region(
            "generate if (P == 1) begin : t assign y = 4'b1; \
             end else begin : f assign y = 4'b0; end endgenerate",
        );
        match &r.items[0] {
            GenerateItem::If(g) => {
                assert!(g.else_branch.is_some());
                assert_eq!(g.then_branch.len(), 1);
                assert_eq!(g.else_branch.as_ref().unwrap().len(), 1);
            }
            other => panic!("harus if-generate, dapat {:?}", other),
        }
    }

    #[test]
    fn if_generate_tanpa_else_diterima() {
        let r = region("generate if (P) begin : t assign y = 1'b1; end endgenerate");
        match &r.items[0] {
            GenerateItem::If(g) => assert!(g.else_branch.is_none()),
            other => panic!("harus if-generate, dapat {:?}", other),
        }
    }

    #[test]
    fn if_generate_dengan_loop_bersarang() {
        let r = region(
            "generate if (P) begin : t \
               for (genvar i = 0; i < 4; i = i + 1) begin : b \
                 assign y[i] = 1'b1; \
               end \
             end endgenerate",
        );
        match &r.items[0] {
            GenerateItem::If(g) => assert!(matches!(g.then_branch[0], GenerateItem::For(_))),
            other => panic!("harus if-generate, dapat {:?}", other),
        }
    }

    #[test]
    fn case_generate_diparse() {
        let r = region(
            "generate case (SEL) \
               0: begin : a assign y[0] = 1'b0; end \
               1: begin : b assign y[1] = 1'b1; end \
               default: begin : d assign y[2] = 1'b0; end \
             endcase endgenerate",
        );
        match &r.items[0] {
            GenerateItem::Case(g) => {
                assert_eq!(g.arms.len(), 3);
                assert!(!g.arms[0].is_default);
                assert!(g.arms[2].is_default);
            }
            other => panic!("harus case-generate, dapat {:?}", other),
        }
    }

    #[test]
    fn case_generate_tanpa_default_ditolak() {
        // Tanpa `default`, nilai selektor yang tak terdaftar tak punya
        //cabang saat elaborasi.
        let tokens =
            lex("generate case (SEL) 0: begin : a assign y[0] = 1'b0; end endcase endgenerate")
                .unwrap();
        let err = parse_generate_region(&tokens, 0, &[]).unwrap_err();
        assert!(err.contains("default"), "{}", err);
    }

    // BUG: `parse_if_generate` mengembalikan jumlah token relatif, sementara
    // `parse_generate_item` mengandaikan indeks absolut.
    #[test]
    fn if_generate_mengkonsumsi_seluruh_token() {
        let src = "generate if (M == 1) begin : g assign q = 4'b0; end endgenerate";
        let tokens = lex(src).unwrap();
        let (_, consumed) = parse_generate_region(&tokens, 0, &[]).expect("parse");
        assert_eq!(consumed, tokens.len());
    }

    #[test]
    fn case_generate_meng_konsumsi_token_sepada_dengan_jumlah_token() {
        let src = "generate case (M) 0: begin : a assign q[0] = 1'b0; end \
                   default: begin : d assign q[3] = 1'b1; end endcase endgenerate";
        let tokens = lex(src).unwrap();
        let (_, consumed) = parse_generate_region(&tokens, 0, &[]).expect("parse");
        assert_eq!(consumed, tokens.len());
    }
}
