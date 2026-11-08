// Tanggung jawab: modul root pertama tanpa `initial` — proses waktu `always #N`
// sebagai output root yang terpisah saat dijalankan tanpa `--top` (LRM §23.1).
module root_a(output logic [7:0] y);
  logic [7:0] cnt = 0;
  always begin #2 cnt = cnt + 1; end
  assign y = cnt;
endmodule
