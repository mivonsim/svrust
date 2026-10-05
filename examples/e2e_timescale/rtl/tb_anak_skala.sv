// Tanggung jawab: testbench demo `timescale` modul anak setelah flatten.
// LRM §21.8: `$time` dan `%t` memakai skala modul yang memuatnya, bukan skala
// modul top. Setelah instance di-flatten, skala itu harus ikut dibawa.
`timescale 1us/1ns
module anak_skala;
  initial begin
    #1 $display("anak t=%0d [%t]", $time, $time);
  end
endmodule

`timescale 1ns/1ns
module tb_anak_skala;
  initial begin
    #1 $display("top t=%0d [%t]", $time, $time);
  end
  anak_skala u();
endmodule