#!/bin/sh
# Tanggung jawab: demo always_ff dievaluasi melalui step_scheduler
# (LRM §4.4 region event NonBlocking). Tanpa blok initial, driver
# menggunakan step_scheduler() yang memanggil eval_seq untuk proses
# sekuensial, sehingga counter bertambah pada setiap posedge clock.
set -e
cd "$(dirname "$0")/../.."

RTL_DIR=examples/e2e_ff_counter/rtl
SV="cargo run -q -p svrust --"

echo "=== 1. check (parse + elaborate) ==="
$SV check --rtl $RTL_DIR/tb_ff_counter.sv --top tb_ff_counter
echo "check OK"

echo "=== 2. run: counter bertambah pada setiap posedge (scheduler-driven) ==="
keluaran=$($SV run --rtl $RTL_DIR/tb_ff_counter.sv --top tb_ff_counter --steps 20 2>/dev/null)
echo "$keluaran"

val=$(echo "$keluaran" | grep '^cnt_o=' | sed 's/cnt_o=//')
[ "$val" != "00" ] || { echo "GAGAL: counter tetap 00, eval_seq tidak dipanggil dari step_scheduler"; exit 1; }
echo "  counter_bertambah   cnt_o=$val (dari 00, bertambah karena eval_seq dipanggil step_scheduler)"

echo "DEMO OK"
