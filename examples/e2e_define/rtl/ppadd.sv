// Tanggung jawab: contoh `include, `define, `ifdef di RTL.
`timescale 1ns/1ps
`include "defs.svh"
module ppadd(input [3:0] a, input [3:0] b, output [3:0] y);
`ifdef EXTRA
assign y = a + b + `ADDER_BIAS;
`else
assign y = a - b;
`endif
endmodule
