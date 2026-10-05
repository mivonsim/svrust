module adder #(parameter WIDTH = 8) (input [WIDTH-1:0] a, input [WIDTH-1:0] b, output [WIDTH-1:0] y, output zero);
assign y = a + b;
assign zero = (y == 0);
endmodule
