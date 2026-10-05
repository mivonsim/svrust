// Tanggung jawab: konversi satuan waktu pada #delay untuk demo e2e.
module tb_unit;
  initial begin
    #1 $display("t=%0d", $time);
    #1ns $display("t=%0d", $time);
    #1000ps $display("t=%0d", $time);
    #1us $display("t=%0d", $time);
    #1ms $display("t=%0d", $time);
    #1s $display("t=%0d", $time);
    $finish;
  end
endmodule
