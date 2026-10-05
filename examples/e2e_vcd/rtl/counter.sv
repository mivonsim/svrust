// Tanggung jawab: contoh register counter sekuensial untuk demo VCD.
module counter(input clk, output [3:0] q, output carry);
always_ff @(posedge clk) q <= q + 1;
assign carry = (q == 15);
endmodule
