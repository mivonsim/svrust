#!/bin/sh
# Tanggung jawab: demo e2e module berparameter (LRM §6.20).
set -e
cd "$(dirname "$0")/../.."

RTL_DIR=examples/e2e_param/rtl
SV="cargo run -q -p cargo-sv -- sv"

# Cek bahwa satu baris keluaran bernilai persis seperti yang diharapkan.
cek() {
  nama="$1"
  ekspektasi="$2"
  baris="$3"
  printf '  %-16s %s\n' "$nama" "$baris"
  if [ "$baris" != "$ekspektasi" ]; then
    echo "GAGAL: $nama seharusnya '$ekspektasi' tapi '$baris'"
    exit 1
  fi
}

# Cek bahwa pesan error yang diharapkan muncul pada keluaran CLI.
cek_gagal() {
  nama="$1"
  pola="$2"
  keluaran="$3"
  printf '  %-16s %s\n' "$nama" "$keluaran"
  case "$keluaran" in
    *"$pola"*) ;;
    *)
      echo "GAGAL: $nama seharusnya memuat '$pola'"
      exit 1
      ;;
  esac
}

echo "=== 1. check ==="
$SV check --rtl $RTL_DIR/adder.sv 2>/dev/null
echo "check OK"

echo "=== 2. inspect (WIDTH=8 tresolve) ==="
$SV inspect --rtl $RTL_DIR/adder.sv 2>/dev/null

echo "=== 3. codegen (MAX_WIDTH harus 8) ==="
$SV codegen --rtl $RTL_DIR/adder.sv 2>/dev/null
grep "MAX_WIDTH: usize = 8" target/sv/generated/adder.rs

echo "=== 4. run (input nol -> y=00, zero=1) ==="
$SV run --rtl $RTL_DIR/adder.sv --steps 8 2>/dev/null

echo "=== 5. parameter modul sebagai konstanta (LRM §6.20) ==="
# LRM §6.20: `parameter` boleh dipakai di mana saja dalam modul yang
# mendeklarasikannya. Sebelumnya modul **top** tidak punya jalur substitusi sama
# sekali — hanya modul anak yang parameternya di-inline saat instansiasi — jadi
# `assign y = WIDTH;` gagal dengan "undefined signal 'WIDTH'" padahal kodenya
# sah. Condition `generate` punya masalah serupa.
#
# Nilai di bawah diverifikasi terhadap iverilog 12.0 (`-g2012`).
$SV check --rtl $RTL_DIR/tb_param.sv --top tb_param >/dev/null 2>&1
echo "  check OK"
keluaran=$($SV run --rtl $RTL_DIR/tb_param.sv --top tb_param --steps 4 2>/dev/null)

cek "R1_body" "R1=4" "$(echo "$keluaran" | grep '^R1=')"
cek "R2_aritmetika" "R2=12" "$(echo "$keluaran" | grep '^R2=')"
cek "R4_init" "R4=4" "$(echo "$keluaran" | grep '^R4=')"
cek "R5_init_lokal" "R5=5" "$(echo "$keluaran" | grep '^R5=')"
cek "R6_generate" "R6=4" "$(echo "$keluaran" | grep '^R6=')"
cek "R7_label" "R7=4" "$(echo "$keluaran" | grep '^R7=')"
cek "R8_dimension" "R8=4" "$(echo "$keluaran" | grep '^R8=')"
cek "R10_override" "R10=8" "$(echo "$keluaran" | grep '^R10=')"

echo "=== 6. override parameter anak tidak tertimpa default ==="
# BUG: kalau substitusi parameter dijalankan untuk SEMUA modul sebelum
# instansiasi, anak memakai nilai default dan override `#(.W(8))` hilang —
# tapi lebar port anak jadi 8, jadi connection check tetap lolos dan hasilnya
# hanya terlihat dari nilainya.
OVR=$RTL_DIR/param_override.sv
cek "OVR8" "OVR8=08" "$($SV run --rtl $OVR --top t8 --steps 3 2>/dev/null | grep '^OVR8=')"
# `%h` selebar nilai: `o` 4 bit jadi satu digit heksa, jadi "4" bukan "04".
# iverilog juga mencetak `DEF4=4`.
cek "DEF4" "DEF4=4" "$($SV run --rtl $OVR --top t4 --steps 3 2>/dev/null | grep '^DEF4=')"

echo "=== 7. ekspresi konstanta pada dimension packed (LRM §7.3) ==="
# Batas dimension boleh ekspresi konstanta apa pun, bukan hanya `P` atau
# `P-1:0`. Bentuk `[W+1:0]`, `[W*2-1:0]`, `[W-2:0]` sangat umum di RTL dan
# sebelumnya ditolak parser dengan pesan "pola parameter didukung: [P] atau
# [P-1:0]".
#
# Nilai di bawah diverifikasi terhadap iverilog 12.0 (`-g2012`).
$SV check --rtl $RTL_DIR/tb_dimension.sv --top tb_dimension >/dev/null 2>&1
echo "  check OK"
dim=$($SV run --rtl $RTL_DIR/tb_dimension.sv --top tb_dimension --steps 3 2>/dev/null)

cek "E1_tambah" "E1=6" "$(echo "$dim" | grep '^E1=')"
cek "E2_kali" "E2=8" "$(echo "$dim" | grep '^E2=')"
cek "E3_dua_param" "E3=7" "$(echo "$dim" | grep '^E3=')"
cek "E4_kurung" "E4=10" "$(echo "$dim" | grep '^E4=')"
cek "E5_lsb_ekspresi" "E5=4" "$(echo "$dim" | grep '^E5=')"
cek "E6_literal" "E6=6" "$(echo "$dim" | grep '^E6=')"
cek "E7_bentuk_lama" "E7=4" "$(echo "$dim" | grep '^E7=')"
cek "E8_bentuk_tanpa_kurung" "E8=4" "$(echo "$dim" | grep '^E8=')"
cek "E9_port_msb" "E9=6" "$(echo "$dim" | grep '^E9=')"
cek "E10_port_lsb" "E10=4" "$(echo "$dim" | grep '^E10=')"

echo "=== 8. localparam (LRM §6.20) ==="
# `localparam` adalah konstanta lokal modul: boleh dipakai di mana saja
# dalam modul itu — body statement, dimension, kondisi `generate`, nilai awal
# deklarasi — tetapi tidak bisa di-override saat instansiasi, berbeda dari
# `parameter`.
#
# Seatinya di `Module::localparams` terpisah dari daftar parameter port karena
# kalau dicampur, `#(.NAMA(...))` akan diam-diam diterima untuk konstanta yang
# memang harus terkunci.
#
# Nilai di bawah diverifikasi terhadap iverilog 12.0 (`-g2012`).
$SV check --rtl $RTL_DIR/tb_localparam.sv --top tb_localparam >/dev/null 2>&1
echo "  check OK"
lp=$($SV run --rtl $RTL_DIR/tb_localparam.sv --top tb_localparam --steps 3 2>/dev/null)

cek "P1_sederhana" "P1=8" "$(echo "$lp" | grep '^P1=')"
cek "P2_rujuk_lokal" "P2=4" "$(echo "$lp" | grep '^P2=')"
cek "P3_rujuk_param" "P3=15" "$(echo "$lp" | grep '^P3=')"
cek "P4_bertipe" "P4=3" "$(echo "$lp" | grep '^P4=')"
cek "P5_daftar_koma" "P5=4" "$(echo "$lp" | grep '^P5=')"
cek "P6_daftar_koma2" "P6=6" "$(echo "$lp" | grep '^P6=')"
cek "P7_param_modul" "P7=4" "$(echo "$lp" | grep '^P7=')"
cek "P8_dimension_lp" "P8=8" "$(echo "$lp" | grep '^P8=')"
cek "P9_dimension_mask" "P9=16" "$(echo "$lp" | grep '^P9=')"
cek "P10_anak_override" "P10=1" "$(echo "$lp" | grep '^P10=')"

echo "=== 9. array unpacked (LRM §7.8) ==="
# `logic [7:0] mem [0:3];` adalah ARRAY, bukan vektor: `$bits(mem)`
# mengembalikan lebar ELEMEN (8), bukan lebar total (32). Sinyalnya sendiri
# disimpan rata, jadi `mem[k]` dipetakan ke rentang bit elemennya.
#
# Bentuk yang didukung: indeks KONSTAN pada dimension SATU. Indeks variabel
# (`mem[addr]`) dan dimensi bertingkat belum ada — keduanya butuh dynamic index
# yang belum ada di IR.
UN=examples/e2e_param/rtl/tb_unpacked.sv
$SV check --rtl $UN --top tb_unpacked >/dev/null 2>&1
echo "  check OK"
un=$($SV run --rtl $UN --top tb_unpacked --steps 3 2>/dev/null)

cek "U1_ascend_0" "U1=11" "$(echo "$un" | grep '^U1=')"
cek "U2_ascend_3" "U2=44" "$(echo "$un" | grep '^U2=')"
# Pada `[3:0]` urutan elemen terbalik dibanding `[0:3]`.
cek "U3_descend_0" "U3=aa" "$(echo "$un" | grep '^U3=')"
cek "U4_descend_3" "U4=dd" "$(echo "$un" | grep '^U4=')"
cek "U5_elemen_4bit" "U5=c" "$(echo "$un" | grep '^U5=')"
# `$bits` = lebar elemen, bukan lebar total.
cek "U6_bits_array" "U6=8" "$(echo "$un" | grep '^U6=')"
cek "U7_bits_elemen" "U7=8" "$(echo "$un" | grep '^U7=')"
cek "U8_bits_4bit" "U8=4" "$(echo "$un" | grep '^U8=')"
cek "U9_elemen_terisi" "U9=22" "$(echo "$un" | grep '^U9=')"

echo "=== 10. indeks unpacked di luar jangkauan ditolak ==="
# LRM §7.8: indeks di luar jangkauan adalah error, bukan pemotongan diam-diam.
printf 'module bad(output logic [7:0] o);
  logic [7:0] mem [0:3];
  initial o = mem[4];
endmodule
' \
  > /tmp/sv_un_neg.sv
gagal_un=$($SV check --rtl /tmp/sv_un_neg.sv --top bad 2>&1 || true)
cek_gagal "indeks_luar" "di luar jangkauan" "$gagal_un"

# Dimensi bertingkat perlu model berlapis; ditolak dengan pesan yang menyebut
# batasannya, bukan "expected ';'".
printf 'module bad2(output logic [7:0] o);
  logic [7:0] big [0:1][0:3];
endmodule
' \
  > /tmp/sv_un_2d.sv
gagal_2d=$($SV check --rtl /tmp/sv_un_2d.sv --top bad2 2>&1 || true)
cek_gagal "dua_dimensi" "bertingkat" "$gagal_2d"

echo "DEMO OK"
