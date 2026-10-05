// Tanggung jawab: testbench clock + $finish untuk membuktikan simulasi berhenti di demo e2e.
module tb_finish(
  input logic clk,
  input logic rst,
  output logic [7:0] hitung
);
  logic [7:0] langkah;
  integer i;

  // Berjalan pada setiap edge posedge clk.
  always_ff @(posedge clk) begin
    langkah <= langkah + 1;
  end

  assign hitung = rst ? 8'd0 : langkah;

  // LRM §15.2: stimulus pada waktu nol; $display mencetak, $finish menghentikan.
  initial begin
    rst = 1'b1;
    langkah = 8'd0;
    $display("mulai rst=%d", rst);
    rst = 1'b0;
    for (i = 0; i < 3; i = i + 1) begin
      $display("langkah %d", i);
    end
    $display("selesai");
    $finish;
    // LRM §20.3: baris setelah `$finish` tidak boleh tereksekusi.
    $display("tidak tampil");
    langkah = 8'd99;
  end
endmodule