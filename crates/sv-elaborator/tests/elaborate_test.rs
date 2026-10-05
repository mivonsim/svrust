// Tanggung jawab: integration test elaborasi SV source menjadi IR design.
use sv_elaborator::design::DesignStats;
use sv_elaborator::elaborate;
use sv_ir::process::{EdgeSensitivity, ProcessKind};

fn build(source: &str) -> sv_ir::Design {
    let tokens = sv_lexer::lex(source).expect("lex");
    let ast = sv_parser::parse_module(&tokens).expect("parse");
    elaborate(&ast).expect("elaborate")
}

#[test]
fn elaborate_empty_module() {
    let design = build("module hello; endmodule");
    assert_eq!(design.name, "hello");
    assert_eq!(DesignStats::of(&design).ports, 0);
}

#[test]
fn elaborate_continuous_assign() {
    let design = build("module and_gate(input a, input b, output y); assign y = a & b; endmodule");
    let stats = DesignStats::of(&design);
    assert_eq!(stats.ports, 3);
    assert_eq!(stats.signals, 3);
    assert_eq!(stats.processes, 1);

    let process = &design.processes[0];
    assert_eq!(process.kind, ProcessKind::Combinational);
    // Sensitif terhadap a dan b, bukan y.
    assert_eq!(process.sensitivity.len(), 2);
    assert!(process
        .sensitivity
        .iter()
        .all(|s| s.edge == EdgeSensitivity::AnyChange));
}

#[test]
fn elaborate_always_ff_detects_posedge() {
    let design = build("module dff(input clk, input d, output q); always_ff @(posedge clk) begin q <= d; end endmodule");
    let process = &design.processes[0];
    assert_eq!(process.kind, ProcessKind::Sequential);
    assert_eq!(process.sensitivity.len(), 1);
    assert_eq!(process.sensitivity[0].edge, EdgeSensitivity::Posedge);
}

#[test]
fn elaborate_always_ff_event_gabungan_async_reset() {
    // BUG-46: `@(posedge clk or posedge rst)` menurunkan dua sensitivitas
    // dengan edge masing-masing, bukan hanya clock pertama.
    let design = build(
        "module m(input clk, input rst, output logic [7:0] q); \
         always_ff @(posedge clk or posedge rst) \
           if (rst) q <= 8'd0; else q <= q + 8'd1; endmodule",
    );
    let process = &design.processes[0];
    assert_eq!(process.kind, ProcessKind::Sequential);
    assert_eq!(process.sensitivity.len(), 2);
    assert_eq!(process.sensitivity[0].signal, 0); // clk
    assert_eq!(process.sensitivity[1].signal, 1); // rst
    assert!(process
        .sensitivity
        .iter()
        .all(|s| s.edge == EdgeSensitivity::Posedge));
    // BUG-48: body `if (rst) ... else ...` turun menjadi statement `If`.
    assert!(matches!(
        process.body[0],
        sv_ir::process::Statement::If { .. }
    ));
}

#[test]
fn signal_ids_match_declaration_order() {
    let design = build(
        "module m(input a, input b, output y); logic t; assign y = a & t; assign t = b; endmodule",
    );
    // a=0, b=1, y=2, t=3
    assert_eq!(design.variables[0].name, "a");
    assert_eq!(design.variables[0].signal_id, 0);
    assert_eq!(design.variables[2].name, "y");
    assert_eq!(design.variables[2].signal_id, 2);
    assert_eq!(design.variables[3].name, "t");
    assert_eq!(design.variables[3].signal_id, 3);
}

#[test]
fn width_propagates_from_declaration() {
    let design = build(
        "module m(input [7:0] a, input [3:0] b, output [7:0] y); assign y = a & b; endmodule",
    );
    let a = design.find_variable("a").expect("a");
    assert_eq!(a.data_type.width, 8);
    let b = design.find_variable("b").expect("b");
    assert_eq!(b.data_type.width, 4);
}

#[test]
fn binary_result_width_is_max_operand() {
    let design = build(
        "module m(input [7:0] a, input [15:0] b, output [15:0] y); assign y = a & b; endmodule",
    );
    let process = &design.processes[0];
    match &process.body[0] {
        sv_ir::Statement::Assign { assignment, .. } => {
            assert_eq!(assignment.value.data_type().width, 16);
        }
        _ => panic!("expected assignment"),
    }
}

#[test]
fn undefined_signal_is_reported() {
    let tokens = sv_lexer::lex("module m(output y); assign y = zzz; endmodule").expect("lex");
    let ast = sv_parser::parse_module(&tokens).expect("parse");
    let result = elaborate(&ast);
    assert!(result.is_err());
    assert!(result.err().unwrap().message.contains("zzz"));
}

#[test]
fn duplicate_signal_is_reported() {
    let tokens = sv_lexer::lex("module m(input a); logic a; endmodule").expect("lex");
    let ast = sv_parser::parse_module(&tokens).expect("parse");
    let result = elaborate(&ast);
    assert!(result.is_err());
    assert!(result.err().unwrap().message.contains("duplicate"));
}

#[test]
fn param_width_tresolve_dari_default() {
    let design = build(
        "module adder #(parameter WIDTH = 8) (input [WIDTH-1:0] a, input [WIDTH-1:0] b, output [WIDTH-1:0] y); assign y = a + b; endmodule",
    );
    let a = design.find_variable("a").expect("a");
    assert_eq!(a.data_type.width, 8);
    let y = design.find_variable("y").expect("y");
    assert_eq!(y.data_type.width, 8);
}

#[test]
fn param_tak_dikenal_dilaporkan() {
    let tokens = sv_lexer::lex("module m(input [DEPTH-1:0] a, output y); assign y = a; endmodule")
        .expect("lex");
    let ast = sv_parser::parse_module(&tokens).expect("parse");
    let result = elaborate(&ast);
    assert!(result.is_err());
    assert!(result.err().unwrap().message.contains("DEPTH"));
}

#[test]
fn always_comb_sensitivity_collects_all_reads() {
    let source = "module m(input [1:0] sel, input [3:0] a, input [3:0] b, output [3:0] y); \
                 always_comb begin \
                   case (sel) \
                     0: y = a; \
                     default: y = b; \
                   endcase \
                 end endmodule";
    let design = build(source);
    let process = &design.processes[0];
    assert_eq!(process.kind, ProcessKind::Combinational);
    // sel, a, b terbaca; y tidak.
    assert_eq!(process.sensitivity.len(), 3);
}

fn build_top(source: &str, top: &str) -> sv_ir::Design {
    let tokens = sv_lexer::lex(source).expect("lex");
    let modules = sv_parser::parse_file(&tokens).expect("parse");
    sv_elaborator::elaborate_top(&modules, top).expect("elaborate")
}

#[test]
fn instansi_flatten_dua_halfadd() {
    let design = build_top(
        "module halfadd(input a, input b, output s); assign s = a ^ b; endmodule \
         module top(input x, input y, output s0, output s1); \
         halfadd u0 (.a(x), .b(y), .s(s0)); \
         halfadd u1 (.a(x), .b(x), .s(s1)); \
         endmodule",
        "top",
    );
    assert_eq!(design.variables.len(), 4);
    assert_eq!(design.processes.len(), 2);
}

#[test]
fn override_parameter_mengubah_lebar() {
    let design = build_top(
        "module adder #(parameter WIDTH = 8) (input [WIDTH-1:0] a, output [WIDTH-1:0] y); assign y = a; endmodule \
         module top(input [15:0] x, output [15:0] y); \
         adder #(.WIDTH(16)) u0 (.a(x), .y(y)); \
         endmodule",
        "top",
    );
    let y = design.find_variable("y").expect("y");
    assert_eq!(y.data_type.width, 16);
}

#[test]
fn koneksi_lebar_beda_ditolak() {
    let tokens = sv_lexer::lex(
        "module adder #(parameter WIDTH = 8) (input [WIDTH-1:0] a, output [WIDTH-1:0] y); assign y = a; endmodule \
         module top(input [3:0] x, output [3:0] y); \
         adder u0 (.a(x), .y(y)); \
         endmodule",
    )
    .expect("lex");
    let modules = sv_parser::parse_file(&tokens).expect("parse");
    assert!(sv_elaborator::elaborate_top(&modules, "top").is_err());
}

#[test]
fn port_tak_tersambung_ditolak() {
    let tokens = sv_lexer::lex(
        "module halfadd(input a, input b, output s); assign s = a; endmodule \
         module top(input x, output s); halfadd u0 (.a(x), .s(s)); endmodule",
    )
    .expect("lex");
    let modules = sv_parser::parse_file(&tokens).expect("parse");
    assert!(sv_elaborator::elaborate_top(&modules, "top").is_err());
}

#[test]
fn elaborate_initial_menjadi_proses_initial() {
    let design = build("module tb; logic [7:0] a; initial begin a = 8'd42; end endmodule");
    let process = design
        .processes
        .iter()
        .find(|p| p.kind == ProcessKind::Initial)
        .expect("proses initial");
    assert_eq!(process.body.len(), 1);
    // Blok initial tidak punya sensitivitas seperti proses combinational.
    assert!(process.sensitivity.is_empty());
}

#[test]
fn elaborate_display_menjadi_system_task_display() {
    let design = build(
        "module tb; logic [7:0] a; initial begin a = 8'd1; $display(\"a=%d\", a); end endmodule",
    );
    let process = design
        .processes
        .iter()
        .find(|p| p.kind == ProcessKind::Initial)
        .expect("proses initial");
    let task = process
        .body
        .iter()
        .find_map(|s| match s {
            sv_ir::process::Statement::SystemTask { kind, args, .. } => Some((*kind, args.len())),
            _ => None,
        })
        .expect("system task");
    assert_eq!(task.0, sv_ir::expr::SystemTaskKind::Display);
    // Satu argumen format + satu argumen nilai.
    assert_eq!(task.1, 2);
}

#[test]
fn elaborate_finish_menjadi_system_task_finish() {
    let design = build("module tb; initial begin $finish; end endmodule");
    let process = &design.processes[0];
    assert_eq!(process.kind, ProcessKind::Initial);
    assert!(matches!(
        process.body[0],
        sv_ir::process::Statement::SystemTask {
            kind: sv_ir::expr::SystemTaskKind::Finish,
            ..
        }
    ));
}

#[test]
fn argumen_display_membaca_sinyal_yang_benar() {
    let design =
        build("module tb; logic [7:0] a; initial begin $display(\"a=%d\", a); end endmodule");
    let process = &design.processes[0];
    let mut ids = Vec::new();
    sv_elaborator::lower_stmt::collect_statement_reads(&process.body[0], &mut ids);
    let a = design.find_variable("a").expect("a");
    assert_eq!(ids, vec![a.signal_id]);
}

#[test]
fn elaborate_delay_menjadi_statement_delay() {
    let design = build("module tb; logic [7:0] a; initial begin #5 a = 8'd2; end endmodule");
    let process = &design.processes[0];
    assert!(matches!(
        process.body[0],
        sv_ir::process::Statement::Delay { .. }
    ));
}

#[test]
fn elaborate_time_menjadi_ekspresi_simtime_64_bit() {
    let design = build("module tb; initial begin $display(\"t=%d\", $time); end endmodule");
    let process = &design.processes[0];
    let sv_ir::process::Statement::SystemTask { args, .. } = &process.body[0] else {
        panic!("harus system task");
    };
    let sv_ir::expr::SystemArg::Value(expr) = &args[1] else {
        panic!("argumen kedua harus nilai");
    };
    assert!(matches!(expr, sv_ir::Expr::SimTime { .. }));
    assert_eq!(expr.data_type().width, 64);
}

#[test]
fn delay_sebagai_badan_blok_yang_sama_masih_terurai() {
    let design =
        build("module tb; logic [7:0] a; initial begin a = 8'd1; #2 a = 8'd2; end endmodule");
    let process = &design.processes[0];
    assert_eq!(process.body.len(), 2);
    assert!(matches!(
        process.body[1],
        sv_ir::process::Statement::Delay { .. }
    ));
}

#[test]
fn elaborasi_menyalin_satuan_waktu_ke_ir() {
    let design = build("module tb; logic [7:0] a; initial begin #1ps a = 8'd1; end endmodule");
    let process = &design.processes[0];
    let sv_ir::process::Statement::Delay { unit, .. } = &process.body[0] else {
        panic!("harus delay");
    };
    assert_eq!(*unit, sv_ir::TimeUnit::PicoSeconds);
}

#[test]
fn elaborasi_satuan_tanpa_nama_menjadi_nanosecond() {
    let design = build("module tb; logic [7:0] a; initial begin #5 a = 8'd1; end endmodule");
    let process = &design.processes[0];
    let sv_ir::process::Statement::Delay { unit, .. } = &process.body[0] else {
        panic!("harus delay");
    };
    assert_eq!(*unit, sv_ir::TimeUnit::NanoSeconds);
}
