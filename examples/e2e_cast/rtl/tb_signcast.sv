// Testbench `$signed` / `$unsigned` (LRM §11.4.7, §20).
//
// Bentuk system function-nya identik dengan `signed'(x)` / `unsigned'(x)` pada
// LRM §6.14: hanya mengubah signedness, lebar TIDAK berubah. Keyword-nya sudah
// ada, jadi bentuk ini sempat ditolak parser.
//
// Semua nilai diverifikasi terhadap iverilog 12.0 (`-g2012`).
module tb_signcast;
  logic [7:0] ua;         // unsigned 8-bit = 0xFD
  logic signed [7:0] sa;  // signed 8-bit   = -3 (0xFD)
  logic [7:0]  out8;
  logic [15:0] out16;
  logic [31:0] out32;
  logic signed [7:0]  s8;
  logic signed [7:0]  sb;
  logic signed [7:0]  sc;

  initial begin
    ua = 8'hFD;
    sa = -8'sd3;
    #1;

    // --- 1. Nilai apa adanya, dicetak pada lebar ekspresinya sendiri ---
    // `$signed(8'hFF)` = 8'hFF (bukan 16'hFFFF): `$signed` tidak memperlebar.
    $display("K1=%h", $signed(ua));
    $display("K2=%h", $unsigned(sa));
    $display("K3=%h", $signed(8'hFF));
    $display("K4=%h", $unsigned(16'hFFFF));

    // --- 2. Setara dengan bentuk cast `signed'` / `unsigned'` ---
    out8 = $signed(ua);
    out8 = signed'(ua);
    $display("K5=%h", out8);

    // --- 3. Lebar tetap, jadi `$bits` tidak berubah ---
    $display("K6=%0d", $bits($signed(ua)));

    // --- 4. Assignment ke target lebih lebar memakai aturan LRM §11.6.1 ---
    // Ekspresi `$signed(ua)` bertanda 8-bit, jadi assignment sign-extend.
    out32 = $signed(ua);
    $display("K7=%h", out32);
    // `$unsigned(sa)` unsigned 8-bit, jadi zero-extend.
    out32 = $unsigned(sa);
    $display("K8=%h", out32);

    // --- 5. Perbandingan tetap unsigned kalau satu operand unsigned ---
    // LRM §11.4.5: bertanda hanya bila KEDUA operand signed.
    $display("K9=%b", ua < $signed(sa));
    $display("K10=%b", ua < $unsigned(sa));

    // --- 6. Dua operand signed -> perbandingan bertanda: -3 < 1 benar ---
    sb = -8'sd3;
    sc = 8'sd1;
    s8  = (sb < sc) ? 8'd1 : 8'd0;
    out8 = s8;
    out16 = {8'd0, out8};
    $display("K11=%h", out8);

    $finish;
  end
endmodule
