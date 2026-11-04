#!/bin/sh
# Tanggung jawab: demo e2e typedef di luar module (compilation-unit scope, LRM §8.20).
# `typedef` di luar module tersedia untuk semua module dalam kompilasi unit yang sama.
set -e
cd "$(dirname "$0")/../.."

RTL_DIR=examples/e2e_typedef_global/rtl
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
$SV check --rtl $RTL_DIR/tb_typedef_global.sv --top tb_typedef_global
echo "check OK"

echo "=== 2. simulasi typedef global ==="
keluaran=$($SV run --rtl $RTL_DIR/tb_typedef_global.sv --top tb_typedef_global --steps 4 2>/dev/null)
echo "$keluaran"

# byte_t = 8'h42, word_t = -16'sd5 = -5, state_t RUN = 1
# %d untuk 16-bit signed punya lebar baku 6 kolom, jadi -5 = "    -5"
cek "nilai_a" "a=42 b=    -5 st=1" "$(echo "$keluaran" | grep '^a=')"

echo "=== 3. typedef global tersedia untuk semua module ==="
# child_uses_global juga menggunakan byte_t dan state_t dari luar module
keluaran=$($SV run --rtl $RTL_DIR/tb_typedef_global.sv --top child_uses_global --steps 4 2>/dev/null)
echo "$keluaran"
# byte_t = 8'hAB, state_t IDLE = 0
cek "nilai_x" "x=ab cs=0" "$(echo "$keluaran" | grep '^x=')"

echo "DEMO OK"
