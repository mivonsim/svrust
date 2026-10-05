// Tanggung jawab: deklarasi variabel lokal dan langkah loop for untuk demo e2e.
module local_ops(
  output [7:0] lokal,
  output [7:0] lokal_majemuk,
  output [7:0] lokal_if,
  output [7:0] lokal_for,
  output [7:0] step_incr,
  output [7:0] step_tambah,
  output [7:0] step_kurang,
  output [7:0] body_kosong,
  output [7:0] hitung_kecil
);
  // Variabel lokal di dalam blok prosedural (LRM §12.8).
  always_comb begin
    logic [7:0] t;
    t = 8'd5;
    lokal = t;
  end

  // Deklarator majemuk di dalam blok.
  always_comb begin
    logic [3:0] a, b;
    a = 4'd2;
    b = 4'd3;
    lokal_majemuk = {a, b};
  end

  // Deklarasi lokal dipakai di kedua cabang if.
  always_comb begin
    logic [7:0] t;
    if (1'b0) t = 8'd9; else t = 8'd1;
    lokal_if = t;
  end

  // Deklarasi lokal di dalam blok yang memuat loop for.
  always_comb begin
    logic [3:0] k;
    logic [7:0] s;
    s = 8'd0;
    for (k = 0; k < 4; k = k + 1) s = s + k;
    lokal_for = s;
  end

  // LRM §12.5: `i++` sama dengan `i = i + 1`.
  always_comb begin
    logic [3:0] i;
    logic [7:0] c;
    c = 8'd0;
    for (i = 0; i < 5; i++) c = c + 1;
    step_incr = c;
  end

  // LRM §11.3: `i += 2` sama dengan `i = i + 2`.
  always_comb begin
    logic [3:0] i;
    logic [7:0] c;
    c = 8'd0;
    for (i = 0; i < 8; i += 2) c = c + 1;
    step_tambah = c;
  end

  // `i--` sama dengan `i = i - 1`.
  always_comb begin
    logic [3:0] i;
    logic [7:0] c;
    c = 8'd0;
    for (i = 9; i > 0; i--) c = c + 1;
    step_kurang = c;
  end

  // Body kosong: `for (...);` hanya menjalankan loop.
  always_comb begin
    logic [3:0] i;
    for (i = 0; i < 4; i = i + 1);
    body_kosong = i;
  end

  // Loop variabel lokal dan majemuk tingkat modul dipakai bersama.
  logic [3:0] n, m;
  always_comb begin
    logic [7:0] total;
    total = 8'd0;
    for (n = 0; n < 3; n++) total = total + n;
    for (m = 0; m < 4; m++) total = total + m;
    hitung_kecil = total;
  end
endmodule