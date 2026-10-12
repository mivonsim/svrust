// Tanggung jawab: testbench demo dua proses waktu dengan PERIODE BERBEDA
// (LRM §4.4 + §11.2). Tiap proses punya jam bangun sendiri — `#3` dan `#4`
// tidak boleh saling menumpuk, dan jumlah posedge tiap clock harus cocok
// iverilog.
`timescale 1ns/1ns
module tb_dua_periode;
  logic a = 0;
  logic b = 0;
  integer na = 0;
  integer nb = 0;

  always #3 a = ~a;
  always #4 b = ~b;

  // LRM §9.7: hanya posedge nyata yang memicu, bukan tiap langkah driver.
  always @(posedge a) na = na + 1;
  always @(posedge b) nb = nb + 1;

  initial begin
    #12 $display("na=%0d nb=%0d t=%0t", na, nb, $time);
    $finish;
  end
endmodule

// Tiga proses waktu: `#2` + `#3` + `#5`. Per-irisan periodenya menghasilkan
// event di t=2,3,4,5,6,7,8,9,10,12 — bukan kelipatan satu periode.
`timescale 1ns/1ns
module tb_tiga_periode;
  logic a = 0;
  logic b = 0;
  logic c = 0;
  integer na = 0;
  integer nb = 0;
  integer nc = 0;

  always #2 a = ~a;
  always #3 b = ~b;
  always #5 c = ~c;

  always @(posedge a) na = na + 1;
  always @(posedge b) nb = nb + 1;
  always @(posedge c) nc = nc + 1;

  initial begin
    #30 $display("na=%0d nb=%0d nc=%0d", na, nb, nc);
    $finish;
  end
endmodule
