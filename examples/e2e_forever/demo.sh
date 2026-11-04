#!/bin/sh
# Tanggung jawab: demo e2e forever loop (LRM §12.8).
# `always #5 clk = ~clk;` — clock generator (proses waktu, sudah didukung).
# `forever @(posedge clk)` — loop tak berujung dipicu tiap edge clock,
# berhenti saat $finish dipanggil di dalam loop.
set -e
cd "$(dirname "$0")/../.."

RTL_DIR=examples/e2e_forever/rtl
SV="cargo run -q -p svrust --"

cek() {
  nama="$1"
  ekspektasi="$2"
  baris="$3"
  printf '  %-12s %s\n' "$nama" "$baris"
  if [ "$baris" != "$ekspektasi" ]; then
    echo "GAGAL: $nama seharusnya '$ekspektasi' tapi '$baris'"
    exit 1
  fi
}

echo "=== 1. check (parse + elaborate) ==="
$SV check --rtl $RTL_DIR/tb_forever.sv --top tb_forever
echo "check OK"

echo "=== 2. simulasi forever loop ==="
# cnt di-increment pada setiap posedge clk, sampai cnt=10 lalu $finish.
keluaran=$($SV run --rtl $RTL_DIR/tb_forever.sv --top tb_forever --steps 50 2>/dev/null)
echo "$keluaran"

# $display mencetak cnt=1 sampai cnt=10, lalu $finish menghentikan simulasi.
case "$keluaran" in
  *"cnt =  10"*)
    ;;
  *)
    echo "GAGAL: cnt seharusnya mencapai 10 sebelum $finish"
    echo "keluaran: $keluaran"
    exit 1
    ;;
esac

# Simulasi harus berhenti setelah $finish (tidak ada output setelah cnt=10)
terakhir=$(echo "$keluaran" | grep '^cnt' | tail -1)
case "$terakhir" in
  *"cnt =  10"*) ;;
  *)
    echo "GAGAL: output terakhir seharusnya cnt=10, dapat: $terakhir"
    exit 1
    ;;
esac

echo "DEMO OK"
