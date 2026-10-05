// Tanggung jawab: testbench dengan blok initial, $display, dan $finish untuk demo e2e.
module tb_ops;
  logic [7:0] a;
  logic [7:0] b;
  logic signed [7:0] s;
  logic [3:0] m;

  // LRM §15.2: blok `initial` berjalan sekali pada waktu nol.
  initial begin
    a = 8'd42;
    b = 8'd3;
    m = 4'hA;
    s = -8'd1;
    $display("a=%d b=%d", a, b);
    $display("jumlah=%d", a + b);
    $display("m=%h m=%b", m, m);
    $display("s=%d", s);
    $display("teks saja");
    $finish;
  end
endmodule