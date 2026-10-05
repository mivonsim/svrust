// Tanggung jawab: integrasi parameter modul anak saat elaborasi.
use sv_elaborator::elaborate_top;
use sv_lexer::lex;
use sv_parser::parse_file;

/// Elaborate satu sumber SV dengan top tertentu.
fn desain(src: &str, top: &str) -> Result<sv_ir::Design, String> {
    let tokens = lex(src).map_err(|e| e.to_string())?;
    let modules = parse_file(&tokens)?;
    elaborate_top(&modules, top).map_err(|e| e.to_string())
}

#[test]
fn parameter_anak_dipakai_di_ekspresi_badan_anak() {
    // `SEL` adalah parameter modul anak. Former buggy: `lingkup_expr`
    // memberi prefix instans sehingga dicari sinyal `dut__SEL` yang tidak ada.
    let src = "module child #(parameter SEL = 1) (input [7:0] a, output [7:0] y); \
               assign y = a >> SEL; endmodule \
               module top; \
               logic [7:0] x, z; \
               child #(.SEL(3)) dut (.a(x), .y(z)); \
               endmodule";
    let d = desain(src, "top").expect("elaborate harus berhasil");
    assert!(d.find_variable("z").is_some());
}

#[test]
fn parameter_anak_tidak_jadi_nama_berprefiks_instans() {
    // Kalau parameter ikut di-prefix, design akan memuat sinyal `dut__SEL`.
    let src = "module child #(parameter SEL = 3) (input [7:0] a, output [7:0] y); \
               assign y = a >> SEL; endmodule \
               module top; \
               logic [7:0] x, z; \
               child dut (.a(x), .y(z)); \
               endmodule";
    let d = desain(src, "top").expect("elaborate");
    assert!(
        d.find_variable("dut__SEL").is_none(),
        "parameter tidak boleh menjadi sinyal berprefiks instans"
    );
}

#[test]
fn nilai_override_parameter_anak_terpakai() {
    // Dua instansi dengan SEL berbeda harus memberi proses berbeda; bila
    // parameter tidak ter-inline, keduanya memakai nilai yang sama.
    let src = "module child #(parameter SEL = 1) (input [7:0] a, output [7:0] y); \
               assign y = a >> SEL; endmodule \
               module top; \
               logic [7:0] x, p, q; \
               child #(.SEL(2)) u0 (.a(x), .y(p)); \
               child #(.SEL(4)) u1 (.a(x), .y(q)); \
               endmodule";
    let d = desain(src, "top").expect("elaborate");
    assert!(d.find_variable("p").is_some());
    assert!(d.find_variable("q").is_some());
}

#[test]
fn parameter_dipakai_di_width_port_anak() {
    // Width `[W-1:0]` sudah didukung lewat ParamTable; test ini menjaga
    // agar tidak rusak oleh perubahan scoping parameter.
    let src = "module child #(parameter W = 4) (input [W-1:0] a, output [W-1:0] y); \
               assign y = a; endmodule \
               module top; \
               logic [7:0] x, z; \
               child #(.W(8)) dut (.a(x), .y(z)); \
               endmodule";
    let d = desain(src, "top").expect("elaborate");
    let z = d.find_variable("z").expect("sinyal z");
    assert_eq!(z.data_type.width, 8);
}

// BUG: `lower_instance` tidak pernah memproses `child.instances`, sehingga
// modul tingkat ketiga dan seterusnya hilang tanpa error.
#[test]
fn hierarki_tiga_tingkat_tetap_elaborate() {
    let src = "module leaf(input a, output y); assign y = ~a; endmodule \
               module mid(input a, output y); \
                 leaf u (.a(a), .y(y)); \
               endmodule \
               module top; \
                 logic i, o; \
                 mid m (.a(i), .y(o)); \
               endmodule";
    let d = desain(src, "top").expect("elaborate");
    let proses_combinational = d
        .processes
        .iter()
        .filter(|p| p.name.starts_with("assign"))
        .count();
    assert_eq!(
        proses_combinational, 1,
        "assign dari leaf harus ikut terelaborasi"
    );
}

#[test]
fn hierarki_empat_tingkat_tetap_elaborate() {
    let src = "module l4(input a, output y); assign y = ~a; endmodule \
               module l3(input a, output y); l4 u (.a(a), .y(y)); endmodule \
               module l2(input a, output y); l3 u (.a(a), .y(y)); endmodule \
               module top; logic i, o; l2 t (.a(i), .y(o)); endmodule";
    let d = desain(src, "top").expect("elaborate");
    assert_eq!(
        d.processes
            .iter()
            .filter(|p| p.name.starts_with("assign"))
            .count(),
        1
    );
}
