// Tanggung jawab: demo e2e async reset (event gabungan) dan keyword `always` polos.
module tb_async(
  input logic clk,
  output logic [7:0] q,
  output logic [7:0] r
);
  logic rst;
  logic [7:0] total;

  // LRM §9.7: `@(posedge clk or posedge rst)` — reset asinkron, proses
  // dievaluasi saat salah satu edge terjadi.
  always_ff @(posedge clk or posedge rst)
    if (rst) q <= 8'd0;
    else q <= q + 8'd1;

  // LRM §9.2.1: `always` polos dengan edge list = proses sekuensial.
  always @(posedge clk) r <= r + 8'd2;

  // LRM §9.4.1: `always @(a or b)` level-sensitive = combinational.
  always @(q or r) total = q + r;

  initial begin
    q = 8'd0;
    r = 8'd0;
    rst = 1'b1;
    $display("s0 rst=1 q=%0d r=%0d total=%0d", q, r, total);
    #4 rst = 1'b0;
    $display("s1 rst=0 q=%0d r=%0d total=%0d", q, r, total);
    #4 $display("s2 q=%0d r=%0d total=%0d", q, r, total);
    #4 rst = 1'b1;
    $display("s3 rst=1 q=%0d r=%0d total=%0d", q, r, total);
    #4 $finish;
  end
endmodule
