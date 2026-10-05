#!/bin/sh
# Tanggung jawab: demo e2e SV -> Rust -> biner -> jalan.
set -e
cd "$(dirname "$0")/../.."
echo "=== 1. check (parse+elaborate) ==="
cargo run -q -p cargo-sv -- sv check --rtl examples/e2e_alu/rtl/alu.sv --rtl examples/e2e_alu/rtl/shift.sv 2>/dev/null
echo "check OK"
echo "=== 2. codegen ==="
cargo run -q -p cargo-sv -- sv codegen --rtl examples/e2e_alu/rtl/alu.sv 2>/dev/null
grep -c "match .*to_u64" target/sv/generated/alu.rs | xargs echo "case->match OK:"
grep -c "to_u64() ==" target/sv/generated/alu.rs | xargs echo "== bool OK:"
echo "=== 3. run alu (input nol -> y=0, zero=1) ==="
cargo run -q -p cargo-sv -- sv run --rtl examples/e2e_alu/rtl/alu.sv --steps 8 2>/dev/null
echo "=== 4. run shift (input nol -> s=00, gt=0) ==="
cargo run -q -p cargo-sv -- sv run --rtl examples/e2e_alu/rtl/shift.sv --steps 8 2>/dev/null
echo "DEMO OK"
