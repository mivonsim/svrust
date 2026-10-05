// Tanggung jawab: testbench demo lipatan konstanta geser aritmetik di sv-opt.
// Ekspresi konstan `>>>` harus terlipat dengan bit sign yang benar.
module tb_fold_shift;
  initial begin
    $display("fold sar_1=%h", 8'hF0 >>> 4);
    $display("fold shr_1=%h", 8'hF0 >> 4);
    $display("fold sar_2=%h", 8'shF0 >>> 1);
    $display("fold sar_besar=%h", 8'h80 >>> 20);
  end
endmodule