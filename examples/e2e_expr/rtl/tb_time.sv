// Tanggung jawab: testbench dengan #delay dan $time untuk demo e2e.
module tb_time;
  logic [7:0] a;
  logic [7:0] b;
  integer i;

  // LRM §11.2: `#n` menunda statement; §20: `$time` membaca waktu berjalan.
  initial begin
    a = 8'd1;
    $display("t=%0d a=%0d", $time, a);
    #5 a = 8'd2;
    $display("t=%0d a=%0d", $time, a);
    #10;
    $display("t=%0d a=%0d", $time, a);
    #3ns b = 8'd7;
    $display("t=%0d b=%0d", $time, b);
    for (i = 0; i < 3; i = i + 1) begin
      #2 $display("loop t=%0d i=%0d", $time, i);
    end
    $display("t=%0d total=%0d", $time, $time);
    $finish;
  end
endmodule
