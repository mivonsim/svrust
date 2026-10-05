// Tanggung jawab: demo deklarasi genvar terpisah di body modul (LRM §27.3).
module genvar_decl(
  input  logic       din,
  output logic [3:0] y,
  output logic [1:0] z
);
  // LRM §27.3: genvar boleh dideklarasikan sekali di body modul lalu dipakai
  // di loop tanpa mengulang keyword `genvar`.
  genvar i, j;

  generate
    // `i` berasal dari deklarasi di atas, bukan dari `for (genvar i = ...)`.
    for (i = 0; i < 4; i = i + 1) begin : g
      assign y[i] = ~din;
    end
    // Genvar yang sama boleh dipakai ulang; nilainya di-reset ke 0 tiap loop.
    for (j = 0; j < 2; j = j + 1) begin : h
      assign z[j] = din;
    end
  endgenerate
endmodule