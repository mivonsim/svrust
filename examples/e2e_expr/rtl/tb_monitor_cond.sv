// Tanggung jawab: $monitor if (kondisi) hanya mencetak saat syarat benar, untuk demo e2e.
module tb_monitor_cond(
  input logic clk,
  output logic [7:0] hitung
);
  always_ff @(posedge clk) begin
    hitung <= hitung + 1;
  end

  // LRM §20.2: $monitor if (kondisi) mencetak hanya ketika kondisi bernilai
  // benar. Syarat dievaluasi ulang setiap ada perubahan sinyal.
  initial begin
    hitung = 8'd0;
    $monitor if (hitung >= 8'd3) "t=%0t h=%0d", $time, hitung;
    #4 hitung = 8'd9;
    #4 $finish;
  end
endmodule