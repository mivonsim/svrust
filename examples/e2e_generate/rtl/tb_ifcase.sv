// Tanggung jawab: testbench demo if-generate dan case-generate.
module tb_ifcase;
  logic       din_inv;
  logic [3:0] q_inv;
  logic       din_buf;
  logic [3:0] q_buf;

  // MODE=1 -> cabang `if` membalik (inv), case jatuh ke lengan `default`.
  gen_ifcase #(.MODE(1)) u_inv (.din(din_inv), .q(q_inv));
  // MODE=0 -> cabang `else` meneruskan (buffer), case memakai lengan `0`.
  gen_ifcase #(.MODE(0)) u_buf (.din(din_buf), .q(q_buf));

  initial begin
    din_inv = 1'b0;
    din_buf = 1'b0;
    #1 $display("mode1 din=%b q=%b", din_inv, q_inv);
    #1 din_inv = 1'b1;
    #1 din_buf = 1'b1;
    #1 $display("mode1 din=%b q=%b", din_inv, q_inv);
    #1 $display("mode0 din=%b q=%b", din_buf, q_buf);
    #1 $finish;
  end
endmodule