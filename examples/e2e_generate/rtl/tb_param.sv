// Tanggung jawab: testbench demo parameter modul anak (hierarki tiga tingkat).
module tb_param_wrap;
  logic [7:0] x;
  logic [7:0] p;
  logic [7:0] q;

  // `tb_param` memuat dua instansi `shifter`; `shifter` memakai parameternya
  // sendiri di badan modul. Tiga tingkat hierarki sekaligus.
  tb_param dut (.x(x), .p(p), .q(q));

  initial begin
    // 10110000 >> 2 = 00101100, >> 4 = 00001011
    x = 8'b10110000;
    #1 $display("param p=%b q=%b", p, q);
    #1 $finish;
  end
endmodule