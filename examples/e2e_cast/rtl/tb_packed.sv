module m;
  typedef logic [7:0] byte_t;
  byte_t [3:0] nib;
  initial begin
    nib = 32'hF;
    $display("P1=%h", nib);
    $finish;
  end
endmodule
