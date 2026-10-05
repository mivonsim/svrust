// Testbench NBA di dalam `initial` dan ekspresi konstanta pada kondisi `generate`
// (LRM §10.4, §15.2, §27.1).
//
// Dua gap yang ditutup di sini:
//   - `initial q <= 8'hAA;` sah menurut LRM §15.2, tapi parser menolaknya
//     karena AST `CombinationalStatement` tidak punya varian NBA.
//   - "Konstan" pada kondisi `generate` juga mencakup cast, select, concat,
//     dan replication; evaluator `konst` dulu hanya mengenali
//     literal/parameter/operator.
//
// Tiap cabang generate memakai sinyal sendiri supaya tidak ada dua driver pada
// satu net (yang iverilog tolak di sintaks).
//
// Semua nilai diverifikasi terhadap iverilog 12.0 (`-g2012`).
module tb_genconst #(parameter W = 4) ();
  typedef logic [7:0] byte_t;

  // --- NBA di dalam `initial` (LRM §15.2) ---
  logic [7:0] nba_a;
  logic [7:0] nba_b;
  logic [7:0] blk_c;

  initial begin
    nba_a <= 8'hAA;
    nba_b <= 8'hBB;
    blk_c  = 8'hCC;
    #1;
    $display("G1=%h %h %h", nba_a, nba_b, blk_c);
  end

  // --- Ekspresi konstanta pada kondisi `generate` (LRM §27.1) ---
  // Tiap cabang bernilai 1 kalau syaratnya benar, jadi hasilnya adalah
  // jumlah syarat yang terpenuhi.
  logic size_salah;   // 16'(W) > 8 SALAH untuk W=4
  logic size_ok;      // 8'(W) > 2
  logic typedef_ok;   // byte_t'(W) == 8'h04
  logic sign_ok;      // signed'(W) > 0
  logic concat_ok;    // {4'hA, 4'h1} == 8'hA1
  logic repl_ok;      // {2{4'hC}} == 8'hCC

  generate
    if (16'(W) > 8) begin : g_size_salah
      assign size_salah = 1'b1;
    end else begin : h_size_benar
      assign size_salah = 1'b0;
    end

    if (8'(W) > 2) begin : g_size_ok
      assign size_ok = 1'b1;
    end else begin : h_size_ok
      assign size_ok = 1'b0;
    end

    if (byte_t'(W) == 8'h04) begin : g_typedef
      assign typedef_ok = 1'b1;
    end else begin : h_typedef
      assign typedef_ok = 1'b0;
    end

    if (signed'(W) > 0) begin : g_sign
      assign sign_ok = 1'b1;
    end else begin : h_sign
      assign sign_ok = 1'b0;
    end

    // Part-select pada kondisi generate TIDAK diuji di sini: iverilog
    // menolaknya di sisi parser walau LRM §27.1 hanya mensyaratkan ekspresi
    // konstan, jadi tidak bisa diverifikasi silang. Dukungannya tetap ada
    // dan diuji lewat unit test.

    if ({4'hA, 4'h1} == 8'hA1) begin : g_concat
      assign concat_ok = 1'b1;
    end else begin : h_concat
      assign concat_ok = 1'b0;
    end

    if ({2{4'hC}} == 8'hCC) begin : g_replicate
      assign repl_ok = 1'b1;
    end else begin : h_replicate
      assign repl_ok = 1'b0;
    end
  endgenerate

  logic [2:0] total;

  initial begin
    total = size_ok + typedef_ok + sign_ok + concat_ok + repl_ok;
    #2;
    $display("G2=%0d", total);
    $display("G3=%b%b%b%b%b", size_salah, size_ok, typedef_ok, sign_ok, concat_ok);
    $finish;
  end
endmodule
