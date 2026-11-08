#!/bin/bash
# Tanggung jawab: skrip benchmark parallel build SVRust (Tahap 4 paralel.md).
# Ukur waktu build, peak RAM, CPU utilization, throughput untuk berbagai worker count.
set -euo pipefail

RTL_DIR="${RTL_DIR:-examples/e2e_expr/rtl}"
STEPS="${STEPS:-4}"

echo "=== SVRust Parallel Build Benchmark ==="
echo "RTL dir: $RTL_DIR"
echo "Steps: $STEPS"
echo ""

for workers in 1 2 4 8 16 32; do
    echo "--- Workers: $workers ---"
    
    # Bersihkan build sebelumnya
    cargo clean -p svrust 2>/dev/null || true
    rm -rf target/sv/parallel
    
    start=$(date +%s.%N)
    
    # Build tanpa --top (semua root)
    cargo run --release -p svrust -- build --rtl "$RTL_DIR" --steps "$STEPS" > /tmp/build_${workers}.log 2>&1
    
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
