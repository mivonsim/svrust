// Tanggung jawab: seleksi bit dan presedence operator untuk demo e2e.
module sel_ops(
  input  [7:0] a,
  output [3:0] part,
  output       bit0,
  output [7:0] comb,
  output [7:0] sel_not
);
  // BUG-6: part-select dan bit-select pada literal.
  assign part = 8'hB7[7:4];
  assign bit0 = 8'hB7[0];
  // `~` lebih kuat dari `+`, jadi ini `(~8'h0F) + 8'h01` = f0 + 01 = f1.
  assign comb = ~8'h0F + 8'h01;
  // Unary pada sinyal port (bukan literal) untuk memeriksa jalur sinyal.
  assign sel_not = ~a;
endmodule
