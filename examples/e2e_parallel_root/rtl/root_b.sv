// Tanggung jawab: modul root kedua tanpa `initial` — proses waktu `always #N`
// sebagai output root yang terpisah saat dijalankan tanpa `--top` (LRM §23.1).
module root_b(output logic [7:0] z);
  logic [7:0] cnt = 0;
  always begin #3 cnt = cnt + 1; end
  assign z = cnt;
endmodule
