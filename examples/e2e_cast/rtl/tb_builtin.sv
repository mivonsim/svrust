// Testbench cast ke tipe bawaan dan size cast (LRM §6.14).
//
// Ini melengkapi `tb_cast.sv` yang hanya menguji typedef. Bentuk `integer'(x)`
// adalah cast yang paling sering dipakai kedua di SV, dan sebelum fit ini ia
// gagal di lexer: `'` dibaca sebagai awal based literal dengan basis `(`.
//
// Semua nilai diverifikasi terhadap iverilog 12.0 (`-g2012`).
module tb_builtin;
  logic [7:0]  a8;
  logic [15:0] a16;
  logic signed [7:0]  sa;
  logic signed [15:0] s16;
  integer      n32;
  logic [31:0] out32;
  logic [7:0]  out8;
  logic [15:0] out16;
  logic [63:0] out64;

  initial begin
    a8  = 8'hA5;
    a16 = 16'hBEEF;
    sa  = -8'sd3;
    s16 = -16'sd3;

    // --- 1. Lebar & signedness tiap tipe bawaan (Tabel 6-22) ----------
    // `integer`/`int` = 32-bit signed, `byte` = 8-bit unsigned,
    // `shortint` = 16-bit unsigned, `longint` = 64-bit signed,
    // `time` = 64-bit unsigned.
    $display("b1=%0d", $bits(integer'(a8)));
    $display("b2=%0d", $bits(byte'(a8)));
    $display("b3=%0d", $bits(shortint'(a8)));
    $display("b4=%0d", $bits(longint'(a8)));
    $display("b5=%0d", $bits(time'(a8)));

    // --- 2. `logic`/`bit` tanpa range adalah vektor 1 bit --------------
    // LRM §6.16: tanpa `[msb:lsb]` keduanya 1 bit, bukan tipe skalar penuh.
    // LSB dari 8'hA5 adalah 1.
    $display("b6=%h", logic'(a8));
    $display("b7=%h", bit'(a8));

    // --- 3. Arah perluasan tetap ikut signedness OPERAND --------------
    // Cast ke tipe bawaan bukan pengecualian: operand unsigned di-zero-extend
    // walau target signed, dan sebaliknya.
    out32 = integer'(a8);
    $display("b8=%h", out32);
    out32 = integer'(sa);
    $display("b9=%h", out32);

    // --- 4. Size cast hanya mengubah lebar, signedness ikut operand ----
    // LRM §6.14: `16'(x)` tidak menandai apa pun, jadi sign operand tetap
    // menentukan arah perluasan. 16'(-3) = 16'hFFFD.
    out16 = 16'(sa);
    $display("b10=%h", out16);
    out16 = 16'(a8);
    $display("b11=%h", out16);

    // --- 5. Size cast ke lebar non-standar -----------------------------
    out32 = 20'(a8);
    $display("b12=%h", out32);
    out64 = 48'(a8);
    $display("b13=%h", out64);

    // --- 6. Size cast pada hasil aritmetika ----------------------------
    out16 = 16'(a8 + 8'h0F);
    $display("b14=%h", out16);

    // --- 7. Cast bawaan pada select ------------------------------------
    out8 = byte'(a16[15:8]);
    $display("b15=%h", out8);

    // --- 8. Size cast menyempit dari lebar besar -----------------------
    out8 = byte'(a16);
    $display("b16=%h", out8);

    // --- 9. Tanda pada cast bawaan terlihat di %d ----------------------
    n32 = integer'(sa);
    $display("b17=%0d", n32);
    n32 = integer'(a8);
    $display("b18=%0d", n32);

    $finish;
  end
endmodule
