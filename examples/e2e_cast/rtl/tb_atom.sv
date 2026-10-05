// Tanggung jawab: testbench deklarasi tipe integer atom (LRM §6.16, Tabel 6-22).
//
// `byte`/`shortint`/`longint`/`time` sebelumnya hanya hidup di posisi cast
// `byte'(x)`, sehingga `byte b;` gagal dengan pesan "unexpected token" yang
// menyesatkan. Sekarang keyword-nya bisa dipakai di deklarasi, port, argumen
// task/function, dan `typedef`.
//
// Semua nilai diverifikasi terhadap iverilog 12.0 (`-g2012`).
module tb_atom;
  // Deklarasi di body modul.
  byte    b;
  shortint s;
  longint l;
  time    t;

  // Deklarasi sebagai port diuji di modul terpisah; di sini cukup deklarasi
  // di body modul, di dalam blok, dan sebagai tipe balik/argumen subrutin.

  integer i;
  int       ii;
  logic [7:0] logic_polos;

  function automatic byte f(input [7:0] x);
    f = byte'(x);
  endfunction

  task automatic tg(output time q);
    q = 8'h01;
  endtask

  typedef logic [3:0] nib_t;

  initial begin
    b = 8'hA5;       // 165
    s = 16'hBEEF;    // -16657 (16-bit signed)
    l = 64'h0;       // 0
    t = 8'h05;       // 5
    i = -1;
    ii = -1;
    logic_polos = 1'b1;

    $display("d1=%0d", b);
    $display("d2=%0d", s);
    $display("d3=%0d", l);
    $display("d4=%0d", t);
    $display("d5=%0d", f(8'h2A));
    $display("d6=%0d", $bits(b));
    $display("d7=%0d", $bits(s));
    $display("d8=%0d", $bits(l));
    $display("d9=%0d", $bits(t));
    $display("d10=%0d", $bits(i));
    $display("d11=%0d", $bits(logic_polos));

    tg(t);
    $display("d12=%0d", t);

    // Signedness tiap tipe atom pada deklarasi, dilihat dari size cast ke 32
    // bit: operand signed di-sign-extend.
    $display("d13=%h", 32'(b));
    $display("d14=%h", 32'(t));

    $finish;
  end
endmodule
