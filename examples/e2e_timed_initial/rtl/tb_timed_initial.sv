// Tanggung jawab: demo proses waktu `always #N` dengan blok `initial` dan
// delay di dalam `if` (LRM §9.2.1, §11.2, §25). Tanpa proses sekuensial,
// driver cabang "has initial, no sequential" harus memanggil `eval_timed()`
// agar `#delay` di dalam `if` benar-benar menunda proses dan counter
// bertambah hingga batas, lalu `$finish` menghentikan simulasi.
module tb(output logic [7:0] cnt_o);
  logic [7:0] cnt = 0;
  initial begin
    #10 $finish;
  end
  always begin
    if (cnt < 5) begin
      #2 cnt = cnt + 1;
    end
  end
  assign cnt_o = cnt;
endmodule
