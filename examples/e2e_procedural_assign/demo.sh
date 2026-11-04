#!/bin/sh
# Tanggung jawab: demo penetapans berkelanjutan prosedural (LRM §14).
set -e
cd "$(dirname "$0")/../.."

RTL_DIR=examples/e2e_procedural_assign/rtl
SV="cargo run -q -p svrust --"

cek() {
  nama="$1"
  ekspektasi="$2"
  baris="$3"
  printf '  %-22s %s\n' "$nama" "$baris"
  if [ "$baris" != "$ekspektasi" ]; then
    echo "GAGAL: $nama seharusnya '$ekspektasi' tapi '$baris'"
    exit 1
  fi
}

cek_gagal() {
  nama="$1"
  pola="$2"
  keluaran="$3"
  printf '  %-22s %s\n' "$nama" "$keluaran"
  case "$keluaran" in
  *"$pola"*) ;;
  *)
    echo "GAGAL: $nama seharusnya mengandung '$pola'"
    exit 1
    ;;
  esac
}

echo "=== 1. check (parse + elaborate) ==="
$SV check --rtl $RTL_DIR/tb_procedural_assign.sv --top tb_procedural_assign
echo "check OK"

echo "=== 2. override prosedural menangani penulangan biasa ==="
keluaran=$($SV run --rtl $RTL_DIR/tb_procedural_assign.sv --top tb_procedural_assign --steps 4 2>/dev/null)
cek "x_awal" "x_awal=20" "$(echo "$keluaran" | grep '^x_awal')"
cek "x_override" "x_override=aa" "$(echo "$keluaran" | grep '^x_override')"
cek "x_setelah_deassign" "x_setelah_deassign=55" "$(echo "$keluaran" | grep '^x_setelah_deassign')"

echo "=== 3. deassign tanpa override sebelumnya tidak rusak ==="
cat > /tmp/sv_deassign_murni.sv <<'SV'
module tb;
  logic [7:0] x;
  initial begin
    deassign x;
    x = 8'h3C;
    $display("x=%h", x);
    $finish;
  end
endmodule
SV
keluaran=$($SV run --rtl /tmp/sv_deassign_murni.sv --top tb --steps 2 2>/dev/null)
cek "deassign_murni" "x=3c" "$(echo "$keluaran" | grep '^x=')"

echo "DEMO OK"
