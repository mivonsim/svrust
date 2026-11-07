#!/bin/sh
# Tanggung jawab: demo supply0/supply1 dan implicit net Z (LRM §6.6).
set -e
cd "$(dirname "$0")/../.."

RTL_DIR=examples/e2e_supply01/rtl
SV="cargo run -q -p svrust --"

echo "=== 1. check (parse + elaborate) ==="
$SV check --rtl $RTL_DIR/tb_supply01.sv --top tb_supply01
echo "check OK"

echo "=== 2. run supply0/supply1 + implicit net Z ==="
keluaran=$($SV run --rtl $RTL_DIR/tb_supply01.sv --top tb_supply01 --steps 4 2>/dev/null)
echo "$keluaran"

cek() {
  nama="$1"; pola="$2"; baris="$3"
  printf '  %-22s %s\n' "$nama" "$baris"
  case "$baris" in
    *"$pola"*) ;;
    *) echo "GAGAL: $nama seharusnya mengandung '$pola'"; exit 1 ;;
  esac
}

cek "z_0" "z=0" "$(echo "$keluaran" | grep '^z=')"
cek "o_1" "o=1" "$(echo "$keluaran" | grep '^o=')"
cek "w_z" "w=z" "$(echo "$keluaran" | grep '^w=')"
cek "v_xx" "v=xx" "$(echo "$keluaran" | grep '^v=')"

echo "DEMO OK"
