// Tanggung jawab: test klasifikasi `always` polos sebagai proses waktu
// (`always #N`) atau sensitivitas implisit (LRM §9.2.1 + §11.2).
use sv_ast::combinational::CombinationalStatement;
use sv_ast::statement::Statement;
use sv_lexer::spanned::SpannedToken;
use sv_lexer::token::Token;

/// Parse modul satu statement lalu kembalikan statement modul pertamanya.
fn parse_satu(src: &str) -> Statement {
    let tokens: Vec<SpannedToken> =
        sv_lexer::lex(&format!("module m; logic c; {src} endmodule")).expect("lex");
    let module = sv_parser::parse_module(&tokens).expect("parse");
    module.statements.into_iter().next().expect("ada statement")
}

/// Klasifikasi statement `always`: proses waktu, sekuensial, atau combinational.
fn always(src: &str) -> Statement {
    parse_satu(src)
}

/// Badannya, apa pun jenisnya.
fn badan(src: &str) -> Vec<CombinationalStatement> {
    match parse_satu(src) {
        Statement::AlwaysTimed { body, .. }
        | Statement::AlwaysComb { body, .. }
        | Statement::Initial { body, .. } => body,
        other => panic!("bukan always: {other:?}"),
    }
}

/// BUG: `@(posedge clk)` di dalam body `always` sempat ikut dihitung sebagai
/// timing control, sehingga bentuk ini dipetakan ke proses waktu. Akibatnya
/// proses tidak lagi ditangguhkan, jam simulasi ikut bergerak diam-diam, dan
/// blok `initial` kehilangan seluruh `$display` (keluar kosong).
/// `@(...)` menyuspensi proses, bukan menggeser waktu simulasi.
#[test]
fn always_dengan_event_control_di_body_bukan_proses_waktu() {
    assert!(
        matches!(
            always("always begin @(posedge c) c = ~c; end"),
            Statement::AlwaysComb { .. }
        ),
        "event control menyuspensi proses, bukan menggerakkan jam"
    );
}

/// BUG: predikat `hanya_penundaan` menuntut SELURUH statement adalah delay,
/// lalu rekursinya memanggil `all()` atas `[BlockingAssign]` yang jatuh ke
/// `_ => false`. Predikat itu selalu salah, jadi `Statement::AlwaysTimed` tidak
/// pernah terbentuk dan `always #5 clk = ~clk;` dipetakan ke `always_comb` —
/// penundaannya dieksekusi tiap langkah driver, jam tidak pernah menghasilkan
/// edge, dan `@(posedge clk)` tak pernah terpenuhi.
#[test]
fn always_dengan_delay_leading_jadi_proses_waktu() {
    assert!(
        matches!(always("always #5 c = ~c;"), Statement::AlwaysTimed { .. }),
        "`always #5 c = ~c;` harus jadi AlwaysTimed"
    );
}

/// Bentuk yang sama ditulis dengan blok `begin...end` — idiom paling umum di
/// RTL dan testbench. Padanya berlaku klasifikasi yang persis sama.
#[test]
fn always_blok_dengan_delay_leading_jadi_proses_waktu() {
    assert!(
        matches!(
            always("always begin #5 c = ~c; end"),
            Statement::AlwaysTimed { .. }
        ),
        "`always begin #5 ... end` harus jadi AlwaysTimed"
    );
}

/// Delay di AKHIR body, bukan di depan. Diverifikasi terhadap iverilog 12.0:
/// `always begin c = ~c; #5; end` tetap proses waktu yang menggeser jam, jadi
/// penelusuran ke seluruh statement wajib ada — memeriksa statement pertama
/// saja akan salah.
#[test]
fn always_dengan_delay_di_akhir_body_jadi_proses_waktu() {
    assert!(
        matches!(
            always("always begin c = ~c; #5; end"),
            Statement::AlwaysTimed { .. }
        ),
        "delay di akhir body juga proses waktu (iverilog 12.0)"
    );
}

/// Delay bersarang di dalam `if` juga menunda proses. Diverifikasi terhadap
/// iverilog 12.0: jam simulasi bergerak dan `c` berayun tiap periode.
#[test]
fn always_dengan_delay_di_dalam_if_jadi_proses_waktu() {
    assert!(
        matches!(
            always("always begin if (1'b1) #5 c = ~c; end"),
            Statement::AlwaysTimed { .. }
        ),
        "delay di dalam `if` juga proses waktu (iverilog 12.0)"
    );
}

/// Delay di dalam loop `while`/`repeat` juga proses waktu.
///
/// Loop `for` memakai variabel yang dideklarasikan di luar modul — SVRust belum
/// menerima deklarasi `int i` di dalam `for`, jadi bentuk itu diuji terpisah di
/// `always_timed_for_test`.
#[test]
fn always_dengan_delay_di_dalam_loop_jadi_proses_waktu() {
    for src in [
        "always begin while (1'b1) #5 c = ~c; end",
        "always begin repeat (2) #5 c = ~c; end",
    ] {
        assert!(
            matches!(always(src), Statement::AlwaysTimed { .. }),
            "delay di dalam loop harus proses waktu: {src}"
        );
    }
}

/// Delay di dalam loop `for` juga proses waktu — `for` diuji dengan variabel
/// modul karena deklarasi di dalam header `for` belum didukung parser.
#[test]
fn always_dengan_delay_di_dalam_for_jadi_proses_waktu() {
    let tokens = sv_lexer::lex(
        "module m; logic c; integer i; always begin for (i = 0; i < 1; i = i + 1) #5 c = ~c; end endmodule",
    )
    .expect("lex");
    let module = sv_parser::parse_module(&tokens).expect("parse");
    assert!(
        matches!(
            module.statements.into_iter().next(),
            Some(Statement::AlwaysTimed { .. })
        ),
        "delay di dalam `for` juga proses waktu"
    );
}

/// Tanpa timing control apa pun, `always` TIDAK boleh jadi proses waktu — akan
/// menjadi loop tanpa waktu yang tidak bisa disimulasikan (iverilog 12.0
/// menolaknya di elaborasi dengan pesan `always process does not have any
/// delay`).
#[test]
fn always_tanpa_delay_tetap_sensitivitas_implisit() {
    assert!(
        matches!(always("always c = ~c;"), Statement::AlwaysComb { .. }),
        "`always` tanpa delay tetap combinational"
    );
    assert!(
        matches!(
            always("always begin c = ~c; end"),
            Statement::AlwaysComb { .. }
        ),
        "`always begin ... end` tanpa delay tetap combinational"
    );
}

/// Body proses waktu tetap menyimpan statement-nya utuh — elaborator memakai
/// `#delay` itu untuk menggeser jam simulasi.
#[test]
fn badan_proses_waktu_memuat_delay_dan_assignment() {
    let body = badan("always #5 c = ~c;");
    assert_eq!(body.len(), 1, "satu statement: {body:?}");
    assert!(
        matches!(body[0], CombinationalStatement::Delay { .. }),
        "statement pertama harus Delay: {body:?}"
    );
}

/// `@(...)` mengembalikan always ke jalur sensitivitas — daftar event tidak
/// boleh salah dikira sebagai proses waktu.
#[test]
fn always_dengan_daftar_event_bukan_proses_waktu() {
    assert!(
        matches!(
            always("always @(posedge c) begin c = ~c; end"),
            Statement::AlwaysFf { .. }
        ),
        "`always @(posedge c)` harus jadi AlwaysFf"
    );
    assert!(
        matches!(
            always("always @* begin c = ~c; end"),
            Statement::AlwaysComb { .. }
        ),
        "`always @*` harus jadi AlwaysComb"
    );
    assert!(
        matches!(
            always("always @(*) begin c = ~c; end"),
            Statement::AlwaysComb { .. }
        ),
        "`always @(*)` harus jadi AlwaysComb"
    );
}

/// BUG: `always_comb` eksplisit punya kontrak sendiri (LRM §9.2.1) dan tidak
/// boleh ikut diturunkan ke jalur `always` polos — kalau tidak, keyword yang
/// eksplisit bisa berakhir di `AlwaysTimed` dan menggeser jam simulasi.
#[test]
fn always_comb_eksplisit_tidak_bisa_jadi_proses_waktu() {
    let tokens = sv_lexer::lex("module m; logic c; always_comb #5 c = ~c; endmodule").expect("lex");
    let module = sv_parser::parse_module(&tokens).expect("parse");
    assert!(
        matches!(
            module.statements.into_iter().next(),
            Some(Statement::AlwaysComb { .. })
        ),
        "`always_comb` tidak boleh jadi AlwaysTimed"
    );
}

/// Token `At` tetap menjadi penentu utama klasifikasi `always`.
#[test]
fn token_at_menentukan_klasifikasi() {
    let tokens =
        sv_lexer::lex("module m; logic c; always @(posedge c) c = ~c; endmodule").expect("lex");
    assert!(
        tokens.iter().any(|t| t.token == Token::At),
        "token At harus terdeteksi"
    );
    assert!(
        matches!(
            always("always @(posedge c) c = ~c;"),
            Statement::AlwaysFf { .. }
        ),
        "event list menghasilkan AlwaysFf"
    );
}
