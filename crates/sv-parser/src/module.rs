// Tanggung jawab: parse module SystemVerilog.
use crate::pesan;
use sv_ast::module::Module;
use sv_lexer::spanned::SpannedToken;
use sv_lexer::token::Token;
/// Token qualifier tipe net/variabel yang boleh muncul di daftar port (LRM §6.6).
fn is_net_type(token: &Token) -> bool {
    // LRM §6.6 dan Tabel 6-22: tipe integer atom juga boleh muncul sebagai
    // qualifier tipe pada daftar port.
    sv_lexer::token::tipe::bisa_mulai_deklarasi(token)
}

/// Token qualifier signedness pada deklarasi (LRM §6.2.1).
fn is_signedness(token: &Token) -> bool {
    matches!(token, Token::Signed | Token::Unsigned)
}

/// Lewati satu grup `[...]` seimbang; kembalikan indeks sesudah `]`.
fn lewati_kurung_siku(tokens: &[SpannedToken], mut idx: usize) -> usize {
    let mut depth = 0u32;
    while idx < tokens.len() {
        match &tokens[idx].token {
            Token::LBracket => depth += 1,
            Token::RBracket => {
                depth -= 1;
                if depth == 0 {
                    return idx + 1;
                }
            }
            _ => {}
        }
        idx += 1;
    }
    idx
}

/// True bila `tokens[idx]` adalah nama tipe yang benar-benar diikuti nama port.
///
/// `input my_t a` → `my_t` tipe, `a` nama. `input a,` dan
/// `input a [3:0],` → tidak ada nama setelahnya, jadi `a` adalah nama port.
/// Tanpa pemeriksaan ini parser memakan nama port sebagai tipe lalu berjalan
/// melewati `)` sampai `endmodule`.
fn tipe_diikuti_nama(tokens: &[SpannedToken], idx: usize) -> bool {
    if !matches!(tokens.get(idx).map(|t| &t.token), Some(Token::Ident(_))) {
        return false;
    }
    let mut j = idx + 1;
    while j < tokens.len() && tokens[j].token == Token::LBracket {
        j = lewati_kurung_siku(tokens, j);
    }
    matches!(tokens.get(j).map(|t| &t.token), Some(Token::Ident(_)))
}

/// Parse `parameter` di body modul (LRM §6.20).
///
/// Bentuk: `parameter [tipe] NAME = <konstanta> [, ...] ;`
///
/// Nilai harus literal karena `ParamDecl` menyimpan `u64`; bentuk berparameter
/// hanya ada di daftar port `#(...)` yang diparse terpisah. Yang membedakannya
/// dari `localparam` adalah bahwa bentuk ini tetap bisa di-override saat
/// instansiasi.
fn parse_param_body(
    tokens: &[SpannedToken],
) -> Result<(Vec<sv_ast::width::ParamDecl>, usize), String> {
    let mut idx = 1; // lewati keyword `parameter`
    if tokens
        .get(idx)
        .is_some_and(|t| sv_lexer::token::tipe::bisa_mulai_deklarasi(&t.token))
    {
        idx += 1;
    }
    // Signedness dan dimensi packed setelah tipe: `parameter logic [15:0] E`.
    if tokens.get(idx).is_some_and(|t| is_signedness(&t.token)) {
        idx += 1;
    }
    while tokens.get(idx).is_some_and(|t| t.token == Token::LBracket) {
        idx = lewati_kurung_siku(tokens, idx);
    }
    let mut out = Vec::new();
    loop {
        let span = tokens
            .get(idx)
            .map(|t| t.span)
            .ok_or_else(|| "expected nama parameter, tetapi input habis".to_string())?;
        let Token::Ident(name) = &tokens[idx].token else {
            return Err(pesan::harap(tokens, idx, "expected nama parameter"));
        };
        let name = name.clone();
        idx += 1;
        if tokens
            .get(idx)
            .is_some_and(|t| matches!(t.token, Token::Signed | Token::Unsigned))
        {
            idx += 1;
        }
        if tokens.get(idx).map(|t| &t.token) != Some(&Token::Eq) {
            return Err(pesan::tandai(
                tokens,
                idx,
                format!("expected '=' setelah nama parameter '{name}'"),
            ));
        }
        idx += 1;
        // Nilai default boleh angka atau string literal, misal `parameter string Name = "dmi0"`.
        let (default, default_str) = match &tokens[idx].token {
            Token::StringLiteral(s) => {
                let s = s.clone();
                idx += 1;
                (0, Some(s))
            }
            _ => {
                let (expr, eaten) = super::expression::parse_expression(&tokens[idx..])?;
                idx += eaten;
                match expr {
                    sv_ast::expression::Expr::Number(nilai) => (nilai, None),
                    // `parameter logic [15:0] E = 16'h0001;` — based literal
                    // adalah konstanta yang sah, bukan error.
                    sv_ast::expression::Expr::Sized { value, .. } => (value, None),
                    _ => return Err(format!(
                        "nilai default parameter '{name}' harus literal angka atau string di body modul"
                    )),
                }
            }
        };
        out.push(sv_ast::width::ParamDecl {
            name,
            type_name: None,
            default,
            default_str,
            span,
        });
        if tokens.get(idx).map(|t| &t.token) == Some(&Token::Comma) {
            idx += 1;
            continue;
        }
        break;
    }
    if tokens.get(idx).map(|t| &t.token) != Some(&Token::Semi) {
        return Err(pesan::harap(tokens, idx, "expected ';' setelah parameter"));
    }
    Ok((out, idx + 1))
}

/// Parse `localparam` di body modul (LRM §6.20).
///
/// Bentuk: `localparam [tipe] NAME = <konstanta> [, NAME2 = <konstanta>] ;`
///
/// Nilai TIDAK dihitung di sini: `localparam HALF = DEPTH / 2;` merujuk
/// `localparam` sebelumnya dan `parameter` modul, yang nilainya baru diketahui
/// saat elaborasi.
fn parse_localparam(
    tokens: &[SpannedToken],
) -> Result<(Vec<sv_ast::width::LocalParamDecl>, usize), String> {
    let mut idx = 1; // lewati keyword `localparam`
                     // Tipe opsional (LRM §6.20.1): `localparam int X`, `localparam int
                     // unsigned N`, `localparam bit [N-1:0] X`, `localparam pkg::t X`, dan
                     // `localparam my_t X`. Tanpa penanganan signedness/dimensi/scope paket,
                     // deklarasi seperti `localparam int unsigned NumSp2VSig = 8;` melaporkan
                     // "expected nama localparam" di baris `endmodule`.
    if tokens
        .get(idx)
        .is_some_and(|t| sv_lexer::token::tipe::bisa_mulai_deklarasi(&t.token))
    {
        idx += 1;
        if tokens.get(idx).is_some_and(|t| is_signedness(&t.token)) {
            idx += 1;
        }
        while idx < tokens.len() && tokens[idx].token == Token::LBracket {
            idx = lewati_kurung_siku(tokens, idx);
        }
    } else if matches!(tokens.get(idx).map(|t| &t.token), Some(Token::Ident(_)))
        && tokens.get(idx + 1).map(|t| &t.token) == Some(&Token::Colon)
    {
        // `localparam pkg::tipe Nama = ...`
        idx += 1;
        while idx < tokens.len() && tokens[idx].token == Token::Colon {
            idx += 1;
        }
        if matches!(tokens.get(idx).map(|t| &t.token), Some(Token::Ident(_))) {
            idx += 1;
        }
        while idx < tokens.len() && tokens[idx].token == Token::LBracket {
            idx = lewati_kurung_siku(tokens, idx);
        }
    } else if tipe_diikuti_nama(tokens, idx) {
        // `localparam my_t Nama = ...` — identifier pertama adalah tipe.
        idx += 1;
        while idx < tokens.len() && tokens[idx].token == Token::LBracket {
            idx = lewati_kurung_siku(tokens, idx);
        }
    }
    let mut out = Vec::new();
    loop {
        let span = tokens
            .get(idx)
            .map(|t| t.span)
            .ok_or_else(|| "expected nama localparam, tetapi input habis".to_string())?;
        let Token::Ident(name) = &tokens[idx].token else {
            return Err(pesan::harap(tokens, idx, "expected nama localparam"));
        };
        let name = name.clone();
        idx += 1;
        if tokens
            .get(idx)
            .is_some_and(|t| matches!(t.token, Token::Signed | Token::Unsigned))
        {
            idx += 1;
        }
        // `=` adalah `Token::Eq`; `Token::Assign` adalah keyword `assign`.
        if tokens.get(idx).map(|t| &t.token) != Some(&Token::Eq) {
            return Err(pesan::tandai(
                tokens,
                idx,
                format!("expected '=' setelah nama localparam '{name}'"),
            ));
        }
        idx += 1;
        let (value, eaten) = super::expression::parse_expression(&tokens[idx..])?;
        idx += eaten;
        out.push(sv_ast::width::LocalParamDecl { name, value, span });
        if tokens.get(idx).map(|t| &t.token) == Some(&Token::Comma) {
            idx += 1;
            continue;
        }
        break;
    }
    if tokens.get(idx).map(|t| &t.token) != Some(&Token::Semi) {
        return Err(pesan::harap(tokens, idx, "expected ';' setelah localparam"));
    }
    Ok((out, idx + 1))
}

pub fn parse_module(tokens: &[SpannedToken]) -> Result<Module, String> {
    let mut idx = 0;

    // Expect 'module' keyword
    if idx >= tokens.len() || tokens[idx].token != Token::Module {
        return Err("expected 'module'".to_string());
    }
    let module_span = tokens[idx].span;
    idx += 1;

    // Expect module name
    if idx >= tokens.len() {
        return Err("expected module name".to_string());
    }
    let name = match &tokens[idx].token {
        Token::Ident(n) => n.clone(),
        _ => return Err("expected module name".to_string()),
    };
    idx += 1;

    // LRM §26.8: `import` paket boleh muncul setelah nama modul, sebelum
    // daftar parameter atau port list, misal `module m import pkg::*; (`..
    // SVRust tidak memproses impor secara semantik, hanya melewatinya.
    while idx < tokens.len() {
        match &tokens[idx].token {
            Token::Ident(n) if n == "import" => {
                idx += 1; // skip 'import'
                          // Skip hingga ';' atau '(' atau '#'
                while idx < tokens.len() {
                    match &tokens[idx].token {
                        Token::Semi => {
                            idx += 1;
                            break;
                        }
                        Token::LParen | Token::Hash => break,
                        _ => {
                            idx += 1;
                        }
                    }
                }
            }
            Token::Hash | Token::LParen => break,
            _ => break,
        }
    }

    // Daftar parameter opsional: #(parameter WIDTH = 8, ...). `localparam`
    // di dalam daftar ini berbagi scope dengan parameter modul (LRM §6.20.1),
    // jadi hasilnya digabung dengan `localparams` body modul.
    let (mut params, localparams_header, makan) = super::param::parse_param_list(&tokens[idx..])?;
    idx += makan;

    // Port list optional: ( ... ) or empty
    let mut ports = Vec::new();
    // Arah dan tipe yang berlaku untuk deklarator berikutnya pada port list
    // ANSI: `input logic [7:0] a, b` membuat `b` juga input 8-bit
    // (LRM §23.2.2.3).
    let mut arah_terakhir = sv_ast::port::PortDirection::Inout;
    let mut tipe_terakhir: Option<(sv_ast::width::WidthExpr, bool)> = None;
    if idx < tokens.len() && tokens[idx].token == Token::LParen {
        idx += 1;
        while idx < tokens.len() && tokens[idx].token != Token::RParen {
            // Implicit port connection: `.name` (LRM §23.2.2), umum pada
            // wrapper autogenerated OpenTitan.
            if tokens[idx].token == Token::Dot {
                idx += 1; // skip '.'
                let port_name = match &tokens[idx].token {
                    Token::Ident(n) => n.clone(),
                    _ => return Err("expected port name after '.'".to_string()),
                };
                let port_span = tokens[idx].span;
                idx += 1;
                if idx < tokens.len() && tokens[idx].token == Token::Comma {
                    idx += 1;
                }
                ports.push(sv_ast::port::Port {
                    direction: sv_ast::port::PortDirection::Inout,
                    name: port_name.clone(),
                    width: sv_ast::width::WidthExpr::Literal(1),
                    signed: false,
                    span: port_span,
                });
                continue;
            }

            // `;`/`endmodule` di sini berarti daftar port tidak tertutup.
            // Tanpa cabang ini parser berjalan sampai ujung modul dan
            // melaporkan error di baris `endmodule` — lokasi yang sesat.
            if matches!(tokens[idx].token, Token::Semi | Token::Endmodule) {
                return Err(format!(
                    "expected ')' penutup port list, ditemukan {:?}",
                    tokens[idx].token
                ));
            }

            // Arah eksplisit; bila tidak ada, arah port sebelumnya berlaku
            // (LRM §23.2.2.3: `input a, b` memberi `b` arah `input`).
            let direction = match tokens[idx].token {
                Token::Input => sv_ast::port::PortDirection::Input,
                Token::Output => sv_ast::port::PortDirection::Output,
                Token::Inout => sv_ast::port::PortDirection::Inout,
                // Tipe net tanpa qualifier arah: `logic clk_i` → clk_i adalah
                // nama port, tipe adalah logic implisit.
                Token::Wire | Token::Logic | Token::Reg | Token::Bit | Token::Ident(_) => {
                    arah_terakhir
                }
                _ => {
                    return Err(pesan::harap(tokens, idx, "expected port direction"));
                }
            };
            if matches!(
                tokens[idx].token,
                Token::Input | Token::Output | Token::Inout
            ) {
                arah_terakhir = direction;
                tipe_terakhir = None;
                idx += 1;
            }

            // Qualifier tipe opsional sebelum dimension: `logic`/`wire`/`reg`/`bit`.
            // Tipe atom (`int`, `byte`, ...) membawa lebar dan signedness tetap.
            let mut qualifier_dipakai = false;
            let mut lebar_atom: Option<usize> = None;
            let mut signed_atom: Option<bool> = None;
            while idx < tokens.len() {
                if let Some((lebar, signed)) = sv_lexer::token::tipe::atom(&tokens[idx].token) {
                    lebar_atom = Some(lebar as usize);
                    signed_atom = Some(signed);
                    qualifier_dipakai = true;
                    idx += 1;
                    continue;
                }
                if is_net_type(&tokens[idx].token) {
                    qualifier_dipakai = true;
                    idx += 1;
                    continue;
                }
                break;
            }

            // Tipe nama diikuti nama port: `type nama` atau `type [dim] nama`.
            // Hanya dilewati bila benar-benar ada nama port sesudahnya;
            // `input a,` adalah nama port, bukan tipe.
            if !qualifier_dipakai && tipe_diikuti_nama(tokens, idx) {
                idx += 1; // lewati nama tipe
                qualifier_dipakai = true;
                while idx < tokens.len() && tokens[idx].token == Token::LBracket {
                    idx = lewati_kurung_siku(tokens, idx);
                }
            }

            // Qualified type: `nama_paket::nama_tipe` (misal `ast_pkg::awire_t`).
            // `::` adalah dua token `Colon`, keduanya harus dilewati.
            while idx < tokens.len() {
                if matches!(tokens[idx].token, Token::Ident(_))
                    && !is_net_type(&tokens[idx].token)
                    && tokens.get(idx + 1).map(|t| &t.token) == Some(&Token::Colon)
                {
                    idx += 1; // skip nama paket
                              // Lewati semua token `Colon` (biasanya dua untuk `::`).
                    while idx < tokens.len() && matches!(tokens[idx].token, Token::Colon) {
                        idx += 1;
                    }
                    if matches!(tokens.get(idx).map(|t| &t.token), Some(&Token::Ident(_))) {
                        idx += 1; // skip nama tipe
                                  // Lewati dimensi packed opsional [dim] setelah tipe.
                        while idx < tokens.len() && tokens[idx].token == Token::LBracket {
                            let mut depth = 1u32;
                            idx += 1;
                            while idx < tokens.len() && depth > 0 {
                                match &tokens[idx].token {
                                    Token::LBracket => depth += 1,
                                    Token::RBracket => depth -= 1,
                                    _ => {}
                                }
                                idx += 1;
                            }
                        }
                    }
                } else {
                    break;
                }
            }

            // Dimensi packed TIDAK dilewati di sini: `parse_dimension` di
            // bawah yang membaca `[MSB:LSB]` menjadi lebar port. Blok yang
            // dulu ada di sini memakan dimensi lebih dulu, sehingga setiap
            // port `input logic [3:0] a` berakhir 1 bit.

            // Qualifier signedness opsional setelah tipe: `signed`/`unsigned`.
            let ada_signedness = tokens.get(idx).is_some_and(|t| is_signedness(&t.token));
            let mut signed = match tokens.get(idx).map(|t| &t.token) {
                Some(&Token::Signed) => true,
                Some(&Token::Unsigned) => false,
                _ => signed_atom.unwrap_or(false),
            };
            if ada_signedness {
                idx += 1;
            }

            // Optional packed dimension: [N] atau [MSB:LSB]
            let (lebar_tertulis, consumed) = super::dimension::parse_dimension(tokens, idx)?;
            idx += consumed;
            // Additional dimensions for multi-dimensional ports, misal
            // `[NAlerts-1:0] [CLASS_DW-1:0] alert_class_i`. SVRust tidak
            // mendukung multi-dimensional penuh, jadi dimensi tambahan hanya
            // dikonsumsi tanpa diproses.
            while idx < tokens.len() && tokens[idx].token == Token::LBracket {
                let mut depth = 1u32;
                idx += 1;
                while idx < tokens.len() && depth > 0 {
                    match &tokens[idx].token {
                        Token::LBracket => depth += 1,
                        Token::RBracket => depth -= 1,
                        _ => {}
                    }
                    idx += 1;
                }
            }
            // Tipe atom membawa lebarnya sendiri; selain itu pakai dimension
            // yang tertulis, atau tipe port sebelumnya untuk deklarator
            // lanjutan (`input logic [7:0] a, b`).
            let mut width = match lebar_atom {
                Some(lebar) => sv_ast::width::WidthExpr::Literal(lebar),
                None => lebar_tertulis,
            };
            if qualifier_dipakai || ada_signedness || width != sv_ast::width::WidthExpr::Literal(1)
            {
                tipe_terakhir = Some((width.clone(), signed));
            } else if let Some((lebar, ttd)) = tipe_terakhir.clone() {
                width = lebar;
                signed = ttd;
            }

            // Port name
            if idx >= tokens.len() {
                return Err("expected port name".to_string());
            }
            let port_name = match &tokens[idx].token {
                Token::Ident(n) => n.clone(),
                _ => return Err("expected port name".to_string()),
            };
            let port_span = tokens[idx].span;
            idx += 1;

            ports.push(sv_ast::port::Port {
                direction,
                name: port_name,
                width,
                signed,
                span: port_span,
            });

            // Dimensi unpacked setelah nama port: `input [7:0] d_i[Share]`.
            while idx < tokens.len() && tokens[idx].token == Token::LBracket {
                idx = lewati_kurung_siku(tokens, idx);
            }
            // Nilai default port ANSI (LRM §23.2.2.3): `input int x = 0`.
            if idx < tokens.len() && tokens[idx].token == Token::Eq {
                while idx < tokens.len()
                    && tokens[idx].token != Token::Comma
                    && tokens[idx].token != Token::RParen
                {
                    idx += 1;
                }
            }

            // Optional comma
            if idx < tokens.len() && tokens[idx].token == Token::Comma {
                idx += 1;
            }
        }
        if idx >= tokens.len() || tokens[idx].token != Token::RParen {
            let ditemukan = tokens
                .get(idx)
                .map(|t| format!("{:?}", t.token))
                .unwrap_or_else(|| "akhir berkas".to_string());
            return Err(format!(
                "expected ')' penutup port list, ditemukan {ditemukan}"
            ));
        }
        idx += 1; // lewati ')'
    }

    // Expect ';'
    if idx >= tokens.len() || tokens[idx].token != Token::Semi {
        return Err(pesan::harap(tokens, idx, "expected ';'"));
    }
    idx += 1;

    // Parse declarations and statements until 'endmodule'
    let mut declarations = Vec::new();
    let mut statements = Vec::new();
    let mut instances = Vec::new();
    let mut generates = Vec::new();
    let mut genvars: Vec<String> = Vec::new();
    let mut routines = Vec::new();
    let mut typedefs = Vec::new();
    let mut localparams = localparams_header;

    while idx < tokens.len() && tokens[idx].token != Token::Endmodule {
        // Compiler directive dilewati.
        if matches!(&tokens[idx].token, Token::Directive(_)) {
            idx += 1;
            if idx < tokens.len() && tokens[idx].token == Token::LParen {
                let mut depth = 1u32;
                idx += 1;
                while idx < tokens.len() && depth > 0 {
                    match &tokens[idx].token {
                        Token::LParen => depth += 1,
                        Token::RParen => depth -= 1,
                        _ => {}
                    }
                    idx += 1;
                }
            }
            if idx < tokens.len() && tokens[idx].token == Token::Semi {
                idx += 1;
            }
            continue;
        }
        // LRM §25.6: `bind target if #(...) instance (.port(sig), ...);` —
        // binding antarmuka ke instance. SVRust tidak memproses binding
        // secara semantik; lewati seluruh statement hingga `;`.
        if tokens[idx].token == Token::Bind {
            while idx < tokens.len() && tokens[idx].token != Token::Semi {
                idx += 1;
            }
            if idx < tokens.len() {
                idx += 1; // skip ';'
            }
            continue;
        }
        match &tokens[idx].token {
            // LRM §6.20: `localparam NAME = <konstanta>;`, opsional bertipe dan
            // boleh beberapa nama dipisah koma. Nilai disimpan sebagai ekspresi
            // karena bisa merujuk `localparam` sebelumnya dan `parameter` modul.
            Token::LocalParam => {
                let (decls, next) = parse_localparam(&tokens[idx..])?;
                localparams.extend(decls);
                idx += next;
            }
            // LRM §6.20 juga mengizinkan `parameter` di body modul (di luar
            // daftar `#(...)`) dan bentuknya tetap bisa di-override saat
            // instansiasi, jadi masuk ke `params` — bukan `localparams`.
            Token::Parameter => {
                let (decls, next) = parse_param_body(&tokens[idx..])?;
                params.extend(decls);
                idx += next;
            }
            t if sv_lexer::token::tipe::bisa_mulai_deklarasi(t) => {
                let tipe = tokens[idx].token.clone();
                let (decls, next) = super::declaration::parse_declaration(&tokens[idx..])?;
                for decl in decls {
                    // LRM §6.2.2: inisialisasi pada net menjadi driver kontinu,
                    // sedangkan pada variabel hanya nilai awal waktu nol.
                    if tipe == Token::Wire {
                        if let Some(rhs) = &decl.init {
                            statements.push(sv_ast::statement::Statement::ContinuousAssign {
                                lhs: sv_ast::lvalue::Lvalue::simple(decl.name.clone(), decl.span),
                                rhs: rhs.clone(),
                                span: decl.span,
                            });
                        }
                    }
                    declarations.push(decl);
                }
                idx += next;
            }
            Token::Assign => {
                match super::statement::parse_continuous_assign(&tokens[idx..]) {
                    Ok((stmt, next)) => {
                        statements.push(stmt);
                        idx += next;
                    }
                    // Error `assign` tidak boleh ditelan. Versi lama melewati
                    // assignment yang gagal diparse, sehingga `a[3:0` tanpa
                    // `]` dan `{2{2{a}}}` lolos sebagai module yang sah.
                    Err(error) => return Err(error),
                }
            }
            Token::AlwaysFf => match super::always_ff::parse_always_ff(&tokens[idx..]) {
                Ok((stmt, next)) => {
                    statements.push(stmt);
                    idx += next;
                }
                Err(_) => {
                    while idx < tokens.len() && tokens[idx].token != Token::Semi {
                        idx += 1;
                    }
                    if idx < tokens.len() {
                        idx += 1;
                    }
                }
            },
            // LRM §9.2.1: `always` polos — edge list → sekuensial, selain itu
            // (level list / `@*` / tanpa `@`) → combinational.
            Token::Always => match super::always_ff::parse_always(&tokens[idx..]) {
                Ok((stmt, next)) => {
                    statements.push(stmt);
                    idx += next;
                }
                Err(_) => {
                    while idx < tokens.len() && tokens[idx].token != Token::Semi {
                        idx += 1;
                    }
                    if idx < tokens.len() {
                        idx += 1;
                    }
                }
            },
            Token::AlwaysComb => match super::always_comb::parse_always_comb(&tokens[idx..]) {
                Ok((stmt, next)) => {
                    statements.push(stmt);
                    idx += next;
                }
                Err(_) => {
                    while idx < tokens.len() && tokens[idx].token != Token::Semi {
                        idx += 1;
                    }
                    if idx < tokens.len() {
                        idx += 1;
                    }
                }
            },
            // Testbench: blok `initial` untuk stimulus dan system task.
            Token::Initial => match super::initial::parse_initial(&tokens[idx..]) {
                Ok((stmt, next)) => {
                    statements.push(stmt);
                    idx += next;
                }
                Err(_) => {
                    while idx < tokens.len() && tokens[idx].token != Token::Semi {
                        idx += 1;
                    }
                    if idx < tokens.len() {
                        idx += 1;
                    }
                }
            },
            // Testbench: blok `final` untuk cleanup akhir simulasi (LRM §15.2).
            Token::Final => match super::r#final::parse_final(&tokens[idx..]) {
                Ok((stmt, next)) => {
                    statements.push(stmt);
                    idx += next;
                }
                Err(_) => {
                    while idx < tokens.len() && tokens[idx].token != Token::Semi {
                        idx += 1;
                    }
                    if idx < tokens.len() {
                        idx += 1;
                    }
                }
            },
            // LRM §8.20 dan §6.7: deklarasi `typedef` di body modul.
            Token::Typedef => match super::typedef::parse_typedef(&tokens[idx..]) {
                Ok((decl, next)) => {
                    typedefs.push(decl);
                    idx += next;
                }
                Err(_) => {
                    while idx < tokens.len() && tokens[idx].token != Token::Semi {
                        idx += 1;
                    }
                    if idx < tokens.len() {
                        idx += 1;
                    }
                }
            },
            // LRM §13.3/§13.4: deklarasi task dan function di body modul.
            Token::Task | Token::Function => {
                match super::routine::parse_routine(&tokens[idx..]) {
                    Ok((routine, next)) => {
                        routines.push(routine);
                        idx += next;
                    }
                    // Versi lama melewati badan subrutin tanpa menyimpannya,
                    // sehingga `module.routines` selalu kosong dan function
                    // yang dipanggil tidak pernah di-inline. Error parse juga
                    // tidak boleh ditelan.
                    Err(error) => return Err(error),
                }
            }
            // LRM §20/§21: `assert property (@(posedge clk) cond);` — concurrent
            // assertion. Plain `assert (cond);` di level modul (mis. dari
            // ekspansi makro ASSERT_INIT) dilewati.
            Token::Assert => {
                if tokens.get(idx + 1).map(|t| &t.token) == Some(&Token::Property) {
                    match super::combinational::parse_assert_property(&tokens[idx..]) {
                        Ok((stmt, next)) => {
                            statements.push(stmt);
                            idx += next;
                        }
                        Err(_) => {
                            while idx < tokens.len() && tokens[idx].token != Token::Semi {
                                idx += 1;
                            }
                            if idx < tokens.len() {
                                idx += 1;
                            }
                        }
                    }
                } else {
                    // Plain assert di level modul — lewati hingga ';'.
                    idx += 1; // skip 'assert'
                    while idx < tokens.len() && tokens[idx].token != Token::Semi {
                        idx += 1;
                    }
                    if idx < tokens.len() {
                        idx += 1; // skip ';'
                    }
                }
            }
            Token::Ident(n) if n == "import" => {
                // `import "DPI-C" function ...;` (DPI) atau `import pkg::*;`.
                // SVRust tidak memproses impor secara semantik, lewati sampai ';'.
                while idx < tokens.len() && tokens[idx].token != Token::Semi {
                    idx += 1;
                }
                if idx < tokens.len() {
                    idx += 1;
                }
                continue;
            }
            Token::Ident(_) => {
                // `nama_tipe var;` adalah deklarasi bertipe hasil typedef,
                // sedangkan `nama inst(...)` adalah instansiasi modul. Keduanya
                // diawali identifier dan tanpa `#(...)`, jadi bedanya terlihat
                // dari identifier kedua: bila ada dan diikuti `;`/`,`, itu
                // deklarasi bertipe.
                if typed_declaration(&tokens[idx..]) {
                    let (_, first, consumed) =
                        super::typedef::parse_typed_declaration(&tokens[idx..])?;
                    let type_name = first.type_name.clone().unwrap_or_default();
                    let (tambahan, akhir) = super::typedef::parse_more_typed_declarators(
                        &tokens[idx..],
                        consumed,
                        &type_name,
                        &first.width,
                        first.signed,
                        first.span,
                    )?;
                    declarations.push(first);
                    declarations.extend(tambahan);
                    if tokens.get(idx + akhir).map(|t| &t.token) != Some(&Token::Semi) {
                        return Err(pesan::harap(
                            tokens,
                            idx + akhir,
                            "expected ';' setelah deklarasi bertipe",
                        ));
                    }
                    idx += akhir + 1;
                    continue;
                }
                // Kualifikasi tipe dengan paket: `pkg::type nama;` atau
                // `pkg::type [dim] nama;`
                if tokens
                    .get(idx)
                    .is_some_and(|t| matches!(&t.token, Token::Ident(_)))
                    && tokens.get(idx + 1).is_some_and(|t| t.token == Token::Colon)
                    && tokens.get(idx + 2).is_some_and(|t| t.token == Token::Colon)
                    && tokens
                        .get(idx + 3)
                        .is_some_and(|t| matches!(&t.token, Token::Ident(_)))
                {
                    // `pkg::type [dim] nama;` — lewati pkg :: type [dim] nama,
                    // cari ';'
                    idx += 4; // lewati pkg :: type
                              // Lewati dimensi packed opsional [dim]
                    while idx < tokens.len() && tokens[idx].token == Token::LBracket {
                        let mut depth = 1u32;
                        idx += 1;
                        while idx < tokens.len() && depth > 0 {
                            match &tokens[idx].token {
                                Token::LBracket => depth += 1,
                                Token::RBracket => depth -= 1,
                                _ => {}
                            }
                            idx += 1;
                        }
                    }
                    // Nama variabel
                    if idx < tokens.len() && matches!(tokens[idx].token, Token::Ident(_)) {
                        idx += 1;
                    }
                    while idx < tokens.len() && tokens[idx].token != Token::Semi {
                        idx += 1;
                    }
                    if idx < tokens.len() {
                        idx += 1; // lewati ';'
                    }
                    continue;
                }
                // Instansi modul: coba parse, tapi lewati bila gagal (bisa
                // berupa deklarasi variabel dengan tipe yang tidak dikenal,
                // misal `chandle ctx;`).
                match super::instance::parse_instance(&tokens[idx..]) {
                    Ok((inst, next)) => {
                        instances.push(inst);
                        idx += next;
                    }
                    Err(_) => {
                        while idx < tokens.len() && tokens[idx].token != Token::Semi {
                            idx += 1;
                        }
                        if idx < tokens.len() {
                            idx += 1;
                        }
                    }
                }
            }
            // LRM §27.3: `genvar i, j;` di body modul mendeklarasikan variabel
            // loop generate. Nama yang dideklarasikan di sini lalu boleh
            // dipakai di `for (i = ...)` tanpa mengulang keyword `genvar`.
            Token::Genvar => match parse_genvar_declaration(&tokens[idx..]) {
                Ok((nama, next)) => {
                    for n in nama {
                        if !genvars.contains(&n) {
                            genvars.push(n);
                        }
                    }
                    idx += next;
                }
                // Kesalahan `genvar` tidak boleh ditelan: `genvar;` yang
                // diterima diam-diam membuat design tanpa loop generate
                // terlihat sah.
                Err(error) => return Err(error),
            },
            // LRM §27.1: region `generate ... endgenerate` disimpan terpisah
            // agar elaborator bisa meng-instantiate tiap iterasi dengan nilai
            // genvar yang berbeda.
            Token::Generate => {
                match super::generate::parse_generate_region(&tokens[idx..], 0, &genvars) {
                    Ok((region, next)) => {
                        generates.push(region);
                        idx += next;
                    }
                    // Error di dalam generate (misal loop yang tidak pernah
                    // selesai) harus sampai ke pengguna. Versi lama menelan
                    // error lalu melewati blok, sehingga design yang salah
                    // lolos `check` dan error hanya muncul jauh di kemudian.
                    Err(error) => return Err(error),
                }
            }
            // LRM §27.1: generate `for (genvar i = ...; ...; ...) begin ... end`
            // di isi modul. SVRust tidak mengevaluasi loop generate; lewati
            // seluruh blok hingga `end` penutup.
            Token::For => {
                let mut depth = 0i32;
                idx += 1; // skip 'for'
                          // Lewati hingga '(' untuk kondisi for
                while idx < tokens.len() && tokens[idx].token != Token::LParen {
                    idx += 1;
                }
                if idx < tokens.len() {
                    idx += 1; // skip '('
                }
                // Lewati isi kondisi for hingga ')' dengan depth tracking
                let mut paren_depth = 1u32;
                while idx < tokens.len() && paren_depth > 0 {
                    match tokens[idx].token {
                        Token::LParen => paren_depth += 1,
                        Token::RParen => paren_depth -= 1,
                        _ => {}
                    }
                    idx += 1;
                }
                // Lewati hingga 'begin'
                while idx < tokens.len() && tokens[idx].token != Token::Begin {
                    idx += 1;
                }
                if idx < tokens.len() {
                    depth += 1;
                    idx += 1; // skip 'begin'
                }
                // Lewati isi blok generate hingga 'end' yang cocok
                while idx < tokens.len() && depth > 0 {
                    match tokens[idx].token {
                        Token::Begin => depth += 1,
                        Token::End => {
                            depth -= 1;
                            if depth == 0 {
                                idx += 1; // lewati 'end'
                                          // Opsional label: `end : nama`
                                if idx < tokens.len() && tokens[idx].token == Token::Colon {
                                    idx += 1; // lewati ':'
                                    if idx < tokens.len()
                                        && matches!(tokens[idx].token, Token::Ident(_))
                                    {
                                        idx += 1; // lewati nama label
                                    }
                                }
                                break;
                            }
                        }
                        _ => {}
                    }
                    idx += 1;
                }
            }
            // System task pemanggilan di level modul (mis. `$error(...)`,
            // `$display(...)`). Lewati hingga `;`.
            Token::SystemTask(_) => {
                while idx < tokens.len() && tokens[idx].token != Token::Semi {
                    idx += 1;
                }
                if idx < tokens.len() {
                    idx += 1; // skip ';'
                }
            }
            // Bare `end` di level modul (sisa dari blok generate/proses yang
            // tidak sepenuhnya dikonversi). Lewati.
            Token::End => {
                idx += 1;
            }
            // `if` di level modul (generate if). Lewati hingga `;` atau `end`,
            // lalu opsional `else` cabang.
            Token::If => {
                idx += 1; // skip 'if'
                          // Lewati kondisi dalam kurung
                if idx < tokens.len() && tokens[idx].token == Token::LParen {
                    let mut depth = 1u32;
                    idx += 1;
                    while idx < tokens.len() && depth > 0 {
                        match tokens[idx].token {
                            Token::LParen => depth += 1,
                            Token::RParen => depth -= 1,
                            _ => {}
                        }
                        idx += 1;
                    }
                }
                // Lewati statement tunggal atau blok hingga ';' atau 'end'.
                while idx < tokens.len() {
                    match tokens[idx].token {
                        Token::Semi => {
                            idx += 1;
                            break;
                        }
                        Token::End => {
                            idx += 1; // lewati 'end'
                                      // Opsional label: `end : nama`
                            if idx < tokens.len() && tokens[idx].token == Token::Colon {
                                idx += 1; // lewati ':'
                                if idx < tokens.len()
                                    && matches!(tokens[idx].token, Token::Ident(_))
                                {
                                    idx += 1; // lewati nama label
                                }
                            }
                            break;
                        }
                        _ => {}
                    }
                    idx += 1;
                }
                // `else` cabang opsional: lewati hingga `;` atau `end`.
                if idx < tokens.len() && tokens[idx].token == Token::Else {
                    idx += 1; // skip 'else'
                    while idx < tokens.len() {
                        match tokens[idx].token {
                            Token::Semi => {
                                idx += 1;
                                break;
                            }
                            Token::End => {
                                idx += 1; // lewati 'end'
                                          // Opsional label: `end : nama`
                                if idx < tokens.len() && tokens[idx].token == Token::Colon {
                                    idx += 1; // lewati ':'
                                    if idx < tokens.len()
                                        && matches!(tokens[idx].token, Token::Ident(_))
                                    {
                                        idx += 1; // lewati nama label
                                    }
                                }
                                break;
                            }
                            _ => {}
                        }
                        idx += 1;
                    }
                }
            }
            _ => {
                // Konstruk yang tidak didukung di level modul — lewati satu
                // token dan lanjutkan, agar error tidak merambat ke
                // `endmodule` dan menyesatkan lokasi bug sebenarnya.
                idx += 1;
            }
        }
    }

    if idx >= tokens.len() {
        return Err("expected 'endmodule'".to_string());
    }
    // `endmodule : name` bentuk labeled endmodule (LRM §12.9).
    if idx + 1 < tokens.len() && tokens[idx + 1].token == Token::Colon {
        // skip ':' and label name
    }

    // Deklarasi lokal di dalam blok prosedural diberi nama unik per module,
    // sekaligus rujukannya ditulis ulang mengikuti scope blok.
    super::local_scope::unik_nama_modul(&mut statements);

    Ok(Module {
        name,
        span: module_span,
        params,
        ports,
        declarations,
        statements,
        instances,
        routines,
        typedefs,
        generates,
        localparams,
        genvars,
        time_scale: sv_ast::time_scale::TimeScale::default(),
    })
}

/// True bila token membentuk deklarasi bertipe, misal `byte_t data;`.
///
/// Bentuk yang dikenali: identifier (nama tipe), lalu opsional `signed`/
/// `unsigned` dan `[ ... ]`, lalu identifier (nama signal), lalu penutup yang
/// menandai akhir deklarasi (`;`, `,`, atau `=`). Instansiasi modul selalu
/// punya `(` atau `#(` sebelum nama signal, jadi tidak tertangkap di sini.
fn typed_declaration(tokens: &[SpannedToken]) -> bool {
    if !matches!(tokens.first().map(|t| &t.token), Some(Token::Ident(_))) {
        return false;
    }
    let mut idx = 1;
    if matches!(
        tokens.get(idx).map(|t| &t.token),
        Some(Token::Signed) | Some(Token::Unsigned)
    ) {
        idx += 1;
    }
    if tokens.get(idx).map(|t| &t.token) == Some(&Token::LBracket) {
        while idx < tokens.len() && tokens[idx].token != Token::RBracket {
            idx += 1;
        }
        if tokens.get(idx).map(|t| &t.token) != Some(&Token::RBracket) {
            return false;
        }
        idx += 1;
    }
    if !matches!(tokens.get(idx).map(|t| &t.token), Some(Token::Ident(_))) {
        return false;
    }
    idx += 1;
    matches!(
        tokens.get(idx).map(|t| &t.token),
        Some(Token::Semi) | Some(Token::Comma) | Some(Token::Eq)
    )
}

/// Parse deklarasi `genvar a, b, c;` pada body modul (LRM §27.3).
///
/// Mengembalikan daftar nama dan jumlah token yang dikonsumsi.
fn parse_genvar_declaration(tokens: &[SpannedToken]) -> Result<(Vec<String>, usize), String> {
    let mut idx = 1; // lewati keyword `genvar`
    let mut nama = Vec::new();
    loop {
        match tokens.get(idx).map(|t| &t.token) {
            Some(Token::Ident(n)) => nama.push(n.clone()),
            other => {
                return Err(format!(
                    "expected nama genvar setelah 'genvar', dapat {:?}",
                    other
                ))
            }
        }
        idx += 1;
        match tokens.get(idx).map(|t| &t.token) {
            Some(Token::Comma) => idx += 1,
            Some(Token::Semi) => {
                idx += 1;
                break;
            }
            other => {
                return Err(format!(
                    "expected ',' atau ';' pada deklarasi genvar, dapat {:?}",
                    other
                ))
            }
        }
    }
    Ok((nama, idx))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sv_lexer::lex;

    #[test]
    fn localparam_dalam_daftar_parameter_masuk_scope_modul() {
        let src = "module m #(parameter int A = 4, localparam int B = A * 2) \
                   (input [B-1:0] x); endmodule";
        let module = parse_module(&lex(src).unwrap()).unwrap();
        assert_eq!(module.params.len(), 1);
        assert_eq!(module.localparams.len(), 1);
        assert_eq!(module.localparams[0].name, "B");
        assert_eq!(module.ports[0].name, "x");
    }

    #[test]
    fn routine_function_disimpan_di_module() {
        let src =
            "module m; function automatic byte f(input [7:0] x); f = x; endfunction endmodule";
        let module = parse_module(&lex(src).unwrap()).unwrap();
        assert_eq!(module.routines.len(), 1);
        assert_eq!(module.routines[0].name, "f");
    }

    // BUG: parser port ANSI memperlakukan nama port sebagai nama tipe, lalu
    // memakan `)` sehingga daftar port berjalan sampai `endmodule` dan error
    // E0002 dilaporkan di baris `endmodule` (lokasi sesat).
    #[test]
    fn port_ansi_tipe_dan_nama_tidak_saling_memakan() {
        let tokens = lex("module m(input logic [7:0] a, b, output logic y); endmodule").unwrap();
        let module = parse_module(&tokens).unwrap();
        let nama: Vec<&str> = module.ports.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(nama, vec!["a", "b", "y"]);
        assert_eq!(
            module.ports[1].width,
            sv_ast::width::WidthExpr::Literal(8),
            "deklarator lanjutan berbagi lebar port sebelumnya"
        );
    }

    #[test]
    fn port_tanpa_arah_mewarisi_arah_sebelumnya() {
        let tokens = lex("module m(output logic a, b); endmodule").unwrap();
        let module = parse_module(&tokens).unwrap();
        assert_eq!(
            module.ports[1].direction,
            sv_ast::port::PortDirection::Output
        );
    }

    #[test]
    fn port_bertipe_nama_diikuti_nama_port() {
        let tokens = lex("module m(input my_t a, output my_t b); endmodule").unwrap();
        let module = parse_module(&tokens).unwrap();
        assert_eq!(module.ports[0].name, "a");
        assert_eq!(module.ports[1].name, "b");
    }

    #[test]
    fn port_dengan_dimensi_unpacked_setelah_nama() {
        let tokens = lex("module m(input [7:0] d[Share], output o); endmodule").unwrap();
        let module = parse_module(&tokens).unwrap();
        assert_eq!(module.ports[0].name, "d");
        assert_eq!(module.ports.len(), 2);
    }

    #[test]
    fn port_bertipe_scope_paket() {
        let tokens = lex("module m(input prim_pkg::mubi4_t v, output o); endmodule").unwrap();
        let module = parse_module(&tokens).unwrap();
        assert_eq!(module.ports[0].name, "v");
    }

    #[test]
    fn port_list_tidak_tertutup_melaporkan_token_penyebab() {
        let tokens = lex("module m(input a; endmodule").unwrap();
        let err = parse_module(&tokens).unwrap_err();
        assert!(err.contains("penutup port list"), "pesan: {err}");
        assert!(err.contains("Semi"), "pesan: {err}");
    }

    // BUG: `genvar i, j;` di body modul ditolak sebagai
    // "unexpected token in module body: Genvar" (LRM §27.3).

    #[test]
    fn deklarasi_genvar_terpisah_di_body_modul_diterima() {
        let tokens = lex("module m; genvar i, j; endmodule").unwrap();
        let module = parse_module(&tokens).unwrap();
        assert_eq!(module.genvars, vec!["i".to_string(), "j".to_string()]);
    }

    #[test]
    fn deklarasi_genvar_tunggal_diterima() {
        let tokens = lex("module m; genvar k; endmodule").unwrap();
        let module = parse_module(&tokens).unwrap();
        assert_eq!(module.genvars, vec!["k".to_string()]);
    }

    #[test]
    fn deklarasi_genvar_ganda_tidak_diduplikasi() {
        let tokens = lex("module m; genvar i; genvar i; endmodule").unwrap();
        let module = parse_module(&tokens).unwrap();
        assert_eq!(module.genvars, vec!["i".to_string()]);
    }

    #[test]
    fn deklarasi_genvar_tanpa_nama_ditolak() {
        let tokens = lex("module m; genvar; endmodule").unwrap();
        let err = parse_module(&tokens).unwrap_err();
        assert!(err.contains("nama genvar"), "pesan: {err}");
    }

    #[test]
    fn deklarasi_genvar_tanpa_semikolon_ditolak() {
        let tokens = lex("module m; genvar i endmodule").unwrap();
        let err = parse_module(&tokens).unwrap_err();
        assert!(err.contains("',' atau ';'"), "pesan: {err}");
    }

    #[test]
    fn modul_tanpa_genvar_memisi_daftar_kosong() {
        let tokens = lex("module m; endmodule").unwrap();
        let module = parse_module(&tokens).unwrap();
        assert!(module.genvars.is_empty());
    }

    #[test]
    fn parse_stage0_empty_module() {
        let tokens = lex("module hello; endmodule").unwrap();
        let module = parse_module(&tokens).unwrap();
        assert_eq!(module.name, "hello");
        assert_eq!(module.ports.len(), 0);
        assert_eq!(module.declarations.len(), 0);
        assert_eq!(module.statements.len(), 0);
    }

    #[test]
    fn parse_stage1_assign() {
        let src = "module and_gate(input a, output y); logic b; assign y = a & b; endmodule";
        let tokens = lex(src).unwrap();
        let module = parse_module(&tokens).unwrap();
        assert_eq!(module.name, "and_gate");
        assert_eq!(module.ports.len(), 2);
        assert_eq!(module.declarations.len(), 1);
        assert_eq!(module.statements.len(), 1);
    }

    #[test]
    fn parse_stage2_always_ff() {
        let src = "module dff(input clk, input d, output q); always_ff @(posedge clk) begin q <= d; end endmodule";
        let tokens = lex(src).unwrap();
        let module = parse_module(&tokens).unwrap();
        assert_eq!(module.name, "dff");
        assert_eq!(module.ports.len(), 3);
        assert_eq!(module.statements.len(), 1);
    }

    #[test]
    fn parse_stage3_always_comb_case() {
        let src = "module mux(input [1:0] sel, input [3:0] a, input [3:0] b, output [3:0] y); \
                   always_comb begin \
                     case (sel) \
                       0: y = a; \
                       1: y = b; \
                       default: y = 0; \
                     endcase \
                   end endmodule";
        let tokens = lex(src).unwrap();
        let module = parse_module(&tokens).unwrap();
        assert_eq!(module.name, "mux");
        assert_eq!(module.ports.len(), 4);
        assert_eq!(module.statements.len(), 1);

        match &module.statements[0] {
            sv_ast::statement::Statement::AlwaysComb { body, .. } => {
                assert_eq!(body.len(), 1);
                match &body[0] {
                    sv_ast::combinational::CombinationalStatement::Case { arms, .. } => {
                        assert_eq!(arms.len(), 3);
                        assert!(arms[2].is_default);
                    }
                    _ => panic!("expected case statement"),
                }
            }
            _ => panic!("expected always_comb"),
        }
    }

    #[test]
    fn parse_stage4_parameter_width() {
        let src = "module adder #(parameter WIDTH = 8) (input [WIDTH-1:0] a, input [WIDTH-1:0] b, output [WIDTH-1:0] y); assign y = a + b; endmodule";
        let tokens = lex(src).unwrap();
        let module = parse_module(&tokens).unwrap();
        assert_eq!(module.name, "adder");
        assert_eq!(module.params.len(), 1);
        assert_eq!(module.params[0].name, "WIDTH");
        assert_eq!(module.params[0].default, 8);
        assert_eq!(module.ports.len(), 3);
        assert_eq!(
            module.ports[0].width,
            sv_ast::width::WidthExpr::ParamMinusOne("WIDTH".to_string())
        );
    }

    #[test]
    fn parse_stage3_always_comb_if_else() {
        let src = "module gate(input a, input b, output y); \
                   always_comb begin \
                     if (a) y = b; else y = 0; \
                   end endmodule";
        let tokens = lex(src).unwrap();
        let module = parse_module(&tokens).unwrap();
        assert_eq!(module.statements.len(), 1);

        match &module.statements[0] {
            sv_ast::statement::Statement::AlwaysComb { body, .. } => {
                assert_eq!(body.len(), 1);
                assert!(matches!(
                    body[0],
                    sv_ast::combinational::CombinationalStatement::IfElse { .. }
                ));
            }
            _ => panic!("expected always_comb"),
        }
    }

    // --- BUG-2: qualifier tipe pada port ---

    #[test]
    fn port_logic_diterima() {
        let src = "module m(input logic [3:0] a, output logic [3:0] y); assign y = a; endmodule";
        let module = parse_module(&lex(src).unwrap()).unwrap();
        assert_eq!(module.ports.len(), 2);
        assert_eq!(module.ports[0].name, "a");
        assert_eq!(module.ports[0].width, sv_ast::width::WidthExpr::Literal(4));
    }

    #[test]
    fn port_output_reg_diterima() {
        let src = "module dff(input clk, input d, output reg q); \
                   always_ff @(posedge clk) q <= d; endmodule";
        let module = parse_module(&lex(src).unwrap()).unwrap();
        assert_eq!(module.ports.len(), 3);
        assert_eq!(module.ports[2].name, "q");
    }

    #[test]
    fn port_wire_diterima() {
        let src = "module m(input wire [7:0] a, output wire [7:0] y); assign y = a; endmodule";
        let module = parse_module(&lex(src).unwrap()).unwrap();
        assert_eq!(module.ports.len(), 2);
    }

    #[test]
    fn qualifier_tanpa_dimension_tetap_diparse() {
        // `output reg y` tanpa packed dimension harus sah dan berlebar 1.
        let src = "module m(input a, output reg y); assign y = a; endmodule";
        let module = parse_module(&lex(src).unwrap()).unwrap();
        assert_eq!(module.ports[1].name, "y");
        assert_eq!(module.ports[1].width, sv_ast::width::WidthExpr::Literal(1));
    }

    #[test]
    fn qualifier_bersamaan_dan_dimension_keduanya_ok() {
        let src =
            "module m(input logic [3:0] a, output logic reg [3:0] y); assign y = a; endmodule";
        let module = parse_module(&lex(src).unwrap()).unwrap();
        assert_eq!(module.ports.len(), 2);
        assert_eq!(module.ports[1].width, sv_ast::width::WidthExpr::Literal(4));
    }

    #[test]
    fn daftar_port_tanpa_qualifier_tetap_ok() {
        // Tidak boleh ada regresi pada bentuk yang sudah didukung sebelumnya.
        let src = "module m(input [3:0] a, output [3:0] y); assign y = a; endmodule";
        let module = parse_module(&lex(src).unwrap()).unwrap();
        assert_eq!(module.ports.len(), 2);
    }

    // --- LRM §27.1: region generate ---

    #[test]
    fn generate_region_terkumpul_di_bidang_generate() {
        let src = "module m(output [3:0] y); \
                   generate for (genvar i = 0; i < 4; i = i + 1) begin : g \
                     assign y = y; \
                   end endgenerate endmodule";
        let module = parse_module(&lex(src).unwrap()).unwrap();
        assert_eq!(module.generates.len(), 1);
        assert_eq!(module.generates[0].genvars, vec!["i".to_string()]);
        // Isi generate tidak bocor ke statements modul biasa.
        assert!(module.statements.is_empty());
    }

    #[test]
    fn modul_tanpa_generate_punya_bidang_kosong() {
        let src = "module m(input a, output y); assign y = a; endmodule";
        let module = parse_module(&lex(src).unwrap()).unwrap();
        assert!(module.generates.is_empty());
    }

    #[test]
    fn beberapa_region_generate_berurutan() {
        let src = "module m(output [3:0] y); \
                   generate for (genvar i = 0; i < 2; i = i + 1) begin : a \
                     assign y = 1'b1; \
                   end endgenerate \
                   generate for (genvar j = 2; j < 4; j = j + 1) begin : b \
                     assign y = 1'b0; \
                   end endgenerate endmodule";
        let module = parse_module(&lex(src).unwrap()).unwrap();
        assert_eq!(module.generates.len(), 2);
    }

    #[test]
    fn loop_generate_tidak_pernah_selesai_ditolak_di_parser() {
        // Kesalahan harus tertangkap saat parse, bukan menggantung elaborasi.
        let src = "module m(output [3:0] y); \
                   generate for (genvar i = 0; i < 4; i--) begin : g \
                     assign y = 1'b1; \
                   end endgenerate endmodule";
        let err = parse_module(&lex(src).unwrap()).unwrap_err();
        assert!(err.contains("tidak pernah selesai"), "{}", err);
    }

    #[test]
    fn indeks_genvar_pada_lvalue_generate_tersimpan_sebagai_genvar() {
        // Pola kanonik `assign y[i]` harus tersimpan sebagai indeks genvar,
        // bukan irisan literal; nilainya baru diketahui saat elaborasi.
        let src = "module m(output [3:0] y); \
                   generate for (genvar i = 0; i < 4; i = i + 1) begin : g \
                     assign y[i] = 1'b1; \
                   end endgenerate endmodule";
        let module = parse_module(&lex(src).unwrap()).unwrap();
        match &module.generates[0].items[0] {
            sv_ast::generate::GenerateItem::For(f) => match &f.body[0] {
                sv_ast::generate::GenerateItem::Assign(inner) => {
                    let (lhs, _, _) = inner.as_ref();
                    assert_eq!(lhs.name, "y");
                    assert_eq!(lhs.genvar_index.as_deref(), Some("i"));
                    assert!(lhs.slice.is_none());
                }
                other => panic!("harus assign, dapat {:?}", other),
            },
            other => panic!("harus for-generate, dapat {:?}", other),
        }
    }
}
