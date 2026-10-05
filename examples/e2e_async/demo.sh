#!/bin/sh
# Tanggung jawab: demo e2e async reset event gabungan dan keyword `always` polos.
set -e
cd "$(dirname "$0")/../.."
SV="cargo run -q -p cargo-sv -- sv"

# Cek satu baris keluaran persis seperti ekspektasi.
cek() {
  nama="$1"; ekspektasi="$2"; baris="$3"
  printf '  %-12s %s\n' "$nama" "$baris"
  if [ "$baris" != "$ekspektasi" ]; then
    echo "GAGAL: $nama seharusnya '$ekspektasi' tapi '$baris'"
    exit 1
  fi
}

echo "=== 1. check (parse + elaborate multi-event & always polos) ==="
$SV check --rtl examples/e2e_async/rtl/tb_async.sv
echo "check OK"

echo "=== 2. codegen: edge test event gabungan muncil di generated ==="
$SV codegen --rtl examples/e2e_async/rtl/tb_async.sv
GEN=target/sv/generated/tb_async.rs
if grep -q "edge_prev\[" "$GEN" && grep -q "sample_edge" "$GEN"; then
  echo "  edge_prev + sample_edge ada di generated"
else
  echo "GAGAL: codegen tidak menghasilkan deteksi edge event gabungan"
  exit 1
fi

echo "=== 3. run: reset asinkron, always polos, level-sensitive ==="
keluaran=$($SV run --rtl examples/e2e_async/rtl/tb_async.sv --steps 4)
# rst=1 saat t=0: q tetap 0 walau langkah clock berjalan.
cek "reset_aktif" "s0 rst=1 q=0 r=0 total=x" "$(echo "$keluaran" | grep '^s0 ')"
# Setelah lepas reset, langkah berikutnya mulai menghitung (1 langkah per #4).
cek "lepas_reset" "s1 rst=0 q=0 r=0 total=0" "$(echo "$keluaran" | grep '^s1 ')"
# Satu langkah kemudian: q dan r naik; `total` masih menunggu settle berikutnya.
cek "menghitung"  "s2 q=1 r=2 total=0"       "$(echo "$keluaran" | grep '^s2 ')"
# `always @(q or r)` membuktikan level-sensitive: total = q+r (1+2) pada
# langkah ini. rst dinaikkan lagi tanpa menunggu edge clock.
cek "level_always" "s3 rst=1 q=2 r=4 total=3" "$(echo "$keluaran" | grep '^s3 ')"
# Langkah terakhir memakai rst=1 => q bersih; r naik 2 tiap edge clock.
# LRM §20.3: `$finish` menghentikan simulasi, jadi segmen yang memanggilnya
# tidak lagi menjalankan `eval_seq`. Tiga edge clock terjadi (setelah s1, s2,
# dan s3) sehingga r = 3 * 2 = 6. Nilai lama `0a` (10) menghitung dua edge
# liar sesudah `$finish` — driver tidak boleh menjalankan proses setelah finish.
cek "q_akhir" "q = 00" "$(echo "$keluaran" | grep '^q =')"
cek "r_akhir" "r = 06" "$(echo "$keluaran" | grep '^r =')"

echo "=== 4. negative test: always_ff tanpa edge tetap ditolak ==="
cat > /tmp/sv_always_err.sv <<'SV'
module bad(input clk, input d, output logic q);
  always_ff @(clk) q <= d;
endmodule
SV
if $SV check --rtl /tmp/sv_always_err.sv 2>&1 | grep -q "posedge/negedge/edge"; then
  echo "  always_ff @(clk) -> error OK"
else
  echo "GAGAL: always_ff tanpa edge tidak ditolak"
  exit 1
fi

echo "DEMO OK"
