// Tanggung jawab: testbench root kedua untuk demo multi-root (LRM §23.1).
// Berkas `tb_root.sv` dan ini masing-masing modul root: keduanya tidak
// diinstansiasi modul lain, jadi keduanya harus dijalankan.
module tb_root_kedua;
  initial $display("dari modul root kedua");
endmodule
