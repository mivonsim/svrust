// Tanggung jawab: demo indeks genvar pada LHS di dalam loop generate (LRM §27.3).
module gen_lhs(
  input  logic       din,
  output logic [3:0] q
);
  // Pola kanonik SystemVerilog: indeks genvar pada sisi kiri assignment.
  // Tiap iterasi menulis satu bit berbeda dari `q`.
  generate
    for (genvar i = 0; i < 4; i = i + 1) begin : g
      assign q[i] = din;
    end
  endgenerate

  initial begin
    din = 1'b1;
    #1 $display("lhs din=1 gen=%0d", q);
    #1 din = 1'b0;
    #1 $display("lhs din=0 gen=%0d", q);
    #1 din = 1'b1;
    #1 $display("lhs din=1 lagi gen=%0d", q);
    #1 $finish;
  end
endmodule