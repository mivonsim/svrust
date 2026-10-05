#!/bin/sh
# Tanggung jawab: demo e2e operator geser logis dan aritmetik (LRM §11.4.10).
# Ekspektasi diverifikasi silang dengan iverilog 12.0 (`-g2012`).
set -e
cd "$(dirname "$0")/../.."

RTL_DIR=examples/e2e_shift/rtl
SV="cargo run -q -p cargo-sv -- sv"

cek() {
  nama="$1"
  ekspektasi="$2"
  baris="$3"
  printf '  %-14s %s\n' "$nama" "$baris"
  if [ "$baris" != "$ekspektasi" ]; then
    echo "GAGAL: $nama seharusnya '$ekspektasi' tapi '$baris'"
    exit 1
  fi
}

echo "=== 1. check (parse + elaborate) ==="
$SV check --rtl $RTL_DIR/tb_shift.sv --top tb_shift
echo "check OK"

echo "=== 2. simulasi ==="
keluaran=$($SV run --rtl $RTL_DIR/tb_shift.sv --top tb_shift --steps 2 2>/dev/null)

# LRM §11.4.10: `>>` selalu logis, meski operand kirinya signed.
cek "shr" "shr s80=01000000 sF0=01111000 u80=01000000" "$(echo "$keluaran" | grep '^shr ')"
# `>>>` mengisi bit sign pada tipe signed, dan nol pada unsigned.
cek "sar" "sar s80=11000000 sF0=11111000 u80=01000000" "$(echo "$keluaran" | grep '^sar ')"
# `<<<` identik dengan `<<`.
cek "shl" "shl s80=00000000 sar_ke_kiri=00000000" "$(echo "$keluaran" | grep '^shl ')"
# Jumlah geser `x`/`z` menghasilkan unknown — dulu `to_u64()` menghitungnya
# sebagai 0 sehingga hasilnya angka pasti.
cek "xamt" "xamt shr=xxxxxxxx shl=xxxxxxxx" "$(echo "$keluaran" | grep '^xamt ')"
cek "zamt" "zamt shr=xxxxxxxx sar=xxxxxxxx" "$(echo "$keluaran" | grep '^zamt ')"
# Jumlah geser melewati lebar logis: geser logis mengosongkan seluruh bit,
# sedangkan geser aritmetik pada nilai negatif memenuhi semuanya dengan sign.
cek "lewat" "lewat sar=11111111 shr=00000000" "$(echo "$keluaran" | grep '^lewat ')"
cek "wide" "wide sar=-63 shr=4033" "$(echo "$keluaran" | grep '^wide ')"
cek "sar_assign" "sar_assign s80=11110000" "$(echo "$keluaran" | grep '^sar_assign s80')"
cek "sar_assign_u" "sar_assign u=00001111" "$(echo "$keluaran" | grep '^sar_assign u')"

echo "=== 3. constant folding sv-opt untuk geser ==="
# Nilai konstan `>>>` juga harus terlipat dengan bit sign yang benar;
# sebelumnya operator `>>>` belum ada sama sekali.
fold=$($SV run --rtl $RTL_DIR/tb_fold_shift.sv --top tb_fold_shift --steps 1 2>/dev/null)
cek "fold_sar_1" "fold sar_1=0f" "$(echo "$fold" | grep '^fold sar_1')"
cek "fold_shr_1" "fold shr_1=0f" "$(echo "$fold" | grep '^fold shr_1')"
# Literal `8'shF0` bertipe signed, jadi `>>>` mengisi bit sign (0xf8);
# literal `8'hF0` unsigned, jadi hasilnya sama dengan `>>` (0x0f).
cek "fold_sar_2" "fold sar_2=f8" "$(echo "$fold" | grep '^fold sar_2')"
cek "fold_sar_besar" "fold sar_besar=00" "$(echo "$fold" | grep '^fold sar_besar')"

echo "DEMO OK"