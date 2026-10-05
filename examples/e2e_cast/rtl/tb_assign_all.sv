// BUG-5/6/7/8: asimetri konversi antar jalur assignment.
//
// `=` (blocking), `<=` (NBA), `+=` (compound), init/step `for`, dan nilai awal
// deklarasi semuanya harus menerapkan aturan yang sama (LRM §11.6.1). Sebelum
// perbaikan ini hanya blocking assign dan continuous assign yang meneruskan
// lebar konteks; sisanya memakai `lower_expression` polos sehingga:
//   - `qn <= sa` dengan `sa` signed 8-bit dan `qn` unsigned 32-bit menghasilkan
//     0x000000FD, bukan 0xFFFFFFFD seperti pada blocking assign;
//   - `it += s` dengan `s` signed 8-bit dan `it` integer menghasilkan 253
//     (harus -3);
//   - `y[15:8] += x` membaca seluruh `y`, bukan hanya irisannya;
//   - `for (si = -8'sd1; si > 0; si = si - 1)` berjalan 255 iterasi;
//   - `logic [7:0] s = -1;` menyimpan 64 bit di sinyal 8-bit sehingga
//     `s == 8'hFF` salah.
//
// Semua nilai diverifikasi terhadap iverilog 12.0 (`-g2012`).
module m;
  logic signed [7:0] sa;
  logic        clk;
  logic [31:0] qn;
  logic [15:0] wn;
  logic [15:0] y;
  integer     it, it2;
  logic signed [31:0] si;
  integer     steps;
  logic [7:0]  init_s;
  logic [4:0]  init_u;
  logic [3:0]  init_q;
  logic        f1, f2, f3, f4;

  assign f1 = (init_s == 8'hFF);
  assign f2 = (init_u == 5'h1F);
  assign f3 = (init_q == 4'hF);
  assign f4 = (1'b0 == 1'b0);

  always_ff @(posedge clk) begin
    qn <= sa;
    wn <= sa;
  end

  initial begin
    clk = 1'b0;
    sa  = -8'sd3;
    qn  = 32'd0;
    wn  = 16'd0;
    #1 clk = 1'b1;
    #1 clk = 1'b0;
    #1 clk = 1'b1;
    #1;
    $display("B1=%h", qn);
    $display("B2=%h", wn);

    // --- Compound assignment: operand kanan ikut signedness ---
    it  = 0;
    it2 = 0;
    #1;
    it += sa;
    it2 += sa;
    $display("B3=%0d", it);
    $display("B4=%0d", it2);

    // --- Compound pada LHS teriris: hanya irisan yang dibaca & ditulis ---
    y = 16'h0F00;
    #1;
    y[15:8] += 8'h01;
    $display("B5=%h", y);

    // --- Inisialisasi `for` negatif: 0 iterasi ---
    steps = 0;
    si    = 0;
    #1;
    for (si = -8'sd1; si > 0; si = si - 1) steps = steps + 1;
    $display("B6=%0d", steps);

    // --- Nilai awal deklarasi dipotong ke lebar sinyal ---
    init_s = -1;
    init_u = -1;
    init_q = -1;
    #1;
    $display("B7=%b%b%b%b", f1, f2, f3, f4);
    $finish;
  end
endmodule
