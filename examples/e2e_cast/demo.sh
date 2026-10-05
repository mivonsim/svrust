#!/bin/sh
# Tanggung jawab: demo e2e cast SystemVerilog (LRM §6.14, §8.20, §11.6.1, §11.8, §12.5, §5.7.1).
#
# Semua nilai yang diharapkan sudah diverifikasi terhadap iverilog 12.0
# (`iverilog -g2012`), bukan hanya terhadap implementasi SVRust sendiri.
set -e
cd "$(dirname "$0")/../.."

RTL_DIR=examples/e2e_cast/rtl
SV="cargo run -q -p cargo-sv -- sv"

# Cek bahwa satu baris keluaran bernilai persis seperti yang diharapkan.
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

# Cek bahwa pesan error yang diharapkan muncul pada keluaran CLI.
cek_gagal() {
  nama="$1"
  pola="$2"
  keluaran="$3"
  printf '  %-22s %s\n' "$nama" "$keluaran"
  case "$keluaran" in
    *"$pola"*) ;;
    *)
      echo "GAGAL: $nama seharusnya memuat '$pola'"
      exit 1
      ;;
  esac
}

echo "=== 1. check (parse + elaborate) ==="
$SV check --rtl $RTL_DIR/tb_cast.sv --top tb_cast
echo "check OK"

echo "=== 2. simulasi ==="
keluaran=$($SV run --rtl $RTL_DIR/tb_cast.sv --steps 4 2>/dev/null)
# LRM §6.14: cast menyempit ke lebar tipe tujuan.
cek "c1_melebar" "c1 w=00a5" "$(echo "$keluaran" | grep '^c1 w=')"
cek "c2_menyempit" "c2 b=ef" "$(echo "$keluaran" | grep '^c2 b=')"
# Cast ke signed: -8'sd3 = 8'hFD -> 16'hFFFD = -3.
cek "c3_desimal" "c3 sw_dec=-3" "$(echo "$keluaran" | grep '^c3 sw_dec=')"
cek "c3_hex" "c3 sw=fffd" "$(echo "$keluaran" | grep '^c3 sw=')"
# Sign cast hanya mengubah signedness, lebarnya tetap.
cek "c4_signcast" "c4 bits=8" "$(echo "$keluaran" | grep '^c4 bits=')"
# $bits dari part-select memakai lebar select.
cek "c5_select" "c5 bits=8" "$(echo "$keluaran" | grep '^c5 bits=')"
# Cast ke unsigned membuang interpretasi negatif, bit tetap.
cek "c6_unsigned" "c6 b=fd" "$(echo "$keluaran" | grep '^c6 b=')"
# Cast berlaku atas hasil aritmetika.
cek "c7_aritmetika" "c7 w=00b4" "$(echo "$keluaran" | grep '^c7 w=')"
# Cast di dalam concat: tiap itemlebarnya sendiri.
cek "c8_concat" "c8 w=efa5" "$(echo "$keluaran" | grep '^c8 w=')"
# $bits dari concat = jumlah lebar seluruh item.
cek "c9_concat" "c9 bits=16" "$(echo "$keluaran" | grep '^c9 bits=')"

echo "=== 3. nama tipe tak dikenal ditolak ==="
# LRM §8.20: `nibble_t` tidak pernah di-typedef, jadi harus gagal dengan pesan
# yang menyebut nama tipenya.
gagal=$($SV check --rtl $RTL_DIR/tb_cast_err.sv --top tb_bad_cast 2>&1 || true)
cek_gagal "tipe_asing" "undefined type 'nibble_t'" "$gagal"

echo "=== 4. arah perluasan ikut signedness operand (LRM §6.14) ==="
# Cast ke tipe bawaan bukan pengecualian: operand unsigned di-zero-extend
# walau target signed, dan sebaliknya.
signed_k=$($SV run --rtl $RTL_DIR/tb_signed.sv --top tb_signed --steps 3 2>/dev/null)
cek "s1_signed_ke_unsigned" "s1=fffd" "$(echo "$signed_k" | grep '^s1=')"
cek "s2_unsigned_ke_signed" "s2=00fd" "$(echo "$signed_k" | grep '^s2=')"
cek "s3_signed_ke_signed" "s3=fffd" "$(echo "$signed_k" | grep '^s3=')"
cek "s4_unsigned_ke_unsigned" "s4=00fd" "$(echo "$signed_k" | grep '^s4=')"
cek "s5_sign_cast" "s5=fd" "$(echo "$signed_k" | grep '^s5=')"
cek "s6_lebar_sign_cast" "s6_bits=8" "$(echo "$signed_k" | grep '^s6_bits=')"
cek "s7_concat" "s7=fdfd" "$(echo "$signed_k" | grep '^s7=')"

echo "=== 5. nilai 4-state: x/z (LRM §5.7.1, §11.8, §20.4) ==="
# Digit `x`/`z` adalah bagian nilai, bukan `0`. Sebelum perbaikan ini literal
# 4-state dirakit lewat `to_u64` yang membuangnya, jadi `8'hxA` tercetak `0a`.
xz=$($SV run --rtl $RTL_DIR/tb_xz.sv --top tb_xz --steps 2 2>/dev/null)
cek "x1_literal_x" "x1=xa" "$(echo "$xz" | grep '^x1=')"
cek "x2_literal_z" "x2=z5" "$(echo "$xz" | grep '^x2=')"
cek "x3_bit" "x3=xxxxzzzz" "$(echo "$xz" | grep '^x3=')"
# Padding LRM §5.7.1: digit paling kiri `z` di-pad dengan `z`, bukan nol.
cek "x4_pad_z" "x4=zzzz" "$(echo "$xz" | grep '^x4=')"
cek "x5_campur" "x5=3x4z" "$(echo "$xz" | grep '^x5=')"
cek "x6_pad_nol" "x6=00ab" "$(echo "$xz" | grep '^x6=')"
# Cast LRM §6.14: x/z ikut terbawa, dan melebar ke unsigned mengisi nol.
cek "x7_cast_unsigned" "x7=00xx" "$(echo "$xz" | grep '^x7=')"
cek "x8_cast_signed" "x8=00xa" "$(echo "$xz" | grep '^x8=')"
# Concat LRM §11.8.1: setiap item punya lebar sendiri dan x/z-nya ikut.
cek "x9_concat" "x9=xa5z" "$(echo "$xz" | grep '^x9=')"
cek "x9_concat_bit" "x9b=xxxx10100101zzzz" "$(echo "$xz" | grep '^x9b=')"
cek "xa_replicate" "xa=00xx" "$(echo "$xz" | grep '^xa=')"
cek "xab_replicate_bit" "xab=000000001x1x1x1x" "$(echo "$xz" | grep '^xab=')"
# Part-select: x di dalam rentang terbawa, di luar tidak.
cek "xb_select" "xb=x" "$(echo "$xz" | grep '^xb=')"
# Lebar di atas 64 bit: MAX_WIDTH mengikuti lebar typedef, bukan sinyal.
cek "xc_128bit" "xc=000000000000000000000000deadbeef" "$(echo "$xz" | grep '^xc=')"
cek "xd_128bit_z" "xd=000000000000000000000000000000za" "$(echo "$xz" | grep '^xd=')"
# %h meminta sebanyak digit heksa sebesar lebar logis operand.
cek "xe_konst32" "xe=deadbeef" "$(echo "$xz" | grep '^xe=')"
cek "xf_konst8" "xf=ff" "$(echo "$xz" | grep '^xf=')"

echo "=== 6. cast ke tipe bawaan & size cast (LRM §6.14) ==="
# `integer'(x)` adalah cast yang paling sering dipakai kedua di SV setelah
# typedef. Sebelum fit ini ia gagal di lexer: `'` dibaca sebagai awal based
# literal dengan basis `(`.
builtin=$($SV run --rtl $RTL_DIR/tb_builtin.sv --top tb_builtin --steps 2 2>/dev/null)
cek "b1_integer" "b1=32" "$(echo "$builtin" | grep '^b1=')"
cek "b2_byte" "b2=8" "$(echo "$builtin" | grep '^b2=')"
cek "b3_shortint" "b3=16" "$(echo "$builtin" | grep '^b3=')"
cek "b4_longint" "b4=64" "$(echo "$builtin" | grep '^b4=')"
cek "b5_time" "b5=64" "$(echo "$builtin" | grep '^b5=')"
# LRM §6.16: `logic`/`bit` tanpa `[msb:lsb]` adalah vektor 1 bit.
cek "b6_logic_1bit" "b6=1" "$(echo "$builtin" | grep '^b6=')"
cek "b7_bit_1bit" "b7=1" "$(echo "$builtin" | grep '^b7=')"
# Cast bawaan bukan pengecualian: arah perluasan tetap ikut signedness OPERAND.
cek "b8_unsigned_ke_integer" "b8=000000a5" "$(echo "$builtin" | grep '^b8=')"
cek "b9_signed_ke_integer" "b9=fffffffd" "$(echo "$builtin" | grep '^b9=')"
# Size cast hanya mengubah lebar; tanda operand tetap menentukan perluasan.
cek "b10_size_signed" "b10=fffd" "$(echo "$builtin" | grep '^b10=')"
cek "b11_size_unsigned" "b11=00a5" "$(echo "$builtin" | grep '^b11=')"
cek "b12_size20" "b12=000000a5" "$(echo "$builtin" | grep '^b12=')"
cek "b13_size48" "b13=00000000000000a5" "$(echo "$builtin" | grep '^b13=')"
cek "b14_aritmetika" "b14=00b4" "$(echo "$builtin" | grep '^b14=')"
cek "b15_select" "b15=be" "$(echo "$builtin" | grep '^b15=')"
cek "b16_menyempit" "b16=ef" "$(echo "$builtin" | grep '^b16=')"
cek "b17_negatif" "b17=-3" "$(echo "$builtin" | grep '^b17=')"
cek "b18_positif" "b18=165" "$(echo "$builtin" | grep '^b18=')"

echo "=== 7. typedef module-scoped (LRM §8.20) ==="
# (a) Typedef milik modul anak terlihat di badan anak itu sendiri.
ok_anak=$($SV check --rtl $RTL_DIR/tb_scope.sv --top top_ok 2>&1)
printf '  %-22s %s\n' "anak_lokal" "$(echo "$ok_anak" | head -1)"
# (b) Dua modul boleh punya typedef nama sama dengan lebar BERBEDA.
ok_nama=$($SV check --rtl $RTL_DIR/tb_scope.sv --top top_nama_sama 2>&1)
printf '  %-22s %s\n' "nama_sama" "$(echo "$ok_nama" | head -1)"
# (c) Typedef milik induk yang dipakai di badan anak harus ditolak.
# iverilog: "Unable to bind parameter 'byte_t' in 'top.u0'"
# Pesan error harus menyebut nama yang ditulis user, bukan nama internal yang
# sudah di-prefix instans (`u0__byte_t`) — AGENTS.md aturan 3.
gagal_scope=$($SV check --rtl $RTL_DIR/tb_scope.sv --top top_bocor 2>&1 || true)
cek_gagal "bocor_ke_anak" "undefined type 'byte_t'" "$gagal_scope"
# (d) Nama yang sama untuk variabel dan typedef harus ditolak (LRM §8.20).
# Tanpa pemeriksaan ini `$bits(foo)` akan memilih tipe diam-diam.
# iverilog: "'foo' has already been declared in this scope"
gagal_ganda=$($SV check --rtl $RTL_DIR/tb_scope.sv --top top_nama_ganda 2>&1 || true)
cek_gagal "nama_ganda" "'foo' sudah dipakai sebagai nama sinyal" "$gagal_ganda"

echo "=== 8. bentuk size cast yang salah ditolak ==="
# Pesan error harus menyebut bentuk yang benar, bukan "expected basis" yang
# menyesatkan karena `'` dibaca sebagai awal based literal. Tiap kasus di berkas
# sendiri supaya error yang dilaporkan jelas milik kasus itu.
err_kutip=$($SV check --rtl $RTL_DIR/tb_err_quote.sv --top tb_builtin_err 2>&1 || true)
cek_gagal "kutip_tanpa_lebar" "size cast butuh lebar eksplisit" "$err_kutip"
err_kurung=$($SV check --rtl $RTL_DIR/tb_err_paren.sv --top tb_size_no_paren 2>&1 || true)
cek_gagal "tanpa_kurung" "size cast" "$err_kurung"
err_nol=$($SV check --rtl $RTL_DIR/tb_err_zero.sv --top tb_size_nol 2>&1 || true)
cek_gagal "lebar_nol" "lebar size cast harus > 0" "$err_nol"

echo "=== 9. deklarasi tipe integer atom (LRM §6.16, Tabel 6-22) ==="
# `byte`/`shortint`/`longint`/`time` sebelumnya hanya hidup di posisi cast
# `byte'(x)`, sehingga deklarasinya gagal dengan pesan "unexpected token" yang
# menyesatkan. Keyword-nya sekarang bisa dipakai di mana saja.
atom=$($SV run --rtl $RTL_DIR/tb_atom.sv --top tb_atom --steps 3 2>/dev/null)
cek "d1_byte" "d1=-91" "$(echo "$atom" | grep '^d1=')"
cek "d2_shortint" "d2=-16657" "$(echo "$atom" | grep '^d2=')"
cek "d3_longint" "d3=0" "$(echo "$atom" | grep '^d3=')"
cek "d4_time" "d4=5" "$(echo "$atom" | grep '^d4=')"
cek "d5_function" "d5=42" "$(echo "$atom" | grep '^d5=')"
cek "d12_task" "d12=1" "$(echo "$atom" | grep '^d12=')"
cek "d6_byte" "d6=8" "$(echo "$atom" | grep '^d6=')"
cek "d7_shortint" "d7=16" "$(echo "$atom" | grep '^d7=')"
cek "d8_longint" "d8=64" "$(echo "$atom" | grep '^d8=')"
cek "d9_time" "d9=64" "$(echo "$atom" | grep '^d9=')"
cek "d10_integer" "d10=32" "$(echo "$atom" | grep '^d10=')"
cek "d11_logic_polos" "d11=8" "$(echo "$atom" | grep '^d11=')"
cek "d13_byte_signed" "d13=ffffffa5" "$(echo "$atom" | grep '^d13=')"
cek "d14_time_unsigned" "d14=00000001" "$(echo "$atom" | grep '^d14=')"

echo "=== 10. konversi batas assignment (LRM §11.6.1) ==="
# Ekspresi kanan pada assignment adalah context-determined: lebarnya
# disesuaikan ke target, dan operand signed di-sign-extend. Sebelum fitur ini
# batas assignment hanya memotong, jadi `logic [31:0] o = sa` dengan `sa`
# signed 8-bit menghasilkan 0x000000FD.
assign=$($SV run --rtl $RTL_DIR/tb_assign.sv --top tb_assign --steps 3 2>/dev/null)
cek "a1_sign_extend" "a1=fffffffd" "$(echo "$assign" | grep '^a1=')"
cek "a2_ke_16bit" "a2=fffd" "$(echo "$assign" | grep '^a2=')"
cek "a3_zero_extend" "a3=000000fd" "$(echo "$assign" | grep '^a3=')"
# Signedness target tidak mempengaruhi arah perluasan.
cek "a4_target_signed" "a4=000000fd" "$(echo "$assign" | grep '^a4=')"
cek "a5_operand_signed" "a5=fffffffd" "$(echo "$assign" | grep '^a5=')"
cek "a6_potong_4bit" "a6=d" "$(echo "$assign" | grep '^a6=')"
cek "a7_potong_8bit" "a7=fd" "$(echo "$assign" | grep '^a7=')"
cek "a8_aritmetika" "a8=fffffffa" "$(echo "$assign" | grep '^a8=')"
cek "a9_literal_signed" "a9=fffffffd" "$(echo "$assign" | grep '^a9=')"
cek "a10_size_cast" "a10=fffffffd" "$(echo "$assign" | grep '^a10=')"
cek "a11_byte_signed" "a11=fffffffd" "$(echo "$assign" | grep '^a11=')"
# LHS teriris hanya memakai lebar irisan.
cek "a12_slice_16" "a12=0000fffd" "$(echo "$assign" | grep '^a12=')"
cek "a13_slice_4" "a13=fd" "$(echo "$assign" | grep '^a13=')"
cek "a14_perbandingan" "a14=01" "$(echo "$assign" | grep '^a14=')"

echo "=== 11. operator geser (LRM §11.6.1 Tabel 11-21) ==="
# Pada `i << j` hanya `j` yang self-determined; `i` ikut context-determined.
# Kalau `j` ikut diprioritaskan, `a << 32'd1` akan bergeser 32 bit dan nol.
geser=$($SV run --rtl $RTL_DIR/tb_shift.sv --top m --steps 4 2>/dev/null)
cek "g1_kiri_context" "G1=000001fe" "$(echo "$geser" | grep '^G1=')"
cek "g2_kanan_self_det" "G2=000001fe" "$(echo "$geser" | grep '^G2=')"
cek "g3_kiri_sign_extend" "G3=ffffffd0" "$(echo "$geser" | grep '^G3=')"
cek "g4_kanan_aritmetik" "G4=7ffffffe" "$(echo "$geser" | grep '^G4=')"
cek "g5_kanan_logika" "G5=0000007f" "$(echo "$geser" | grep '^G5=')"

echo "=== 12. propagate signedness hasil (LRM §11.6.1 langkah 3) ==="
# Operand context-determined dikonversi ke tipe HASIL. `sa + 16'd1` hasilnya
# unsigned (Tabel 11-22), jadi `sa` di-zero-extend — memakai signedness operand
# sendiri menghasilkan 0xFFFFFFFE yang keliru dianggap negatif.
lebar=$($SV run --rtl $RTL_DIR/tb_ctxwidth.sv --top m --steps 4 2>/dev/null)
cek "s1_geser_kiri" "S1=ffffffd0" "$(echo "$lebar" | grep '^S1=')"
cek "s2_geser_kanan" "S2=7ffffffe" "$(echo "$lebar" | grep '^S2=')"
cek "s3_geser_16bit" "S3=0fffffff" "$(echo "$lebar" | grep '^S3=')"
cek "s4_tambah_campuran" "S4=000000fe" "$(echo "$lebar" | grep '^S4=')"
cek "s5_kurang_campuran" "S5=000000fc" "$(echo "$lebar" | grep '^S5=')"
cek "s6_kali_campuran" "S6=000001fa" "$(echo "$lebar" | grep '^S6=')"
cek "s7_bagi_campuran" "S7=0000007e" "$(echo "$lebar" | grep '^S7=')"
cek "s8_ternary_context" "S8=fffffffd" "$(echo "$lebar" | grep '^S8=')"
cek "s9_nested_geser" "S9=fffffff7" "$(echo "$lebar" | grep '^S9=')"

echo "=== 13. asimetri antar jalur assignment (LRM §11.6.1) ==="
# `<=` (NBA), `+=` (compound), init/step `for`, dan nilai awal deklarasi
# semuanya harus menerapkan aturan yang sama seperti `=`. Sebelumnya hanya
# blocking assign dan continuous assign yang meneruskan lebar konteks.
all=$($SV run --rtl $RTL_DIR/tb_assign_all.sv --top m --steps 14 2>/dev/null)
cek "b1_nba_32bit" "B1=fffffffd" "$(echo "$all" | grep '^B1=')"
cek "b2_nba_16bit" "B2=fffd" "$(echo "$all" | grep '^B2=')"
cek "b3_compound_int" "B3=-3" "$(echo "$all" | grep '^B3=')"
cek "b4_compound_32" "B4=-3" "$(echo "$all" | grep '^B4=')"
cek "b5_compound_slice" "B5=1000" "$(echo "$all" | grep '^B5=')"
cek "b6_for_negatif" "B6=0" "$(echo "$all" | grep '^B6=')"
cek "b7_init_dipotong" "B7=1111" "$(echo "$all" | grep '^B7=')"

echo "=== 14. replication dengan count ekspresi (LRM §11.8.2) ==="
# `{WIDTH{1'b1}}` dengan `WIDTH` parameter modul adalah idiom RTL paling umum.
# Dulu count hanya boleh literal, jadi bentuk ini diam-diam dibaca sebagai
# concatenation `{WIDTH, 1'b1}` — nilai salah tanpa error apa pun.
repl=$($SV run --rtl $RTL_DIR/tb_repl.sv --top top --steps 3 2>/dev/null)
cek "r1_w12" "R1=0fff" "$(echo "$repl" | grep '^R1=')"
cek "r2_w4" "R2=000f" "$(echo "$repl" | grep '^R2=')"

echo "=== 15. packed array dari typedef (LRM §7.3) ==="
# `byte_t [3:0] nib;` dengan `typedef logic [7:0] byte_t` adalah packed array
# 4 elemen = 32 bit. Implementasi lama memperlakukannya sebagai "persempit" ke 4
# bit dan menolak N yang melebihi lebar tipe dasar — jadi RTL yang sah gagal.
packed=$($SV run --rtl $RTL_DIR/tb_packed.sv --top m --steps 2 2>/dev/null)
cek "p1_packed_32bit" "P1=0000000f" "$(echo "$packed" | grep '^P1=')"

echo "=== 9. \$signed / \$unsigned (LRM §11.4.7, §20) ==="
# Bentuk system function-nya identik dengan `signed'(x)` / `unsigned'(x)` pada
# LRM §6.14: hanya mengubah signedness, lebar TIDAK berubah. Keyword-nya sudah
# ada, jadi bentuk ini sempat ditolak parser.
sc=$($SV run --rtl $RTL_DIR/tb_signcast.sv --top tb_signcast --steps 3 2>/dev/null)

cek "k1_signed_nilai" "K1=fd" "$(echo "$sc" | grep '^K1=')"
cek "k2_unsigned_nilai" "K2=fd" "$(echo "$sc" | grep '^K2=')"
cek "k3_tanpa_perlebar" "K3=ff" "$(echo "$sc" | grep '^K3=')"
cek "k4_unsigned_16bit" "K4=ffff" "$(echo "$sc" | grep '^K4=')"
cek "k5_setara_cast" "K5=fd" "$(echo "$sc" | grep '^K5=')"
cek "k6_bits_tetap" "K6=8" "$(echo "$sc" | grep '^K6=')"
# Assignment ke target lebih lebar ikut aturan LRM §11.6.1: yang menentukan arah
# perluasan adalah signedness EKSPRESINYA, bukan operand aslinya.
cek "k7_sign_extend" "K7=fffffffd" "$(echo "$sc" | grep '^K7=')"
cek "k8_zero_extend" "K8=000000fd" "$(echo "$sc" | grep '^K8=')"
# LRM §11.4.5: perbandingan bertanda hanya bila KEDUA operand signed.
cek "k9_bukan_kedua_signed" "K9=0" "$(echo "$sc" | grep '^K9=')"
cek "k10_unsigned_kedua" "K10=0" "$(echo "$sc" | grep '^K10=')"
cek "k11_kedua_signed" "K11=01" "$(echo "$sc" | grep '^K11=')"

echo "DEMO OK"
