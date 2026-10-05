// Tanggung jawab: counter clock tanpa initial/$finish sebagai pembanding di demo e2e.
module tb_clock(
  input logic clk,
  output logic [7:0] hitung
);
  always_ff @(posedge clk) begin
    hitung <= hitung + 1;
  end
endmodule