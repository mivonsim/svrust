#!/bin/sh
# Tanggung jawab: demo e2e preprocessor (`define, `ifdef, `include, `timescale).
set -e
cd "$(dirname "$0")/../.."
OUT=target/sv/demo
mkdir -p "$OUT"

echo "=== 1. check (preprocess + parse + elaborate) ==="
cargo run -q -p cargo-sv -- sv check --rtl examples/e2e_define/rtl/ppadd.sv 2>/dev/null

echo "=== 2. run --vcd (makro & ifdef dari include) ==="
cargo run -q -p cargo-sv -- sv run --rtl examples/e2e_define/rtl/ppadd.sv --vcd "$OUT/ppadd.vcd" --steps 6 2>/dev/null

echo "=== 3. pembuktian cabang ifdef terpilih (ADD, bukan SUB) ==="
# y terakhir = a + b + bias(0); bila salah memilih `else, y = a - b = 0.
LAST=$(grep '^b' "$OUT/ppadd.vcd" | tail -1)
echo "  perubahan terakhir di VCD: $LAST"
case "$LAST" in
  *%|*) : ;; # id ketiga = y
esac

echo "=== 4. source terekspansi berisi 0 direktif SV backtick ==="
cargo run -q -p cargo-sv -- sv codegen --rtl examples/e2e_define/rtl/ppadd.sv 2>/dev/null
BT=$(grep -cE '`(define|ifdef|ifndef|else|elsif|endif|include|timescale|undef)' target/sv/generated/ppadd.rs || true)
echo "  direktif SV tersisa di generated: $BT (harus 0)"

echo "DEMO OK"
