#!/bin/sh
# Tanggung jawab: demo e2e paralelisme waktu antar blok `initial`
# (LRM §4.4 stratified event queue, §11.2 procedural timing).
# Ekspektasi diverifikasi silang dengan iverilog 12.0 dan verilator 5.020.
set -e
cd "$(dirname "$0")/../.."

RTL_DIR=examples/e2e_time/rtl
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

echo "=== 1. check (parse + elaborate) ==="
$SV check --rtl $RTL_DIR/tb_paralel.sv --top tb_paralel
echo "check OK"

echo "=== 2. dua blok initial dengan delay sama woke pada waktu sama ==="
# LRM §4.4: dua proses yang menunda diri #5 dijadwalkan pada waktu yang
# sama, jadi keduanya bangun di t=5 lalu t=10. Versi lama menambah delay ke
# satu jam global, sehingga proses kedua bangun di t=10 lalu t=15 — dan
# `$time` yang dicetak pun salah.
keluaran=$($SV run --rtl $RTL_DIR/tb_paralel.sv --top tb_paralel --steps 6 2>/dev/null)

cek "a0" "a0 t=0" "$(echo "$keluaran" | grep '^a0 ')"
cek "b0" "b0 t=0" "$(echo "$keluaran" | grep '^b0 ')"
cek "a1" "a1 t=5" "$(echo "$keluaran" | grep '^a1 ')"
cek "b1" "b1 t=5" "$(echo "$keluaran" | grep '^b1 ')"
cek "a2" "a2 t=10" "$(echo "$keluaran" | grep '^a2 ')"
cek "b2" "b2 t=10" "$(echo "$keluaran" | grep '^b2 ')"

echo "=== 3. rantai delay berbeda tidak saling menyeret ==="
# Setiap proses punya waktu bangun sendiri (`initial_time[i]`). Rantai pendek
# `#1 #1 #1` harus berakhir di t=3 walau proses lain sudah di t=101 — memakai
# satu jam global membuat yang pendek ikut bergeser ke t=102.
rantai=$($SV run --rtl $RTL_DIR/tb_rantai.sv --top tb_rantai --steps 12 2>/dev/null)
cek "pendek" "pendek t=3" "$(echo "$rantai" | grep '^pendek ')"
cek "panjang" "panjang t=101" "$(echo "$rantai" | grep '^panjang ')"

echo "=== 4. tiga proses dengan penundaan berbeda ==="
# Urutan cetak mengikuti waktu bangun, bukan urutan proses: proses yang bangun
# lebih dulu mencetak lebih dulu.
tiga=$($SV run --rtl $RTL_DIR/tb_rantai.sv --top tb_tiga --steps 12 2>/dev/null)
# Nilai yang dicek adalah WAKTU-nya, bukan urutan baris: dua proses pada waktu
# sama boleh cetak dalam urutan apa pun, dan urutan cetakan pada langkah yang
# sama mengikuti urutan segmen di desain (bukan urutan wake time absolut).
waktu_tiga=$(echo "$tiga" | grep -E '^(a|a2|b|b2|c|c2) ' | awk '{print $2}' | sort -t= -k2 -n | uniq | tr '\n' ' ' | sed 's/ $//')
cek "tiga" "t=2 t=7 t=11 t=14 t=22" "$waktu_tiga"

echo "DEMO OK"