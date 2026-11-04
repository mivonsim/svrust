module tb_unpacked_multi;
  logic [7:0] mem [0:3][0:15];
  logic [7:0] val;

  initial begin
    // Inisialisasi semua elemen
    mem[0][0] = 8'h00;
    mem[0][1] = 8'h11;
    mem[1][0] = 8'h22;
    mem[1][1] = 8'h33;
    mem[2][15] = 8'hFF;
    mem[3][15] = 8'hEE;

    // Tampilkan nilai via $display
    $display("mem[0][0]=%h mem[0][1]=%h", mem[0][0], mem[0][1]);
    $display("mem[1][0]=%h mem[1][1]=%h", mem[1][0], mem[1][1]);
    $display("mem[2][15]=%h mem[3][15]=%h", mem[2][15], mem[3][15]);

    // Baca kembali dan simpan ke variabel
    val = mem[2][15];
    $display("val=%h", val);

    $finish;
  end
endmodule
