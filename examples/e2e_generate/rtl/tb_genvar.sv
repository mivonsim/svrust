// Tanggung jawab: testbench demo deklarasi genvar terpisah di body modul.
module tb_genvar_wrap;
  logic din;
  logic [3:0] y;
  logic [1:0] z;

  genvar_decl dut (.din(din), .y(y), .z(z));

  initial begin
    din = 1'b0;
    #1 $display("gv din=%b y=%b z=%b", din, y, z);
    din = 1'b1;
    #1 $display("gv din=%b y=%b z=%b", din, y, z);
    din = 1'b1;
    #1 $display("gv din=%b y=%b z=%b", din, y, z);
    $finish;
  end
endmodule