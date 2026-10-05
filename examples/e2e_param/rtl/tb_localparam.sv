// Testbench `localparam` (LRM §6.20).
//
// `localparam` adalah konstanta lokal modul: boleh dipakai di mana saja dalam
// modul itu — body statement, dimension, kondisi `generate`, nilai awal
// deklarasi — tetapi tidak bisa di-override saat instansiasi, berbeda dari
// `parameter`.
//
// Seatinya di `Module::localparams` terpisah dari daftar parameter port karena
// kalau dicampur, `#(.NAMA(...))` akan diam-diam diterima untuk konstanta yang
// memang harus terkunci.
//
// Semua nilai diverifikasi terhadap iverilog 12.0 (`-g2012`).
module tb_localparam;
  parameter W = 4;

  localparam DEPTH  = 8;                 // konstanta sederhana
  localparam HALF   = DEPTH / 2;         // merujuk localparam sebelumnya
  localparam MASK   = (1 << W) - 1;      // merujuk parameter modul
  localparam int  TYPED = 3;             // qualifier tipe opsional (LRM §6.20.1)
  localparam SUM    = TYPED + 1, PAIR = 2 * TYPED;  // daftar dipisah koma

  logic [W-1:0]        sized;
  logic [DEPTH-1:0]     sized_localparam;
  logic [MASK:0]        sized_mask;

  assign sized           = MASK;
  assign sized_localparam = HALF;
  assign sized_mask      = PAIR;

  // `localparam` di dalam modul anak memakai parameter EFEKTIF-nya, bukan
  // default modul.
  logic [7:0] from_child;

  child #(.W(8)) u_child (.y(from_child));

  generate
    if (DEPTH > 4) begin : g
      logic [31:0] gen_val;
      assign gen_val = W;
    end else begin : h
      logic [31:0] gen_val;
      assign gen_val = 8'hFF;
    end
  endgenerate

  initial begin
    #1;
    $display("P1=%0d", DEPTH);
    $display("P2=%0d", HALF);
    $display("P3=%0d", MASK);
    $display("P4=%0d", TYPED);
    $display("P5=%0d", SUM);
    $display("P6=%0d", PAIR);
    $display("P7=%0d", $bits(sized));
    $display("P8=%0d", $bits(sized_localparam));
    $display("P9=%0d", $bits(sized_mask));
    $display("P10=%0d", from_child);
    $finish;
  end
endmodule

// Modul anak: `localparam`-nya memakai `W` yang sudah di-override.
module child #(parameter W = 4) (output logic [W-1:0] y);
  localparam MASK = (1 << W) - 1;
  assign y = MASK[0];
endmodule
