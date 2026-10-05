// Tanggung jawab: testbench demo operator geser logis dan aritmetik (LRM §11.4.10).
// Semua nilai diverifikasi silang dengan iverilog 12.0 (`-g2012`).
module tb_shift;
  logic signed [7:0] s80, sF0;
  logic [7:0] u80, a;
  logic [3:0] n;
  logic signed [15:0] wide;

  initial begin
    s80 = 8'sh80;
    sF0 = 8'shF0;
    u80 = 8'h80;

    // LRM §11.4.10: `>>` adalah geser logis dan selalu mengisi nol, apa pun
    // signedness operand kirinya.
    $display("shr s80=%b sF0=%b u80=%b", s80 >> 1, sF0 >> 1, u80 >> 1);

    // `>>>` mengisi bit sign hanya bila tipe HASIL signed.
    $display("sar s80=%b sF0=%b u80=%b", s80 >>> 1, sF0 >>> 1, u80 >>> 1);

    // `<<<` identik dengan `<<` pada geser kiri (keduanya mengisi nol).
    $display("shl s80=%b sar_ke_kiri=%b", s80 <<< 1, s80 <<< 1);

    // Jumlah geser `x`/`z` membuat hasil unknown.
    a = 8'hF0;
    n = 4'b1x01;
    $display("xamt shr=%b shl=%b", a >> n, a << n);
    n = 4'bzzzz;
    $display("zamt shr=%b sar=%b", a >> n, a >>> n);

    // Geser melebihi lebar logis: seluruh bit keluar dari jangkauan.
    $display("lewat sar=%b shr=%b", s80 >>> 20, s80 >> 20);

    // Tipe hasil lebih lebar: `>>>` memakai sign bit pada lebar hasil.
    wide = -16'sd1000;
    $display("wide sar=%0d shr=%0d", wide >>> 4, wide >> 4);

    // Compound assignment `>>>=` (LRM §11.4.10).
    s80 >>>= 3;
    $display("sar_assign s80=%b", s80);
    a = 8'hF0;
    a >>>= 4;
    $display("sar_assign u=%b", a);
  end
endmodule