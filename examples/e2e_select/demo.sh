#!/bin/sh
# Tanggung jawab: demo e2e bit-select/part-select (LRM §7.8).
set -e
cd "$(dirname "$0")/../.."

RTL_DIR=examples/e2e_select/rtl
SV="cargo run -q -p cargo-sv -- sv"

# Cek bahwa satu baris keluaran bernilai persis seperti yang diharapkan.
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

# Cek bahwa pesan error yang diharapkan muncul pada keluaran CLI.
cek_gagal() {
  nama="$1"
  pola="$2"
  keluaran="$3"
  printf '  %-12s %s\n' "$nama" "$keluaran"
  case "$keluaran" in
  *"$pola"*) ;;
  *)
    echo "GAGAL: $nama sequestra '$pola'"
    exit 1
    ;;
  esac
}

echo "=== 1. check (parse + elaborate) ==="
$SV check --rtl $RTL_DIR/tb_select.sv --top tb_select
echo "check OK"

echo "=== 2. bit-select, part-select, dan seleksi bertingkat ==="
# a = 8'hA5 = 1010_0101
keluaran=$($SV run --rtl $RTL_DIR/tb_select.sv --top tb_select --steps 2 2>/dev/null)

# LRM §7.8.1: bit-select mengambil satu bit.
cek "bit" "bit a[0]=1" "$(echo "$keluaran" | grep '^bit ')"
# Part-select bawah 4 bit dari 0xA5 adalah 0x5.
cek "part_bawah" "part a[3:0]=5" "$(echo "$keluaran" | grep '^part a\[3:0\]')"
# Part-select atas 4 bit adalah 0xA.
cek "part_atas" "part a[7:4]=a" "$(echo "$keluaran" | grep '^part a\[7:4\]')"
# Part-select pada vektor yang lebih lebar: `w = {24'b0, a[7:4]}` = 0x0000000A.
cek "part_w" "part w[7:0]=0a" "$(echo "$keluaran" | grep '^part w')"
# LRM §7.8: seleksi bertingkat `a[3:0][1]` — bit 1 dari 0x5 adalah 0.
cek "nested" "nested a[3:0][1]=0" "$(echo "$keluaran" | grep '^nested')"

echo "=== 3. array unpacked dan indeks dinamis ==="
cek "array" "array mem[2]=3c" "$(echo "$keluaran" | grep '^array')"
# LRM §7.8: `a[idx]` pada sinyal PACKED adalah bit-select, jadi hasilnya satu
# bit. a[2] dari 0xA5 (1010_0101) adalah 1. Dulu helper menggeser
# `idx * 8` bit (semantik array unpacked) sehingga hasilnya selalu 00.
cek "dinamis" "dinamis a[idx]=01" "$(echo "$keluaran" | grep '^dinamis')"

echo "=== 3b. indeks dinamis pada array unpacked tetap elemen penuh ==="
keluaran2=$($SV run --rtl $RTL_DIR/tb_select_unpacked.sv --top tb_select_unpacked --steps 2 2>/dev/null)
# LRM §7.8: `mem[k]` membaca satu elemen utuh, bukan satu bit.
cek "unpacked" "unpacked mem[k]=3c" "$(echo "$keluaran2" | grep '^unpacked')"
cek "packed" "packed flat[k]=1" "$(echo "$keluaran2" | grep '^packed')"

echo "=== 3c. sinyal lebih dari 64 bit dan array besar ==="
# LRM §10.10.1 + §7.8: penulisan pada posisi di atas bit 63 harus mendarat di
# posisi yang benar. Dulu masker bertipe u64, jadi irisan di atas bit 63 panic
# di debug dan wrap diam-diam di release — tulisan mendarat di bit 0.
keluaran3=$($SV run --rtl $RTL_DIR/tb_select_wide.sv --top tb_select_wide --steps 2 2>/dev/null)
cek "mem0" "mem0=11" "$(echo "$keluaran3" | grep '^mem0')"
cek "mem15" "mem15=bb" "$(echo "$keluaran3" | grep '^mem15')"
cek "wide100" "wide100=1" "$(echo "$keluaran3" | grep '^wide100')"
cek "widetop" "widetop=f1" "$(echo "$keluaran3" | grep '^widetop=')"
cek "widetop_lo" "widetop_lo=31" "$(echo "$keluaran3" | grep '^widetop_lo')"
# LRM §5.7.1: digit `x`/`z` pada LHS part-select harus tersimpan. Dulu
# `write_masked` memakai `to_u64()` yang membuangnya.
cek "xz_b" "xz_b=1x0z0000" "$(echo "$keluaran3" | grep '^xz_b')"
cek "xz_s" "xz_s=1x0z" "$(echo "$keluaran3" | grep '^xz_s')"

echo "=== 4. seleksi di luar jangkauan ditolak ==="
# LRM §7.8: bit di luar lebar operand menghasilkan X.indices konstan bisa
# diperiksa saat elaborasi, jadi ditolak sekalian daripada menghasilkan 0.
gagal=$($SV check --rtl $RTL_DIR/tb_select_oob.sv --top tb_select_oob 2>&1 || true)
cek_gagal "oob" "di luar lebar" "$gagal"

echo "=== 5. range select pada array unpacked ditolak eksplisit ==="
# LRM §7.8.2: `mem[3:1]` membaca tiga elemen. Jalur ini belum ada; tanpa
# penjaga hasilnya hanya elemen `msb` — nilai salah tanpa pesan.
gagal=$($SV check --rtl $RTL_DIR/tb_select_unpacked_range.sv --top tb_select_unpacked_range 2>&1 || true)
cek_gagal "unpacked" "range select" "$gagal"

echo "=== 6. indeks bertingkat pada LHS ditolak ==="
# Dulu `y[3:0][1] = 1'b1` Accepted lalu token `[1]` dibuang diam-diam.
gagal=$($SV check --rtl $RTL_DIR/tb_select_lhs_nested.sv --top tb_select_lhs_nested 2>&1 || true)
cek_gagal "lhs_nested" "indeks bertingkat" "$gagal"

echo "=== 7. indeks genvar berpola aritmetika ditolak ==="
# `r[i - 1]` di dalam generate dulu dipakai penuh padahal `- 1` dibuang,
# sehingga tulisannya jatuh pada bit yang salah.
gagal=$($SV check --rtl $RTL_DIR/tb_select_genvar_arit.sv --top tb_select_genvar_arit 2>&1 || true)
cek_gagal "genvar" "satu token" "$gagal"

echo "DEMO OK"