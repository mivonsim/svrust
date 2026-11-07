// Tanggung jawab: demo pemanggilan task di dalam always_ff (LRM §13.3).
// Badan task di-inline sebagai statement sekuensial via comb_to_seq.
// Task di dalam always_ff dievaluasi pada setiap edge clock.
module tb_task_ff;
  logic clk;
  logic [7:0] cnt;

  task t(input logic [7:0] v);
    cnt = v;
  endtask

  always_ff @(posedge clk) begin
    t(8'h42);
  end

  initial begin
    clk = 1'b0;
    #1 clk = 1'b1;
    #1 clk = 1'b0;
    $display("cnt=%h", cnt);        // 42
    $finish;
  end
endmodule
