// Tanggung jawab: testbench demo modul root ganda (LRM §23.1).
// Dua modul root pada dua berkas terpisah; keduanya tidak diinstansiasi
// modul lain, jadi keduanya harus dijalankan. Dulu SVRust hanya meng-el~”borasi
// modul terakhir, jadi output modul pertama hilang tanpa pesan.
module tb_root_pertama;
  initial $display("dari modul root pertama");
endmodule
