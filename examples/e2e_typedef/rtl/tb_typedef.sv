// Testbench typedef dan enumerated types SystemVerilog (LRM §8.20, §6.7).
//
// Semua stimulus berupa literal sehingga hasil simulasi deterministik.
//
// Catatan cakupan: cast `byte_t'(x)` (LRM §6.14) dan `$bits` belum
// didukung, jadi keduanya tidak dipakai di sini. Lebar tipe hasil typedef
// diuji lewat pembulatan nilai saat disimpan ke sinyal yang lebih sempit.
module tb_typedef;
  // LRM §8.20: typedef menyalin tipe yang sudah ada.
  typedef logic [7:0] byte_t;
  typedef logic signed [15:0] word_t;

  // LRM §6.7: enumerasi dengan lebar tipe dasar eksplisit. Nilai anggota
  // dihitung berurutan: IDLE=0, LOAD=1, RUN=2, DONE=3.
  typedef enum logic [2:0] { IDLE, LOAD, RUN, DONE } state_t;

  // Enum tanpa lebar eksplisit: default 1 bit, jadi hanya dua nilai.
  typedef enum { FLAG_OFF, FLAG_ON } flag_t;

  // Enum dengan nilai eksplisit; anggota berikutnya melanjutkan dari situ.
  typedef enum logic [3:0] { KILO = 10, MEGA, GIGA } prefix_t;

  byte_t  data;
  word_t  acc;
  state_t q;
  flag_t  f;
  prefix_t p;

  logic [7:0] hasil_byte;

  // LRM §7.3: `byte_t [3:0] nib;` dengan `typedef logic [7:0] byte_t` adalah
  // packed array 4 elemen bertipe byte_t, jadi 32 bit — bukan penyempitan.
  byte_t [3:0] nib;

  initial begin
    data = 8'hA5;
    $display("t1 byte=%h", data);

    acc = -16'sd3;
    $display("t2 acc=%0d", acc);

    // LRM §6.7: nama anggota enum adalah konstanta, boleh dipakai langsung.
    q = LOAD;
    $display("t3 q=%0d", q);

    f = FLAG_ON;
    $display("t4 f=%0d", f);

    p = MEGA;
    $display("t5 p=%0d", p);

    // Perbandingan antar literal enum.
    hasil_byte = (q == LOAD) ? 8'd1 : 8'd0;
    $display("t6 sama=%0d", hasil_byte);

    nib = 32'hF;
    $display("t7 nib=%h", nib);

    // Lebar `nib` 32 bit, jadi 8'hFF terpotong ke 0xFF (bukan 0xF seperti
    // saat deklarasi ini dianggap "mempersempit").
    nib = 8'hFF;
    $display("t8 nib_potong=%h", nib);

    $finish;
  end
endmodule