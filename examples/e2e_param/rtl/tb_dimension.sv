// Testbench ekspresi konstanta pada dimension packed (LRM §7.3).
//
// Batas dimension boleh ekspresi konstanta apa pun, bukan hanya `P` atau
// `P-1:0`. Bentuk seperti `[W+1:0]`, `[W*2-1:0]`, dan `[W-2:0]` sangat umum
// di RTL dan sebelumnya ditolak parser dengan pesan
// "pola parameter didukung: [P] atau [P-1:0]".
//
// Semua nilai diverifikasi terhadap iverilog 12.0 (`-g2012`).
module tb_dimension #(parameter W = 4, parameter DEPTH = 3) ();

  // Batas atas berupa ekspresi.
  logic [W+1:0]       a;   // msb 5        => 6 bit
  logic [W*2-1:0]     b;   // msb 7        => 8 bit
  logic [W+DEPTH-1:0] c;   // msb 6        => 7 bit
  logic [(W+1)*2-1:0] d;   // msb 9        => 10 bit

  // Batas bawah juga boleh ekspresi.
  logic [W+1:2]       e;   // msb 5, lsb 2 => 4 bit
  logic [7:2]         f;   //              => 6 bit

  // Bentuk lama tetap jalan.
  logic [W-1:0]       g;   //              => 4 bit
  logic [W]           h;   //              => 4 bit

  // Port modul juga boleh memakai ekspresi dimension.
  logic [W+1:0] port_in;
  logic [W-1:0] port_out;

  initial begin
    #1;
    $display("E1=%0d", $bits(a));
    $display("E2=%0d", $bits(b));
    $display("E3=%0d", $bits(c));
    $display("E4=%0d", $bits(d));
    $display("E5=%0d", $bits(e));
    $display("E6=%0d", $bits(f));
    $display("E7=%0d", $bits(g));
    $display("E8=%0d", $bits(h));
    $display("E9=%0d", $bits(port_in));
    $display("E10=%0d", $bits(port_out));
    $finish;
  end
endmodule
