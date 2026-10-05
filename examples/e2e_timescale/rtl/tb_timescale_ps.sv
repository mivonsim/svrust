// Tanggung jawab: testbench demo presisi `timescale` 1ns/1ps pada $time/%t.
`timescale 1ns/1ps
module tb_timescale_ps;
  initial begin
    #1 $display("g t=%0d [%t]", $time, $time);
    #1500 $display("h t=%0d [%t]", $time, $time);
  end
endmodule