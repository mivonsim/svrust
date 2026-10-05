// Testbench parameter modul SystemVerilog (LRM §6.20).
//
// `parameter` adalah konstanta yang boleh dipakai di mana saja dalam modul
// yang mendeklarasikannya: body statement, nilai awal deklarasi, nilai awal
// deklarasi lokal di dalam `begin...end`, kondisi/label `generate`, dan
// dimension packed `[P]` / `[P-1:0]`.
//
// Sebelumnya modul **top** tidak punya jalur substitusi sama sekali — hanya
// modul anak yang mendapat parameter-nya di-inline saat instansiasi — sehingga
// `assign a = W;` gagal dengan "undefined signal 'W'" padahal kodenya sah.
// Condition `generate` punya masalah serupa.
//
// Semua nilai diverifikasi terhadap iverilog 12.0 (`-g2012`).
module tb_param #(parameter W = 4, parameter D = 3) ();
  // Body statement memakai parameter modul.
  logic [31:0] r1_val;    // `assign r1_val = W;`
  logic [31:0] r2_calc;   // `W * D`
  logic [31:0] r4_init;   // nilai awal deklarasi module-level

  // Nilai awal deklarasi lokal di dalam `begin...end`.
  logic [31:0] r5_local;

  // Condition generate memakai parameter.
  logic [31:0] r6_gen;
  logic [7:0]  r7_label;

  // Dimension packed berbasis parameter.
  logic [W-1:0] r8_dim;

  // Parameter di port dan di-override lewat instansiasi.
  logic [7:0]  r10_child;

  counter #(.W(8)) u_cnt (.q(r10_child));

  assign r1_val = W;
  assign r2_calc = W * D;
  logic [W-1:0] r4_seed = W;
  assign r4_init = r4_seed;
  logic [W-1:0] r8_seed = W;
  assign r8_dim = r8_seed;

  always_comb begin
    logic [W-1:0] tmp;
    tmp = W + 1;
    r5_local = tmp;
  end

  generate
    if (W == 4) begin : g_small
      assign r6_gen = W;
      assign r7_label = 8'(W);
    end else begin : g_big
      assign r6_gen = 8'hFF;
      assign r7_label = 8'hFF;
    end
  endgenerate

  initial begin
    #1;
    $display("R1=%0d", r1_val);
    $display("R2=%0d", r2_calc);
    $display("R4=%0d", r4_init);
    $display("R5=%0d", r5_local);
    $display("R6=%0d", r6_gen);
    $display("R7=%0d", r7_label);
    $display("R8=%0d", r8_dim);
    $display("R10=%0d", r10_child);
    $finish;
  end
endmodule

// Modul dengan parameter yang dipakai di banyak tempat (LRM §6.20).
module counter #(parameter W = 4) (output logic [W-1:0] q);
  // Body statement memakai parameter.
  logic [W-1:0] cur;
  assign cur = W;

  // Condition dan label `generate` memakai parameter.
  generate
    if (W == 4) begin : g_small
      logic [7:0] tag;
      assign tag = W;
    end else begin : g_big
      logic [7:0] tag;
      assign tag = W;
    end
  endgenerate

  // Deklarasi lokal di dalam blok procedural memakai parameter pada nilainya.
  always_comb begin
    logic [W-1:0] tmp;
    tmp = W;
    q = tmp;
  end
endmodule
