// Tanggung jawab: demo system task di dalam badan proses sekuensial (LRM §20).
module gen_task(
  input  logic       clk,
  input  logic       din,
  output logic [3:0] q
);
  // LRM §20 tidak membatasi system task pada blok `initial` saja, sehingga
  // `$display` sah di dalam `always_ff`.
  always_ff @(posedge clk) begin
    q <= din;
    $display("task a q=%b din=%b", q, din);
  end

  // LRM §20.3: `$finish` menghentikan simulasi dari proses mana pun.
  always_ff @(posedge clk) begin
    if (q) begin
      $display("task berhenti di clk kedua");
      $finish;
    end
  end
endmodule