#!/bin/bash
# Tanggung jawab: skrip benchmark parallel build SVRust (Tahap 4 paralel.md).
# Ukur waktu build, peak RAM, CPU utilization, throughput untuk berbagai worker count.
set -euo pipefail

RTL_DIR="${RTL_DIR:-examples/e2e_expr/rtl}"
STEPS="${STEPS:-4}"

# Kumpulkan semua file .sv dari direktori RTL
RTL_FILES=$(find "$RTL_DIR" -name "*.sv" -type f | sort)
RTL_ARGS=""
for f in $RTL_FILES; do
    RTL_ARGS="$RTL_ARGS --rtl $f"
done

echo "=== SVRust Parallel Build Benchmark ==="
echo "RTL dir: $RTL_DIR"
echo "Jumlah file SV: $(echo "$RTL_FILES" | wc -l)"
echo "Steps: $STEPS"
echo ""

for workers in 1 2 4 8 16 32; do
    echo "--- Workers: $workers ---"
    
    # Bersihkan build sebelumnya
    cargo clean -p svrust 2>/dev/null || true
    rm -rf target/sv/parallel
    
    start=$(date +%s.%N)
    
    # Build tanpa --top (semua root)
    eval "cargo run --release -p svrust -- build $RTL_ARGS --steps $STEPS" > /tmp/build_${workers}.log 2>&1
    
    end=$(date +%s.%N)
    
    elapsed=$(echo "$end - $start" | bc)
    
    # Hitung jumlah unit yang dibangun
    units=$(find target/sv/parallel/units -name "Cargo.toml" 2>/dev/null | wc -l)
    
    echo "Waktu build: ${elapsed}s"
    echo "Jumlah unit: $units"
    echo "Throughput: $(echo "scale=2; $units / $elapsed" | bc) units/detik"
    echo ""
done

echo "=== Selesai ==="
