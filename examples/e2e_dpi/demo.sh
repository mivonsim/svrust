#!/bin/sh
# Tanggung jawab: demo e2e DPI-C import + export (LRM §35).
# 1. check: parse + elaborate deklarasi `import`/`export "DPI-C" function`.
# 2. codegen: verifikasi kode Rust di-generate berisi `extern "C"` +
#    fungsi pembawa untuk import dan `#[no_mangle] extern "C"` untuk export.
# 3. Compile library C dan jalankan program pemanggil import via dlopen/dlsym.
set -e
cd "$(dirname "$0")/../.."

RTL_DIR=examples/e2e_dpi/rtl
SV="cargo run -q -p svrust --"

echo "=== 1. check (parse + elaborate deklarasi DPI-C import/export) ==="
$SV check --rtl $RTL_DIR/tb_dpi.sv --top tb_dpi
echo "check OK"

echo "=== 2. codegen (verifikasi `extern "C"` dan wrapper DPI-C) ==="
$SV codegen --rtl $RTL_DIR/tb_dpi.sv --top tb_dpi
GEN=$(ls target/sv/generated/sv_design_tb_dpi.rs 2>/dev/null || ls target/sv/generated/tb_dpi.rs 2>/dev/null)
if [ -z "$GEN" ]; then
  echo "ERROR: file generated tidak ditemukan"
  ls target/sv/generated/ 2>/dev/null || true
  exit 1
fi
if grep -q 'extern "C"' "$GEN"; then echo "  extern \"C\" deklarasi: DITEMUKAN"; else echo "ERROR: extern \"C\" tidak ditemukan"; exit 1; fi
grep -q 'dpi_set_data\|dpi_hitung_paritas' "$GEN" && echo "  fungsi pembawa import: DITEMUKAN" || { echo "ERROR: fungsi pembawa import tidak ditemukan"; exit 1; }
grep -q '#\[no_mangle\]' "$GEN" && echo "  wrapper export #[no_mangle]: DITEMUKAN" || { echo "ERROR: wrapper export tidak ditemukan"; exit 1; }
grep -q 'c_tampilkan_hasil' "$GEN" && echo "  simbol export c_tampilkan_hasil: DITEMUKAN" || { echo "ERROR: simbol export tidak ditemukan"; exit 1; }
echo "codegen OK"

echo "=== 3. compile library C -> libdpi.so ==="
gcc -shared -fPIC -o libdpi.so examples/e2e_dpi/dpi_impl.c
echo "libdpi.so OK"

echo "=== 4. compile & jalankan program pemanggil DPI-C import ==="
rustc --edition 2021 examples/e2e_dpi/call_dpi.rs -o call_dpi
./call_dpi
echo "pemanggilan C nyata OK"

echo "DEMO OK"
