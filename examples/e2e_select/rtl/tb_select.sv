// Tanggung jawab: testbench demo bit-select dan part-select (LRM §7.8).
module tb_select;
  logic [7:0] a;
  logic [3:0] y;
  logic [7:0] mem [0:3];
  logic [31:0] w;

  // LRM §7.8.1: bit-select `a[i]` dan part-select `a[msb:lsb]`.
  assign a = 8'hA5;
  assign y = a[3:0];
  assign w = {24'b0, a[7:4]};

  // LRM §7.8: elemen array unpacked diakses lewat indeks konstan.
  assign mem[2] = 8'h3C;

  // LRM §7.8: seleksi boleh bertingkat.
  logic nested;
  assign nested = a[3:0][1];

  // LRM §7.8: indeks dinamis `a[i]` dibaca lewat helper runtime.
  logic [7:0] shifted;
  logic [2:0] idx;
  assign idx = 3'd2;
  assign shifted = a[idx];

  initial begin
    #1;
    $display("bit a[0]=%b", a[0]);
    $display("part a[3:0]=%h", a[3:0]);
    $display("part a[7:4]=%h", a[7:4]);
    $display("part w[7:0]=%h", w[7:0]);
    $display("nested a[3:0][1]=%b", nested);
    $display("array mem[2]=%h", mem[2]);
    $display("dinamis a[idx]=%h", shifted);
  end
endmodule