// Tanggung jawab: @(posedge clk) di dalam blok initial testbench untuk demo e2e.
module tb_event(
  input logic clk,
  output logic [7:0] hitung
);
  always_ff @(posedge clk) begin
    hitung <= hitung + 1;
  end

  // LRM §9.7: proses initial menangguhkan diri sampai posedge clk terjadi.
  // Dua kali menunggu berarti butuh dua edge yang berbeda.
  initial begin
    hitung = 8'd0;
    $display("t=%0t mulai", $time);
    @(posedge clk);
    $display("t=%0t setelah-edge-1", $time);
    #4;
    @(posedge clk);
    $display("t=%0t setelah-edge-2", $time);
    #4 $finish;
  end
endmodule