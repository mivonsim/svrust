// Tanggung jawab: demo blok berlabel (LRM §12.9).
// `nama: begin ... end` menurunkan statement tubuh ke dalam blok induk;
// nama blok tidak berubahikan nilai runtime (scope hierarki disimpan sebagai
// metadata, tidak ada efek eksekusi langsung).
module tb_labeled_block;
  logic [7:0] x;

  initial begin
    blk: begin
      x = 8'hAA;
    end
    $display("x=%h", x);
    $finish;
  end
endmodule
