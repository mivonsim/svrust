// Testbench task/function SystemVerilog (LRM §13.3/§13.4).
//
// Semua hasil deterministik karena stimulus berupa literal, jadi demo bisa
// memeriksa nilaioutput secara persis tanpa mengandalkan clock.
module tb_routine;
  logic [7:0] a, b, y, z;
  logic       p;

  // LRM §13.4: function dengan dua argumen input dan nilai balik 8 bit.
  function [7:0] jumlah(input [7:0] x, input [7:0] w);
    jumlah = x + w;
  endfunction

  // Function yang memanggil function lain; argumen formal fungsi luar
  // harus ikut tersubstitusi ke dalam argumen fungsi dalam.
  function [7:0] dobel(input [7:0] v);
    dobel = jumlah(v, v);
  endfunction

  // `return expr;` setara dengan assignment ke nama fungsi (LRM §13.4).
  function [7:0] tambah_satu(input [7:0] v);
    return v + 1;
  endfunction

  // Reduction tanpa lebar eksplisit menghasilkan 1 bit (LRM §13.4).
  function paritas;
    paritas = ^a;
  endfunction

  // Badan `if/else` yang mengesahkan nama fungsi menjadi operator ternair.
  function [7:0] ambang(input [7:0] v);
    if (v > 8'd4) ambang = 8'hFF;
    else ambang = 8'h00;
  endfunction

  // Keyword `automatic` diterima; badan di-inline sehingga tidak ada
  // penyimpanan statis yang perlu dibedakan.
  function automatic [7:0] otomatis(input [7:0] v);
    otomatis = v ^ 8'h0F;
  endfunction

  // LRM §13.3: task dengan argumen input dan output.
  task kuadrat(input [7:0] v, output [7:0] hasil);
    hasil = v * v;
  endtask

  // Task tanpa argumen yang menulis sinyal modul; boleh dipanggil tanpa
  // kurung (LRM §13.3).
  task set_nol;
    y = 8'd0;
    z = 8'd0;
  endtask

  // Task dengan deklarasi lokal. Dua pemanggilan task yang sama harus tetap
  // aman: badan di-inline dua kali, jadi variabel lokalnya harus mendapat
  // nama berbeda per titik pemanggilan.
  task pakai_lokal(input [7:0] v, output [7:0] hasil);
    logic [7:0] t;
    t = v + 1;
    hasil = t;
  endtask

  initial begin
    a = 8'd7;
    b = 8'd5;

    // Ekspresi dengan beberapa pemanggilan function.
    y = jumlah(a, b);
    z = dobel(a);
    p = paritas();
    $display("fn1 jumlah=%d dobel=%d paritas=%b", y, z, p);

    // Task dengan argumen output menulis langsung ke sinyal pemanggil.
    kuadrat(a, z);
    $display("task1 kuadrat=%d", z);

    // Task tanpa kurung.
    set_nol;
    $display("task2 set_nol y=%d z=%d", y, z);

    // Function dengan `return`, ambang, dan `automatic`. Ambang diuji pada
    // nilai di bawah dan di atas batas 4 supaya kedua cabang terlihat.
    y = tambah_satu(b);
    $display("fn2 tambah_satu=%d", y);
    z = ambang(8'd2);
    $display("fn3 ambang_bawah=%h", z);
    z = ambang(a);
    $display("fn4 ambang_atas=%h", z);
    y = otomatis(b);
    $display("fn5 otomatis=%h", y);

    // Function yang memanggil task, dan task yang memanggil function.
    z = 8'd0;
    set_nol;
    $display("fn6 setelah_set_nol=%d", z);

    // Task dengan variabel lokal, dipanggil dua kali. Kalau nama lokalnya
    // tidak dibedakan per pemanggilan, elaborasi gagal dengan
    // `duplicate signal 't'`.
    pakai_lokal(a, y);
    pakai_lokal(a, z);
    $display("lokal1 y=%d z=%d", y, z);

    $finish;
  end
endmodule