// Tanggung jawab: demo scope per-iterasi generate (LRM §27.4).
module buf1(input d, output q);
  assign q = d;
endmodule

module gen_scope(
  input  logic       din,
  output logic [3:0] q
);
  // LRM §27.4: tiap iterasi loop adalah blok hierarki tersendiri, sehingga
  // `t` yang dideklarasikan di dalam loop harus menjadi empat sinyal berbeda.
  // Koneksi `.q(q[i])` memakai indeks genvar, sedangkan `.q(t)` memakai
  // sinyal lokal blok iterasi tersebut.
  generate
    for (genvar i = 0; i < 4; i = i + 1) begin : g
      logic t;
      buf1 b1 (.d(din), .q(t));
      buf1 b2 (.d(t), .q(q[i]));
    end
  endgenerate

  initial begin
    din = 1'b1;
    #1 $display("scope din=1 gen=%0d", q);
    #1 din = 1'b0;
    #1 $display("scope din=0 gen=%0d", q);
    #1 $finish;
  end
endmodule