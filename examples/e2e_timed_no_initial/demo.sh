#!/bin/sh
# Tanggung jawab: verifikasi proses waktu `always #N` tanpa blok `initial`
# dievaluasi melalui driver plain (eval_timed dalam loop scheduler).
set -e
cd "$(dirname "$0")/../.."

RTL_DIR=examples/e2e_timed_no_initial/rtl
SV="cargo run -q -p svrust --"

echo "=== 1. check (parse + elaborate) ==="
$SV check --rtl $RTL_DIR/tb_timed_no_initial.sv --top tb
echo "check OK"

echo "=== 2. run: counter bertambah melalui proses waktu ==="
keluaran=$($SV run --rtl $RTL_DIR/tb_timed_no_initial.sv --top tb --steps 20 2>/dev/null)
echo "$keluaran"

val=$(echo "$keluaran" | grep '^cnt_o=' | sed 's/cnt_o=//')
[ "$val" != "00" ] || { echo "GAGAL: counter tetap 00, eval_timed tidak dipanggil di driver plain"; exit 1; }
echo "  counter_bertambah   cnt_o=$val (proses waktu jalan tanpa initial)"

echo "DEMO OK"
