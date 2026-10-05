// BUG-2: geser — operand kiri context-determined, operand kanan self-determined.
module m;
  logic [7:0] a;
  logic [7:0] b;
  logic [31:0] y;
  logic signed [7:0] sa;
  initial begin
    a = 8'hFF;
    b = 8'h01;
    sa = -8'sd3;
    #1;
    y = a << b;        // 8'hFF << 1 = 8'hFE pada lebar 8
    $display("G1=%h", y);
    y = a << 32'd1;    // operand kanan self-determined: tetap 8'hFE
    $display("G2=%h", y);
    y = sa << 4;       // operand kiri signed: sign-extend sebelum geser
    $display("G3=%h", y);
    y = sa >> 1;       // `>>` pada operand signed = aritmetik
    $display("G4=%h", y);
    y = a >> 1;        // unsigned: logika
    $display("G5=%h", y);
    $finish;
  end
endmodule
