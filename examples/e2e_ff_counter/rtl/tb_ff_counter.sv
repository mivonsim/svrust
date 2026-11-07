// Tanggung jawab: demo `always_ff` dievaluasi melalui step_scheduler
// (LRM §4.4 region event NonBlocking). Tanpa blok `initial`, driver
// menggunakan `step_scheduler()` yang memanggil `eval_seq` untuk proses
// sekuensial, sehingga counter bertambah pada setiap posedge clock.
module tb_ff_counter(output logic [7:0] cnt_o);
  logic clk;
  logic [7:0] cnt = 0;
  always #5 clk = ~clk;
  always_ff @(posedge clk) cnt <= cnt + 1;
  assign cnt_o = cnt;
endmodule
