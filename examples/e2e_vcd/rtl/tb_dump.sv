// Tanggung jawab: $dumpfile/$dumpvars controlling VCD recording for demo e2e.
module tb_dump(
  input logic clk,
  output logic [7:0] hitung
);
  always_ff @(posedge clk) begin
    hitung <= hitung + 1;
  end

  // LRM §23.2: $dumpfile sets the VCD file name, $dumpvars turns on recording.
  initial begin
    hitung = 8'd0;
    $dumpfile("target/sv/demo/tb_dump.vcd");
    $dumpvars;
    #8 $finish;
  end
endmodule