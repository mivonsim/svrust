#!/bin/sh
# Tanggung jawab: demo e2e DPI-C import (LRM §35) — pemanggilan fungsi C nyata.
# 1. check: parse + elaborate deklarasi `import`/`export "DPI-C" function`.
# 2. Compile library C (libdpi.so) berisi implementasi fungsi import.
# 3. Jalankan program Rust pemanggil yang memuat libdpi.so via dlopen/dlsym.
set -e
cd "$(dirname "$0")/../.."

RTL_DIR=examples/e2e_dpi/rtl
SV="cargo run -q -p svrust --"

echo "=== 1. check (parse + elaborate deklarasi DPI-C) ==="
$SV check --rtl $RTL_DIR/tb_dpi.sv --top tb_dpi
echo "check OK"

echo "=== 2. compile library C -> libdpi.so ==="
gcc -shared -fPIC -o libdpi.so examples/e2e_dpi/dpi_impl.c
echo "libdpi.so OK"

echo "=== 3. compile & jalankan program pemanggil DPI-C ==="
rustc --edition 2021 examples/e2e_dpi/call_dpi.rs -o call_dpi
./call_dpi
echo "pemanggilan C nyata OK"

echo "DEMO OK"
