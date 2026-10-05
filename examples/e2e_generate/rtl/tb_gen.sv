// Tanggung jawab: demo e2e generate for yang meng-instantiate modul anak per iterasi.
module inv #(parameter W = 1) (input a, output y);
  assign y = ~a;
endmodule

module tb_gen(
  input  logic       din,
  output logic [3:0] q
);
  // LRM §27.3: loop generate memakai genvar; tiap iterasi meng-instantiate
  // modul anak dan menyambungkan bit ke-i lewat indeks genvar `q[i]`.
  generate
    for (genvar i = 0; i < 4; i = i + 1) begin : g
      inv #(.W(1)) u (.a(din), .y(q[i]));
    end
  endgenerate

  initial begin
    din = 1'b0;
    #1 $display("din=0 gen=%0d", q);
    #1 din = 1'b1;
    #1 $display("din=1 gen=%0d", q);
    #1 din = 1'b0;
    #1 $display("din=0 lagi gen=%0d", q);
    #1 $finish;
  end
endmodule