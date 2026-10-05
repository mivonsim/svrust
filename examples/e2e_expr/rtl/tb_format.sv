// Tanggung jawab: format %t pada $display dan always_ff dengan body kosong.
module tb_format(
  input logic clk,
  output logic [7:0] gak_nulis,
  output logic konstan
);
  // LRM §9.3: body kosong sah; proses tetap punya sensitivitas clock.
  always_ff @(posedge clk);

  assign konstan = 1'b1;
  always_comb begin
    gak_nulis = 8'd0;
  end

  // LRM §20.4 + §21.8: `%t` menampilkan waktu sebagai bilangan bulat dalam
  // satuan `timeprecision` modul, bukan teks bersufiks satuan.
  initial begin
    #100 $display("a=%t", $time);
    #900 $display("b=%0t", $time);
    #1500 $display("c=%0t", $time);
    #1000 $display("d=%t", $time);
    $finish;
  end
endmodule
