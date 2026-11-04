// Tanggung jawab: demo indeks bertingkat pada LHS (LRM §7.8).
// `y[3:0][1] = 1'b1` ditulis ke bit 1 dari y (karena offset 0 + lsb extra 1 = 1).
module tb_select_lhs_nested;
  logic [7:0] y;
  always_comb y[3:0][1] = 1'b1;
  initial begin
    y = 8'h00;
    #1 $display("y=%0h", y);
    #1 $finish;
  end
endmodule
