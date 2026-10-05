// Tanggung jawab: testbench demo pass constant folding sv-opt.
module tb_fold(
  input  logic       din,
  output logic [7:0] jumlah,
  output logic       paritas,
  output logic [3:0] potong
);
  // Seluruh operand konstanta: `2 * 8'd3 + 4` terlipat saat kompilasi.
  assign jumlah = 8'd2 * 8'd3 + 8'd4;

  // Reduksi atas konstanta: `|4'b1010` = 1.
  assign paritas = |4'b1010;

  // Select atas konstanta: `8'hB6[5:4]` = 2'b01.
  assign potong = 8'hB6[5:4];

  always_comb begin
    // Beracuan sinyal: tidak boleh terlipat, dan hasilnya harus tetap benar.
    paritas = paritas ^ din;
  end
endmodule