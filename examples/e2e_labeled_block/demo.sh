#!/bin/sh
# Tanggung jawab: demo blok berlabel (LRM §12.9).
set -e
cd "$(dirname "$0")/../.."

RTL_DIR=examples/e2e_labeled_block/rtl
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
$SV check --rtl $RTL_DIR/tb_labeled_block.sv --top tb_labeled_block
echo "check OK"

echo "=== 2. blok berlabel mengeksekusi tubuhnya ==="
keluaran=$($SV run --rtl $RTL_DIR/tb_labeled_block.sv --top tb_labeled_block --steps 2 2>/dev/null)
cek "x" "x=aa" "$(echo "$keluaran" | grep '^x=')"

echo "DEMO OK"
