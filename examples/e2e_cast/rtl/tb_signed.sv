// Testbench arah perluasan bit pada cast (LRM §6.14).
//
// Ini adalah kasus yang paling mudah keliru: cast berarti "dievaluasi seolah
// operand di-assign ke tipe tujuan" (LRM §6.14), jadi arah perluasan saat
// melebar ditentukan signedness OPERAND — bukan signedness tipe tujuan.
// Mengambilnya dari tipe tujuan membuat dua dari empat kombinasi salah dan
// saling berlawanan.
//
// Semua stimulus berupa literal supaya deterministik di kedua simulator.
// Nilai yang diharapkan sudah diverifikasi terhadap iverilog 12.0 (`-g2012`).
module tb_signed;
  typedef logic [7:0]        byte_t;
  typedef logic [15:0]       word_t;
  typedef logic signed [7:0]  sbyte_t;
  typedef logic signed [15:0] sword_t;

  logic signed [7:0] sa;   // -8'sd3 = 8'hFD
  logic        [7:0] uu;   // 8'hFD

  word_t  r1;   // signed   -> unsigned: harus sign-extend
  sword_t r2;   // unsigned -> signed:   harus zero-extend
  sword_t r3;   // signed   -> signed:   sign-extend
  word_t  r4;   // unsigned -> unsigned: zero-extend

  assign r1 = word_t'(sa);
  assign r2 = sword_t'(uu);
  assign r3 = sword_t'(sa);
  assign r4 = word_t'(uu);

  initial begin
    sa = -8'sd3;
    uu = 8'hFD;
    #1;

    // r1: operand signed, target unsigned. Kalau arah perluasan diambil dari
    // target, hasilnya 00fd dan nilai -3 hilang.
    // Catatan: `$signed` belum didukung parser, jadi nilai desimal signed tidak
    // bisa dicetak di sini; hex sudah cukup membedakan 00fd dari fffd.
    $display("s1=%h", r1);

    // r2: operand unsigned, target signed. Kalau salah sign-extend, 253 akan
    // terbaca sebagai -3.
    $display("s2=%h", r2);

    // r3 dan r4: kombinasi "cocok" yang kebetulan benar sebelumnya.
    $display("s3=%h", r3);
    $display("s4=%h", r4);

    // Sign cast tidak mengubah lebar maupun arah perluasan.
    $display("s5=%h", byte_t'(sa));
    $display("s6_bits=%0d", $bits(signed'(uu)));

    // Cast di dalam concatenation: tiap item punya lebar sendiri.
    $display("s7=%h", {sbyte_t'(sa), byte_t'(uu)});

    $finish;
  end
endmodule
