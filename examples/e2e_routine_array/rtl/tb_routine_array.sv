// Tanggung jawab: demo argumen task bertipe array unpacked (LRM §13.3 + §7.8).
// `input logic [7:0] a [0:1]` dan `output logic [7:0] q [0:1]` — argumen array
// dikaitkan ke sinyal pemanggil saat task di-inline.
module tb_routine_array;
  logic [7:0] m [0:1];

  task ti(input logic [7:0] a [0:1]);
    $display("ti_a0=%h", a[0]);
  endtask

  task to(output logic [7:0] q [0:1]);
    q[0] = 8'h01;
  endtask

  initial begin
    m[0][0] = 8'hAB;
    ti(m);
    to(m);
    $display("after_to_m00=%h", m[0][0]);
    $finish;
  end
endmodule
