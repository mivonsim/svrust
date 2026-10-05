// Tanggung jawab: membuktikan langkah waktu menyisipkan edge clock dan settle combinational antar #delay.
module tb_step(
  input logic clk,
  output logic [7:0] hitung,
  output logic [7:0] bayangan
);
  logic [7:0] deposit;

  always_ff @(posedge clk) begin
    hitung <= hitung + 1;
  end

  // Bayangan mengikuti deposit lewat logika combinational.
  always_comb begin
    bayangan = deposit + 1;
  end

  // Tiga delay memaksa tiga langkah waktu; clock harus.edge tiga kali.
  initial begin
    deposit = 8'd10;
    $display("s0 t=%0d hitung=%0d bayangan=%0d", $time, hitung, bayangan);
    #4 deposit = 8'd20;
    $display("s1 t=%0d hitung=%0d bayangan=%0d", $time, hitung, bayangan);
    #4 deposit = 8'd30;
    $display("s2 t=%0d hitung=%0d bayangan=%0d", $time, hitung, bayangan);
    $finish;
  end
endmodule
