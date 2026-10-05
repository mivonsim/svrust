// Tanggung jawab: target assignment sliced untuk demo e2e LHS select.
module lhs_slice(
  input  [7:0] a,
  output [7:0] y,
  output [7:0] z
);
  // Isi penuh dulu supaya nilai bit yang tidak tersentuh bisa diamati.
  assign y = 8'hFF;
  // Part-select pada LHS: hanya 4 bit bawah yang tertimpa.
  assign y[3:0] = a[3:0];
  // Bit-select pada LHS: hanya bit 0 yang tertimpa.
  assign z[0] = a[7];
endmodule
