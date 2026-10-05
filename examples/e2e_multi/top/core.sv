module core(input [7:0] p, input [7:0] q, input [7:0] r, output [7:0] s1, output [7:0] s2);
add8 u_a (.a(p), .b(q), .y(s1));
inc8 u_b (.a(p), .y(s2));
endmodule
