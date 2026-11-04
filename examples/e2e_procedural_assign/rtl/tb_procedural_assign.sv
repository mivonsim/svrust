// Tanggung jawab: demo penetapans berkelanjutan prosedural (LRM §14).
// `assign x = expr;` di dalam blok prosedural menetapkan x secara terus-menerus
// hingga `deassign x;` melepaskannya. Override menangkal penulangan prosedural
// biasa selama aktif.
module tb_procedural_assign;
  logic [7:0] x;

  initial begin
    x = 8'h20;
    $display("x_awal=%h", x);              // 20

    assign x = 8'hAA;                      // override kontinu x = 0xAA
    x = 8'h55;                             // penulangan biasa diabaikan
    $display("x_override=%h", x);          // aa (override menang)

    deassign x;                            // lepas override
    x = 8'h55;                             // penulangan biasa kini berlaku
    $display("x_setelah_deassign=%h", x);  // 55

    $finish;
  end
endmodule
