// Tanggung jawab: @(posedge clk or negedge rst) menggabungkan dua pemicu untuk demo e2e.
module tb_event_or(
  input logic clk,
  output logic [7:0] hitung,
  output logic rst
);
  always_ff @(posedge clk) begin
    hitung <= hitung + 1;
  end

  // LRM §9.7: daftar item dipisah `or`; proses lanjut saat salah satu edge terjadi.
  initial begin
    hitung = 8'd0;
    rst = 1'b1;
    $display("t=%0t mulai rst=%0d", $time, rst);
    @(posedge clk or negedge rst);
    $display("t=%0t setelah-gabungan rst=%0d h=%0d", $time, rst, hitung);
    #4 $finish;
  end
endmodule