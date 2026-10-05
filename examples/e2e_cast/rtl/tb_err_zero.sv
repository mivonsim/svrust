// Tanggung jawab: error size cast dengan lebar nol (LRM §6.14).
//
// Lebar harus positif; `0'(a)` tidak sah.
module tb_size_nol;
  logic [15:0] a;
  logic [15:0] y;
  initial y = 0'(a);
endmodule
