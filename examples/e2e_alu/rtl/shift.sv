// Tanggung jawab: contoh RTL geser dan banding untuk demo e2e.
module shift(input [7:0] a, input [7:0] b, output [7:0] s, output gt);
assign s = a << b;
assign gt = (a > b);
endmodule
