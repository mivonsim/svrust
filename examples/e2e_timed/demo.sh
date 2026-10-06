#!/bin/sh
# Tanggung jawab: demo e2e proses waktu `always #N` (LRM §9.2.1 + §11.2).
# Ekspektasi diverifikasi silang dengan iverilog 12.0.
set -e
cd "$(dirname "$0")/../.."

RTL_DIR=examples/e2e_timed/rtl
SV="cargo run -q -p cargo-sv -- sv"

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
$SV check --rtl $RTL_DIR/tb_jam.sv --top tb_jam
$SV check --rtl $RTL_DIR/tb_dua_jam.sv --top tb_dua_jam
echo "check OK"

# LRM §11.2: proses `always #5` menggeser jam simulasi. Versi lama memetakan
# `always` tanpa `@` ke `always_comb`, sehingga penundaannya dieksekusi pada
# SETIAP langkah driver — jam bergerak jauh sebelum stimulus, `@(posedge clk)`
# tidak pernah terpenuhi, dan hanya baris pertama testbench yang tercetak.
echo "=== 2. jam simulasi bergerak dan edge clock nyata ==="
# iverilog: jum=t=52 / jum2=t=62 / cnt=6
jam=$($SV run --rtl $RTL_DIR/tb_jam.sv --top tb_jam --steps 40 2>/dev/null)
cek "t52" "jum=t=52" "$(echo "$jam" | grep '^jum=')"
cek "t62" "jum2=t=62" "$(echo "$jam" | grep '^jum2=')"
# `cnt` bertambah satu kali per posedge clk. Jam wallclock yang salah (per
# langkah driver) memberi jumlah yang jauh lebih besar.
cek "cnt" "cnt=6" "$(echo "$jam" | grep '^cnt=')"

# LRM §4.4: dua proses waktu punya jam bangun sendiri. Versi lama menjalankan
# tiap proses tepat satu kali per langkah driver, sehingga proses `#3` baru
# berayun 6 kali pada t=30 padahal harus 10 — dan penghitungnya salah diam-diam.
echo "=== 3. dua proses waktu dengan periode berbeda ==="
# iverilog: na=3 nb=5 t=30
dua=$($SV run --rtl $RTL_DIR/tb_dua_jam.sv --top tb_dua_jam --steps 40 2>/dev/null)
cek "dua_jam" "na=3 nb=5 t=30" "$(echo "$dua" | grep '^na=')"

echo "DEMO OK"
