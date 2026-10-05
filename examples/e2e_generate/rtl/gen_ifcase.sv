// Tanggung jawab: demo if-generate dan case-generate (LRM §27.1).
module inv(input a, output y);
  assign y = ~a;
endmodule

// MODE=0 -> buffer, 1 -> pembalik, 2 -> identitas
module gen_ifcase #(
  parameter MODE = 1
) (
  input  logic       din,
  output logic [3:0] q
);
  generate
    // LRM §27.1: hanya cabang dengan kondisi benar yang dielaborasi, sehingga
    // sinyal dari cabang yang tidak diambil tidak muncul di design.
    if (MODE == 1) begin : g_inv
      for (genvar i = 0; i < 4; i = i + 1) begin : b
        inv u (.a(din), .y(q[i]));
      end
    end else begin : g_other
      for (genvar i = 0; i < 4; i = i + 1) begin : b
        assign q[i] = din;
      end
    end

    // LRM §27.1: case-generate memilih tepat satu lengan.
    case (MODE)
      0: begin : c_zero
        assign q[0] = 1'b0;
      end
      default: begin : c_def
        assign q[3] = 1'b1;
      end
    endcase
  endgenerate
endmodule