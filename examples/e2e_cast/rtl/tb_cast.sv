// Testbench type cast SystemVerilog (LRM §6.14) dan $bits (LRM §20).
//
// Semua stimulus berupa literal sehingga hasil simulasi deterministik.
// Catatan cakupan: `typedef` di luar module (compilation-unit scope, §8.20)
// belum didukung parser, jadi semua typedef di sini berada di body module.
module tb_cast;
  // LRM §8.20: typedef menyalin tipe yang sudah ada; lebar dan signedness
  // ikut tersalin sehingga bisa jadi target cast.
  typedef logic [7:0]       byte_t;
  typedef logic [15:0]      word_t;
  typedef logic signed [7:0]  sbyte_t;
  typedef logic signed [15:0] sword_t;

  logic [7:0]  a8;
  logic [15:0] a16;
  logic signed [7:0]  s8;
  logic signed [15:0] s16;
  byte_t  b;
  word_t  w;
  sbyte_t sb;
  sword_t sw;
  integer  nbits;
  integer  nbits_sel;
  integer  nbits_concat;

  initial begin
    a8  = 8'hA5;
    a16 = 16'hBEEF;
    s8  = -8'sd3;
    s16 = -16'sd3;

    // --- 1. cast melebar ke unsigned: bit atas diisi nol ---
    // LRM §6.14: 8'hA5 -> 16'h00A5, bukan 16'hFFA5.
    w = word_t'(a8);
    $display("c1 w=%h", w);

    // --- 2. cast menyempit: bit atas dipotong ---
    // 16'hBEEF -> 8'hEF.
    b = byte_t'(a16);
    $display("c2 b=%h", b);

    // --- 3. cast ke signed saat melebar: MSB sumber disalin ---
    // -8'sd3 = 8'hFD -> 16'hFFFD = -3.
    sw = sword_t'(s8);
    $display("c3 sw_dec=%0d", sw);
    $display("c3 sw=%h", sw);

    // --- 4. sign cast hanya mengubah signedness, bukan lebar ---
    // unsigned'(a8) tetap 8 bit, jadi $bits-nya 8.
    nbits = $bits(unsigned'(a8));
    $display("c4 bits=%0d", nbits);

    // --- 5. $bits dari part-select: lebarnya select, bukan sinyalnya ---
    // a16[7:0] adalah 8 bit walau a16 sendiri 16 bit.
    nbits_sel = $bits(a16[7:0]);
    $display("c5 bits=%0d", nbits_sel);

    // --- 6. cast signed ke unsigned: bit dipertahankan, tanda diabaikan ---
    // -8'sd3 = 8'hFD, dan(unsigned) hanya melepas maknanya sebagai negatif.
    b = byte_t'(unsigned'(s8));
    $display("c6 b=%h", b);

    // --- 7. cast pada hasil aritmetika, bukan hanya literal ---
    // 8'hA5 + 8'h0F = 8'hB4 -> 16'h00B4.
    w = word_t'(a8 + 8'h0F);
    $display("c7 w=%h", w);

    // --- 8. cast di dalam concatenate: setiap item lebarnya sendiri ---
    // {byte_t'(16'hBEEF), 8'hA5} = {8'hEF, 8'hA5} = 16'hEFA5.
    w = {byte_t'(a16), a8};
    $display("c8 w=%h", w);

    // --- 9. $bits dari concatenation: jumlah lebar seluruh item ---
    nbits_concat = $bits({a8, a8});
    $display("c9 bits=%0d", nbits_concat);

    $finish;
  end
endmodule
