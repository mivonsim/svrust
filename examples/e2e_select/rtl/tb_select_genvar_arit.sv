// Tanggung jawab: berkas error indeks genvar berpola aritmetika (LRM §27).
// `r[i-1]` di dalam loop generate dulu diklaim lengkap padahal `- 1` dibuang,
// sehingga penulisan jatuh pada bit yang salah tanpa pesan apa pun.
module tb_select_genvar_arit;
  logic [7:0] r;
  genvar i;
  generate
    for (i = 1; i < 4; i = i + 1) begin : g
      always_comb r[i - 1] = 1'b1;
    end
  endgenerate
endmodule