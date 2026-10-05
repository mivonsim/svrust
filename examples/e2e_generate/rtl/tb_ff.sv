// Tanggung jawab: testbench demo proses di dalam region generate.
module tb_ff_wrap;
  logic clk;
  logic din;
  logic [3:0] q;

  gen_ff dut (.clk(clk), .din(din), .q(q));

  initial begin
    clk = 1'b0;
    din = 1'b1;
    // Satu langkah untuk assert clock, satu lagi untuk settle NBA.
    #1 clk = 1'b1;
    #1 $display("ff a clk=%b din=%b q=%b", clk, din, q);
    clk = 1'b0;
    din = 1'b0;
    #1 clk = 1'b1;
    #1 $display("ff b clk=%b din=%b q=%b", clk, din, q);
    clk = 1'b0;
    din = 1'b1;
    #1 clk = 1'b1;
    #1 $display("ff c clk=%b din=%b q=%b", clk, din, q);
    #1 $finish;
  end
endmodule