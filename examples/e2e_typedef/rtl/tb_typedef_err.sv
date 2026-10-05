// Kasus typedef/enum yang harus ditolak elaborator (LRM §8.20, §6.7).
//
// Nama anggota enum melebihi lebar tipe dasar, sehingga harus ditolak.
module tb_bad_enum;
  typedef enum logic [2:0] { A = 9 } e_t;
  e_t q;

  always_comb q = A;
endmodule