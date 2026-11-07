#!/bin/sh
# Tanggung jawab: demo resolusi multi-driver wired-AND/wired-OR (LRM §6.6).
set -e
cd "$(dirname "$0")/../.."

RTL_DIR=examples/e2e_wired/rtl
SV="cargo run -q -p svrust --"

echo "=== 1. check (parse + elaborate) ==="
$SV check --rtl $RTL_DIR/tb_wired.sv --top tb_wired
echo "check OK"

echo "=== 2. run resolusi wired-AND / wired-OR ==="
keluaran=$($SV run --rtl $RTL_DIR/tb_wired.sv --top tb_wired --steps 4 2>/dev/null)
echo "$keluaran"

cek() {
  nama="$1"; pola="$2"; baris="$3"
  printf '  %-20s %s\n' "$nama" "$baris"
  case "$baris" in
    *"$pola"*) ;;
    *) echo "GAGAL: $nama seharusnya mengandung '$pola'"; exit 1 ;;
  esac
}

cek "wire_lastwins" "wire_lastwins=11110000" "$(echo "$keluaran" | grep '^wire_lastwins=')"
cek "wand_and"      "wand_and=10100000"      "$(echo "$keluaran" | grep '^wand_and=')"
cek "wor_or"        "wor_or=11111010"        "$(echo "$keluaran" | grep '^wor_or=')"
cek "tri_or"        "tri_or=10101111"        "$(echo "$keluaran" | grep '^tri_or=')"

echo "DEMO OK"
