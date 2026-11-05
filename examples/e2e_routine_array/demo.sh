#!/bin/sh
# Tanggung jawab: demo argumen task bertipe array unpacked (LRM §13.3 + §7.8).
set -e
cd "$(dirname "$0")/../.."

RTL_DIR=examples/e2e_routine_array/rtl
SV="cargo run -q -p svrust --"

cek() {
  nama="$1"
  ekspektasi="$2"
  baris="$3"
  printf '  %-22s %s\n' "$nama" "$baris"
  if [ "$baris" != "$ekspektasi" ]; then
    echo "GAGAL: $nama seharusnya '$ekspektasi' tapi '$baris'"
    exit 1
  fi
}

echo "=== 1. check (parse + elaborate) ==="
$SV check --rtl $RTL_DIR/tb_routine_array.sv --top tb_routine_array
echo "check OK"

echo "=== 2. argumen task array unpacked (input + output elemen) ==="
keluaran=$($SV run --rtl $RTL_DIR/tb_routine_array.sv --top tb_routine_array --steps 4 2>/dev/null)
cek "ti_a0" "ti_a0=ab" "$(echo "$keluaran" | grep '^ti_a0=')"
cek "after_to_m00" "after_to_m00=1" "$(echo "$keluaran" | grep '^after_to_m00=')"

echo "DEMO OK"
