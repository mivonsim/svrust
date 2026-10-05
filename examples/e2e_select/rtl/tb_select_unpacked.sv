// Tanggung jawab: testbench demo indeks dinamis pada array unpacked (LRM §7.8).
module tb_select_unpacked;
  logic [7:0] mem [0:3];
  logic [2:0] k;
  logic [7:0] hasil;
  logic packed_hasil;

  // LRM §7.8: elemen array unpacked dibaca utuh lewat indeks variabel.
  assign mem[2] = 8'h3C;
  assign k = 3'd2;
  assign hasil = mem[k];

  // Bandingkan dengan bit-select pada sinyal packed: hasilnya satu bit.
  logic [7:0] flat;
  assign flat = 8'h3C;
  assign packed_hasil = flat[k];

  initial begin
    #1;
    $display("unpacked mem[k]=%h", hasil);
    $display("packed flat[k]=%b", packed_hasil);
  end
endmodule