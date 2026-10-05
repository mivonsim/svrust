#!/bin/sh
# Tanggung jawab: demo e2e typedef dan enumerated types SystemVerilog (LRM §8.20, §6.7).
set -e
cd "$(dirname "$0")/../.."

RTL_DIR=examples/e2e_typedef/rtl
SV="cargo run -q -p cargo-sv -- sv"

# Cek bahwa satu baris keluaran bernilai persis seperti yang diharapkan.
cek() {
  nama="$1"
  ekspektasi="$2"
  baris="$3"
  printf '  %-12s %s\n' "$nama" "$baris"
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
  printf '  %-12s %s\n' "$nama" "$keluaran"
  case "$keluaran" in
    *"$pola"*) ;;
    *)
      echo "GAGAL: $nama seharusnya memuat '$pola'"
      exit 1
      ;;
  esac
}

echo "=== 1. check (parse + elaborate) ==="
$SV check --rtl $RTL_DIR/tb_typedef.sv --top tb_typedef
echo "check OK"

echo "=== 2. typedef skalar ==="
keluaran=$($SV run --rtl $RTL_DIR/tb_typedef.sv --top tb_typedef --steps 2 2>/dev/null)
# `byte_t` menyimpan 8 bit penuh.
cek "t1" "t1 byte=a5" "$(echo "$keluaran" | grep '^t1')"
# `word_t` menyimpan signed 16 bit, jadi -3 tetap bernilai -3.
cek "t2" "t2 acc=-3" "$(echo "$keluaran" | grep '^t2')"

echo "=== 3. literal enum ==="
# LRM §6.7: IDLE=0, LOAD=1, RUN=2, DONE=3 pada tipe 3 bit.
cek "t3" "t3 q=1" "$(echo "$keluaran" | grep '^t3')"
# Enum tanpa lebar eksplisit hanya 1 bit: FLAG_ON = 1.
cek "t4" "t4 f=1" "$(echo "$keluaran" | grep '^t4')"
# Nilai eksplisit menggeser urutan: KILO=10, MEGA=11, GIGA=12.
cek "t5" "t5 p=11" "$(echo "$keluaran" | grep '^t5')"
# Perbandingan literal enum bekerja seperti perbandingan konstanta biasa.
cek "t6" "t6 sama=1" "$(echo "$keluaran" | grep '^t6')"

echo "=== 4. deklarasi bertipe dengan dimension (packed array, LRM §7.3) ==="
# `byte_t [3:0] nib;` dengan `typedef logic [7:0] byte_t` adalah packed array
# 4 elemen bertipe byte_t, jadi 4 x 8 = 32 bit — bukan "persempit" ke 4 bit.
cek "t7" "t7 nib=0000000f" "$(echo "$keluaran" | grep '^t7')"
# 8'hFF di-assign ke packed array 32 bit, jadi 0xFF utuh.
cek "t8" "t8 nib_potong=000000ff" "$(echo "$keluaran" | grep '^t8')"

echo "=== 5. nilai enum melebihi lebar tipe ditolak ==="
# LRM §6.7: nilai anggota harus muat dalam lebar tipe dasar (3 bit).
gagal=$($SV check --rtl $RTL_DIR/tb_typedef_err.sv --top tb_bad_enum 2>&1 || true)
cek_gagal "enum_lebar" "nilai enum 'A' = 9 melebihi lebar tipe 3 bit" "$gagal"

echo "=== 6. tipe tak dideklarasikan ditolak ==="
# LRM §8.20: memakai nama tipe yang tidak pernah di-typedef harus gagal.
gagal2=$($SV check --rtl $RTL_DIR/tb_typedef_unknown.sv --top tb_bad_type 2>&1 || true)
cek_gagal "tipe_asing" "undefined type 'byte_t'" "$gagal2"

echo "DEMO OK"
