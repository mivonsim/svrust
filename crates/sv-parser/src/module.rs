// Tanggung jawab: parse module SystemVerilog.
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
    let mut out = Vec::new();
    loop {
        let span = tokens
            .get(idx)
            .map(|t| t.span)
            .ok_or_else(|| "expected nama parameter".to_string())?;
        let Token::Ident(name) = &tokens[idx].token else {
            return Err("expected nama parameter".to_string());
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
            return Err(format!("expected '=' setelah nama parameter '{name}'"));
        }
        idx += 1;
        let (expr, eaten) = super::expression::parse_expression(&tokens[idx..])?;
        idx += eaten;
        let sv_ast::expression::Expr::Number(nilai) = expr else {
            return Err(format!(
                "nilai default parameter '{name}' harus literal di body modul"
            ));
        };
        out.push(sv_ast::width::ParamDecl {
            name,
            default: nilai,
            span,
        });
        if tokens.get(idx).map(|t| &t.token) == Some(&Token::Comma) {
            idx += 1;
            continue;
        }
        break;
    }
    if tokens.get(idx).map(|t| &t.token) != Some(&Token::Semi) {
        return Err("expected ';' setelah parameter".to_string());
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
                     // Tipe opsional: `localparam int X = 1;` (LRM §6.20.1).
    if tokens
        .get(idx)
        .is_some_and(|t| sv_lexer::token::tipe::bisa_mulai_deklarasi(&t.token))
    {
        idx += 1;
    }
    let mut out = Vec::new();
    loop {
        let span = tokens
            .get(idx)
            .map(|t| t.span)
            .ok_or_else(|| "expected nama localparam".to_string())?;
        let Token::Ident(name) = &tokens[idx].token else {
            return Err("expected nama localparam".to_string());
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
            return Err(format!("expected '=' setelah nama localparam '{name}'"));
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
        return Err("expected ';' setelah localparam".to_string());
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

    // Daftar parameter opsional: #(parameter WIDTH = 8, ...)
    let (mut params, makan) = super::param::parse_param_list(&tokens[idx..])?;
    idx += makan;

    // Port list optional: ( ... ) or empty
    let mut ports = Vec::new();
    if idx < tokens.len() && tokens[idx].token == Token::LParen {
        idx += 1;
        while idx < tokens.len() && tokens[idx].token != Token::RParen {
            // Parse port: direction [width] name
            let direction = match &tokens[idx].token {
                Token::Input => sv_ast::port::PortDirection::Input,
                Token::Output => sv_ast::port::PortDirection::Output,
                Token::Inout => sv_ast::port::PortDirection::Inout,
                _ => return Err("expected port direction".to_string()),
            };
            idx += 1;

            // Qualifier tipe opsional sebelum dimension: `logic`/`wire`/`reg`/`bit`.
            while idx < tokens.len() && is_net_type(&tokens[idx].token) {
                idx += 1;
            }

            // Qualifier signedness opsional setelah tipe: `signed`/`unsigned`.
            let tipe_bulat = matches!(tokens[idx].token, Token::Integer | Token::Int);
            let signed = match tokens.get(idx).map(|t| &t.token) {
                Some(&Token::Signed) => true,
                Some(&Token::Unsigned) => false,
                _ => tipe_bulat,
            };
            if idx < tokens.len() && is_signedness(&tokens[idx].token) {
                idx += 1;
            }

            // Optional packed dimension: [N] atau [MSB:LSB]
            let (lebar_tertulis, consumed) = super::dimension::parse_dimension(tokens, idx)?;
            idx += consumed;
            // `integer`/`int` selalu 32-bit signed dan tidak perlu dimension.
            let width = if tipe_bulat {
                sv_ast::width::WidthExpr::Literal(32)
            } else {
                lebar_tertulis
            };

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

            // Optional comma
            if idx < tokens.len() && tokens[idx].token == Token::Comma {
                idx += 1;
            }
        }
        idx += 1; // skip ')'
    }

    // Expect ';'
    if idx >= tokens.len() || tokens[idx].token != Token::Semi {
        return Err("expected ';'".to_string());
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
    let mut localparams = Vec::new();

    while idx < tokens.len() && tokens[idx].token != Token::Endmodule {
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
                let (stmt, next) = super::statement::parse_continuous_assign(&tokens[idx..])?;
                statements.push(stmt);
                idx += next;
            }
            Token::AlwaysFf => {
                let (stmt, next) = super::always_ff::parse_always_ff(&tokens[idx..])?;
                statements.push(stmt);
                idx += next;
            }
            // LRM §9.2.1: `always` polos — edge list → sekuensial, selain itu
            // (level list / `@*` / tanpa `@`) → combinational.
            Token::Always => {
                let (stmt, next) = super::always_ff::parse_always(&tokens[idx..])?;
                statements.push(stmt);
                idx += next;
            }
            Token::AlwaysComb => {
                let (stmt, next) = super::always_comb::parse_always_comb(&tokens[idx..])?;
                statements.push(stmt);
                idx += next;
            }
            // Testbench: blok `initial` untuk stimulus dan system task.
            Token::Initial => {
                let (stmt, next) = super::initial::parse_initial(&tokens[idx..])?;
                statements.push(stmt);
                idx += next;
            }
            // LRM §8.20 dan §6.7: deklarasi `typedef` di body modul.
            Token::Typedef => {
                let (decl, next) = super::typedef::parse_typedef(&tokens[idx..])?;
                typedefs.push(decl);
                idx += next;
            }
            // LRM §13.3/§13.4: deklarasi task dan function di body modul.
            Token::Task | Token::Function => {
                let (decl, next) = super::routine::parse_routine(&tokens[idx..])?;
                routines.push(decl);
                idx += next;
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
                        return Err("expected ';' setelah deklarasi bertipe".to_string());
                    }
                    idx += akhir + 1;
                    continue;
                }
                let (inst, next) = super::instance::parse_instance(&tokens[idx..])?;
                instances.push(inst);
                idx += next;
            }
            // LRM §27.3: `genvar i, j;` di body modul mendeklarasikan variabel
            // loop generate. Nama yang dideklarasikan di sini lalu boleh
            // dipakai di `for (i = ...)` tanpa mengulang keyword `genvar`.
            Token::Genvar => {
                let (nama, next) = parse_genvar_declaration(&tokens[idx..])?;
                for n in nama {
                    if !genvars.contains(&n) {
                        genvars.push(n);
                    }
                }
                idx += next;
            }
            // LRM §27.1: region `generate ... endgenerate` disimpan terpisah
            // agar elaborator bisa meng-instantiate tiap iterasi dengan nilai
            // genvar yang berbeda.
            Token::Generate => {
                let (region, next) =
                    super::generate::parse_generate_region(&tokens[idx..], 0, &genvars)?;
                generates.push(region);
                idx += next;
            }
            _ => {
                return Err(format!(
                    "unexpected token in module body: {:?}",
                    tokens[idx].token
                ))
            }
        }
    }

    if idx >= tokens.len() {
        return Err("expected 'endmodule'".to_string());
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
                sv_ast::generate::GenerateItem::Assign { lhs, .. } => {
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
