// Tanggung jawab: testbench konversi pada batas assignment (LRM §11.6.1, Tabel 11-21).
//
// Ekspresi kanan pada assignment adalah context-determined: lebarnya
// disesuaikan ke lebar target, dan operand signed di-sign-extend sedangkan
// operand unsigned di-zero-extend. Sebelum fitur ini, batas assignment hanya
// memotong — jadi `logic [31:0] o = sa` dengan `sa` signed 8-bit menghasilkan
// 0x000000FD, bukan 0xFFFFFFFD.
//
// Semua nilai diverifikasi terhadap iverilog 12.0 (`-g2012`).
module tb_assign;
  logic signed [7:0]  sa;   // -8'sd3 = 8'hFD
  logic        [7:0]  ua;   //  8'hFD
  logic signed [15:0] s16;
  logic [31:0] o32;
  logic [15:0] o16;
  logic [7:0]  o8;
  logic [3:0]  o4;
  logic signed [31:0] so32;

  initial begin
    sa  = -8'sd3;
    ua  = 8'hFD;
    s16 = -16'sd3;

    // --- 1. Melebar dari operand signed: sign-extend ------------------
    o32 = sa;
    $display("a1=%h", o32);
    o16 = sa;
    $display("a2=%h", o16);

    // --- 2. Melebar dari operand unsigned: zero-extend -----------------
    o32 = ua;
    $display("a3=%h", o32);

    // --- 3. Target signed tidak mengubah arah perluasan -----------------
    // Aturan assignment melihat signedness *operand*, bukan target.
    so32 = ua;
    $display("a4=%h", so32);
    so32 = sa;
    $display("a5=%h", so32);

    // --- 4. Memotong ke target lebih sempit ----------------------------
    o4 = sa;
    $display("a6=%h", o4);
    o8 = s16;
    $display("a7=%h", o8);

    // --- 5. Ekspresi aritmetikanya sendiri signed ----------------------
    o32 = sa * 2;
    $display("a8=%h", o32);
    // `0` literal polos bertipe `integer` signed (LRM §5.7.1), jadi `sa + 0`
    // tetap signed dan sign-extend.
    o32 = sa + 0;
    $display("a9=%h", o32);

    // --- 6. Cast di dalam assignment ------------------------------------
    o32 = 32'(sa);
    $display("a10=%h", o32);
    o32 = byte'(sa);
    $display("a11=%h", o32);

    // --- 7. Target LHS teriris hanya memakai lebar irisan ---------------
    o32 = 16'b0;
    o32[15:0] = sa;
    $display("a12=%h", o32);
    o8[3:0] = sa;
    $display("a13=%h", o8);

    // --- 8. Perbandingan tidak terpengaruh assignment -------------------
    o8 = (sa < 0) ? 8'd1 : 8'd0;
    $display("a14=%h", o8);

    $finish;
  end
endmodule
