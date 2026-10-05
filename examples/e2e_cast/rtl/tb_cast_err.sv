// Testbench error untuk type cast: nama tipe yang tidak pernah di-typedef.
//
// LRM §8.20: `nama_tipe'(x)` harus menunjuk typedef yang ada. Kode ini memakai
// nama yang tidak pernah dideklarasikan, jadi elaborasi harus gagal dengan
// pesan yang menyebut nama tipenya.
module tb_bad_cast;
  typedef logic [7:0] byte_t;
  logic [15:0] a16;
  logic [7:0]  b;

  initial begin
    // `nibble_t` tidak pernah di-typedef di modul ini.
    b = nibble_t'(a16);
    $display("tidak boleh sampai sini: b=%h", b);
  end
endmodule
