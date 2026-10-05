// Tanggung jawab: concatenation, replication, dan signed untuk demo e2e.
module concat_ops(
  output [7:0] cat2,
  output [5:0] cat3,
  output [7:0] rep2,
  output [7:0] rep1,
  output [7:0] mixed,
  output signed [7:0] sgn
);
  // Concatenation: item kiri jadi MSB.
  assign cat2 = {4'hA, 4'h3};
  assign cat3 = {2'h1, 2'h2, 2'h3};
  // Replication 4 x 2 bit.
  assign rep2 = {4{2'b10}};
  // Replication 8 x 1 bit.
  assign rep1 = {8{1'b1}};
  // Replication di dalam concatenation.
  assign mixed = {4'h0, {2{2'b11}}};
  // Deklarasi bertanda.
  assign sgn = 8'shFF;
endmodule
