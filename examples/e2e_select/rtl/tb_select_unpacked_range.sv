// Tanggung jawab: berkas error part-select pada array unpacked (LRM §7.8).
// `mem[3:1]` adalah range select yang belum didukung; sebelumnyaAccepted dan
// hasilnya hanya satu elemen (8 bit) — nilai salah tanpa pesan.
module tb_select_unpacked_range;
  logic [7:0] mem [0:3];
  logic [31:0] y;
  assign mem[2] = 8'h3C;
  assign y = mem[3:1];
endmodule