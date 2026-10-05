// Testbench nilai 4-state: literal `x`/`z` (LRM §5.7.1), cast (§6.14),
// concat/replicate (§11.8), dan printing `%h`/`%b` (§20.4).
//
// Semua nilai diverifikasi terhadap iverilog 12.0 (`-g2012`).
module tb_xz;
  typedef logic [15:0]       word_t;
  typedef logic signed [15:0] sword_t;
  typedef logic [127:0]      wide_t;

  logic [15:0] w;
  logic [7:0]  b;
  logic [15:0] concat;
  logic [15:0] repl;
  logic [127:0] Casting;

  initial begin
    // --- 1. Literal x/z langsung -------------------------------------
    // LRM §5.7.1: digit `x` dan `z` adalah bagian nilai, bukan `0`.
    $display("x1=%h", 8'hxA);
    $display("x2=%h", 8'hz5);
    $display("x3=%b", 8'hxz);

    // --- 2. Padding ke lebar lebih besar (LRM §5.7.1) -----------------
    // `16'hzz` berarti 16 digit `z`, bukan `zz00`.
    $display("x4=%h", 16'hzz);
    $display("x5=%h", 16'h3x4z);
    // Digit paling kiri 0/1 => dipad dengan nol.
    $display("x6=%h", 16'hab);

    // --- 3. x/z dibawa melewati cast (LRM §6.14) -----------------------
    $display("x7=%h", word_t'(8'hxx));
    $display("x8=%h", sword_t'(8'hxA));

    // --- 4. x/z dibawa melewati concat (LRM §11.8.1) -------------------
    concat = {8'hxA, 8'h5Z};
    $display("x9=%h", concat);
    $display("x9b=%b", concat);

    // --- 5. x/z dibawa melewati replicate (LRM §11.8.2) ----------------
    // `2'b1x` = 0b1x, jadi empat salinannya = 0b1x1x1x1x.
    repl = {4{2'b1x}};
    $display("xa=%h", repl);
    $display("xab=%b", repl);

    // --- 6. Part-select mempertahankan x/z ----------------------------
    b = 8'hxA;
    $display("xb=%h", b[7:4]);

    // --- 7. Cast ke lebar di atas 64 bit --------------------------------
    // Lebar penyimpanan harus mengikuti lebar typedef, bukan lebar sinyal.
    Casting = wide_t'(32'hDEADBEEF);
    $display("xc=%h", Casting);
    $display("xd=%h", wide_t'(8'hzA));

    // --- 8. Lebar konstanta pada $display -----------------------------
    // `%h` meminta sebanyak digit heksa sebesar lebar logis operand.
    $display("xe=%h", 32'hDEADBEEF);
    $display("xf=%h", 8'hFF);

    $finish;
  end
endmodule
