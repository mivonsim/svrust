module tb_assert;
  logic clk;
  logic [7:0] cnt;

  always #5 clk = ~clk;

  initial begin
    clk = 0;
    cnt = 8'd0;
    #20;
    cnt = 8'd200;
    #20;
    $finish;
  end

  // SVA concurrent assertion (LRM §20/§21): cnt harus <= 8'd127 di setiap posedge clk.
  // cnt=0 pada t=0 lulus; cnt=200 setelah #20 gagal.
  assert property (@(posedge clk) cnt <= 8'd127);

endmodule
