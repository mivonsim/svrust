module tb_forever;
  logic clk;
  logic [7:0] cnt;

  // Clock generator: always #5 (proses waktu, sudah didukung).
  always #5 clk = ~clk;

  // Counter: forever loop yang dipicu tiap posedge clk (Fase: forever).
  initial begin
    cnt = 0;
    forever @(posedge clk) begin
      cnt = cnt + 1;
      $display("cnt = %d", cnt);
      if (cnt == 8'd10) $finish;
    end
  end
endmodule
