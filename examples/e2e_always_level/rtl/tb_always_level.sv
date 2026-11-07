// Tanggung jawab: demo `always @(sinyal)` level-sensitive (LRM §9.2.1).
// Proses dievaluasi setiap kali sinyal dalam daftar sensitivity berubah nilai,
// bukan setiap langkah driver (bukan `always_comb`).
module tb_always_level;
  logic clk;
  logic [7:0] cnt;

  // Level-sensitive: dievaluasi setiap kali `clk` berubah (0↔1), bukan edge.
  always @(clk) begin
    cnt <= cnt + 1;
  end

  initial begin
    clk = 1'b0;
    cnt = 8'd0;
    #10 clk = 1'b1;   // clk 0->1, cnt <= 1
    #10 clk = 1'b0;   // clk 1->0, cnt <= 2
    #10 clk = 1'b1;   // clk 0->1, cnt <= 3
    #5  $display("cnt=%0d (expect 3)", cnt);
    $finish;
  end
endmodule
