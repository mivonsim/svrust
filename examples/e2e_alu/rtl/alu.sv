// Tanggung jawab: contoh RTL ALU untuk demo e2e.
// module alu: pilih operasi + - & | lewat case, flag zero dari ==.
module alu(input [3:0] a, input [3:0] b, input [1:0] op, output [3:0] y, output zero);
always_comb begin
  case (op)
    0: y = a + b;
    1: y = a - b;
    2: y = a & b;
    default: y = a | b;
  endcase
end
assign zero = (a == b);
endmodule
