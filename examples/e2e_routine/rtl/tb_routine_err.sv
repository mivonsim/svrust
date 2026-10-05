// Kasus task/function yang harus ditolak elaborator (LRM §13.3/§13.4).
//
// Berkas ini tidak boleh berhasil dielaborasi; dipakai demo untuk memastikan
// pesan error menyebut subroutine yang bermasalah dan posisinya benar.
module tb_bad_arg;
  logic [7:0] a, y;

  function [7:0] jumlah(input [7:0] x, input [7:0] w);
    jumlah = x + w;
  endfunction

  // Panggilan dengan satu argumen, padahal deklarasi punya dua.
  always_comb y = jumlah(a);
endmodule