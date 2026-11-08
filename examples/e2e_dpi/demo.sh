#!/bin/sh
# Tanggung jawab: demo e2e deklarasi DPI-C import/export (LRM §35).
# Tahap 2.1: parser + elaborator mengenali `import`/`export "DPI-C" function`
# tanpa error (pemanggilan fungsi C nyata diserahkan ke fase codegen/runtime).
set -e
cd "$(dirname "$0")/../.."

RTL_DIR=examples/e2e_dpi/rtl
SV="cargo run -q -p svrust --"

echo "=== 1. check (parse + elaborate deklarasi DPI-C) ==="
$SV check --rtl $RTL_DIR/tb_dpi.sv --top tb_dpi
echo "check OK"

echo "DEMO OK"
