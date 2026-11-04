#!/bin/sh
# Tanggung jawab: demo e2e tipe `real` dan array unpacked (LRM §5.5, §7.8).
# `%f` mencetak nilai floating point; array unpacked di-akses via indeks.
set -e
cd "$(dirname "$0")/../.."

RTL_DIR=examples/e2e_real_unpacked/rtl
SV="cargo run -q -p svrust --"

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
$SV check --rtl $RTL_DIR/tb_real_unpacked.sv --top tb_real_unpacked
echo "check OK"

echo "=== 2. simulasi real dan unpacked array ==="
keluaran=$($SV run --rtl $RTL_DIR/tb_real_unpacked.sv --top tb_real_unpacked --steps 4 2>/dev/null)
echo "$keluaran"

# r1 = 3, r2 = -2 (integer assignment karena floating point literal belum didukung parser)
cek "nilai_r1" "r1=3 r2=-2" "$(echo "$keluaran" | grep "^r1=")"
# mem[0]=aa, mem[1]=55, mem[2]=ff, mem[3]=00
cek "nilai_mem" "mem[0]=aa mem[1]=55 mem[2]=ff mem[3]=00" "$(echo "$keluaran" | grep "^mem\[0\]=")"

echo "DEMO OK"
