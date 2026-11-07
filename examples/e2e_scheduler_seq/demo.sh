#!/bin/sh
# Tanggung jawab: demo scheduler-codegen mengedepankan proses sekuensial
# (LRM §4.4 NonBlocking phase). Tanpa blok `initial`, driver menggunakan
# `step_scheduler()` yang memanggil `eval_seq` untuk proses
# `always_ff @(posedge clk)`, sehingga counter bertambah pada setiap posedge.
set -e
cd "$(dirname "$0")/../.."

RTL_DIR=examples/e2e_scheduler_seq/rtl
SV="cargo run -q -p svrust --"

echo "=== 1. check (parse + elaborate) ==="
$SV check --rtl $RTL_DIR/tb_sched_seq.sv --top tb_sched_seq
echo "check OK"

echo "=== 2. run: counter bertambah pada setiap posedge (scheduler-driven) ==="
keluaran=$($SV run --rtl $RTL_DIR/tb_sched_seq.sv --top tb_sched_seq --steps 20 2>/dev/null)
echo "$keluaran"

val=$(echo "$keluaran" | grep '^cnt_o=' | sed 's/cnt_o=//')
[ "$val" != "00" ] || { echo "GAGAL: counter tetap 00, eval_seq tidak dipanggil dari step_scheduler"; exit 1; }
echo "  counter_bertambah   cnt_o=$val (dari 00, bertambah karena eval_seq dipanggil step_scheduler)"

echo "DEMO OK"
