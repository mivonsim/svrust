// Tanggung jawab: testbench pembungkus demo pass constant folding sv-opt.
module tb_fold_wrap;
  logic din;
  logic [7:0] jumlah;
  logic paritas;
  logic [3:0] potong;

  tb_fold dut (.din(din), .jumlah(jumlah), .paritas(paritas), .potong(potong));

  initial begin
    din = 1'b0;
    #1 $display("fold a jumlah=%0d paritas=%b potong=%b", jumlah, paritas, potong);
    din = 1'b1;
    #1 $display("fold b jumlah=%0d paritas=%b potong=%b", jumlah, paritas, potong);
    $finish;
  end
endmodule