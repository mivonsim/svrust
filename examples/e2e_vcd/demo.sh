#!/bin/sh
# Tanggung jawab: demo e2e capture VCD untuk desain kombinasional & sekuensial.
set -e
cd "$(dirname "$0")/../.."
OUT=target/sv/demo
mkdir -p "$OUT"

echo "=== 1. gate (always_comb statement tunggal) -> VCD ==="
cargo run -q -p cargo-sv -- sv run --rtl examples/e2e_vcd/rtl/gate.sv --vcd "$OUT/gate.vcd" --steps 8 2>/dev/null
echo "   (statement tunggal tanpa begin/end kini valid)"

echo "=== 2. counter (always_ff + NBA) -> VCD ==="
cargo run -q -p cargo-sv -- sv run --rtl examples/e2e_vcd/rtl/counter.sv --vcd "$OUT/counter.vcd" --steps 12 2>/dev/null

echo "=== 3. validasi struktur VCD ==="
for f in gate counter; do
  echo "-- $f.vcd --"
  echo "  jumlah var : $(grep -c '^\$var wire' "$OUT/$f.vcd")"
  echo "  timestamp  : $(grep -c '^#' "$OUT/$f.vcd")"
  echo "  dumpvars   : $(grep -c '^\$dumpvars' "$OUT/$f.vcd")"
done

echo "=== 4. cuplikan counter.vcd (register counter naik) ==="
grep '^b' "$OUT/counter.vcd" | head -8

echo "=== 5. cek VCD bisa dibuka GTKWave (baris header) ==="
head -5 "$OUT/counter.vcd"

echo "=== 6. \$dumpfile/\$dumpvars di dalam SV (LRM §23.2) ==="
# Berkas lama dihapus dulu supaya keberadaannya benar-benar hasil run ini.
rm -f "$OUT/tb_dump.vcd" "$OUT/cli_iki.vcd"
# Tanpa --vcd: nama berkas diambil dari $dumpfile pada source SV.
cargo run -q -p cargo-sv -- sv run --rtl examples/e2e_vcd/rtl/tb_dump.sv --steps 8 2>/dev/null
if [ ! -f "$OUT/tb_dump.vcd" ]; then
  echo "GAGAL: \$dumpfile tidak menghasilkan berkas VCD"
  exit 1
fi
echo "  var        : $(grep -c '^\$var wire' "$OUT/tb_dump.vcd")"
echo "  timestamp  : $(grep -c '^#' "$OUT/tb_dump.vcd")"
echo "  dumpvars   : $(grep -c '^\$dumpvars' "$OUT/tb_dump.vcd")"
# SV menang atas --vcd: berkas dari CLI tidak boleh dibuat.
cargo run -q -p cargo-sv -- sv run --rtl examples/e2e_vcd/rtl/tb_dump.sv \
  --vcd "$OUT/cli_iki.vcd" --steps 8 2>/dev/null
if [ -f "$OUT/cli_iki.vcd" ]; then
  echo "GAGAL: --vcd menimpa \$dumpfile dari SV"
  exit 1
fi
echo "  sv menang   : cli_iki.vcd tidak dibuat"

echo "DEMO OK"
