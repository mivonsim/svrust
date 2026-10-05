#!/bin/sh
# Tanggung jawab: demo e2e hierarki dua instansi beda WIDTH.
set -e
cd "$(dirname "$0")/../.."
echo "=== 1. check ==="
cargo run -q -p cargo-sv -- sv check --rtl examples/e2e_hier/rtl/ripple.sv 2>/dev/null
echo "check OK"
echo "=== 2. inspect --top ripple ==="
cargo run -q -p cargo-sv -- sv inspect --rtl examples/e2e_hier/rtl/ripple.sv --top ripple 2>/dev/null
echo "=== 3. run (x=z=0 -> big=0001 small=01) ==="
cargo run -q -p cargo-sv -- sv run --rtl examples/e2e_hier/rtl/ripple.sv --top ripple --steps 8 2>/dev/null
echo "DEMO OK"
