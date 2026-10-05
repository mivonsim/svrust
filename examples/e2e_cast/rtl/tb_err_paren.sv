// Tanggung jawab: error size cast tanpa kurung operand (LRM §6.14).
//
// `16'a` punya lebar eksplisit tapi operandnya tidak dikurung. iverilog juga
// menolak (`syntax error`).
module tb_size_no_paren;
  logic [15:0] a;
  logic [15:0] y;
  initial y = 16'a;
endmodule
