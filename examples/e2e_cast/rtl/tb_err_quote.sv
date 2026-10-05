// Tanggung jawab: error size cast tanpa lebar eksplisit (LRM §6.14).
//
// `'(a)` tidak punya lebar, jadi bukan bentuk yang sah. Pesan errornya harus
// menyebut bentuk yang benar — bukan "expected basis" yang menyesatkan karena
// `'` dibaca sebagai awal based literal.
module tb_builtin_err;
  logic [15:0] a;
  logic [7:0]  y;
  initial begin
    y = '(a);
  end
endmodule
