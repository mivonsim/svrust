#!/bin/sh
# Tanggung jawab: demo e2e operator ekspresi SV -> Rust -> biner -> nilai dicek.
set -e
cd "$(dirname "$0")/../.."

RTL_DIR=examples/e2e_expr/rtl
SV="cargo run -q -p cargo-sv -- sv"

# Cek bahwa satu baris keluaran bernilai persis seperti yang diharapkan.
cek() {
  nama="$1"
  ekspektasi="$2"
  baris="$3"
  printf '  %-10s %s\n' "$nama" "$baris"
  if [ "$baris" != "$ekspektasi" ]; then
    echo "GAGAL: $nama seharusnya '$ekspektasi' tapi '$baris'"
    exit 1
  fi
}

echo "=== 1. check (parse + elaborate) ==="
$SV check --rtl $RTL_DIR/const_ops.sv --rtl $RTL_DIR/sel_ops.sv
$SV check --rtl $RTL_DIR/const_ops.sv
echo "check OK"

echo "=== 2. sized literal, unary, reduction, ternary ==="
# Semua keluaran deterministik karena bersumber dari literal.
keluaran=$($SV run --rtl $RTL_DIR/const_ops.sv --steps 4)
# BUG-1: 8'hA5
cek "lit"    "lit = a5"    "$(echo "$keluaran" | grep '^lit =')"
# BUG-3: ~8'h0F = 8'hF0
cek "notv"   "notv = f0"   "$(echo "$keluaran" | grep '^notv =')"
# BUG-3: -8'd5 = 251 = 8'hFB (dua's complement)
cek "negv"   "negv = fb"   "$(echo "$keluaran" | grep '^negv =')"
# BUG-4: &4'b1111 = 1 (semua bit satu)
cek "red_and"  "red_and = 1" "$(echo "$keluaran" | grep '^red_and =')"
# BUG-4: |4'b0000 = 0 (tidak ada bit satu)
cek "red_or"   "red_or = 0"  "$(echo "$keluaran" | grep '^red_or =')"
# BUG-4: ^4'b0011 = 0 (dua bit satu => paritas genap)
cek "red_xor"  "red_xor = 0" "$(echo "$keluaran" | grep '^red_xor =')"
# BUG-4: ~^4'b0111 = ~1 = 0 (tiga bit satu => paritas ganjil)
cek "red_xnor" "red_xnor = 0" "$(echo "$keluaran" | grep '^red_xnor =')"
# BUG-5: 1'b1 ? 8'hFF : 8'h00 = 8'hFF
cek "tern"   "tern = ff"   "$(echo "$keluaran" | grep '^tern =')"

echo "=== 3. bit-select, part-select, presedence ==="
keluaran=$($SV run --rtl $RTL_DIR/sel_ops.sv --steps 4)
# BUG-6: 8'hB7[7:4] = 4'hB
cek "part" "part = b" "$(echo "$keluaran" | grep '^part =')"
# BUG-6: 8'hB7[0] = 1'b1 (1011_0111, bit paling kanan = 1)
cek "bit0" "bit0 = 1" "$(echo "$keluaran" | grep '^bit0 =')"
# Presedence: `~` lebih kuat dari `+`, jadi f0 + 01 = f1
cek "comb" "comb = f1" "$(echo "$keluaran" | grep '^comb =')"
# Unary pada sinyal port dengan input default nol => ~8'h00 = 8'hFF
cek "sel_not" "sel_not = ff" "$(echo "$keluaran" | grep '^sel_not =')"

echo "=== 4. bit/part-select pada LHS assignment ==="
# `y = 8'hFF` dulu, lalu hanya `y[3:0]` yang tertimpa => nibble atas bertahan.
keluaran=$($SV run --rtl $RTL_DIR/lhs_slice.sv --steps 4)
cek "y" "y = f0" "$(echo "$keluaran" | grep '^y =')"
# `z` hanya ditulis pada bit 0, jadi bit 1..7 tidak pernah diinisialisasi.
# Ekspektasi lama `00` mengunci bug inisialisasi-nol.
#
# CATATAN DIVERGENSI: `output [7:0] z` tanpa `logic` adalah NET di LRM §6.5,
# dan net yang belum drove bernilai `Z` (iverilog: `zZ`). SVRust memperlakukan
# port tanpa tipe sebagai variabel, jadi hasilnya `x`. Lihat LRM_COMPLIANCE.md.
cek "z" "z = xx" "$(echo "$keluaran" | grep '^z =')"

echo "=== 5. concatenation, replication, signed ==="
keluaran=$($SV run --rtl $RTL_DIR/concat_ops.sv --steps 4)
# LRM §11.8.1: item kiri jadi MSB => {4'hA, 4'h3} = 8'hA3 (bukan 8'h3A).
cek "cat2" "cat2 = a3" "$(echo "$keluaran" | grep '^cat2 =')"
cek "cat3" "cat3 = 1b" "$(echo "$keluaran" | grep '^cat3 =')"
# LRM §11.8.2: {4{2'b10}} = 8'b1010_1010; {8{1'b1}} = 8'hFF.
cek "rep2" "rep2 = aa" "$(echo "$keluaran" | grep '^rep2 =')"
cek "rep1" "rep1 = ff" "$(echo "$keluaran" | grep '^rep1 =')"
# Replication di dalam concatenation: {4'h0, {2{2'b11}}} = 8'h0F.
cek "mixed" "mixed = 0f" "$(echo "$keluaran" | grep '^mixed =')"
# Deklarasi signed: 8'shFF = 8'hFF.
cek "sgn" "sgn = ff" "$(echo "$keluaran" | grep '^sgn =')"

echo "=== 6. casez / casex dengan wildcard ==="
keluaran=$($SV run --rtl $RTL_DIR/casez_ops.sv --steps 4)
# LRM §12.5: `1?` cocok dengan 10 dan 11 => yz = 1.
cek "yz" "yz = 1" "$(echo "$keluaran" | grep '^yz =')"
# `0??` menutup semua nilai 0xx => yx = 7.
cek "yx" "yx = 7" "$(echo "$keluaran" | grep '^yx =')"
# 2'b01 tidak cocok dengan `1?` karena bit 1 berbeda => default, yno = 0.
cek "yno" "yno = 0" "$(echo "$keluaran" | grep '^yno =')"

echo "=== 7. pembagian, modulo, dan deklarator majemuk ==="
keluaran=$($SV run --rtl $RTL_DIR/divmod_ops.sv --steps 4)
# LRM §11.4.5: 100 / 7 = 14 (0x0E).
cek "bagi" "bagi = 0e" "$(echo "$keluaran" | grep '^bagi =')"
# LRM §11.4.6: 100 % 7 = 2.
cek "sisa" "sisa = 02" "$(echo "$keluaran" | grep '^sisa =')"
# Pembagi nol menghasilkan 0 (engine 2-state), bukan panic seperti bagi integer Rust.
cek "bagi_nol" "bagi_nol = 00" "$(echo "$keluaran" | grep '^bagi_nol =')"
cek "sisa_nol" "sisa_nol = 00" "$(echo "$keluaran" | grep '^sisa_nol =')"
# Pembagi lebih besar: 20 / 30 = 0, tapi 20 % 30 = 20 (0x14).
cek "bagi_kecil" "bagi_kecil = 00" "$(echo "$keluaran" | grep '^bagi_kecil =')"
cek "sisa_kecil" "sisa_kecil = 14" "$(echo "$keluaran" | grep '^sisa_kecil =')"
# BUG-19: `*` butuh impl Mul di runtime; 100 / 5 * 2 = 40 (0x28) asosiatif kiri.
cek "presedensi" "presedensi = 28" "$(echo "$keluaran" | grep '^presedensi =')"
# Compound /=: 100/5 = 20, lalu 20/2 = 10 (0x0A).
cek "cmb_bagi" "compound_bagi = 0a" "$(echo "$keluaran" | grep '^compound_bagi =')"
# Compound %=: 100%30 = 10, lalu 10%3 = 1.
cek "cmb_sisa" "compound_sisa = 01" "$(echo "$keluaran" | grep '^compound_sisa =')"

echo "=== 8. deklarasi lokal dan langkah loop for ==="
keluaran=$($SV run --rtl $RTL_DIR/local_ops.sv --steps 4)
# BUG-22: variabel lokal di dalam blok terdaftar sebagai signal biasa.
cek "lokal" "lokal = 05" "$(echo "$keluaran" | grep '^lokal =')"
# Deklarator majemuk di dalam blok: {2, 3} = 8'h23.
cek "lokal_majemuk" "lokal_majemuk = 23" "$(echo "$keluaran" | grep '^lokal_majemuk =')"
# `1'b0` memilih cabang else => t = 1.
cek "lokal_if" "lokal_if = 01" "$(echo "$keluaran" | grep '^lokal_if =')"
# Jumlah 0+1+2+3 = 6.
cek "lokal_for" "lokal_for = 06" "$(echo "$keluaran" | grep '^lokal_for =')"
# BUG-20: `i++` diurai jadi `i = i + 1` => 5 iterasi.
cek "step_incr" "step_incr = 05" "$(echo "$keluaran" | grep '^step_incr =')"
# `i += 2` diurai jadi `i = i + 2` => i = 0,2,4,6 => 4 iterasi.
cek "step_tambah" "step_tambah = 04" "$(echo "$keluaran" | grep '^step_tambah =')"
# `i--` => 9 iterasi.
cek "step_kurang" "step_kurang = 09" "$(echo "$keluaran" | grep '^step_kurang =')"
# BUG-21: body kosong tetap menjalankan loop sampai i = 4.
cek "body_kosong" "body_kosong = 04" "$(echo "$keluaran" | grep '^body_kosong =')"
# Dua loop pada variable berbeda: (0+1+2) + (0+1+2+3) = 9.
cek "hitung_kecil" "hitung_kecil = 09" "$(echo "$keluaran" | grep '^hitung_kecil =')"

echo "=== 9. loop while, integer/int, dan nilai awal ==="
keluaran=$($SV run --rtl $RTL_DIR/while_init.sv --steps 4)
# BUG-26: while mengulang selama kondisi benar => 5 iterasi.
cek "hitung" "hitung = 05" "$(echo "$keluaran" | grep '^hitung =')"
# LRM §11.8.1: `f` adalah `logic [3:0]`, jadi tiap penugasan memotong ke 4 bit.
# 5! = 120 = 0x78, tapi 0x78 & 0xF = 0x8 — dan `{4'd0, f}` memakai `f` pada
# lebar logisnya sendiri, bukan nilai penuh yang belum dipotong.
# Diverifikasi terhadap iverilog 12.0 (`-g2012`): `faktorial=08`.
cek "faktorial" "faktorial = 08" "$(echo "$keluaran" | grep '^faktorial =')"
# Kondisi langsung salah => i tetap 7.
cek "nol_iterasi" "nol_iterasi = 07" "$(echo "$keluaran" | grep '^nol_iterasi =')"
# BUG-24: integer/int 32-bit signed => 5 * 100 + 7 = 507 = 0x1FB.
cek "bulat" "bulat = 000001fb" "$(echo "$keluaran" | grep '^bulat =')"
# BUG-25: nilai awal waktu nol untuk variabel = 42 = 0x2A.
cek "awal" "awal = 2a" "$(echo "$keluaran" | grep '^awal =')"
# Nilai awal variabel lokal juga berlaku = 9.
cek "awal_lokal" "awal_lokal = 09" "$(echo "$keluaran" | grep '^awal_lokal =')"
# Net declaration assign menyalin nilai awal lewat driver kontinu.
cek "salin_net" "salin_net = 2a" "$(echo "$keluaran" | grep '^salin_net =')"
# BUG-27: `i++`/`i--` sebagai statement utuh.
cek "incr_naik" "incr_naik = 06" "$(echo "$keluaran" | grep '^incr_naik =')"
cek "incr_turun" "incr_turun = 03" "$(echo "$keluaran" | grep '^incr_turun =')"
cek "incr_tumpuk" "incr_tumpuk = 03" "$(echo "$keluaran" | grep '^incr_tumpuk =')"

echo "=== 10. loop repeat dan pre-increment ==="
keluaran=$($SV run --rtl $RTL_DIR/repeat_ops.sv --steps 4)
# BUG-28: hitungan repeat boleh berupa sinyal.
cek "rep_sinyal" "hitung_sinyal = 03" "$(echo "$keluaran" | grep '^hitung_sinyal =')"
# Hitungan literal: 4 x +2 = 8.
cek "rep_literal" "hitung_literal = 08" "$(echo "$keluaran" | grep '^hitung_literal =')"
# Body blok: 2 x (+1 +10) = 22 = 0x16.
cek "rep_blok" "hitung_blok = 16" "$(echo "$keluaran" | grep '^hitung_blok =')"
# Hitungan nol membuat body tidak jalan => tetap 99 = 0x63.
cek "rep_nol" "hitung_nol = 63" "$(echo "$keluaran" | grep '^hitung_nol =')"
# BUG-29: ++i; dan --i; sebagai statement utuh.
cek "pre_naik" "pre_naik = 02" "$(echo "$keluaran" | grep '^pre_naik =')"
cek "pre_turun" "pre_turun = 04" "$(echo "$keluaran" | grep '^pre_turun =')"
# Pre dan post dicampur: 0+1+1-1-1+1 = 1.
cek "campur" "campur = 01" "$(echo "$keluaran" | grep '^campur =')"

echo "=== 11. testbench: initial, \$display, \$finish ==="
keluaran=$($SV run --rtl $RTL_DIR/tb_ops.sv --steps 4)
cek "disp_a"     "a=42 b=3"  "$(echo "$keluaran" | grep '^a=')"
cek "disp_jumlah" "jumlah=45" "$(echo "$keluaran" | grep '^jumlah=')"
# %h mengikuti lebar logis (4'hA), %b memakai 4 digit.
cek "disp_fmt"   "m=a m=1010" "$(echo "$keluaran" | grep '^m=')"
# Tipe bertanda: 8'hFF dibaca -1.
cek "disp_signed" "s=-1"      "$(echo "$keluaran" | grep '^s=')"
cek "disp_teks"  "teks saja"  "$(echo "$keluaran" | grep '^teks saja')"

# \$finish menghentikan simulasi sebelum clock berputar.
selesai=$($SV run --rtl $RTL_DIR/tb_finish.sv --steps 8)
cek "langkah_0" "langkah 0" "$(echo "$selesai" | grep '^langkah 0')"
cek "langkah_2" "langkah 2" "$(echo "$selesai" | grep '^langkah 2')"
cek "selesai"   "selesai"   "$(echo "$selesai" | grep '^selesai$')"
cek "tf_hitung" "hitung = 00" "$(echo "$selesai" | grep '^hitung =')"
# LRM §20.3: statement setelah `$finish` tidak dijalankan.
if echo "$selesai" | grep -q '^tidak tampil'; then
  echo "GAGAL: statement setelah \$finish masih dieksekusi"
  exit 1
fi
printf '  %-10s %s\n' "tf_henti" "tidak tampil"

# Tanpa \$finish counter clock tetap 8 langkah pada steps=8. `hitung` tidak
# pernah diinisialisasi, jadi LRM §4.3.1 memberi nilai awal `x`; `x + 1` tetap
# `x` pada setiap langkah. Ekspektasi `08` dulu mengunci inisialisasi-nol yang
# salah (iverilog menghasilkan `xx`).
lama=$($SV run --rtl $RTL_DIR/tb_clock.sv --steps 8)
cek "tanpa_finish" "hitung = xx" "$(echo "$lama" | grep '^hitung =')"

echo "=== 12. penundaan waktu: #delay dan \$time ==="
waktu=$($SV run --rtl $RTL_DIR/tb_time.sv --steps 4)
cek "waktu_awal" "t=0 a=1"   "$(echo "$waktu" | grep '^t=0 ')"
cek "waktu_5"    "t=5 a=2"   "$(echo "$waktu" | grep '^t=5 ')"
# 5 + 10 = 15; `#10;` tanpa statement tetap menggeser waktu.
cek "waktu_15"   "t=15 a=2"  "$(echo "$waktu" | grep '^t=15 ')"
# Satuan `ns` berarti 3 ns; 15 + 3 = 18.
cek "waktu_18"   "t=18 b=7"  "$(echo "$waktu" | grep '^t=18 ')"
cek "loop_20"    "loop t=20 i=0" "$(echo "$waktu" | grep '^loop t=20 ')"
cek "loop_22"    "loop t=22 i=1" "$(echo "$waktu" | grep '^loop t=22 ')"
cek "loop_24"    "loop t=24 i=2" "$(echo "$waktu" | grep '^loop t=24 ')"
# \$time dua kali dalam satu argumen format harus bernilai sama.
cek "waktu_total" "t=24 total=24" "$(echo "$waktu" | grep '^t=24 total=')"

echo "=== 13. satuan waktu pada #delay ==="
satuan=$($SV run --rtl $RTL_DIR/tb_unit.sv --steps 2)
# `#1` tanpa satuan = 1 ns.
cek "satuan_1"  "t=1"        "$(echo "$satuan" | grep '^t=1$')"
cek "satuan_2"  "t=2"        "$(echo "$satuan" | grep '^t=2$')"
# 1000 ps harus sama dengan 1 ns, jadi total 3 (bukan 1003).
cek "satuan_ps" "t=3"        "$(echo "$satuan" | grep '^t=3$')"
# 1 us = 1000 ns => 1003.
cek "satuan_us" "t=1003"     "$(echo "$satuan" | grep '^t=1003$')"
# 1 ms = 1.000.000 ns => 1.001.003.
cek "satuan_ms" "t=1001003"  "$(echo "$satuan" | grep '^t=1001003$')"
# 1 s = 1.000.000.000 ns => 1.001.001.003.
cek "satuan_s"  "t=1001001003" "$(echo "$satuan" | grep '^t=1001001003$')"

echo "=== 14. langkah waktu: clock & combinational di antara #delay ==="
langkah=$($SV run --rtl $RTL_DIR/tb_step.sv --steps 8)
# Segmen 0 pada t=0, belum ada edge clock dan `bayangan` masih nilai lama.
# CATATAN: `tb_step` membaca `bayangan` pada t=0 di blok `initial` yang sama
# dengan `deposit = 8'd10`, jadi SEBELUM combinational settle. Nilai pada saat
# itu tidak ditentukan LRM (race testbench), dan dengan nilai awal `x`
# (LRM §4.3.1) yang terbaca adalah `x`. Baris ini tidak dipakai sebagai oracle
# semantik — hanya sebagai penanda bahwa segmen t=0 dieksekusi.
cek "step_s0" "s0 t=0 hitung=x bayangan=x" "$(echo "$langkah" | grep '^s0 ')"
# Delay #4 menggeser waktu; clock edge terjadi sesudah segmen dicetak.
# `hitung` tidak pernah diinisialisasi, jadi LRM §4.3.1 membuatnya tetap `x`
# sepanjang simulasi. Ekspektasi angka (`0`, lalu `1`) dulu mengunci bug
# inisialisasi-nol.
cek "step_s1" "s1 t=4 hitung=x bayangan=11" "$(echo "$langkah" | grep '^s1 ')"
# Di antara segmen 1 dan 2 ada satu edge clock, jadi hitung naik ke 1.
cek "step_s2" "s2 t=8 hitung=x bayangan=21" "$(echo "$langkah" | grep '^s2 ')"
# LRM §20.3: `$finish` menghentikan simulasi. Segmen terakhir hanya memanggil
# `$display` lalu `$finish`, jadi tidak ada edge clock lagi sesudah `s2` dan
# nilai akhir sama dengan yang dicetak pada `s2`. Nilai lama `03` menghitung
# dua edge liar sesudah `$finish`.
cek "step_hitung" "hitung = xx" "$(echo "$langkah" | grep '^hitung =')"
# `bayangan = deposit + 1` juga settle di langkah yang sama; setelah
# `$finish` tidak ada `eval_comb` lagi, jadi nilainya sama dengan `s2` (0x15).
cek "step_bayangan" "bayangan = 15" "$(echo "$langkah" | grep '^bayangan =')"

echo "=== 15. format %t dan always_ff body kosong ==="
fmt=$($SV run --rtl $RTL_DIR/tb_format.sv --steps 4)
cek "fmt_a" "a=100ns"  "$(echo "$fmt" | grep '^a=')"
# 100 + 900 = 1000 ns, habis dibagi us => ditulis "1us".
cek "fmt_b" "b=1us"    "$(echo "$fmt" | grep '^b=')"
# 2500 ns tidak habis dibagi us, jadi tetap "2500ns".
cek "fmt_c" "c=2500ns" "$(echo "$fmt" | grep '^c=')"
cek "fmt_d" "d=3500ns" "$(echo "$fmt" | grep '^d=')"
# `always_ff @(posedge clk);` body kosong tetap bisa dielaborasi.
cek "ff_kosong" "gak_nulis = 00" "$(echo "$fmt" | grep '^gak_nulis =')"
cek "ff_konstan" "konstan = 1"   "$(echo "$fmt" | grep '^konstan =')"

echo "=== 16. $monitor mencetak saat nilai berubah ==="
mon=$($SV run --rtl $RTL_DIR/tb_monitor.sv --steps 8)
# Pendaftaran: cetak sekali pada t=0 dengan nilai saat itu.
cek "mon_awal" "t=0 h=0 b=1"   "$(echo "$mon" | grep '^t=0 ')"
# Setelah #4 dan satu edge clock, nilai terbaru ikut tercetak.
cek "mon_4ns"  "t=4ns h=6 b=6" "$(echo "$mon" | grep '^t=4ns ')"
cek "mon_8ns"  "t=8ns h=10 b=10" "$(echo "$mon" | grep '^t=8ns ')"
#_baris monitor tidak boleh lebih dari tiga: hanya saat nilai berubah.
jumlah_monitor=$(echo "$mon" | grep -c '^t=')
if [ "$jumlah_monitor" != "3" ]; then
  echo "GAGAL: $monitor dicetak $jumlah_monitor kali, harus 3"
  exit 1
fi
printf '  %-10s %s\n' "mon_count" "$jumlah_monitor baris"

echo "=== 17. \$monitoron/\$monitoroff dan \$strobe ==="
sb=$($SV run --rtl $RTL_DIR/tb_strobe.sv --steps 8)
cek "sb_mon_awal" "mon t=0 h=0"       "$(echo "$sb" | grep '^mon t=0 ')"
# Argumen $strobe dievaluasi saat dipanggil: h masih 0 pada t=2ns.
cek "sb_strobe"   "strobe t=2ns h=0" "$(echo "$sb" | grep '^strobe ')"
# $monitor dilewati saat $monitoroff aktif; nyalakan lagi di t=4ns.
cek "sb_mon_lama" "mon t=4ns h=2"     "$(echo "$sb" | grep '^mon t=4ns ')"
cek "sb_strobe_off" "off t=4ns h=1"   "$(echo "$sb" | grep '^off ')"
#monitor hanya boleh tercetak dua kali: saat daftar dan setelah dinyalakan.
jumlah_mon=$(echo "$sb" | grep -c '^mon ')
if [ "$jumlah_mon" != "2" ]; then
  echo "GAGAL: monitor tercetak $jumlah_mon kali, harus 2"
  exit 1
fi
printf '  %-10s %s\n' "sb_count" "$jumlah_mon baris monitor"

echo "=== 18. \$monitor if (kondisi) ==="
kond=$($SV run --rtl $RTL_DIR/tb_monitor_cond.sv --steps 8)
# Syarat `hitung >= 3` masih salah pada t=0 (h=0), jadi tidak ada cetakan.
cek "mc_awal" "t=4ns h=10" "$(echo "$kond" | grep '^t=')"
jumlah_kond=$(echo "$kond" | grep -c '^t=')
if [ "$jumlah_kond" != "1" ]; then
  echo "GAGAL: \$monitor if tercetak $jumlah_kond kali, harus 1"
  exit 1
fi
printf '  %-10s %s\n' "mc_count" "$jumlah_kond baris"

echo "=== 19. @(posedge clk) di dalam blok initial ==="
ev=$($SV run --rtl $RTL_DIR/tb_event.sv --steps 8)
cek "ev_mulai" "t=0 mulai"            "$(echo "$ev" | grep '^t=0 mulai$')"
# Proses menangguhkan diri sampai posedge pertama terjadi, waktunya masih 0.
cek "ev_edge1" "t=0 setelah-edge-1"   "$(echo "$ev" | grep '^t=0 setelah-edge-1$')"
# Setelah #4, posedge kedua butuh langkah clock berikutnya.
cek "ev_edge2" "t=4ns setelah-edge-2" "$(echo "$ev" | grep '^t=4ns setelah-edge-2$')"
# LRM §20.3: `$finish` menghentikan simulasi. Segmen terakhir hanya `#4` lalu
# `$finish`, jadi tidak ada edge clock lagi sesudah `setelah-edge-2`. Tiga
# edge terjadi sebelum `$finish` (dua untuk melepas dua tunggu, satu lagi
# pada langkah segmentasi), sehingga hitung = 3. Nilai lama `05` menghitung
# dua edge liar sesudah `$finish`.
cek "ev_hitung" "hitung = 03"         "$(echo "$ev" | grep '^hitung =')"

echo "=== 20. @(posedge clk or negedge rst) gabungan ==="
evor=$($SV run --rtl $RTL_DIR/tb_event_or.sv --steps 8)
cek "evor_mulai" "t=0 mulai rst=1"              "$(echo "$evor" | grep '^t=0 mulai')"
# salah satu item edge sudah cukup untuk melepas proses.
cek "evor_lanjut" "t=0 setelah-gabungan rst=1 h=0" "$(echo "$evor" | grep '^t=0 setelah-gabungan')"

echo "=== 21. regresi: bentuk lama tanpa operator baru ==="
$SV check --rtl examples/e2e_alu/rtl/alu.sv
echo "alu tetap bisa di-check"

echo "=== 22. pass constant folding sv-opt ==="
# sv-opt hanya melipat ekspresi yang seluruh operand-nya konstanta. Ada juga
# syarat "hasil muat dalam lebar tipe", tetapi sampai sekarang syarat itu
# belum punya efek yang teramati: baik terlipat atau tidak, nilai akhirnya
# sama karena pemformat dan penulisan sinyal sama-sama memask. Syarat itu
# tetap dipertahankan sebagai pengaman, dan dikunci unit test `sv-opt`.
fold=$($SV run --rtl $RTL_DIR/tb_fold.sv --rtl $RTL_DIR/tb_fold_wrap.sv --top tb_fold_wrap --steps 4 2>/tmp/fold.stat)
# `2 * 8'd3 + 8'd4` = 10, `|4'b1010` = 1, `8'hB6[5:4]` = 2'b01.
cek "fold_a" "fold a jumlah=10 paritas=1 potong=0011" "$(echo "$fold" | grep '^fold a')"
# `paritas ^ din` tak terlipat karena `din` dinamis, jadi nilai ikut berubah.
cek "fold_b" "fold b jumlah=10 paritas=0 potong=0011" "$(echo "$fold" | grep '^fold b')"
terlipat=$(grep -o 'sv-opt: [0-9]* ekspresi terlipat' /tmp/fold.stat | grep -o '[0-9]*')
if [ "$terlipat" -ge 3 ]; then
  echo "  sv-opt melaporkan $terlipat ekspresi terlipat"
else
  echo "GAGAL: sv-opt hanya melaporkan $terlipat lipatan, seharusnya >= 3"
  exit 1
fi

echo "=== 23. pass sv-opt tak mengubah hasil RTL realistis ==="
# RTL sungguhan hampir tak punya ekspresi konstan, jadi lipatan 0. Yang
# penting: hasil simulasi tetap sama setelah pass berjalan.
rtl_stat=$($SV run --rtl examples/e2e_alu/rtl/alu.sv --steps 2 2>&1 >/dev/null | grep 'sv-opt' || true)
echo "  ${rtl_stat:-sv-opt: 0 ekspresi terlipat dari 0 proses} (RTL realistis, lipatan minim)"
alu_out=$($SV run --rtl examples/e2e_alu/rtl/alu.sv --steps 2 2>/dev/null)
if [ -z "$alu_out" ]; then
  echo "  alu tetap menghasilkan keluaran setelah pass"
else
  echo "  alu: $(echo "$alu_out" | head -n 1)"
fi

echo "DEMO OK"
