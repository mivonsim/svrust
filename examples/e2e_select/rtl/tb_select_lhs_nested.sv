// Tanggung jawab: berkas error indeks bertingkat pada LHS (LRM §7.8).
// `y[3:0][1] = ...` dulu diterima parser lalu token `[1]` dibuang diam-diam.
module tb_select_lhs_nested;
  logic [7:0] y;
  always_comb y[3:0][1] = 1'b1;
endmodule