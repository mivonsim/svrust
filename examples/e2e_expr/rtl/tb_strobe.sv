// Tanggung jawab: $monitoron/$monitoroff dan $strobe untuk demo e2e.
module tb_strobe(
  input logic clk,
  output logic [7:0] hitung
);
  always_ff @(posedge clk) begin
    hitung <= hitung + 1;
  end

  // LRM §20.2: $monitoron/$monitoroff mematikan dan menyalakan cetakan.
  // LRM §20.3: argumen $strobe dievaluasi saat dipanggil, lalu dicetak
  // di akhir timestep berjalan.
  initial begin
    hitung = 8'd0;
    $monitor("mon t=%0t h=%0d", $time, hitung);
    #2 $strobe("strobe t=%0t h=%0d", $time, hitung);
    $monitoroff;
    #2 $strobe("off t=%0t h=%0d", $time, hitung);
    $monitoron;
    #2 $finish;
  end
endmodule
