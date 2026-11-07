// Tanggung jawab: demo scheduler-codegen mengedepankan proses sekuensial
// (LRM §4.4 NonBlocking phase). Tanpa blok `initial`, driver menggunakan
// `step_scheduler()` yang sekarang memanggil `eval_seq` untuk proses
// `always_ff @(posedge clk)`, sehingga counter bertambah pada setiap posedge.
module tb_sched_seq(output logic [7:0] cnt_o);
  logic clk;
  logic [7:0] cnt = 0;
  always #5 clk = ~clk;
  always_ff @(posedge clk) cnt <= cnt + 1;
  assign cnt_o = cnt;
endmodule
