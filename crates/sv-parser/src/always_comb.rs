// Tanggung jawab: parse always_comb.
use sv_ast::combinational::CombinationalStatement;
use sv_ast::statement::Statement;
use sv_lexer::spanned::SpannedToken;
use sv_lexer::token::Token;

pub fn parse_always_comb(tokens: &[SpannedToken]) -> Result<(Statement, usize), String> {
    if tokens.is_empty() || tokens[0].token != Token::AlwaysComb {
        return Err("expected 'always_comb'".to_string());
    }
    let span = tokens[0].span;
    let mut idx = 1;

    // `always_comb @(a, b)` — daftar sensitivitas boleh ditulis tapi tidak
    // mengubah perilaku: simulator mengevaluasi ulang pada setiap langkah.
    if idx < tokens.len() && tokens[idx].token == Token::At {
        idx += 1;
        if idx >= tokens.len() || tokens[idx].token != Token::LParen {
            return Err("expected '(' after @".to_string());
        }
        idx += 1;
        while idx < tokens.len() && tokens[idx].token != Token::RParen {
            idx += 1;
        }
        if idx >= tokens.len() {
            return Err("expected ')'".to_string());
        }
        idx += 1;
    }

    let (body, next) = parse_comb_body(tokens, idx)?;
    Ok((Statement::AlwaysComb { body, span }, next))
}

/// Parse body proses combinational: blok `begin...end` atau satu statement.
///
/// Mengembalikan daftar statement dan jumlah token yang dikonsumsi. Dipakai
/// oleh `parse_always_comb` dan parser `always` polos supaya aturan body
/// hanya ditulis sekali.
pub(super) fn parse_comb_body(
    tokens: &[SpannedToken],
    mut idx: usize,
) -> Result<(Vec<CombinationalStatement>, usize), String> {
    if idx >= tokens.len() {
        return Err("expected body".to_string());
    }

    if tokens[idx].token == Token::Begin {
        idx += 1;
        let (stmts, consumed) = super::combinational::parse_combinational_block(&tokens[idx..])?;
        idx += consumed;
        if idx >= tokens.len() || tokens[idx].token != Token::End {
            return Err("expected 'end'".to_string());
        }
        idx += 1;
        return Ok((stmts, idx));
    }

    let (stmt, consumed) = super::combinational::parse_combinational_statement(&tokens[idx..])?;
    idx += consumed;
    Ok((vec![stmt], idx))
}

/// True bila badan proses memuat `#delay` di mana pun — proses WAKTU.
///
/// LRM §11.2 + §9.2.1: `always` tanpa `@(...)` yang memuat *procedural timing
/// control statement* adalah **proses waktu**: bodannya diulang terus-menerus
/// dan `#delay` di dalamnya yang menentukan kapan iterasi berikutnya berjalan.
/// Tanpa pemeriksaan ini `always #5 clk = ~clk;` dipetakan ke `always_comb`,
/// sehingga penundaannya dieksekusi pada setiap langkah driver dan jam tidak
/// pernah menghasilkan edge.
///
/// Delay di posisi APAPUN dalam body tetap proses waktu — diverifikasi
/// terhadap iverilog 12.0 untuk ketiga bentuk:
///
/// | bentuk | iverilog |
/// |---|---|
/// | `always #5 c = ~c;` | proses waktu |
/// | `always begin c = ~c; #5; end` (delay di akhir) | proses waktu |
/// | `always begin if (1) #5 c = ~c; end` (delay di dalam `if`) | proses waktu |
///
/// Bentuk tanpa timing control (`always c = ~c;`) tetap `always_comb` sebagai
/// sensitivitas implisit. Catatan: iverilog 12.0 justru MENOLAKnya di elaborasi
/// dengan pesan `always process does not have any delay`, karena bentuk itu
/// adalah loop tanpa waktu. SVRust menerimanya — divergensi tercatat di
/// `LRM_COMPLIANCE.md`.
///
/// `@(...)` di dalam body juga menangguhkan proses, jadi ikut dihitung sebagai
/// timing control.
///
/// BUG: versi lama bernama `hanya_penundaan` dan menuntut **seluruh** statement
/// adalah delay, lalu rekursinya memanggil `all()` atas `[BlockingAssign]` yang
/// jatuh ke `_ => false`. Akibatnya predikat selalu salah dan `AlwaysTimed`
/// tidak pernah terbentuk — jam simulasi tidak pernah bergerak sama sekali.
pub fn mengandung_delay(body: &[CombinationalStatement]) -> bool {
    body.iter().any(statement_memuat_delay)
}

/// True bila satu statement kompresional memuat `#delay` pada dirinya atau
/// turunannya (blok, `if`, `for`, `while`, `repeat`, `case`).
fn statement_memuat_delay(stmt: &CombinationalStatement) -> bool {
    match stmt {
        CombinationalStatement::Delay { .. } => true,
        CombinationalStatement::Block { body, .. } => mengandung_delay(body),
        CombinationalStatement::IfElse {
            then_branch,
            else_branch,
            ..
        } => {
            // Delay di dalam cabang `if` pun menunda proses, jadi kedua
            // cabang diperiksa; `else_branch` kosong tidak menambah apa pun.
            mengandung_delay(then_branch) || else_branch.as_deref().is_some_and(mengandung_delay)
        }
        CombinationalStatement::For { body, .. }
        | CombinationalStatement::Repeat { body, .. }
        | CombinationalStatement::While { body, .. } => mengandung_delay(body),
        // `@(posedge clk)` di dalam body MENYUSPENSIKAN proses, tapi tidak
        // menggeser waktu simulasi — jadi BUKAN proses waktu. Menganggapnya sebagai
        // proses waktu membuat jam simulasi diam-diam ikut berjalan dan blok
        // `initial` kehilangan `$display`-nya sama sekali.
        //
        // BUG: sempat dikembalikan `true` karena `@(...)` dianggap timing
        // control; akibatnya `always begin @(posedge c) ... end` tidak lagi
        // ditangguhkan dan design testbench kehilangan seluruh output.
        CombinationalStatement::EventControl { body, .. } => mengandung_delay(body),
        CombinationalStatement::Case { arms, .. } => {
            arms.iter().any(|arm| mengandung_delay(&arm.body))
        }
        CombinationalStatement::Decl(_)
        | CombinationalStatement::BlockingAssign { .. }
        | CombinationalStatement::NonBlockingAssign { .. }
        | CombinationalStatement::CompoundAssign { .. }
        | CombinationalStatement::SystemTask(_)
        | CombinationalStatement::TaskCall(_)
        | CombinationalStatement::Return { .. } => false,
    }
}
