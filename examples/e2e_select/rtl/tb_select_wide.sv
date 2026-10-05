// Tanggung jawab: testbench demo penulisan pada sinyal lebar dan array unpacked
// besar (LRM §10.10.1, §7.8).
module tb_select_wide;
  // 16 elemen 8 bit disimpan rata = 128 bit. Elemen ke-15 ada di bit 120..128,
  // jauh di atas batas 64 bit.
  logic [7:0] mem [0:15];
  logic [127:0] wide;
  logic [7:0] a;

  initial begin
    mem[15] = 8'hBB;
    mem[0] = 8'h11;
    #1;
    $display("mem0=%h", mem[0]);
    $display("mem15=%h", mem[15]);

    // Sinyal 128 bit: bit-select dan part-select pada posisi di atas 63.
    wide = 128'd0;
    wide[100] = 1'b1;
    wide[127:120] = 8'hF1;
    $display("wide100=%b", wide[100]);
    $display("widetop=%h", wide[127:120]);
    $display("widetop_lo=%h", wide[125:120]);

    // LRM §5.7.1 + §10.10.1: digit `x`/`z` pada part-select LHS harus tersimpan,
    // bukan berubah jadi 0.
    a = 8'hF0;
    a[7:4] = 4'b1x0z;
    $display("xz_b=%b", a);
    $display("xz_s=%b", a[7:4]);
  end
endmodule