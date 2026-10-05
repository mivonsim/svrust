#!/bin/sh
# Tanggung jawab: demo e2e `timescale` unit/precision (LRM §21.8) dan `%t`
# (LRM §20.4). Ekspektasi diverifikasi silang dengan iverilog 12.0 dan
# verilator 5.020.
set -e
cd "$(dirname "$0")/../.."

RTL_DIR=examples/e2e_timescale/rtl
SV="cargo run -q -p cargo-sv -- sv"

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

cek_gagal() {
  nama="$1"
  pola="$2"
  keluaran="$3"
  printf '  %-12s %s\n' "$nama" "$keluaran"
  case "$keluaran" in
  *"$pola"*) ;;
  *)
    echo "GAGAL: $nama sequestra '$pola'"
    exit 1
    ;;
  esac
}

echo "=== 1. check (parse + elaborate) ==="
$SV check --rtl $RTL_DIR/tb_timescale.sv --top tb_timescale
echo "check OK"

echo "=== 2. timescale 1us/1ns berlaku per modul ==="
# LRM §21.8: `#n` tanpa satuan memakai `timeunit`. `#1` = 1us, jadi $time = 1
# dan `%t` menampilkannya dalam presisi 1ns sebagai 1000. Dulu delay tanpa
# satuan selalu ditafsirkan nanosecond, jadi t = 1000.
keluaran=$($SV run --rtl $RTL_DIR/tb_timescale.sv --top tb_timescale --steps 4 2>/dev/null)
cek "unit_us" "a t=1 [                1000]" "$(echo "$keluaran" | grep '^a ')"
cek "unit_us2" "b t=2 [                2000]" "$(echo "$keluaran" | grep '^b ')"
# `#500ns` memakai satuan eksplisit. LRM §21.8 membulatkan $time ke timeunit:
# 1.5us -> 2. Pemotongan (bukan pembulatan) memberi 1.
cek "bulat" "c t=3 [                3000]" "$(echo "$keluaran" | grep '^c ')"
cek "ps" "d t=3 [                3000]" "$(echo "$keluaran" | grep '^d ')"

echo "=== 3. timescale kedua hanya berlaku untuk modul setelahnya ==="
kecil=$($SV run --rtl $RTL_DIR/tb_timescale.sv --top tb_timescale_kecil --steps 4 2>/dev/null)
cek "unit_ns" "e t=1 [                1000]" "$(echo "$kecil" | grep '^e ')"
cek "unit_ns2" "f t=8 [                8000]" "$(echo "$kecil" | grep '^f ')"

echo "=== 4. presisi 1ps pada %t ==="
ps=$($SV run --rtl $RTL_DIR/tb_timescale_ps.sv --top tb_timescale_ps --steps 4 2>/dev/null)
cek "ps_a" "g t=1 [                1000]" "$(echo "$ps" | grep '^g ')"
cek "ps_b" "h t=1501 [             1501000]" "$(echo "$ps" | grep '^h ')"

echo "=== 5. pengali timescale selain 1 ditolak ==="
# `10ns/1ns` berarti `#1` = 10 ns. Mengabaikannya membuat semua delay modul
# sepuluh kali terlalu kecil tanpa pesan apa pun.
printf '`timescale 10ns/1ns\nmodule m; endmodule\n' > /tmp/sv_skala.sv
gagal=$($SV check --rtl /tmp/sv_skala.sv --top m 2>&1 || true)
cek_gagal "pengali" "timescale" "$gagal"

echo "=== 6. timeprecision lebih kasar dari timeunit ditolak ==="
# `1us/1ms` melanggar LRM §21.8. Dulu diterima dan menghasilkan satuan yang
# tidak bisa dibaca sebagai waktu ($time = 0 atau angka aneh).
printf '`timescale 1us/1ms\nmodule m; endmodule\n' > /tmp/sv_skala2.sv
gagal=$($SV check --rtl /tmp/sv_skala2.sv --top m 2>&1 || true)
cek_gagal "presisi" "timeprecision" "$gagal"

echo "=== 7. \$time dan %t pakai skala modul sendiri setelah flatten ==="
# LRM §21.8: skala mengikuti modul yang memuat ekspresi. Dulu SVRust memakai
# skala modul top untuk semua `$time`, sehingga modul anak 1us terbaca 1000x
# lebih besar (t=1001, bukan t=1).
anak=$($SV run --rtl $RTL_DIR/tb_anak_skala.sv --top tb_anak_skala --steps 4 2>/dev/null)
cek "anak" "anak t=1 [                1000]" "$(echo "$anak" | grep '^anak ')"
cek "top" "top t=1 [                   1]" "$(echo "$anak" | grep '^top ')"

echo "DEMO OK"