// Tanggung jawab: operand konstan untuk demo e2e operator ekspresi.
// Semua keluaran deterministik (tanpa input) sehingga nilainya bisa diperiksa.
module const_ops(
  output [7:0] lit,
  output [7:0] notv,
  output [7:0] negv,
  output       red_and,
  output       red_or,
  output       red_xor,
  output       red_xnor,
  output [7:0] tern
);
  // BUG-1: sized literal.
  assign lit    = 8'hA5;
  assign notv   = ~8'h0F;
  assign negv   = -8'd5;
  // BUG-4: reduction operator.
  assign red_and  = &4'b1111;
  assign red_or   = |4'b0000;
  assign red_xor  = ^4'b0011;
  assign red_xnor = ~^4'b0111;
  // BUG-5: ternary.
  assign tern   = 1'b1 ? 8'hFF : 8'h00;
endmodule
