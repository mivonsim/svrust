// Tanggung jawab: testbench demo dua blok `initial` yang berjalan paralel
// (LRM §4.4/§11.2). Keduanya harus resume pada waktu yang sama.
`timescale 1ns/1ns
module tb_paralel;
  initial begin
    $display("a0 t=%0t", $time);
    #5 $display("a1 t=%0t", $time);
    #5 $display("a2 t=%0t", $time);
  end
  initial begin
    $display("b0 t=%0t", $time);
    #5 $display("b1 t=%0t", $time);
    #5 $display("b2 t=%0t", $time);
  end
endmodule