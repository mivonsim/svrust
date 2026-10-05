// Tanggung jawab: $monitor mencetak ulang saat nilai sinyal berubah untuk demo e2e.
module tb_monitor(
  input logic clk,
  output logic [7:0] hitung
);
  logic [7:0] bayangan;

  always_ff @(posedge clk) begin
    hitung <= hitung + 1;
  end

  always_comb begin
    bayangan = hitung + 1;
  end

  // LRM §20.2: $monitor cetak sekali saat didaftarkan, lalu mengulang
  // setiap kali nilai argumen berubah.
  initial begin
    hitung = 8'd0;
    $monitor("t=%0t h=%0d b=%0d", $time, hitung, bayangan);
    #4 hitung = 8'd5;
    #4 hitung = 8'd9;
    #4 $finish;
  end
endmodule
