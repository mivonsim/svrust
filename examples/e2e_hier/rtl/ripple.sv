module adder #(parameter WIDTH = 8) (input [WIDTH-1:0] a, output [WIDTH-1:0] y);
assign y = a + 1;
endmodule
module ripple(input [15:0] x, input [7:0] z, output [15:0] big, output [7:0] small);
adder #(.WIDTH(16)) u_big (.a(x), .y(big));
adder u_small (.a(z), .y(small));
endmodule
