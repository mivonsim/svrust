module tb_dpi;
  logic [7:0] data;

  // DPI-C import: deklarasi fungsi C yang dipanggil dari SystemVerilog (LRM §35).
  import "DPI-C" function void set_data(input int nilai);
  import "DPI-C" function int hitung_paritas(input bit [7:0] data);

  // DPI-C export: deklarasi fungsi SystemVerilog yang diexport ke C (LRM §35).
  export "DPI-C" function tampilkan_hasil = c_tampilkan_hasil;
endmodule
