#!/bin/sh
# Tanggung jawab: demo e2e array unpacked multi-dimensi (LRM §7.8).
# `logic [7:0] mem [0:3][0:15];` — 4 x 16 elemen bertipe 8-bit,
# disimpan rata selebar 512 bit. Indeks multi-tingkat `mem[i][j]`
# dipetakan ke offset bit yang benar.
set -e
cd "$(dirname "$0")/../.."

RTL_DIR=examples/e2e_unpacked_multi/rtl
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
$SV check --rtl $RTL_DIR/tb_unpacked_multi.sv --top tb_unpacked_multi
echo "check OK"

echo "=== 2. simulasi array unpacked multi-dimensi ==="
keluaran=$($SV run --rtl $RTL_DIR/tb_unpacked_multi.sv --top tb_unpacked_multi --steps 4 2>/dev/null)
echo "$keluaran"

cek "m00" "mem[0][0]=00 mem[0][1]=11" "$(echo "$keluaran" | grep '^mem\[0\]\[0\]=')"
cek "m10" "mem[1][0]=22 mem[1][1]=33" "$(echo "$keluaran" | grep '^mem\[1\]\[0\]=')"
cek "m215" "mem[2][15]=ff mem[3][15]=ee" "$(echo "$keluaran" | grep '^mem\[2\]\[15\]=')"
cek "val" "val=ff" "$(echo "$keluaran" | grep '^val=')"

echo "DEMO OK"
