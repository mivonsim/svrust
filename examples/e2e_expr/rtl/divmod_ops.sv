// Tanggung jawab: operator pembagian dan modulo untuk demo e2e.
module divmod_ops(
  output [7:0] bagi,
  output [7:0] sisa,
  output [7:0] bagi_nol,
  output [7:0] sisa_nol,
  output [7:0] bagi_kecil,
  output [7:0] sisa_kecil,
  output [7:0] presedensi,
  output [7:0] compound_bagi,
  output [7:0] compound_sisa
);
  logic [7:0] a, b, nol;

  always_comb begin
    a = 8'd100;
    b = 8'd7;
    nol = 8'd0;
    // LRM §11.4.5: 100 / 7 = 14 (bagian bulat).
    bagi = a / b;
    // LRM §11.4.6: 100 % 7 = 2.
    sisa = a % b;
    // Pembagi nol menghasilkan 0 pada engine 2-state (bukan `x`).
    bagi_nol = a / nol;
    sisa_nol = a % nol;
    // Pembagi lebih besar dari pembilang: 20 / 30 = 0 dan 20 % 30 = 20.
    bagi_kecil = 8'd20 / 8'd30;
    sisa_kecil = 8'd20 % 8'd30;
    // LRM §11.4: `*`, `/`, `%` sama kuat dan asosiatif kiri => 100/5*2 = 40.
    presedensi = 100 / 5 * 2;
  end

  // Compound assignment `/=` dan `%=` pada lebar yang sama.
  logic [7:0] bagi_acc, sisa_acc;

  always_comb begin
    // 100 / 5 = 20, lalu 20 / 2 = 10.
    bagi_acc = 8'd100;
    bagi_acc /= 8'd5;
    bagi_acc /= 8'd2;
    // 100 % 30 = 10, lalu 10 % 3 = 1.
    sisa_acc = 8'd100;
    sisa_acc %= 8'd30;
    sisa_acc %= 8'd3;
    compound_bagi = bagi_acc;
    compound_sisa = sisa_acc;
  end
endmodule