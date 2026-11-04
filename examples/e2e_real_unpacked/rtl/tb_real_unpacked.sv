module tb_real_unpacked;
  real r1;
  real r2;
  logic [7:0] mem [0:3];

  initial begin
    r1 = 3;
    r2 = -2;
    mem[0] = 8'hAA;
    mem[1] = 8'h55;
    mem[2] = 8'hFF;
    mem[3] = 8'h00;
    $display("r1=%f r2=%f", r1, r2);
    $display("mem[0]=%h mem[1]=%h mem[2]=%h mem[3]=%h", mem[0], mem[1], mem[2], mem[3]);
    $finish;
  end
endmodule
