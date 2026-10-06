// Tanggung jawab: testbench demo proses waktu `always #N` (LRM §9.2.1 + §11.2):
// jam simulasi yang benar-benar bergerak, edge clock nyata, dan penjadwalan
// blok `initial` berdasarkan waktu — bukan nomor langkah driver.
`timescale 1ns/1ns
module tb_jam;
  logic clk = 0;
  logic [7:0] cnt = 0;

  // LRM §11.2: proses WAKTU — badan diulang terus-menerus, `#5` yang menentukan
  // kapan iterasi berikutnya jalan.
  always #5 clk = ~clk;

  // LRM §9.7: proses sekuensial hanya jalan pada posedge NYATA clk.
  always_ff @(posedge clk) cnt <= cnt + 1;

  // LRM §4.4: blok `initial` resumed pada waktu bangunnya, bukan pada langkah
  // driver ke-2.
  initial begin
    #52 $display("jum=t=%0t", $time);
    #10 $display("jum2=t=%0t", $time);
    $display("cnt=%0d", cnt);
    $finish;
  end
endmodule
