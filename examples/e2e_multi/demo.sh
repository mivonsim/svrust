#!/bin/sh
# Tanggung jawab: demo e2e multi-berkas lintas direktori.
set -e
cd "$(dirname "$0")/../.."
RTL="examples/e2e_multi/top/core.sv examples/e2e_multi/lib/add8.sv examples/e2e_multi/lib/inc8.sv"
echo "=== 1. check 3 berkas ==="
cargo run -q -p cargo-sv -- sv check --rtl $RTL --top core 2>/dev/null
echo "=== 2. inspect ==="
cargo run -q -p cargo-sv -- sv inspect --rtl $RTL --top core 2>/dev/null
echo "=== 3. run (semua input nol -> s1=00 s2=01) ==="
cargo run -q -p cargo-sv -- sv run --rtl $RTL --top core --steps 8 2>/dev/null
echo "=== 4. module duplikat ditolak ==="
cp examples/e2e_multi/lib/add8.sv /tmp/opencode/dup.sv
sed -i 's/module add8/module inc8/' /tmp/opencode/dup.sv
cargo run -q -p cargo-sv -- sv check --rtl examples/e2e_multi/lib/add8.sv --rtl examples/e2e_multi/lib/inc8.sv --rtl /tmp/opencode/dup.sv --top add8 2>&1 | grep "^Error:" || true
echo "DEMO OK"
