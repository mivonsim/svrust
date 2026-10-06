// Tanggung jawab: testbench demo dua proses waktu independen (LRM §4.4).
// Tiap proses punya jam bangun sendiri — `#5` dan `#3` tidak boleh saling
// menumpuk menjadi `#8`, dan jumlah iterasi tiap proses tidak boleh memengaruhi
// satu sama lain.
`timescale 1ns/1ns
module tb_dua_jam;
  logic a = 0;
  logic b = 0;
  integer na = 0;
  integer nb = 0;

  // Dua proses waktu pada modul yang sama: masing-masing menambah waktunya
  // sendiri dari waktu bangunnya, bukan dari jam global.
  always #5 a = ~a;
  always #3 b = ~b;

  // LRM §9.7: hanya posedge nyata yang memicu, bukan tiap langkah driver.
  always @(posedge a) na = na + 1;
  always @(posedge b) nb = nb + 1;

  initial begin
    #30 $display("na=%0d nb=%0d t=%0t", na, nb, $time);
    $finish;
  end
endmodule
