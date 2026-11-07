#!/bin/sh
# Tanggung jawab: demo pemanggilan task di dalam always_ff (LRM §13.3).
set -e
cd "$(dirname "$0")/../.."

RTL_DIR=examples/e2e_task_ff/rtl
SV="cargo run -q -p svrust --"

echo "=== 1. check (parse + elaborate) ==="
$SV check --rtl $RTL_DIR/tb_task_ff.sv --top tb_task_ff
echo "check OK"

echo "=== 2. run task di dalam always_ff ==="
keluaran=$($SV run --rtl $RTL_DIR/tb_task_ff.sv --top tb_task_ff --steps 6 2>/dev/null)
echo "$keluaran"

cek() {
  nama="$1"; pola="$2"; baris="$3"
  printf '  %-22s %s\n' "$nama" "$baris"
  case "$baris" in
    *"$pola"*) ;;
    *) echo "GAGAL: $nama seharusnya mengandung '$pola'"; exit 1 ;;
  esac
}

cek "cnt_42" "cnt=42" "$(echo "$keluaran" | grep '^cnt=')"

echo "DEMO OK"
