// Tanggung jawab: demo parameter modul anak yang dipakai di badan modulnya.
module shifter #(parameter SEL = 1, parameter W = 8) (
  input  logic [W-1:0] a,
  output logic [W-1:0] y
);
  // `SEL` dan `W` adalah parameter modul ini. Former buggy: keduanya di
  // prefix instans sehingga elaborasi gagal mencari sinyal `u0__SEL`.
  assign y = (a >> SEL);
endmodule

module tb_param(
  input  logic [7:0] x,
  output logic [7:0] p,
  output logic [7:0] q
);
  shifter #(.SEL(2), .W(8)) u_slow (.a(x), .y(p));
  shifter #(.SEL(4), .W(8)) u_fast (.a(x), .y(q));

  initial #1 $finish;
endmodule