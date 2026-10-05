// BUG-9: `{WIDTH{1'b1}}` — count replikasi boleh ekspresi konstanta.
module child #(parameter W = 8) (input logic clk, output logic [15:0] d);
  typedef logic [W-1:0] word_t;
  word_t q;
  always_comb begin q = {W{1'b1}}; d = q; end
endmodule
module top;
  logic clk;
  logic [15:0] d1, d2;
  child #(.W(12)) a (.clk(clk), .d(d1));
  child #(.W(4))  b (.clk(clk), .d(d2));
  initial begin clk = 1'b0; #1; $display("R1=%h", d1); $display("R2=%h", d2); $finish; end
endmodule
