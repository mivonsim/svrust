#!/bin/sh
# Tanggung jawab: demo e2e 4-state logic (0,1,X,Z) di jalur simulasi (Fase 9).
# Verifikasi X/Z berpadu melalui operator bitwise, aritmetika, dan kondisi if.
set -e
cd "$(dirname "$0")/../.."

RTL_DIR=examples/e2e_4state/rtl
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
$SV check --rtl $RTL_DIR/tb_4state.sv --top tb_4state
echo "check OK"

echo "=== 2. simulasi 4-state ==="
keluaran=$($SV run --rtl $RTL_DIR/tb_4state.sv --top tb_4state --steps 4 2>/dev/null)
echo "$keluaran"

# a dan b belum diinisialisasi -> X (LRM §4.3.1); %h untuk 8-bit X = "xx"
cek "nilai_a" "a=xx b=xx" "$(echo "$keluaran" | grep '^a=')"

# X & anything = X; hasil AND/OR/XOR/ADD/MUL semuanya x karena operand semua X
cek "y_and" "y_and=xx y_or=xx y_xor=xx" "$(echo "$keluaran" | grep '^y_and=')"
cek "y_add" "y_add=xx y_mul=xx" "$(echo "$keluaran" | grep '^y_add=')"

# if (a[0]) dengan a[0] = X -> treated as false -> c_if = 0
# z_bus adalah logic 1-bit, di-assign 8'bzzzz_zzzz -> truncate ke 1 bit = Z
# %h untuk 1-bit Z = "z"
cek "c_if_z" "c_if=0 z_bus=z" "$(echo "$keluaran" | grep '^c_if=')"

echo "DEMO OK"
