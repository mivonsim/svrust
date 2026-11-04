module tb_4state;
  logic [7:0] a;
  logic [7:0] b;
  logic [7:0] y_and;
  logic [7:0] y_or;
  logic [7:0] y_xor;
  logic [7:0] y_add;
  logic [7:0] y_mul;
  logic       c_if;
  logic       z_bus;

  // Tidak ada inisialisasi: a dan b bernilai X pada t=0 (LRM §4.3.1).

  always_comb begin
    y_and = a & b;   // X & 0 = 0, X & 1 = X
    y_or  = a | b;   // X | 0 = X, X | 1 = 1
    y_xor = a ^ b;   // X ^ 0 = X
    y_add = a + b;   // X + anything = X
    y_mul = a * b;   // X * anything = X
  end

  always_comb begin
    if (a[0])        // X dalam kondisi if -> treated as false
      c_if = 1;
    else
      c_if = 0;
  end

  assign z_bus = 8'bzzzz_zzzz;  // Z sebagai nilai net

  initial begin
    $display("a=%h b=%h", a, b);
    $display("y_and=%h y_or=%h y_xor=%h", y_and, y_or, y_xor);
    $display("y_add=%h y_mul=%h", y_add, y_mul);
    $display("c_if=%b z_bus=%h", c_if, z_bus);
    $finish;
  end

endmodule
