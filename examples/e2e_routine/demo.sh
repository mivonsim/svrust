#!/bin/sh
# Tanggung jawab: demo e2e task/function SystemVerilog (LRM §13.3/§13.4).
set -e
cd "$(dirname "$0")/../.."

RTL_DIR=examples/e2e_routine/rtl
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
$SV check --rtl $RTL_DIR/tb_routine.sv --top tb_routine
echo "check OK"

echo "=== 2. function dan task berjalan ==="
keluaran=$($SV run --rtl $RTL_DIR/tb_routine.sv --top tb_routine --steps 2 2>/dev/null)

# LRM §13.4: jumlah(7, 5) = 12; dobel(7) = jumlah(7, 7) = 14;
# paritas = ^8'd7 = 1 karena ada tiga bit satu.
cek "fn1" "fn1 jumlah= 12 dobel= 14 paritas=1" "$(echo "$keluaran" | grep '^fn1')"
# LRM §13.3: argumen output ditulis langsung ke sinyal pemanggil.
cek "task1" "task1 kuadrat= 49" "$(echo "$keluaran" | grep '^task1')"
# Task tanpa argumen boleh dipanggil tanpa kurung.
cek "task2" "task2 set_nol y=  0 z=  0" "$(echo "$keluaran" | grep '^task2')"

echo "=== 3. bentuk function yang lain ==="
# `return expr;` setara assignment ke nama fungsi.
cek "fn2" "fn2 tambah_satu=  6" "$(echo "$keluaran" | grep '^fn2')"
# Badan `if/else` menjadi operator ternair; kedua cabang diuji.
cek "fn3" "fn3 ambang_bawah=00" "$(echo "$keluaran" | grep '^fn3')"
cek "fn4" "fn4 ambang_atas=ff" "$(echo "$keluaran" | grep '^fn4')"
# Keyword `automatic` diterima tanpa mengubah hasil.
cek "fn5" "fn5 otomatis=0a" "$(echo "$keluaran" | grep '^fn5')"
# Task yang sudah dipanggil tidak mengubah nilai di luarChecked-nya.
cek "fn6" "fn6 setelah_set_nol=  0" "$(echo "$keluaran" | grep '^fn6')"

echo "=== 4. deklarasi lokal badan task ==="
# Badannya di-inline dua kali, jadi variabel lokal harus dibedakan per
# pemanggilan. Tanpa itu, elaborasi gagal dengan `duplicate signal 't'`.
cek "lokal1" "lokal1 y=  8 z=  8" "$(echo "$keluaran" | grep '^lokal1')"

echo "=== 5. jumlah argumen salah ditolak ==="
# LRM §13.3: pemanggilan harus punya argumen sebanyak daftar formal.
gagal=$($SV check --rtl $RTL_DIR/tb_routine_err.sv --top tb_bad_arg 2>&1 || true)
cek_gagal "argumen" "mengharapkan 2 argumen, dapat 1" "$gagal"
# Pesan harus menyebut nama subroutine, bukan "undefined signal".
case "$gagal" in
  *jumlah*) ;;
  *)
    echo "GAGAL: pesan tak menyebut nama routine: $gagal"
    exit 1
    ;;
esac

echo "DEMO OK"