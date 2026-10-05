// Tanggung jawab: contoh combinational statement tunggal untuk demo VCD.
module gate(input [3:0] a, input [3:0] b, output [3:0] y, output same);
always_comb y = a & b;
assign same = (a == b);
endmodule
