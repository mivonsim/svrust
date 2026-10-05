// Testbench scope typedef SystemVerilog (LRM §8.20).
//
// `typedef` bersifat module-scoped: nama tipe hanya terlihat di modul tempat
// ia dideklarasikan. Karena SVRust mem-flatten instans ke satu design, nama
// tipe harus di-prefix instans seperti sinyal — kalau tidak, dua arah
// kesalahan muncul: typedef anak tak terjangkau, dan typedef induk bocor ke
// anak sehingga kode yang salah lolos dengan lebar yang keliru.
//
// Tiga top di berkas ini:
//   top_ok        — typedef anak dipakai di badan anak (harus berhasil)
//   top_nama_sama — dua modul, typedef `w_t` sama tapi lebar beda (berhasil)
//   top_bocor     — typedef induk dipakai di badan anak (harus ditolak)

// --- (a) typedef milik anak, dipakai di badan anak ---

module child_ok(input [7:0] a, output [7:0] y);
  typedef logic [7:0] byte_t;
  assign y = byte_t'(a);
endmodule

module top_ok(input [7:0] a, output [7:0] y);
  child_ok c0(.a(a), .y(y));
endmodule

// --- (b) typedef nama sama, lebar berbeda, di dua modul terpisah ---

module cell_narrow(input [7:0] a, output [7:0] y);
  typedef logic [7:0] w_t;
  assign y = w_t'(a);
endmodule

module cell_wide(input [15:0] a, output [15:0] y);
  typedef logic [15:0] w_t;
  assign y = w_t'(a);
endmodule

module top_nama_sama(
  input [7:0] a8,
  input [15:0] a16,
  output [7:0] y8,
  output [15:0] y16
);
  cell_narrow n0(.a(a8), .y(y8));
  cell_wide   w0(.a(a16), .y(y16));
endmodule

// --- (c) typedef milik induk dipakai di badan anak: harus DITOLAK ---

module child_bocor(input [15:0] a, output [7:0] y);
  typedef logic [7:0] c_t;
  // `byte_t` hanya di-typedef di `top_bocor`, jadi tidak boleh terlihat di sini.
  assign y = c_t'(a) + byte_t'(a);
endmodule

module top_bocor(input [15:0] a, output [7:0] y);
  typedef logic [7:0] byte_t;
  child_bocor c0(.a(a), .y(y));
endmodule

// --- (d) nama yang sama untuk variabel dan typedef: harus DITOLAK ---
//
// LRM §8.20: satu nama hanya boleh dipakai untuk satu hal di satu scope.
// Tanpa pemeriksaan ini, `$bits(foo)` akan memilih tipe diam-diam dan kode
// yang salah lolos tanpa error.
// iverilog: "'foo' has already been declared in this scope"

module top_nama_ganda(output [31:0] y);
  logic [3:0] foo;
  typedef logic [11:0] foo;
  assign y = $bits(foo);
endmodule
