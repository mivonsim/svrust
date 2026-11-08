// Tanggung jawab: demo proses waktu `always #N` tanpa blok `initial`
// (LRM §9.2.1, §11.2). Driver plain (tanpa VCD, tanpa sequential) harus
// memanggil `eval_timed()` di dalam loop scheduler sehingga penundaan
// di dalam body benar-benar menunda penulangan dan counter bertambah.
module tb(output logic [7:0] cnt_o);
  logic [7:0] cnt = 0;
  always begin #2 cnt = cnt + 1; end
  assign cnt_o = cnt;
endmodule
