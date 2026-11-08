module tb_dpi;
  logic [7:0] data;
  logic [7:0] paritas;

  // DPI-C import: deklarasi fungsi C yang dipanggil dari SystemVerilog (LRM §35).
  import "DPI-C" function void set_data(input int nilai);
  import "DPI-C" function int hitung_paritas(input bit [7:0] data);

  // DPI-C export: deklarasi fungsi SystemVerilog yang diexport ke C (LRM §35).
  export "DPI-C" function tampilkan_hasil = c_tampilkan_hasil;

  // Fungsi SV yang diexport ke C.
  function int tampilkan_hasil(input int nilai);
    tampilkan_hasil = nilai * 2;
  endfunction

  initial begin
    // Panggil fungsi C lewat DPI-C import (statement-position, void).
    set_data(42);

    // Panggil fungsi C lewat DPI-C import (expression-position, dengan return).
    paritas = hitung_paritas(8'hFF);

    $display("data = %d, paritas(0xFF) = %d", data, paritas);
    $display("tampilkan_hasil(21) = %d", tampilkan_hasil(21));
    $finish;
  end
endmodule
