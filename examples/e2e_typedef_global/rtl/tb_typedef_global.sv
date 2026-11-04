typedef logic [7:0] byte_t;
typedef logic signed [15:0] word_t;
typedef enum logic [1:0] { IDLE, RUN, DONE } state_t;

module tb_typedef_global;
  byte_t a;
  word_t b;
  state_t st;

  initial begin
    a = 8'h42;
    b = -16'sd5;
    st = RUN;
    $display("a=%h b=%d st=%d", a, b, st);
    $finish;
  end
endmodule

module child_uses_global;
  byte_t x;
  state_t cs;

  initial begin
    x = 8'hAB;
    cs = IDLE;
    $display("x=%h cs=%d", x, cs);
    $finish;
  end
endmodule
