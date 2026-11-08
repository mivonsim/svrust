#!/bin/sh
# Tanggung jawab: verifikasi proses waktu `always #N` dengan blok `initial`
# dan delay di dalam `if`, tanpa proses sekuensial. Driver harus memanggil
# `eval_timed()` di cabang "has initial, no sequential" agar `#delay` dalam
# `if` menangguhkan proses dan counter bertambah hingga 5, lalu $finish
# menghentikan simulasi pada t=10.
set -e
cd "$(dirname "$0")/../.."

RTL_DIR=examples/e2e_timed_initial/rtl
SV="cargo run -q -p svrust --"

echo "=== 1. check (parse + elaborate) ==="
$SV check --rtl $RTL_DIR/tb_timed_initial.sv --top tb
echo "check OK"

echo "=== 2. run: counter bertambah hingga 5 lewat delay di dalam if ==="
keluaran=$($SV run --rtl $RTL_DIR/tb_timed_initial.sv --top tb --steps 20 2>/dev/null)
echo "$keluaran"

val=$(echo "$keluaran" | grep 'cnt_o' | sed 's/.*= *//' | tr -d '[:space:]')
[ "$val" = "05" ] || { echo "GAGAL: cnt_o seharusnya 05 (cnt=5) tapi '$val'"; exit 1; }
echo "  counter_bertambah_sampai_5   cnt_o=$val (delay di if menangguhkan proses)"

echo "DEMO OK"
