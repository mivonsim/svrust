// Tanggung jawab: berkas error untuk seleksi di luar jangkauan (LRM §7.8).
// `a[100]` pada sinyal 8 bit harus ditolak saat elaborasi, bukan menghasilkan
// nol diam-diam.
module tb_select_oob;
  logic [7:0] a;
  logic [7:0] y;
  assign a = 8'hA5;
  assign y = a[100];
endmodule