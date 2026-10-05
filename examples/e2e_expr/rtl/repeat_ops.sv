// Tanggung jawab: loop repeat dan pre-increment untuk demo e2e.
module repeat_ops(
  output [7:0] hitung_sinyal,
  output [7:0] hitung_literal,
  output [7:0] hitung_blok,
  output [7:0] hitung_nol,
  output [7:0] pre_naik,
  output [7:0] pre_turun,
  output [7:0] campur
);
  integer n;

  // LRM §12.8.1: hitungan boleh berupa sinyal.
  always_comb begin
    hitung_sinyal = 8'd0;
    n = 3;
    repeat (n) hitung_sinyal = hitung_sinyal + 1;
  end

  // Hitungan literal.
  always_comb begin
    hitung_literal = 8'd0;
    repeat (4) hitung_literal = hitung_literal + 2;
  end

  // Body berupa blok dengan dua statement.
  always_comb begin
    hitung_blok = 8'd0;
    n = 2;
    repeat (n) begin
      hitung_blok = hitung_blok + 1;
      hitung_blok = hitung_blok + 10;
    end
  end

  // Hitungan nol membuat body tidak pernah jalan.
  always_comb begin
    hitung_nol = 8'd99;
    repeat (0) hitung_nol = hitung_nol + 1;
  end

  // BUG-29: pre-increment `++i;` dan pre-decrement `--i;`.
  logic [3:0] i;
  always_comb begin
    i = 4'd1;
    ++i;
    pre_naik = {4'd0, i};
  end
  always_comb begin
    i = 4'd5;
    --i;
    pre_turun = {4'd0, i};
  end

  // Bentuk pre dan post dicampur; karena hasilnya dibuang keduanya setara.
  always_comb begin
    i = 4'd0;
    ++i;
    i++;
    --i;
    i--;
    i++;
    campur = {4'd0, i};
  end
endmodule