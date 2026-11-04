#!/bin/sh
# Tanggung jawab: demo e2e SVA concurrent assertion (LRM §20/§21).
# assert property (@(posedge clk) cond) — kondisi dicek pada setiap edge clock.
set -e
cd "$(dirname "$0")/../.."

RTL_DIR=examples/e2e_assert/rtl
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
$SV check --rtl $RTL_DIR/tb_assert.sv --top tb_assert
echo "check OK"

echo "=== 2. simulasi concurrent assertion (harus gagal, cnt=200 > 127) ==="
keluaran=$($SV run --rtl $RTL_DIR/tb_assert.sv --top tb_assert --steps 20 2>&1 || true)
echo "$keluaran"
case "$keluaran" in
  *"assertion failed"*) ;;
  *)
    echo "GAGAL: concurrent assertion seharusnya gagal (cnt=200 > 127)"
    exit 1
    ;;
esac

echo "=== 3. assertion yang lulus ==="
cat > /tmp/sv_assert_pass.sv <<'SV'
module tb_assert_pass;
  logic clk;
  logic [7:0] cnt;
  always #5 clk = ~clk;
  initial begin
    clk = 0;
    cnt = 8'd42;
    #20;
    $finish;
  end
  assert property (@(posedge clk) cnt <= 8'd127);
endmodule
SV
keluaran=$($SV run --rtl /tmp/sv_assert_pass.sv --top tb_assert_pass --steps 20 2>&1 || true)
echo "$keluaran"
case "$keluaran" in
  *"assertion failed"*)
    echo "GAGAL: assertion seharusnya lulus (cnt=42 <= 127)"
    exit 1
    ;;
esac

echo "DEMO OK"
