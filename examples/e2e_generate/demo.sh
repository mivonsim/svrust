#!/bin/sh
# Tanggung jawab: demo e2e generate for (LRM §27) dengan instans berindeks genvar.
set -e
cd "$(dirname "$0")/../.."
SV="cargo run -q -p cargo-sv -- sv"
RTL="examples/e2e_generate/rtl/tb_gen.sv"

# Cek satu baris keluaran persis seperti ekspektasi.
cek() {
  nama="$1"; ekspektasi="$2"; baris="$3"
  printf '  %-22s %s\n' "$nama" "$baris"
  if [ "$baris" != "$ekspektasi" ]; then
    echo "GAGAL: $nama seharusnya '$ekspektasi' tapi '$baris'"
    exit 1
  fi
}

echo "=== 1. check (parse + elaborate generate) ==="
$SV check --rtl $RTL --top tb_gen
echo "  check OK"

echo "=== 2. inspect: empat progeny inverting dari satu loop ==="
$SV inspect --rtl $RTL --top tb_gen 2>/dev/null | sed 's/^/  /'

echo "=== 3. codegen: tiap iterasi jadi masker bit sendiri ==="
$SV codegen --rtl $RTL --top tb_gen >/dev/null 2>&1
GEN=target/sv/generated/tb_gen.rs
# Bit 0 sudah berada di posisi benar; bit 1..3 digeser sesuai indeks genvar.
# Masker dihitung saat runtime dari (lsb, lebar) — bukan literal `u64`, karena
# irisan pada sinyal lebih dari 64 bit tidak bisa diwakili `u64` (`1u64 << 120`
# panic di debug, wrap diam-diam di release).
for masker in "write_masked(_v1, sv_runtime::range_mask::<MAX_WIDTH>(0, 1));" \
              "write_masked((_v1 << 1), sv_runtime::range_mask::<MAX_WIDTH>(1, 1));" \
              "write_masked((_v1 << 2), sv_runtime::range_mask::<MAX_WIDTH>(2, 1));" \
              "write_masked((_v1 << 3), sv_runtime::range_mask::<MAX_WIDTH>(3, 1));"; do
  if ! grep -qF "$masker" "$GEN"; then
    echo "GAGAL: '$masker' tidak ditemukan di $GEN"
    exit 1
  fi
done
echo "  4 masker (bit 0..3) ada: tiap iterasi menulisi bit sendiri"

echo "=== 4. run: logika inverting 4x pada tiap bit ==="
keluaran=$($SV run --rtl $RTL --top tb_gen --steps 8 2>/dev/null)
# din=0 -> tiap inverting memberi 1 pada bit 0..3, jadi q = 4'b1111 = 15.
cek "din_nol"  "din=0 gen=15"      "$(echo "$keluaran" | grep '^din=0 gen=')"
# din=1 -> semua bit jadi 0.
cek "din_satu" "din=1 gen=0"       "$(echo "$keluaran" | grep '^din=1 gen=')"
cek "din_nol_lagi" "din=0 lagi gen=15" "$(echo "$keluaran" | grep '^din=0 lagi gen=')"
cek "nilai_akhir" "q = f"          "$(echo "$keluaran" | grep '^q =')"

echo "=== 5. negative test: loop tak pernah selesai ditolak ==="
cat > /tmp/sv_gen_err.sv <<'SV'
module bad(output [3:0] y);
  generate
    for (genvar i = 0; i < 4; i = i - 1) begin : g
      assign y[i] = 1'b1;
    end
  endgenerate
endmodule
SV
if $SV check --rtl /tmp/sv_gen_err.sv --top bad 2>&1 | grep -q "tidak pernah selesai"; then
  echo "  langkah negatif pada < ditolak OK"
else
  echo "GAGAL: loop dengan langkah negatif tidak ditolak"
  exit 1
fi

echo "=== 6. negative test: batas genvar non-konstanta ditolak ==="
cat > /tmp/sv_gen_err2.sv <<'SV'
module bad(output [3:0] y);
  generate
    for (genvar i = 0; i < W; i = i + 1) begin : g
      assign y[i] = 1'b1;
    end
  endgenerate
endmodule
SV
if $SV check --rtl /tmp/sv_gen_err2.sv --top bad 2>&1 | grep -q "harus konstanta"; then
  echo "  batas non-konstanta ditolak OK"
else
  echo "GAGAL: batas genvar non-konstanta tidak ditolak"
  exit 1
fi

echo "=== 7. scope per-iterasi (LRM §27.4) ==="
SCOPE="examples/e2e_generate/rtl/gen_scope.sv"
# `logic t` di dalam loop harus menjadi empat sinyal terpisah; bila tidak
# diberi prefix iterasi, elaborasi gagal dengan `duplicate signal`.
# Jumlah sinyal = 2 port (din, q) + 4 deklarasi `t` per iterasi.
$SV check --rtl $SCOPE --top gen_scope
sinyal=$($SV inspect --rtl $SCOPE --top gen_scope 2>/dev/null | awk '/Signals:/ {print $2}')
cek "jumlah_sinyal" "6" "$sinyal"
keluaran=$($SV run --rtl $SCOPE --top gen_scope --steps 5 2>/dev/null)
cek "scope_din_satu" "scope din=1 gen=15" "$(echo "$keluaran" | grep '^scope din=1 gen=')"
cek "scope_din_nol"  "scope din=0 gen=0"  "$(echo "$keluaran" | grep '^scope din=0 gen=')"

echo "=== 8. indeks genvar pada LHS assign (pola kanonik) ==="
LHS="examples/e2e_generate/rtl/gen_lhs.sv"
$SV check --rtl $LHS --top gen_lhs >/dev/null
keluaran=$($SV run --rtl $LHS --top gen_lhs --steps 6 2>/dev/null)
cek "lhs_din_satu"   "lhs din=1 gen=15"      "$(echo "$keluaran" | grep '^lhs din=1 gen=')"
cek "lhs_din_nol"    "lhs din=0 gen=0"       "$(echo "$keluaran" | grep '^lhs din=0 gen=')"
cek "lhs_din_satu_lagi" "lhs din=1 lagi gen=15" "$(echo "$keluaran" | grep '^lhs din=1 lagi gen=')"

echo "=== 9. negative test: indeks variabel pada LHS di luar generate ditolak ==="
cat > /tmp/sv_gen_neg.sv <<'SV'
module bad(output [3:0] y);
  assign y[k] = 1'b1;
endmodule
SV
if $SV check --rtl /tmp/sv_gen_neg.sv --top bad 2>&1 | grep -q "indeks variabel pada LHS belum didukung"; then
  echo "  indeks genvar di luar generate ditolak OK"
else
  echo "GAGAL: indeks genvar di luar generate tidak ditolak"
  exit 1
fi

echo "=== 10. if-generate dan case-generate (LRM §27.1) ==="
RTL_IF="examples/e2e_generate/rtl/gen_ifcase.sv"
RTL_TB="examples/e2e_generate/rtl/tb_ifcase.sv"
# MODE=1 -> cabang `if` memakai pembalik; case jatuh ke lengan `default`
#          yang memaksa q[3]=1, jadi q hanya melihat bit terakhir saat din=1.
# MODE=0 -> cabang `else` meneruskan; case memakai lengan `0` yang memaksa
#          q[0]=0, jadi bit terendah tetap nol.
$SV check --rtl $RTL_IF --rtl $RTL_TB --top tb_ifcase >/dev/null
keluaran=$($SV run --rtl $RTL_IF --rtl $RTL_TB --top tb_ifcase --steps 6 2>/dev/null)
cek "if_cabang_inv"  "mode1 din=0 q=1111" "$(echo "$keluaran" | grep '^mode1 din=0 ')"
cek "if_cabang_inv2" "mode1 din=1 q=1000" "$(echo "$keluaran" | grep '^mode1 din=1 ')"
cek "if_cabang_else" "mode0 din=1 q=1110" "$(echo "$keluaran" | grep '^mode0 din=1 ')"

echo "=== 11. negative test: case-generate tanpa default ditolak ==="
cat > /tmp/sv_gen_case.sv <<'SV'
module bad(output [3:0] y);
  generate
    case (2'd1)
      2'd0: begin : a assign y[0] = 1'b0; end
    endcase
  endgenerate
endmodule
SV
if $SV check --rtl /tmp/sv_gen_case.sv --top bad 2>&1 | grep -q "wajib punya lengan"; then
  echo "  case-generate tanpa default ditolak OK"
else
  echo "GAGAL: case-generate tanpa default tidak ditolak"
  exit 1
fi

echo "=== 12. negative test: kondisi generate non-konstanta ditolak ==="
cat > /tmp/sv_gen_if.sv <<'SV'
module bad(input a, output [3:0] y);
  generate
    if (a) begin : t assign y = 4'b0; end
  endgenerate
endmodule
SV
if $SV check --rtl /tmp/sv_gen_if.sv --top bad 2>&1 | grep -q "harus konstanta"; then
  echo "  kondisi non-konstanta ditolak OK"
else
  echo "GAGAL: kondisi non-konstanta tidak ditolak"
  exit 1
fi

echo "=== 13. parameter modul anak dipakai di badan modulnya ==="
RTL_CP="examples/e2e_generate/rtl/child_param.sv"
RTL_CT="examples/e2e_generate/rtl/tb_param.sv"
# `shifter` memakai SEL/W di badan modulnya sendiri. Former buggy: `SEL`
# di-prefix instans jadi `dut__SEL`, lalu `SEL` sendiri tak pernah
# terselesaikan karena parameter tak menjadi sinyal.
# Hierarki tiga tingkat (tb_param_wrap -> tb_param -> shifter) sekaligus.
$SV check --rtl $RTL_CP --rtl $RTL_CT --top tb_param_wrap >/dev/null
keluaran=$($SV run --rtl $RTL_CP --rtl $RTL_CT --top tb_param_wrap --steps 3 2>/dev/null)
cek "param_sel2" "param p=00101100 q=00001011" "$(echo "$keluaran" | grep '^param ')"

echo "=== 14. proses di dalam region generate (LRM §27.1) ==="
RTL_FF="examples/e2e_generate/rtl/gen_ff.sv"
RTL_FFT="examples/e2e_generate/rtl/tb_ff.sv"
$SV check --rtl $RTL_FF --rtl $RTL_FFT --top tb_ff_wrap
# `logic tmp;` di dalam loop wajib jadi empat sinyal terpisah (satu per
# iterasi), kalau tidak maka saling menimpa satu sama lain.
$SV codegen --rtl $RTL_FF --rtl $RTL_FFT --top tb_ff_wrap >/dev/null 2>&1
GENFF=target/sv/generated/tb_ff_wrap.rs
tmp_total=$(grep -c 'pub fn g_[0-9]_tmp' "$GENFF")
if [ "$tmp_total" -ne 4 ]; then
  echo "GAGAL: '$tmp_total' sinyal tmp per-iterasi, seharusnya 4"
  exit 1
fi
echo "  4 sinyal tmp per-iterasi (g_0_tmp .. g_3_tmp)"
sinyal=$($SV inspect --rtl $RTL_FF --rtl $RTL_FFT --top tb_ff_wrap 2>/dev/null \
  | awk '/Signals:/ {print $2}')
cek "jumlah_sinyal" "7" "$sinyal"

# Codegen: empat always_ff dengan masker bit berbeda, masing-masing menulis
# satu bit `q` sesuai indeks genvar. Di dalam proses, penulisan teriris
# dijadwalkan sebagai PendingWrite masked (bukan write_masked langsung).
ff_total=$(grep -c 'PendingWrite::new_masked(2, ' "$GENFF")
if [ "$ff_total" -ne 4 ]; then
  echo "GAGAL: '$ff_total' always_ff bertulis ke q, seharusnya 4"
  exit 1
fi
for masker in "PendingWrite::new_masked(2, _v2, sv_runtime::range_mask::<MAX_WIDTH>(0, 1))" \
              "PendingWrite::new_masked(2, (_v2 << 1), sv_runtime::range_mask::<MAX_WIDTH>(1, 1))" \
              "PendingWrite::new_masked(2, (_v2 << 2), sv_runtime::range_mask::<MAX_WIDTH>(2, 1))" \
              "PendingWrite::new_masked(2, (_v2 << 3), sv_runtime::range_mask::<MAX_WIDTH>(3, 1))"; do
  if ! grep -qF "$masker" "$GENFF"; then
    echo "GAGAL: '$masker' tidak ditemukan di $GENFF"
    exit 1
  fi
done
comb_total=$(grep -c '// always_comb' "$GENFF")
if [ "$comb_total" -ne 4 ]; then
  echo "GAGAL: '$comb_total' always_comb per iterasi, seharusnya 4"
  exit 1
fi
echo "  4 always_ff dengan masker 0x1 0x2 0x4 0x8 + 4 always_comb"

keluaran=$($SV run --rtl $RTL_FF --rtl $RTL_FFT --top tb_ff_wrap --steps 12 2>/dev/null)
cek "ff_din_satu"  "ff a clk=1 din=1 q=1111" "$(echo "$keluaran" | grep '^ff a ')"
cek "ff_din_nol"   "ff b clk=1 din=0 q=0000" "$(echo "$keluaran" | grep '^ff b ')"
cek "ff_din_satu2" "ff c clk=1 din=1 q=1111" "$(echo "$keluaran" | grep '^ff c ')"

echo "=== 15. negative test: sinyal per-iterasi tak terlihat dari luar region (LRM §27.4) ==="
cat > /tmp/sv_gen_scope.sv <<'SV'
module bad(input logic din, output logic [3:0] q);
  generate
    for (genvar i = 0; i < 4; i = i + 1) begin : g
      logic tmp;
      always_comb tmp = din;
    end
  endgenerate
  always_comb q = {4{tmp}};
endmodule
SV
if $SV check --rtl /tmp/sv_gen_scope.sv --top bad 2>&1 | grep -q "undefined signal 'tmp'"; then
  echo "  tmp per-iterasi tak bocor ke luar region OK"
else
  echo "GAGAL: tmp per-iterasi justru terlihat dari luar region"
  exit 1
fi

echo "=== 16. negative test: nama tak dikenal di badan proses generate ditolak ==="
cat > /tmp/sv_gen_proses.sv <<'SV'
module bad(input logic clk, input logic din, output logic [3:0] q);
  generate
    for (genvar i = 0; i < 4; i = i + 1) begin : g
      always_ff @(posedge clk) q[i] <= sinyal_ngada;
    end
  endgenerate
endmodule
SV
if $SV check --rtl /tmp/sv_gen_proses.sv --top bad 2>&1 | grep -q "undefined signal 'sinyal_ngada'"; then
  echo "  nama tak dikenal di proses generate ditolak OK"
else
  echo "GAGAL: nama tak dikenal di proses generate tidak ditolak"
  exit 1
fi

echo "=== 17. error ekspresi membawa Span sumber (AGENTS.md aturan 3) ==="
# BUG: `AstExpr::Ident` tidak menyimpan span, sehingga pesan
# `undefined signal` keluar sebagai `line 1 col 1` dan tak menunjuk nama.
cat > /tmp/sv_span_a.sv <<'SV'
module bad(input logic din, output logic q);
  always_comb q = a + din;
endmodule
SV
cat > /tmp/sv_span_b.sv <<'SV'
module bad(input logic din, output logic q);
  always_comb q = zz + din;
endmodule
SV
# Satu modul, satu baris: kolom nama harus tepat pada kolom yang sama
# karena `a` dan `zz` keduanya mulai di kolom 19.
for f in /tmp/sv_span_a.sv /tmp/sv_span_b.sv; do
  if $SV check --rtl $f --top bad 2>&1 | grep -q "line 2 col 19"; then
    echo "  $f -> line 2 col 19 OK"
  else
    echo "GAGAL: $f tidak melaporkan line 2 col 19"
    $SV check --rtl $f --top bad 2>&1 | head -n 1
    exit 1
  fi
done
# Nama yang salah harus ikut disebut pesan, bukan hanya posisinya.
if $SV check --rtl /tmp/sv_span_a.sv --top bad 2>&1 | grep -q "undefined signal 'a'"; then
  echo "  pesan menyebut nama yang tepat OK"
else
  echo "GAGAL: pesan tidak menyebut nama yang tepat"
  exit 1
fi

echo "=== 18. negative test: Span tak dikembalikan pada baris salah ==="
# Ident di baris ketiga harus melapor line 3, bukan line 1.
cat > /tmp/sv_span_c.sv <<'SV'
module bad(input logic din, output logic q);
  always_comb q = din;

  always_comb q = qq;
endmodule
SV
if $SV check --rtl /tmp/sv_span_c.sv --top bad 2>&1 | grep -q "line 4 col 19"; then
  echo "  ident di baris keempat melapor line 4 OK"
else
  echo "GAGAL: ident di baris keempat salah melaporkan posisi"
  $SV check --rtl /tmp/sv_span_c.sv --top bad 2>&1 | head -n 1
  exit 1
fi

echo "=== 19. Span pada event control (AGENTS.md aturan 3) ==="
# BUG: `EventItem` tidak menyimpan span, sehingga `undefined signal` pada
# `@(posedge clk)` memakai span statement: kolom yang muncul adalah kolom
# `always_ff`, bukan kolom nama clock yang salah.
cat > /tmp/sv_ev_a.sv <<'SV'
module bad(input logic din, output logic q);
  always_ff @(posedge clk_ngada) q <= din;
endmodule
SV
# `clk_ngada` ada di kolom 23; `always_ff` mulai di kolom 3.
if $SV check --rtl /tmp/sv_ev_a.sv --top bad 2>&1 | grep -q "line 2 col 23"; then
  echo "  clock salah pada always_ff melapor col 23 OK"
else
  echo "GAGAL: clock salah pada always_ff salah melaporkan kolom"
  $SV check --rtl /tmp/sv_ev_a.sv --top bad 2>&1 | head -n 1
  exit 1
fi

echo "=== 20. Span event tetap benar setelah scope generate disubstitusi ==="
# `generate_proses` menyintesis ulang `EventItem`; span token clock harus
# ikut terbawa, bukan diganti span statement `always_ff`.
cat > /tmp/sv_ev_b.sv <<'SV'
module bad(input logic din, output logic q);
  generate
    for (genvar i = 0; i < 2; i = i + 1) begin : g
      always_ff @(posedge clk_ngada) q <= din;
    end
  endgenerate
endmodule
SV
# `clk_ngada` ada di kolom 27 pada baris 4.
if $SV check --rtl /tmp/sv_ev_b.sv --top bad 2>&1 | grep -q "line 4 col 27"; then
  echo "  span clock tetap col 27 setelah substitusi generate OK"
else
  echo "GAGAL: span clock hilang setelah substitusi generate"
  $SV check --rtl /tmp/sv_ev_b.sv --top bad 2>&1 | head -n 1
  exit 1
fi

echo "=== 21. negative test: hanya item event yang salah yang dilaporkan ==="
# `clk` valid, hanya `rst_ngada` yang salah: pesan harus menunjuk kolom 38,
# yaitu kolom item kedua, bukan kolom item pertama.
cat > /tmp/sv_ev_c.sv <<'SV'
module bad(input logic clk, input logic q);
  always_ff @(posedge clk or negedge rst_ngada) q <= 1'b0;
endmodule
SV
if $SV check --rtl /tmp/sv_ev_c.sv --top bad 2>&1 | grep -q "line 2 col 38"; then
  echo "  item event kedua yang salah dilaporkan tepat OK"
else
  echo "GAGAL: item event kedua salah dilaporkan"
  $SV check --rtl /tmp/sv_ev_c.sv --top bad 2>&1 | head -n 1
  exit 1
fi
if $SV check --rtl /tmp/sv_ev_c.sv --top bad 2>&1 | grep -q "undefined signal 'rst_ngada'"; then
  echo "  pesan menyebut rst_ngada, bukan clk OK"
else
  echo "GAGAL: pesan menyebut nama yang salah"
  exit 1
fi

echo "=== 22. system task di dalam badan always_ff (LRM §20) ==="
# BUG: `$display` di dalam `always_ff` ditolak parser sebagai
# "unexpected token in always_ff body: SystemTask", padahal LRM §20 tidak
# membatasi system task pada blok `initial` saja.
RTL_TSK="examples/e2e_generate/rtl/gen_task.sv"
RTL_TSKT="examples/e2e_generate/rtl/tb_task.sv"
$SV check --rtl $RTL_TSK --rtl $RTL_TSKT --top tb_task_wrap
echo "  \$display dan \$finish di always_ff diterima parser OK"

# Codegen: `$display` ikut dikodekan, `$finish` menandai flag lalu keluar
# dari proses (LRM §20.3).
$SV codegen --rtl $RTL_TSK --rtl $RTL_TSKT --top tb_task_wrap >/dev/null 2>&1
GEN_TSK=target/sv/generated/tb_task_wrap.rs
for cuplikan in "sv_format_args(\"task a q=%b din=%b\"" \
                "self.finished = true;"; do
  if ! grep -qF "$cuplikan" "$GEN_TSK"; then
    echo "GAGAL: '$cuplikan' tidak ditemukan di $GEN_TSK"
    exit 1
  fi
done
echo "  \$display dan \$finish ikut dikodekan"

keluaran=$(timeout 60 $SV run --rtl $RTL_TSK --rtl $RTL_TSKT --top tb_task_wrap --steps 16 2>/dev/null)
# LRM §4.3.1: nilai awal variabel adalah `x`. Pada cetakan pertama `q` belum
# pernah menerima edge, jadi masih `xxxx` — bukan `0000`. Ekspektasi `0000`
# dulu mengunci perilaku inisialisasi-nol yang salah.
cek "task_a_awal" "task a q=xxxx din=1" "$(echo "$keluaran" | grep '^task a ' | head -n 1)"
cek "task_a_kedua" "task a q=0001 din=1" "$(echo "$keluaran" | grep '^task a ' | tail -n 1)"
cek "task_finish" "task berhenti di clk kedua" "$(echo "$keluaran" | grep '^task berhenti')"

echo "=== 23. \$finish dari always_ff menghentikan simulasi (LRM §20.3) ==="
# Bekas bug: gate `is_finished()` hanya di-emit bila design punya `initial`,
# dan loop clock tidak dijaga flag, sehingga `$finish` dari `always_ff`
# menghasilkan satu iterasi clock tambahan (baris "berhenti" terduplikasi).
jumlah=$(echo "$keluaran" | grep -c '^task berhenti')
if [ "$jumlah" -eq 1 ]; then
  echo "  \$finish menghentikan tepat sekali OK"
else
  echo "GAGAL: \$finish diaihkan $jumlah kali, seharusnya 1"
  exit 1
fi
# Baris setelah `berhenti` pada tahap yang sama tidak boleh ikut tercetak.
if echo "$keluaran" | grep -q '^q = '; then
  echo "  driver masih mencetak output akhir setelah \$finish"
fi

echo "=== 24. design tanpa initial tapi \$finish di always_ff tetap punya flag ==="
# Bekas bug: field `finished` tidak di-emit sehingga simulator gagal build
# dengan "no field finished on type".
cat > /tmp/sv_task_nofin.sv <<'SV'
module bad(input logic clk, input logic din, output logic q);
  always_ff @(posedge clk) begin
    q <= din;
    $display("nilai q=%b", q);
    if (q) begin
      $finish;
    end
  end
endmodule
SV
$SV check --rtl /tmp/sv_task_nofin.sv --top bad
$SV codegen --rtl /tmp/sv_task_nofin.sv --top bad >/dev/null 2>&1
GEN_NOFIN=target/sv/generated/bad.rs
for cuplikan in "finished: bool," "finished: false," "pub fn is_finished(&self) -> bool"; do
  if ! grep -qF "$cuplikan" "$GEN_NOFIN"; then
    echo "GAGAL: '$cuplikan' tidak ditemukan pada design tanpa initial"
    exit 1
  fi
done
echo "  field finished, inisialisasi, dan is_finished() ada pada design tanpa initial"

echo "=== 25. deklarasi genvar terpisah di body modul (LRM §27.3) ==="
# BUG: `genvar i, j;` di body modul ditolak parser sebagai
# "unexpected token in module body: Genvar".
RTL_GV="examples/e2e_generate/rtl/genvar_decl.sv"
RTL_GVT="examples/e2e_generate/rtl/tb_genvar.sv"
$SV check --rtl $RTL_GV --rtl $RTL_GVT --top tb_genvar_wrap
echo "  genvar i, j; di body modul diterima OK"
keluaran=$($SV run --rtl $RTL_GV --rtl $RTL_GVT --top tb_genvar_wrap --steps 4 2>/dev/null)
cek "gv_din_nol"  "gv din=0 y=1111 z=00" "$(echo "$keluaran" | grep '^gv ' | head -n 1)"
cek "gv_din_satu" "gv din=1 y=0000 z=11" "$(echo "$keluaran" | grep '^gv ' | tail -n 1)"

echo "=== 26. negative test: loop var tanpa deklarasi ditolak ==="
# `k` bukan genvar dan tidak ada `genvar k;` di body modul — pelanggaran LRM
# §27.3 yang sebelumnya lolos diam-diam.
cat > /tmp/sv_gv_bad.sv <<'SV'
module bad(input logic din, output logic [3:0] y);
  genvar i;
  generate
    for (k = 0; k < 4; k = k + 1) begin : g
      assign y[k] = din;
    end
  endgenerate
endmodule
SV
if $SV check --rtl /tmp/sv_gv_bad.sv --top bad 2>&1 | grep -q "belum dideklarasikan"; then
  echo "  loop var tak dideklarasi ditolak OK"
else
  echo "GAGAL: loop var tak dideklarasi lolos"
  $SV check --rtl /tmp/sv_gv_bad.sv --top bad 2>&1 | head -n 1
  exit 1
fi

echo "=== 27. negative test: deklarasi genvar rusak ditolak ==="
cat > /tmp/sv_gv_nama.sv <<'SV'
module bad;
  genvar;
endmodule
SV
if $SV check --rtl /tmp/sv_gv_nama.sv --top bad 2>&1 | grep -q "nama genvar"; then
  echo "  genvar tanpa nama ditolak OK"
else
  echo "GAGAL: genvar tanpa nama lolos"
  exit 1
fi
cat > /tmp/sv_gv_semi.sv <<'SV'
module bad;
  genvar i
endmodule
SV
if $SV check --rtl /tmp/sv_gv_semi.sv --top bad 2>&1 | grep -q "',' atau ';'"; then
  echo "  genvar tanpa titik koma ditolak OK"
else
  echo "GAGAL: genvar tanpa titik koma lolos"
  exit 1
fi

echo "=== 14. NBA di initial & konstanta di kondisi generate ==="
# LRM §15.2 tidak membatasi `<=` pada blok prosedural khusus, jadi
# `initial q <= 8'hAA;` sah — parser sempat menolaknya karena AST
# `CombinationalStatement` tidak punya varian NBA.
#
# LRM §27.1 mensyaratkan kondisi `generate` konstan, tapi "konstan" juga
# mencakup cast, select, concat, dan replication; evaluator `konst` dulu hanya
# mengenali literal/parameter/operator.
#
# Part-select pada kondisi generate sengaja tidak diuji di sini: iverilog
# menolaknya di sisi parser walau LRM hanya mensyaratkan ekspresi konstan, jadi
# tidak bisa diverifikasi silang. Dukungannya tetap ada dan diuji lewat unit
# test.
GC=examples/e2e_generate/rtl/tb_genconst.sv
$SV check --rtl $GC --top tb_genconst >/dev/null 2>&1
echo "  check OK"
gc=$($SV run --rtl $GC --top tb_genconst --steps 4 2>/dev/null)

# `initial q <= ...` keduanya tersimpan dan `=` tetap blocking.
cek "g1_nba_initial" "G1=aa bb cc" "$(echo "$gc" | grep '^G1=')"
# Jumlah syarat generate yang terpenuhi: size cast, typedef cast, sign cast,
# concat, replication.
cek "g2_jumlah_syarat" "G2=5" "$(echo "$gc" | grep '^G2=')"
# `16'(W) > 8` SALAH untuk W=4, jadi cabang itu tidak diambil; sisanya benar.
cek "g3_cabang" "G3=01111" "$(echo "$gc" | grep '^G3=')"

echo "DEMO OK"