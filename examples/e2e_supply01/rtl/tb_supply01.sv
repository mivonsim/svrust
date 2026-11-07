// Tanggung jawab: demo supply0/supply1 dan implicit net Z (LRM §6.6).
// supply0/supply1 adalah net konstan 1-bit (selalu 0 / selalu 1).
// Deklarasi net tanpa tipe (`wire w;`, `output [7:0] y`) adalah implicit net
// tanpa driver → inisialisasi Z (bukan X seperti variabel).
module tb_supply01;
  supply0 z;
  supply1 o;
  wire w;                 // implicit net, tidak driven → Z
  logic [7:0] v;          // variabel tanpa inisialisasi → X

  initial begin
    $display("z=%b", z);   // 0
    $display("o=%b", o);   // 1
    $display("w=%b", w);   // z (implicit net tanpa driver)
    $display("v=%h", v);   // xx (variabel belum ditulis = X)
    $finish;
  end
endmodule
