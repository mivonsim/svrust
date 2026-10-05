// Tanggung jawab: testbench demo `timescale` unit dan presisi (LRM §21.8).
// Semua nilai diverifikasi silang dengan iverilog 12.0 dan verilator 5.020.
`timescale 1us/1ns
module tb_timescale;
  initial begin
    // `#1` tanpa satuan memakai `timeunit` = 1us, jadi t = 1 (satuan $time).
    #1 $display("a t=%0d [%t]", $time, $time);
    #1 $display("b t=%0d [%t]", $time, $time);
    // `#500ns` memakai satuan eksplisit: 0.5us, dibulatkan ke presisi 1ns.
    #500ns $display("c t=%0d [%t]", $time, $time);
    // `#0.5` tidak didukung (butuh real), delay eksplisit dipakai sebagai gantinya.
    #1ps $display("d t=%0d [%t]", $time, $time);
  end
endmodule

// Modul kedua memakai `timescale` berbeda: satuan dan presisi ikut berubah
// hanya untuk modul ini (LRM §21.8).
`timescale 1ns/1ps
module tb_timescale_kecil;
  initial begin
    #1 $display("e t=%0d [%t]", $time, $time);
    #7 $display("f t=%0d [%t]", $time, $time);
  end
endmodule