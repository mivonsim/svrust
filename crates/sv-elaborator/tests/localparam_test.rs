// Tanggung jawab: test `localparam` (LRM §6.20).
//
// `localparam` adalah konstanta lokal modul: boleh dipakai di mana saja dalam
// modul itu (body statement, dimension, kondisi `generate`, nilai awal
// deklarasi) tetapi tidak bisa di-override saat instansiasi — berbeda dari
// `parameter`.
//
// Seatinya di `Module::localparams` terpisah dari `params` karena kalau
// dicampur, `#(.NAMA(...))` akan diam-diam diterima untuk konstanta yang
// memang harus terkunci.
//
// Semua nilai diverifikasi terhadap iverilog 12.0 (`-g2012`).
mod support;

fn design(src: &str) -> sv_ir::Design {
    support::build(src)
}

fn top(src: &str) -> sv_ir::Design {
    support::build_top(src, "top")
}

/// Nilai ekspresi pada assignment pertama yang menulis ke `nama`.
fn nilai_nama(d: &sv_ir::Design, nama: &str) -> u64 {
    let expr = support::nilai_untuk(d, nama);
    let mut signals = vec![0u64; d.variables.len()];
    support::evaluasi(&expr, &mut signals)
}

#[test]
fn localparam_terkumpul_di_ast() {
    let tokens = sv_lexer::lex("module m #(parameter W = 4) ();\n  localparam D = 8;\nendmodule")
        .expect("lex");
    let m = sv_parser::parse_module(&tokens).expect("parse");
    assert_eq!(m.localparams.len(), 1);
    assert_eq!(m.localparams[0].name, "D");
    assert!(
        !m.localparams.is_empty(),
        "localparam harus terpisah dari daftar parameter port"
    );
}

#[test]
fn localparam_terpakai_di_body_statement() {
    // iverilog: L1=8
    let d = design(
        "module m #(parameter W = 4) (output logic [31:0] y);\n\
           localparam DEPTH = 8;\n\
           assign y = DEPTH;\n\
           endmodule\n",
    );
    assert_eq!(nilai_nama(&d, "y"), 8);
}

#[test]
fn localparam_merujuk_localparam_sebelumnya() {
    // Urutan deklarasi berarti: `HALF` dievaluasi setelah `DEPTH` sudah
    // diketahui. iverilog: L2=4
    let d = design(
        "module m #(parameter W = 4) (output logic [31:0] y);\n\
           localparam DEPTH = 8;\n\
           localparam HALF = DEPTH / 2;\n\
           assign y = HALF;\n\
           endmodule\n",
    );
    assert_eq!(nilai_nama(&d, "y"), 4);
}

#[test]
fn localparam_merujuk_parameter_modul() {
    // iverilog: L3=15 untuk `W = 4` -> (1 << 4) - 1
    let d = design(
        "module m #(parameter W = 4) (output logic [31:0] y);\n\
           localparam MASK = (1 << W) - 1;\n\
           assign y = MASK;\n\
           endmodule\n",
    );
    assert_eq!(nilai_nama(&d, "y"), 15);
}

#[test]
fn localparam_terpakai_pada_dimension() {
    // LRM §6.20: localparam boleh jadi batas dimension.
    let d = design(
        "module m #(parameter W = 4) ();\n\
           localparam DEPTH = 8;\n\
           logic [DEPTH-1:0] buf;\n\
           logic [31:0] y;\n\
           assign y = buf;\n\
           endmodule\n",
    );
    assert_eq!(d.find_variable("buf").expect("buf").data_type.width, 8);
}

#[test]
fn localparam_terpakai_pada_condition_generate() {
    // iverilog: L4=4
    let d = design(
        "module m #(parameter W = 4) (output logic [31:0] y);\n\
           localparam DEPTH = 8;\n\
           generate\n\
             if (DEPTH > 4) begin : g\n\
               assign y = W;\n\
             end else begin : h\n\
               assign y = 8'hFF;\n\
             end\n\
           endgenerate\n\
           endmodule\n",
    );
    assert_eq!(nilai_nama(&d, "y"), 4);
}

#[test]
fn localparam_bertipe_dan_berupa_r() {
    // LRM §6.20.1: `localparam int X = 1;` — qualifier tipe opsional, dan
    // beberapa nama boleh dipisah koma.
    let d = design(
        "module m #(parameter W = 4) (output logic [31:0] y);\n\
           localparam int A = 3;\n\
           localparam B = 5;\n\
           assign y = A + B;\n\
           endmodule\n",
    );
    assert_eq!(nilai_nama(&d, "y"), 8);
}

#[test]
fn localparam_tidak_bisa_di_override() {
    // LRM §6.20: hanya `parameter` yang bisa di-override. Kalau `localparam`
    // tercampur ke daftar parameter port, override diam-diam diterima.
    let err = support::err_top(
        "module c #(parameter W = 4) (output logic [7:0] y);\n\
           localparam LOCKED = 8;\n\
           assign y = LOCKED;\n\
           endmodule\n\
         module top(output logic [7:0] o);\n\
           c #(.LOCKED(3)) u0(.y(o));\n\
           endmodule\n",
        "top",
    );
    assert!(err.contains("LOCKED"), "pesan tak menyebut nama: {err}");
}

#[test]
fn localparam_anak_memakai_parameter_setelah_override() {
    // iverilog: MASK anak memakai W=8 -> (1 << 8) - 1 = 255
    let d = top("module c #(parameter W = 4) (output logic [7:0] y);\n\
           localparam MASK = (1 << W) - 1;\n\
           assign y = MASK;\n\
           endmodule\n\
         module top(output logic [7:0] o);\n\
           c #(.W(8)) u0(.y(o));\n\
           endmodule\n");
    assert_eq!(nilai_nama(&d, "o"), 255);
}

#[test]
fn localparam_ganda_ditolak() {
    let tokens = sv_lexer::lex(
        "module m #(parameter W = 4) ();\n  localparam A = 1;\n  localparam A = 2;\nendmodule",
    )
    .expect("lex");
    let m = sv_parser::parse_module(&tokens).expect("parse");
    let err = sv_elaborator::elaborate(&m).expect_err("localparam ganda harus ditolak");
    assert!(
        err.to_string().contains('A'),
        "pesan tak menyebut nama: {err}"
    );
}

#[test]
fn localparam_menabrak_parameter_modul_ditolak() {
    let tokens = sv_lexer::lex("module m #(parameter W = 4) ();\n  localparam W = 8;\nendmodule")
        .expect("lex");
    let m = sv_parser::parse_module(&tokens).expect("parse");
    assert!(
        sv_elaborator::elaborate(&m).is_err(),
        "nama yang sama untuk parameter dan localparam harus ditolak"
    );
}

#[test]
fn localparam_tanpa_nilai_ditolak() {
    let tokens =
        sv_lexer::lex("module m #(parameter W = 4) ();\n  localparam A;\nendmodule").expect("lex");
    assert!(
        sv_parser::parse_module(&tokens).is_err(),
        "localparam tanpa nilai harus ditolak"
    );
}
