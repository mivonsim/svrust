# Fuzzing SVRust — Skema, Arsitektur, dan Taksonomi Bug

Dokumen ini adalah kontrak skema fuzzing repo ini: apa yang dibangkitkan, apa
yang diperiksa, dan **kelas bug apa saja yang benar-benar terjadi di SVRust**
sehingga skema ini dirancang untuk menangkapnya.

Filosofi: targetnya bukan "jangan temukan bug yang sudah saya tahu bentuknya",
melainkan **menjelajah ruang semantic state SystemVerilog secara sistematis dan
membuktikan setiap transformasi internal mempertahankan observable semantics.**

---

## 0. Dua koreksi falsaf (dan kenapa)

### 0.1 "Tidak boleh ada template" — terlalu ekstrem, dikoreksi

Pernyataan "semua token berasal dari byte fuzzer, nol literal tetap" adalah
**salah** kalau dibaca sebagai taboo konstanta. Yang salah adalah *template
program*: program tetap yang hanya divariasikan. Yang justru **wajib** ada adalah
**dictionary nilai batas**, karena bug di SVRust hampir selalu muncul tepat di
batas implementasi:

```
1u64 << 64   (masker part-select)      to_u64() truncation      MAX_WIDTH < lebar operand
```

Kalau lebar dipilih seragam dari `1..=256`, peluang mendarat di 63/64/65 dan
127/128/129 mendekati nol dan kelas bug itu **tidak akan pernah tersentuh**.

Jadi koreksinya:

| Dilarang | Diwajibkan |
|---|---|
| *Template program* (program tetap, hanya divariasikan) | *Dictionary nilai batas* (`0, 1, 2, 7, 8, 31, 32, 33, 63, 64, 65, 127, 128, 129, 255, 256, MAX_WIDTH, MAX_WIDTH+1`) |
| Nilai konstanta yang tersebar di source generator | Nilai batas terpusat di satu kamus dengan bobot |

Implementasi: `fuzzkit/src/boundary.rs` — enum `Batas` (18 kelas), `bobot()`
(12 untuk kelas `u64`, 3 untuk `MaxWidth`), dan `LebarPool` yang melakukan
sampling **dengan bobot**. Test menjaga agar setiap kelas batas benar-benar
terpilih dalam 512 draw, dan agar batas `u64` muncul > 3× lebar interior
tersering.

### 0.2 "Byte → source" — diganti semantic state-space

Byte-space ≠ semantic state-space. Byte acak **tidak** menghasilkan distribusi
yang seragam terhadap state semantik. Yang penting bukan "byte acak menghasilkan
signed, 127, `+`", tapi **kombinasi**:

```
width(A) > width(B)  ×  signed(A) != signed(B)  ×  context_width != operand_width
×  ekspresi bersarang  ×  4-state  ×  konstanta/runtime  ×  packed/unpacked
```

Implementasi: byte hanya memilih **kombinasi**, bukan isi program.
`boundary::DeklarasiSemantik` (lebar, signedness, constness, array, 4-state)
dan `boundary::Interaksi` (enam mismatch yang disengaja) adalah unit yang
dipilih byte; isi program diturunkan darinya.

---

## 1. Arsitektur

```
                     ┌──────────────────────┐
                     │  Seed / Real RTL     │
                     │  (OpenTitan, tests)  │
                     └──────────┬───────────┘
                                │ mutation
                                ▼
        ┌───────────────────────────────────────────────────┐
        │  Structure-aware Generator (boundary + gen)        │
        │  + Semantic Knobs + Interaction Knobs               │
        └───────────────────────┬───────────────────────────┘
                                ▼
                ┌───────────────────────────────┐
                │ Program / (akan: File Graph)  │
                └───────────────┬───────────────┘
                                ▼
     ┌──────────────┬───────────┴──────────┬───────────────┐
     ▼              ▼                      ▼               ▼
  preprocessor   parser              elaborator         codegen
     │              │                      │               │
     └──────────────┴──────────┬───────────┴───────────────┘
                                ▼
                        ┌───────────────┐
                        │ Oracle Engine │
                        └───────┬───────┘
                                ▼
        ┌────────────────┬──────┴───────┬─────────────────┐
        ▼                ▼              ▼                 ▼
   Invariant        Reference Model  Icarus        Metamorphic
                                        + Verilator
        └────────────────┴──────┬───────┴─────────────────┘
                                ▼
                        ┌───────────────┐
                        │ Bug Detector  │
                        └───────┬───────┘
                                ▼
                     Reducer · Fingerprint · Coverage
                                ▼
                        Regression Corpus
```

Semua kegagalan oracle menghasilkan artefak dan **wajib** dipromosikan jadi regression test
(AGENTS.md: 1 bug = 1 test).

---

## 2. Empat lapis yang sekarang diimplementasikan

| Lapis | Berkas | Isi |
|---|---|---|
| Generator | `fuzzkit/src/gen.rs`, `fuzzkit/src/boundary.rs` | Bangkitkan program dari byte: deklarasi ber-lebar-batas, statement varied, directive seimbang |
| Invariant | `fuzzkit/src/invariant.rs` | 6 oracle struktural (lihat §4) |
| Pipeline | `fuzzkit/src/pipeline.rs` | SV → IR → Rust dengan `catch_unwind` |
| Coverage | `fuzzkit/src/sem.rs` | 63 fitur semantik sebagai bitset, interaction coverage, sidik jari bug |

### 2.1 Semantic coverage bitmap — 63 fitur

`sv-fuzzkit::sem::Fitur` adalah bitset 128-bit dengan 63 fitur yang
didefinisikan **eksplisit** lewat satu makro, jadi nomor bit dan nama tidak
pernah bisa terpisah (test `setiap_konstanta_fitur_punya_nama_yang_benar` dan
`nomor_bit_unik_dan_kontinu` menjaga ini).

Kelompok fitur:

| Kelompok | Fitur |
|---|---|
| Lebar | `W0` `W1` `W8` `W32` `W64` `W65` `W128` `W129` `W>64` `W-BOUNDARY` |
| Tipe/state | `SIGNED` `UNSIGNED` `4STATE_X` `4STATE_Z` `4STATE_MIXED` `UNPACKED` |
| Constness | `CONSTANT` `PARAMETER` `LOCALPARAM` `GENVAR` `RUNTIME` `UNKNOWN_INDEX` |
| Seleksi | `SELECT_BIT` `SELECT_PART` `SELECT_INDEXED` `SELECT_DYNAMIC` `SELECT_NESTED` `SELECT_ARRAY` `SELECT_OOB` `SELECT_ON_LHS` `ARRAY_INDEX` |
| Statement/scheduler | `NBA` `NBA_MASKED` `BLOCKING` `FOR` `REPEAT` `WHILE` `CASE` `CASEZ` `CASEX` `POSEDGE` `NEGEDGE` `DELAY_ZERO` `DELAY_NONZERO` `TIMESCALE` `GEN_FOR` `GEN_IF` `TASK` `FUNCTION` `CONCAT` `REPLICA` `TERNARY` |
| Cast/reduksi | `CAST_TYPEDEF` `CAST_BUILTIN` `CAST_SIGN` `REDUCTION` `DIVISION` `SHIFT` `MULTIPLIER` |
| **Interaksi** | `LEBAR_MISMATCH` `SIGNEDNESS_MISMATCH` `KONTEKS_LEBIH_LEBAR` `INDEKS_OOB` |

### 2.2 Interaction coverage — bug hidup di interaksi

`Fitur::pasangan()` mengembalikan set pasangan fitur yang sama-sama hadir.
Test menjaga agar pasangan yang paling Fertig dijangkau:

- `W>64 x SIGNED` — truncation + tanda
- `SELECT_PART x NBA_MASKED` — masker di jalur NBA

Skor novelty memberi bobot 3× untuk interaksi baru dibanding fitur baru
(`Coverage::skor_novelty`).

### 2.3 Sidik jari bug — deduplikasi

`sem::sidik_jari(target, pesan, sumber)`mengambil kategori + jumlah baris +
panjang. Satu bug yang ditemukan 100.000× tetap satu bug.

---

## 3. Satu bin, tujuh mode

Hanya ada **satu fuzz target** (`svrust`). Mode dipilih dari byte pertama; sisanya
adalah badan program. Tidak ada bin per mode.

Alasannya bukan sekadar hemat effort — korpus jadi **berbagi**: program yang
penting bagi `elaborate` otomatis dicoba juga oleh `codegen`, `determinisme`, dan
`metamorphic`, sehingga cakupan oracle yang berbeda terakumulasi pada input yang
sama. Dengan bin terpisah, cakupan itu terpecah ke korpus yang tidak saling tahu.

| Mode | Yang diperiksa | Bug class |
|---|---|---|
| 0 `parse` | lexer + parser tidak panic, dari byte mentah maupun program generator | panic, `unwrap()`, indeks di luar batas, shift overflow |
| 1 `preprocess` | directive + makro tidak panic dan tidak eksplosif | rekursi `define`, directive tidak seimbang, ledakan ekspansi |
| 2 `elaborate` | tidak panic; setiap error punya `Span` sah | `Span::dummy()` → `line 1 col 1`; span terbalik; span melewati sumber |
| 3 `codegen` | kurung seimbang; lebar logis ≤ `MAX_WIDTH`; seleksi unsigned | kode gagal dikompilasi; helper baca di luar kapasitas; LRM §11.8.1 |
| 4 `determinisme` | dua kali pipeline → IR identik | `HashMap` diiterasi tanpa pengurutan |
| 5 `hierarki` | multi-file: tidak panic, kurung seimbang, lebar cocok | rekursi tak terbatas; parameter lintas instans |
| 6 `metamorphic` | dead code, konstanta ekuivalen, pembrataan menjaga **perilaku** | optimizer merusak semantik; elaborator sensitif pada bentuk |

`determinisme` hanya berlaku untuk compiler/elaborasi/codegen. **Runtime
scheduling tidak boleh dipaksa menghasilkan trace identik** — hanya harus masuk
*himpunan semantik yang diizinkan*, jadi mode ini sengaja tidak diterapkan ke
runtime.

Konsekuensi yang disadari: satu input hanya menjalankan pekerjaan mode yang
dipilih, sehingga exec/s turun dibanding bin terpisah (≈236 exec/s untuk satu
bin, vs ≈515 untuk mode `hierarki` saja). Itu harga yang sepadan dengan korpus,
build, dan perintah jalannya yang tunggal.

### Pelajaran dari mode `metamorphic`

Versi awal membandingkan **seluruh design** untuk relasi "dead code tidak
mengubah design" — dan langsung gagal pada *setiap* program. Riganya: menambah
`logic` memang menambah satu variabel ke design, jadi sidik jarinya pasti
berubah.

Itu **kelemahan oracle, bukan bug SVRust**. Perbaikannya: yang dibandingkan
adalah `design.processes` — *perilaku*; daftar variabel bukan bagian observasi.

---

## 4. Enam oracle struktural

| ID | Invariant | Asal |
|---|---|---|
| I-1 | Bebas panic di seluruh pipeline | `catch_unwind` |
| I-2 | Pesan error punya `Span` yang sah | AGENTS.md aturan 3 |
| I-3 | Lebar logis sinyal ≤ `MAX_WIDTH` | BUG-13 |
| I-4 | Bit-select/part-select selalu unsigned | LRM §11.8.1, BUG-8 |
| I-5 | Kode Rust hasil codegen punya kurung seimbang | `rustc` gagal tanpa konteks SV |
| I-6 | Pipeline deterministik (compiler, bukan runtime) | urutan proses berubah antar-run |
| I-7 | Instansiasi tidak boleh rekursi tanpa batas | stack overflow dari hierarki bersiklus (LRM §23.1.1) |

**I-3 masih diperluas.** Invariant sekarang hanya mengecek lebar vs `MAX_WIDTH`.
Yang belum tercapai: representasi internal (`bits`/`x_mask`/`z_mask`) harus punya
lebar yang konsisten, `x_mask & z_mask == 0`, dan bit di atas lebar logis harus
nol. Lihat §7.

---

## 5. Taksonomi bug yang sudah terjadi

### 5.1 Panic / crash

| Manifestasi | Contoh |
|---|---|
| **Stack overflow via rekursi** | `a` menginstans `b`, `b` menginstans `a` → `lower_instance` rekursif tanpa henti sampai stack habis. Ditemukan fuzzing target `hierarki` |
| Pengindeksan slice kosong | `&tokens[0..1]` saat `[` terakhir → panic |
| `debug_assert` sebagai crash | `debug_assert_eq!` panic untuk `y[3 5]`; di release token sisa dibuang |
| Shift overflow | `1u64 << lsb` dengan `lsb >= 64` |
| Aritmetika `u32` meluap | indeks `u32::from(bit) + lsb` tanpa `checked_add` |
| `unwrap()` di produksi | konversi tipe yang hanya gagal kalau pemanggilnya bug |
| Rekursi tak terbatas | ekspresi rekursif, `define` rekursif |

### 5.2 Nilai salah tanpa pesan

| Manifestasi | Contoh |
|---|---|
| Truncation ke 64 bit | `to_u64()` pada `case`/`casez` selektor 128 bit |
| Truncation destroying 4-state | `write_masked` memakai `to_u64()` → `a[7:4] = 4'b1x0z` jadi `00000000` |
| Helper dikira parameternya salah | `index_read` menggeser `index * lebar` untuk semua bentuk; bit-select packed harus `index` |
| Penjaga dibandingkan parameter salah | penjaga OOB memakai `MAX_WIDTH` (kapasitas Rust) bukan lebar logis |
| Send early return melewati penjaga | jalur `unpacked` `Select` return sebelum cek `lsb > msb` |
| Token dibuang diam-diam | `y[3:0][1] = ...` diterima lalu `[1]` dibuang |
| Non-konstan dipaksa konstanta | label `case` non-konstan jadi label `0`; `localparam` dipaksa lebar 32 |
| Masker wrap diam-diam di release | `1u64 << 120` → `mem[15] = 8'hBB` mendarat di bit 0 |
| Digit `x`/`z` dihitung 0 | indeks bertanda `x`/`z` menghasilkan angka |

### 5.3 Semantik LRM

| Manifestasi | Contoh |
|---|---|
| Signedness salah | bit-select mewarisi signedness base |
| Wildcard salah jenis | `casez` memperlakukan `x` sebagai wildcard |
| Arah perluasan salah | operand signed di-zero-extend |
| Context-determined salah | operand biner tidak diprioritaskan ke lebar ekspresi |
| Out-of-range diabaikan | seleksi di atas lebar sinyal menghasilkan `0` bukan `X` |
| Elemen array salah | range select pada array unpacked mengabaikan `lsb` |

### 5.4 Pesan dan diagnosis

| Manifestasi | Contoh |
|---|---|
| Pesan tak masuk akal | `a[W-1:0]` gagal dengan `expected ']' closing select` |
| Error internal bocor | `a[b +: 4]` reporting `expected identifier or number` |
| Didiagnosis sebagai fitur lain | `a[3:]` dilaporkan "batas dinamis"; `mem[k] = x` dilaporkan "indeks genvar" |
| Span placeholder | `Span::dummy()` → `line 1 col 1` untuk masalah di baris lain |
| Opsi-CTF | operand bertanda `x`/`z` dianggap indeks konstan bernilai 0 |

### 5.5 Integrasi

| Manifestasi | Contoh |
|---|---|
| Kode gagal dikompilasi | helper dipanggil dengan tipe argumen salah |
| Pipeline tidak tersambung | helper ada tapi belum dipakai codegen |
| Demo tidak mencakup jalur | demo hanya array 4 elemen, jalur di atas bit 63 tidak pernah diuji |

---

## 6. Cara menjalankan

```sh
# build — satu perintah untuk semua mode
cd fuzz
cargo +nightly fuzz build svrust

# jalankan
cargo +nightly fuzz run svrust -- -max_total_time=600 -print_final_stats=1

# hanya satu mode: batasi byte pertama lewat -runs pada seed tertentu, atau
# pakai korpus terpisah per mode bila ingin kecepatan maksimum per mode.
#   seed corpus dari program yang pernah jadi bug
mkdir -p fuzz/corpus/svrust && cp examples/*/rtl/*.sv fuzz/corpus/svrust/

# invariant tanpa sanitizer (mode yang sama, tanpa ASan)
cd fuzzkit && cargo test

# pembanding multimah dengan simulator lain (SVRust vs Icarus vs Verilator)
cd fuzz-diff && cargo run --release -- 1 50
SV_FUZZ_VERILATOR=1 cargo run --release -- 1 10   # lambakan Verilator (lambat)
```

---

## 7. Peta semantic (LRM) dan gap yang harus ditutup

Status per area — apa yang sudah ada fuzzing-nya, apa yang belum:

| Area | Fuzz | Oracle | Status |
|---|---|---|---|
| Lexer / preprocessor | ya | I-1 | **sebagian** — belum ada virtual FS / include graph |
| Parser | ya | I-1, I-2 | **sebagian** — belum ada error-recovery oracle |
| Type / ekspresi | ya | I-4 | **sebagian** — belum ada reference model |
| Select / index | sebagian | I-1, I-3 | **sebagian** — matrix belum lengkap |
| Assignment | sebagian | I-1 | **sebagian** |
| Case | ya | I-1 | **sebagian** — `case` >64 ditolak di elaborasi |
| Scheduling / NBA | ya (invariant) | I-1 | **sebagian** — belum ada event trace |
| Timing | ya | I-1 | **sebagian** |
| Concurrency | belum | — | **kosong** — `fork/join` belum ada |
| Assertion / SVA | belum | — | **kosong** |
| 4-state | ya (invariant) | I-1 | **sebagian** — belum ada truth-table oracle |
| Memory | ya | I-1 | **sebagian** |
| Multi-file / multi-module | ya (`hierarki`) | I-1, I-3, I-7 | **sebagian** — multi-module sudah, filesystem include belum |
| Hierarchy | ya (`hierarki`) | I-7 | **sebagian** — rekursi sudah dijaga, port width mismatch belum |
| Resource / complexity | belum | — | **kosong** |
| Metamorphic | ya (`metamorphic`) | I-1 | **sebagian** — 3 relasi dari 5 |

Takeaway: **fuzzing saat ini menyentuh jalur kompilasi dengan baik, dan jalur
runtime dengan tipis.** Untuk simulator HDL, jalur runtime adalah tempat bug
paling mengerikan yang lolos kompilasi/elaborasi/codegen.

---

## 8. Multi-oracle dan cara triase temuan

Runner `fuzz-diff` menjalankan program yang sama pada SVRust, Icarus, dan
Verilator, lalu **menglasifikasikan** hasilnya. Klasifikasi ini wajib karena
`SVRust != Icarus` **bukan** bukti bahwa SVRust salah.

| Klasifikasi | Arti | Tindak lanjut |
|---|---|---|
| `COCOK` | ketiga simulator sepakat | tidak ada |
| `BUG_SVRUST` | SVRust berbeda dari **kedua** acuan | triase manual, lalu perbaiki |
| `ACAKAN_Iverilog` | SVRust == Verilator, Icarus yang beda | kemungkinan limitasi Icarus, **bukan** bug SVRust |
| `TIGA_BEDA` | ketiganya saling berbeda | triase manual |
| `TIDAK_DIDUKUNG` | simulator acuan menolak program | fitur belum didukung di sana — bukan temuan |
| `TIDAK_CUKUP` | hanya satu simulator yang punya jawaban | program di luar jangkauan pembanding |

Hanya `BUG_SVRUST` dan `TIGA_BEDA` yang boleh masuk daftar bug. Kalau desainnya
`SVRust != Icarus → SVRust bug`, maka setiap keterbatasan Icarus dilaporkan
sebagai bug SVRust — dan kalau SVRust dan Icarus sama-sama salah, fuzzer
menganggapnya benar.

### Prosedur triase (wajib, tidak boleh buta)

Setiap temuan differensial harus melewati urutan ini sebelum ada perubahan kode:

1. **Minimal repro** — potong program jadi kasus terkecil yang masih menunjukkan
   perbedaan.
2. **Simulator acuan lain** — apakah kedua simulator lain berbeda? Kalau Icarus
   beda tapi Verilator setuju dengan SVRust, itu limitasi Icarus.
3. **Cek LRM** — apakah standard benar-benar menyatakan begitu? Kalau tidak,
   temuan itu bukan bug.
4. **Ukur dampak** — berapa test Existing yang bergantung pada perilaku lama?
   Ini menentukan apakah ini perbaikan semantik atau perubahan Opportunis.
5. **Cek demo yang gagal** — sering kali ekspektasi demo justru *mengunci bug
   lama*. Kalau begitu, perbaiki ekspektasinya **dan tulis alasannya**, jangan
   dihapus diam-diam.

Contoh nyata dari tahap ini:

- `assign y[3:0] = ...` → Icarus `fx`, SVRust `f0`. Icarus tidak
  mengpropagasi part-select pada LHS continuous assign. **Bukan bug SVRust.**
- `output [7:0] z` tanpa `logic` → Icarus `zZ`, SVRust `xx`. LRM §6.5: itu
  *net*, dan net yang belum drove bernilai `Z`. **Divergensi nyata**, dicatat
  di `LRM_COMPLIANCE.md`, bukan ditutup.
- Relasi metamorphic "dead code" versi pertama gagal pada semua program.
  **Kelemahan oracle**, bukan bug SVRust —详见 §3.

### P1

Timing/event trace · concurrency (`fork/join`, `wait`) · dedicated NBA ·
preprocessor virtual FS · invalid-syntax (mutation) · error-recovery ·
IR-level · scheduler stress (N proses × M sinyal bersama)

### P2

LRM coverage database · coverage-guided seed ranking · long-running campaign ·
mutation OpenTitan · auto regression promotion · cross-version differential

---

## 9. Prinsip yang tidak boleh dilanggar

1. **Nol template program.** Tapi nilai batas wajib dan berbobot.
2. **Byte memilih kombinasi, bukan isi.** State semantik adalah target coverage.
3. **Satu bug = satu regression test.** Artifact dipromosikan, lalu boleh dihapus.
4. **Taksonomi §5 adalah daftar hidup.** Bug baru menambah baris di sana, dan
   biasanya menambah invariant baru di `invariant.rs` — supaya kelas yang sama
   tidak bisa lolos dua kali.
5. **Jangan pernah menyalahkan SVRust hanya karena beda dengan satu simulator.**
   Sifatnya: Icarus bukan kebenaran. Selalu triase dulu.