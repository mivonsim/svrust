// Tanggung jawab: testbench demo system task di dalam badan always_ff.
module tb_task_wrap;
  logic clk;
  logic din;
  logic [3:0] q;

  gen_task dut (.clk(clk), .din(din), .q(q));

  initial begin
    clk = 1'b0;
    din = 1'b1;
    #1 clk = 1'b1;
    #1 clk = 1'b0;
    #1 clk = 1'b1;
    #1 clk = 1'b0;
    #1 clk = 1'b1;
    #1 clk = 1'b0;
    #1 $finish;
  end
endmodule