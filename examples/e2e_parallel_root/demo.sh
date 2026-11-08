#!/bin/sh
# Tanggung jawab: verifikasi build+run TANPA `--top` menghasilkan satu crate
# paralel per modul root (LRM §23.1, parallel.md fase 5), dan setiap root
# dijalankan sebagai binary terpisah dengan outputnya sendiri.
set -e
cd "$(dirname "$0")/../.."

RTL_DIR=examples/e2e_parallel_root/rtl
SV="cargo run --release -q -p svrust --"

echo "=== 1. build tanpa --top (dua root -> dua crate paralel) ==="
$SV build --rtl $RTL_DIR/root_a.sv --rtl $RTL_DIR/root_b.sv --steps 6 2>/dev/null
echo "build OK"

# Harus ada satu crate per root di target/sv/parallel/units/.
[ -d target/sv/parallel/units/sv_design_root_a ] || { echo "GAGAL: crate sv_design_root_a tidak ada"; exit 1; }
[ -d target/sv/parallel/units/sv_design_root_b ] || { echo "GAGAL: crate sv_design_root_b tidak ada"; exit 1; }
[ -f target/sv/parallel/Cargo.toml ] || { echo "GAGAL: parallel/Cargo.toml workspace tidak ada"; exit 1; }
echo "  dua crate unit + workspace Cargo.toml tersedia"

echo "=== 2. run tanpa --top (setiap root sebagai binary terpisah) ==="
keluaran=$($SV run --rtl $RTL_DIR/root_a.sv --rtl $RTL_DIR/root_b.sv --steps 6 2>/dev/null)
echo "$keluaran"

ya=$(echo "$keluaran" | grep '^y =' | sed 's/.*= *//' | tr -d '[:space:]')
zb=$(echo "$keluaran" | grep '^z =' | sed 's/.*= *//' | tr -d '[:space:]')
[ "$ya" != "00" ] || { echo "GAGAL: root_a output y masih 00"; exit 1; }
[ "$zb" != "00" ] || { echo "GAGAL: root_b output z masih 00"; exit 1; }
echo "  root_a y=$ya  root_b z=$zb (kedua root dijalankan sebagai binary terpisah)"

echo "DEMO OK"
