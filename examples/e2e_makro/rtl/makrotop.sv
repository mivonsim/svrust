// Tanggung jawab: contoh e2e function-like makro, default arg, dan makro bersarang.
// Output deterministik (literal) supaya hasil simulasi bisa diperiksa persis.
`timescale 1ns/1ps
`include "defs.svh"
module makrotop(
  input  [`WIDTH-1:0] a,
  output [`WIDTH-1:0] y,
  output [`WIDTH-1:0] z,
  output [`WIDTH-1:0] w
);
// Function-like: substitusi argumen 10 + 5 = 15.
assign y = `ADD(8'd10, 8'd5);
// Default argumen: hi = `WIDTH -> 200 > 8, jadi 8.
assign z = `CLAMP(8'd200);
// Bersarang: DOUBLE(x) -> ADD(x, x) -> 21 + 21 = 42.
assign w = `DOUBLE(8'd21);
endmodule
