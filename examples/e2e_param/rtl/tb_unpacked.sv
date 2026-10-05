// Testbench array unpacked (LRM §7.8).
//
// `logic [7:0] mem [0:3];` adalah ARRAY, bukan vektor: `$bits(mem)`
// mengembalikan lebar ELEMEN (8), bukan lebar total (32). Sinyalnya sendiri
// disimpan rata selebar `elemen * size`, dan `mem[k]` dipetakan ke rentang bit
// elemennya.
//
// Bentuk yang didukung: indeks KONSTAN pada dimension SATU. Indeks variabel
// (`mem[addr]`) dan dimensi bertingkat belum ada — keduanya butuh dynamic index
// yang belum ada di IR.
//
// Semua nilai diverifikasi terhadap iverilog 12.0 (`-g2012`).
module tb_unpacked;
  logic [7:0] mem [0:3];   // ascend: mem[0]..mem[3]
  logic [7:0] rev [3:0];   // descend: elemen pertama yang tertulis adalah rev[3]
  logic [3:0] nib;         // elemen 4-bit di dalam array 8-bit

  initial begin
    mem[0] = 8'h11;
    mem[1] = 8'h22;
    mem[2] = 8'h33;
    mem[3] = 8'h44;
    rev[0] = 8'hAA;
    rev[3] = 8'hDD;
    nib   = 4'hC;

    $display("U1=%h", mem[0]);
    $display("U2=%h", mem[3]);
    $display("U3=%h", rev[0]);
    $display("U4=%h", rev[3]);
    $display("U5=%h", nib);

    // `$bits` mengembalikan lebar ELEMEN, bukan lebar total.
    $display("U6=%0d", $bits(mem));
    $display("U7=%0d", $bits(mem[0]));
    $display("U8=%0d", $bits(nib));

    // Elemen yang tidak pernah ditulis bernilai 0 pada engine 2-state.
    $display("U9=%h", mem[1]);

    $finish;
  end
endmodule
