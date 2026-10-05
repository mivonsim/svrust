module m;
  logic signed [7:0]  sa;
  logic signed [15:0] sb;
  logic [7:0]  y;
  logic [31:0] z;
  initial begin
    sa = -8'sd3; sb = -16'sd3; y = 8'hFF;
    #1;
    z = sa << 4;     $display("S1=%h", z);
    z = sa >> 1;     $display("S2=%h", z);
    z = sb >> 4;     $display("S3=%h", z);
    z = sa + 16'd1;  $display("S4=%h", z);
    z = sa - 16'd1;  $display("S5=%h", z);
    z = sa * 8'd2;   $display("S6=%h", z);
    z = sa / 8'd2;   $display("S7=%h", z);
    z = 1'b1 ? sa : sb; $display("S8=%h", z);
    z = (sa << 1) + sa; $display("S9=%h", z);
    $finish;
  end
endmodule
