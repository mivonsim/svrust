#!/bin/sh
# Tanggung jawab: demo `always @(sinyal)` level-sensitive (LRM §9.2.1).
set -e
cd "$(dirname "$0")/../.."

RTL_DIR=examples/e2e_always_level/rtl
SV="cargo run -q -p svrust --"

echo "=== 1. check (parse + elaborate) ==="
$SV check --rtl $RTL_DIR/tb_always_level.sv --top tb_always_level
echo "check OK"

echo "=== 2. run always @(clk) level-sensitive ==="
keluaran=$($SV run --rtl $RTL_DIR/tb_always_level.sv --top tb_always_level --steps 30 2>/dev/null)
echo "$keluaran"

cek() {
  nama="$1"; pola="$2"; baris="$3"
  printf '  %-20s %s\n' "$nama" "$baris"
  case "$baris" in
    *"$pola"*) ;;
    *) echo "GAGAL: $nama seharusnya mengandung '$pola'"; exit 1 ;;
  esac
}

cek "cnt_3" "cnt=3 (expect 3)" "$(echo "$keluaran" | grep '^cnt=')"

echo "DEMO OK"
