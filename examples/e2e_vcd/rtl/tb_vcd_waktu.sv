// Tanggung jawab: testbench demo timestamp VCD memakai waktu simulasi
// (LRM §21.8 + §23.2), bukan nomor langkah driver.
`timescale 1ns/1ns
module tb_vcd_waktu;
  logic a = 0;
  initial begin
    $dumpfile("vcd_waktu.vcd");
    $dumpvars;
    #10 a = 1;
    #10 a = 0;
    #10 $finish;
  end
endmodule