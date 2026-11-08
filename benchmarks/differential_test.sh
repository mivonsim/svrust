#!/bin/bash
# Tanggung jawab: skrip differential testing SVRust vs Icarus vs Verilator (Tahap 5 paralel.md).
# Bandingkan output print untuk setiap file .sv.
# Klasifikasi: COCOK / BUG_SVRUST / ACAKAN_Iverilog / TIGA_BEDA / TIDAK_DIDUKUNG / TIDAK_CUKUP
set -euo pipefail

RTL_DIR="${RTL_DIR:-examples/e2e_expr/rtl}"
STEPS="${STEPS:-4}"

PASS=0
FAIL=0
SKIP=0

echo "=============================================="
echo "  SVRust Differential Testing (Tahap 5)"
echo "=============================================="
echo "RTL dir: $RTL_DIR"
echo "Reference tools: iverilog, verilator"
echo ""

for svfile in $(find "$RTL_DIR" -name "*.sv" -type f | sort); do
    name=$(basename "$svfile" .sv)

    # Check if design has output ports (needs output to compare)
    if ! grep -q "output" "$svfile"; then
        echo "[SKIP] $name - tidak ada output port"
        SKIP=$((SKIP + 1))
        continue
    fi

    echo "=== $name ==="

    # --- SVRust ---
    svrust_out=$(cargo run --release -p svrust -- run --rtl "$svfile" --top "$name" --steps "$STEPS" 2>/dev/null | grep -E "^[a-z]" || true)

    # --- Icarus Verilog ---
    iver_out=""
    if iverilog -o /tmp/tb_test.vvp "$svfile" 2>/dev/null; then
        iver_out=$(vvp /tmp/tb_test.vvp 2>/dev/null || true)
    else
        iver_out="TIDAK_DIDUKUNG"
    fi

    # --- Verilator ---
    vl_out=""
    if verilator --binary -o /tmp/tb_test_vl "$svfile" 2>/dev/null; then
        vl_out=$(/tmp/tb_test_vl 2>/dev/null || true)
    else
        vl_out="TIDAK_DIDUKUNG"
    fi

    # --- Klasifikasi ---
    if [ "$iver_out" = "TIDAK_DIDUKUNG" ] && [ "$vl_out" = "TIDAK_DIDUKUNG" ]; then
        echo "  [TIDAK_CUKUP] kedua acuan tidak bisa kompile"
        SKIP=$((SKIP + 1))
    elif [ "$svrust_out" = "$iver_out" ] && [ "$svrust_out" = "$vl_out" ]; then
        echo "  [COCOK] semua setuju"
        PASS=$((PASS + 1))
    elif [ "$svrust_out" = "$iver_out" ] && [ "$svrust_out" != "$vl_out" ]; then
        echo "  [ACAKAN_Verilator] SVRust == Icarus, Verilator berbeda"
        FAIL=$((FAIL + 1))
    elif [ "$svrust_out" = "$vl_out" ] && [ "$svrust_out" != "$iver_out" ]; then
        echo "  [ACAKAN_Iverilog] SVRust == Verilator, Icarus berbeda"
        FAIL=$((FAIL + 1))
    elif [ "$iver_out" = "$vl_out" ] && [ "$svrust_out" != "$iver_out" ]; then
        echo "  [BUG_SVRUST] Acayan setuju, SVRust berbeda"
        FAIL=$((FAIL + 1))
    else
        echo "  [TIGA_BEDA] ketiganya berbeda"
        FAIL=$((FAIL + 1))
    fi

    echo "  SVRust:   $(echo "$svrust_out" | head -1)"
    echo "  Icarus:   $(echo "$iver_out" | head -1)"
    echo "  Verilator: $(echo "$vl_out" | head -1)"
    echo ""
done

echo "=============================================="
echo "  Hasil: COCOK=$PASS BEDA=$FAIL SKIP=$SKIP"
echo "=============================================="
