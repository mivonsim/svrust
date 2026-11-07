// Tanggung jawab: demo resolusi multi-driver wired-AND/wired-OR (LRM §6.6).
// `wand` = wired-AND, `wor`/`tri` = wired-OR. Banyak `assign` ke net yang
// sama diselesaikan dengan AND/OR bitwise, bukan last-wins.
module tb_wired;
  wire  [7:0] a;   // last-wins (default wire)
  wand [7:0] w;    // wired-AND
  wor  [7:0] o;    // wired-OR
  tri  [7:0] t;    // wired-OR (tri sama dengan wor)

  assign a = 8'b1010_1010;
  assign a = 8'b1111_0000;          // last-wins → 11110000

  assign w = 8'b1010_1010;
  assign w = 8'b1111_0000;          // wired-AND → 10100000

  assign o = 8'b1010_1010;
  assign o = 8'b1111_0000;          // wired-OR  → 11111010

  assign t = 8'b1010_1010;
  assign t = 8'b0000_1111;          // wired-OR  → 10101111

  initial begin
    #1;
    $display("wire_lastwins=%b (expect 11110000)", a);
    $display("wand_and=%b (expect 10100000)", w);
    $display("wor_or=%b (expect 11111010)", o);
    $display("tri_or=%b (expect 10101111)", t);
    $finish;
  end
endmodule
