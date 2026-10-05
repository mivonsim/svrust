#!/bin/sh
# Tanggung jawab: demo e2e makro function-like, default arg, dan bersarang.
set -e
cd "$(dirname "$0")/../.."
SV="cargo run -q -p cargo-sv -- sv"

# Cek satu baris keluaran persis seperti ekspektasi.
cek() {
  nama="$1"; ekspektasi="$2"; baris="$3"
  printf '  %-10s %s\n' "$nama" "$baris"
  if [ "$baris" != "$ekspektasi" ]; then
    echo "GAGAL: $nama seharusnya '$ekspektasi' tapi '$baris'"
    exit 1
  fi
}

echo "=== 1. check (preprocess + parse + elaborate) ==="
$SV check --rtl examples/e2e_makro/rtl/makrotop.sv
echo "check OK"

echo "=== 2. codegen: makro terekspansi penuh di generated Rust ==="
$SV codegen --rtl examples/e2e_makro/rtl/makrotop.sv
BT=$(grep -cE '`(WIDTH|ADD|CLAMP|DOUBLE|HOAX|ifdef|define|include|timescale)' target/sv/generated/makrotop.rs || true)
echo "  sisa backtick makro di generated: $BT (harus 0)"
if [ "$BT" -ne 0 ]; then
  echo "GAGAL: masih ada makro yang belum terekspansi"
  exit 1
fi

echo "=== 3. run: nilai output dicek persis ==="
keluaran=$($SV run --rtl examples/e2e_makro/rtl/makrotop.sv --steps 8)
cek "y" "y = 0f" "$(echo "$keluaran" | grep '^y =')"
cek "z" "z = 08" "$(echo "$keluaran" | grep '^z =')"
cek "w" "w = 2a" "$(echo "$keluaran" | grep '^w =')"

echo "=== 4. negative test: argumen salah tetap ditolak ==="
cat > /tmp/sv_makro_err.sv <<'SV'
`define ADD(a, b) a + b
module bad(output [7:0] y);
assign y = `ADD(8'd1, 8'd2, 8'd3);
endmodule
SV
if $SV check --rtl /tmp/sv_makro_err.sv 2>&1 | grep -q "terlalu banyak argumen"; then
  echo "  terlalu banyak argumen -> error OK"
else
  echo "GAGAL: kelebihan argumen tidak ditolak"
  exit 1
fi

echo "DEMO OK"
