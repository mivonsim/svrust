// Tanggung jawab: testbench demo dua blok `initial` dengan rantai delay
// BERBEDA (LRM §4.4 stratified event queue, §11.2).
// Rantai pendek tidak boleh ikut bergeser mengikuti rantai panjang.
`timescale 1ns/1ns
module tb_rantai;
  initial begin
    #1;
    #100;
    $display("panjang t=%0t", $time);
  end
  initial begin
    #1;
    #1;
    #1;
    $display("pendek t=%0t", $time);
  end
endmodule

// Tiga proses dengan penundaan berbeda: masing-masing bangun pada waktunya
// sendiri, bukan pada waktu paling maju.
`timescale 1ns/1ns
module tb_tiga;
  initial begin #7 $display("a t=%0t", $time); #7 $display("a2 t=%0t", $time); end
  initial begin #11 $display("b t=%0t", $time); #11 $display("b2 t=%0t", $time); end
  initial begin #2 $display("c t=%0t", $time); #9 $display("c2 t=%0t", $time); end
endmodule