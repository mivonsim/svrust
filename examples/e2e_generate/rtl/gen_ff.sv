// Tanggung jawab: demo proses di dalam region generate (LRM §27.1).
module gen_ff(
  input  logic       clk,
  input  logic       din,
  output logic [3:0] q
);
  // LRM §27.1: setiap iterasi generate boleh memuat proses. Empat proses
  // `always_ff` terpisah masing-masing menulis satu bit dengan NBA masked.
  // `logic tmp;` per iterasi juga harus menjadi sinyal terpisah.
  generate
    for (genvar i = 0; i < 4; i = i + 1) begin : g
      logic tmp;
      always_comb tmp = din;
      always_ff @(posedge clk) q[i] <= tmp;
    end
  endgenerate
endmodule