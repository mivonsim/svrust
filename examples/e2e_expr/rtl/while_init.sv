// Tanggung jawab: loop while, tipe integer, dan inisialisasi deklarasi untuk demo e2e.
module while_init(
  output [7:0] hitung,
  output [7:0] faktorial,
  output [7:0] nol_iterasi,
  output [31:0] bulat,
  output [7:0] awal,
  output [7:0] awal_lokal,
  output [7:0] salin_net,
  output [7:0] incr_naik,
  output [7:0] incr_turun,
  output [7:0] incr_tumpuk
);
  logic [3:0] i;
  logic [3:0] f;

  // LRM §12.7.2: `while` mengulang selama kondisi benar.
  always_comb begin
    hitung = 8'd0;
    i = 4'd0;
    while (i < 5) begin
      hitung = hitung + 1;
      i = i + 1;
    end
  end

  // Faktorial 5 = 120 = 0x78, tapi `f` hanya `logic [3:0]` sehingga tiap
  // penugasan memotong ke 4 bit: 1, 2, 6, 8, 8. Hasil akhirnya 8, bukan 120.
  // Diverifikasi terhadap iverilog 12.0 (`-g2012`): `faktorial=08`.
  always_comb begin
    f = 4'd1;
    i = 4'd1;
    while (i <= 5) begin
      f = f * i;
      i = i + 1;
    end
    faktorial = {4'd0, f};
  end

  // Kondisi langsung salah => body tidak pernah jalan.
  always_comb begin
    i = 4'd7;
    while (i < 3) i = i + 1;
    nol_iterasi = {4'd0, i};
  end

  // LRM §6.2.2: `integer`/`int` selalu 32-bit signed.
  integer g;
  int h;
  always_comb begin
    g = 5;
    h = 7;
    bulat = g * 100 + h;
  end

  // Nilai awal waktu nol untuk variabel modul.
  logic [7:0] seed = 8'd42;

  // Net declaration assign menjadi driver kontinu.
  wire [7:0] net_awal = seed;

  assign awal = seed;
  assign salin_net = net_awal;

  // BUG-27: `i++` dan `i--` boleh jadi statement utuh.
  logic [3:0] k;
  always_comb begin
    k = 4'd0;
    while (k < 6) k++;
    incr_naik = {4'd0, k};
  end
  always_comb begin
    k = 4'd8;
    while (k > 3) k--;
    incr_turun = {4'd0, k};
  end
  always_comb begin
    k = 4'd0;
    k++;
    k++;
    k++;
    incr_tumpuk = {4'd0, k};
  end

  // Nilai awal untuk variabel lokal pun berlaku.
  always_comb begin
    logic [7:0] lokal_awal = 8'd9;
    awal_lokal = lokal_awal;
  end
endmodule