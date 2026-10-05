// Modul uji: override parameter harus dipakai, bukan nilai default.
module c #(parameter W = 4) (output logic [W-1:0] y);
  assign y = W;
endmodule

// Top dengan override: `y` jadi 8 bit dan berisi 8.
module t8(output logic [7:0] o);
  c #(.W(8)) u0(.y(o));
  initial #1 $display("OVR8=%h", o);
endmodule

// Top tanpa override: memakai default 4.
module t4(output logic [3:0] o);
  c u0(.y(o));
  initial #1 $display("DEF4=%h", o);
endmodule
