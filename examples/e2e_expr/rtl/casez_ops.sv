// Tanggung jawab: casez/casex dengan wildcard untuk demo e2e.
module casez_ops(
  output [3:0] yz,
  output [3:0] yx,
  output [3:0] yno
);
  // LRM §12.5: `1?` cocok dengan 2'b10 dan 2'b11, bukan 2'b00 atau 2'b01.
  always_comb casez (2'b10) 2'b1?: yz = 4'd1; default: yz = 4'd0; endcase
  // LRM §12.5: `0??` menutup semua nilai 0xx.
  always_comb casex (2'b00) 2'b0??: yx = 4'd7; default: yx = 4'd0; endcase
  // Selektor 2'b11 tidak cocok dengan `1?`? Tetap cocok karena bit 0 wildcard.
  // Sebaliknya `2'b01` tidak cocok dengan `1?` pada bit 1.
  always_comb casez (2'b01) 2'b1?: yno = 4'd9; default: yno = 4'd0; endcase
endmodule
