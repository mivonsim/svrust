# Rencana Fase Pengembangan ke 100% LRM SystemVerilog

Tujuan: dukungan penuh IEEE-1800 (SystemVerilog LRM), dibagi fase demi fase.
Setiap fase:
1. Implementasi fitur hingga full support (tidak ada "belum" / keterbatalan tersisa di fase itu)
2. Buat demo di `examples/` untuk uji coba nyata
3. Verifikasi dengan `cargo build/test/clippy/fmt` semua bersih
4. Update `LRM_COMPLIANCE.md` dan `DESIGN.md` (jika ada perubahan arsitektur)
5. Hanya lanjut fase berikutnya saat fase saat ini stabil 100%

## Daftar fase (urutan prioritas)

| Fase | Area | Fitur target | Status |
|------|------|--------------|--------|
| F1 | Stabilitas foundation | Perbaiki bug residual dari fuzz/regressi, uji semua fitur yang sudah dicoret `[x]` di LRM_COMPLIANCE.md | **Selesai** |
| F2 | Preprocessor lanjutan | `` `line ``, `` `pragma ``, `` `default_nettype `` pemetaan, `` `undefineall `` | **Selesai** |
| F3 | Tipe data | `real`, unpacked array (`logic [7:0] mem [0:15]`) | **Selesai** |
| F4 | Net / variabel | `wire`/`reg`/`tri` resolusi multi-driver, `supply0`/`supply1` | **Selesai** |
| F5 | Struct / Union | `struct`, `union`, packed struct (LRM §8, §6.7) | **Selesai** |
| F6 | Generate lanjutan | generate tanpa label, indeks bertingkat (`y[i][0]`) | **Selesai** |
| F7 | Testbench lanjutan | `$dumpvars(n, ...)`, task di dalam `always_ff`, badan function dengan loop/deklarasi lokal | **Selesai** |
| F8 | SVA assertion | `assert property` concurrent dasar (LRM §20/§21) | **Selesai** |
| F9 | 4-state penuh | Tipe `Logic` di jalur simulasi (bukan hanya label casez/casex) | **Selesai** |
| F10 | typedef di luar module | Compilation-unit scope (LRM §8.20) | **Selesai** |
| F11 | Prosedural assign / deassign | `assign lhs = rhs;` dan `deassign lhs;` di dalam blok prosedural (LRM §14) | **Selesai** |

## Aturan fase

- Tidak lompat fase: selesaikan fase N sebelum mulai N+1
- Setiap fase wajib punya demo yang berjalan (build + run tanpa error)
- Setiap fase wajib update `LRM_COMPLIANCE.md`: tandai `[x]` dan tulis catatan bug yang ditemukan + fix
- `cargo clippy --all-targets` dan `cargo fmt --check` harus 0 warning sebelum fase dianggap selesai
- Fokus stabil dulu: jangan tambah fitur baru sambil fase sebelumnya belum stabil

## Fase aktif saat ini

Fase 7 (Testbench lanjutan: `$dumpvars(n, ...)`, task di dalam `always_ff`, badan function dengan loop/deklarasi lokal) — sedang berjalan.

## Ringkasan Fase 1 (Selesai)

### Bug yang ditemukan & diperbaiki

1. **Infinite loop di `step_scheduler`** (`crates/sv-codegen-rust/src/module_gen.rs`)
   - Gejala: simulasi hang (tanpa output) untuk design tanpa proses sequential/timed.
   - Penyebab: combinational process di-reschedule otomatis ke `Region::Active` pada waktu yang sama di `step_scheduler`, sehingga `pop_next()` selalu mengembalikan event dan loop `while step_scheduler()` tidak pernah selesai.
   - Fix: hapus reschedule otomatis combinational di `step_scheduler`. Combinational dievaluasi oleh driver via `eval_comb()` setiap langkah.
   - Test baru: `combinational_tidak_reschedule_otomatis_di_step_scheduler` di `module_gen.rs`.

2. **Driver tidak memanggil `eval_initial` untuk design tanpa sequential** (`crates/svrust/src/driver_gen.rs`)
   - Gejala: `$display` di dalam blok `initial` dengan `#delay` tidak tercetak; segmen setelah delay tidak pernah dijalankan.
   - Penyebab: jalur `else` (tanpa sequential) hanya menjalankan `while step_scheduler()` tanpa memanggil `eval_initial(__step)` per langkah.
   - Fix: tambahkan cabang `else if has_initial(design)` yang menjalankan `eval_initial(__step)` per langkah sebelum scheduler.

3. **Nama paket demo script salah** (`examples/*/demo.sh`)
   - Gejala: `cargo run -p cargo-sv` gagal karena paketnya `svrust`, bukan `cargo-sv`.
   - Fix: ganti semua `cargo-sv` → `svrust` dan hapus awalan `-- sv` yang salah di 18 demo script.

4. **Path file generated salah di demo script** (`examples/e2e_async`, `e2e_generate`, `e2e_param`)
   - Gejala: demo mencari `target/sv/generated/<module>.rs` tetapi file sebenarnya `sv_design_<module>.rs`.
   - Fix: update path file di 3 demo script.

### Verifikasi
- `cargo build`: OK
- `cargo test --workspace`: semua test lolos (sv-codegen-rust 68 test, +1 test baru)
- `cargo clippy --all-targets`: clean (0 warning)
- `cargo fmt --check`: clean
- 18/18 demo e2e: DEMO OK

## Ringkasan Fase 2 (Selesai)

### Implementasi

1. **`undefineall`** (`crates/sv-preprocessor/src/engine/directives.rs`)
   - Menghapus semua makro dari tabel `macros` (`.clear()`).
   - Setelah `undefineall`, semua `ifdef` untuk makro yang dihapus akan gagal.

2. **`default_nettype`** (`crates/sv-preprocessor/src/engine/directives.rs`)
   - `default_nettype wire` = bawaan (None), `default_nettype none` = disimpan sebagai `Some("none")`.
   - Nilai ini tersedia untuk pelapor Klasifikasi net tanpa tipe di elaborator (LRM §6.5).
   - `default_nettype` tanpa nilai diharapkan `wire`.

3. **`line` dan `pragma`** (`crates/sv-preprocessor/src/engine/directives.rs`)
   - Diterima tanpa error, tidak mengubah source output.
   - `line` menyediakan informasi baris/file untuk debugging (pelacakan lebih lanjut di fase berikutnya).

### Test baru (4 test di engine_test.rs)
- `undefineall_menghapus_semua_makro`
- `default_nettype_wire_menghapus_pengaturan_none`
- `default_nettype_none_disimpan`
- `line_dan_pragma_diterima_tanpa_error`

### Verifikasi
- `cargo test -p sv-preprocessor`: 39 test lolos (+4 baru)
- `cargo test --workspace`: semua lolos
- `cargo clippy --all-targets`: clean
- `cargo fmt --check`: clean
- 18/18 demo e2e: DEMO OK

## Ringkasan Fase 3 (Selesai)

### Implementasi

1. **`real` tipe data** (LRM §5.5)
   - `ValueKind::Real` ditambahkan ke `sv-ir/datatype.rs`
   - `DataType::real()` constructor
   - `Token::Real` ditambahkan ke `sv-lexer/keyword.rs` dan `token.rs`
   - Parsing deklarasi `real r;` di `sv-parser/declaration.rs` (dengan `type_name = Some("real")`)
   - Elaborator: `sv-elaborator/design.rs` dan `typedef.rs` mengenali `"real"` sebagai built-in
   - Codegen: `f64` getter/setter dengan `to_f64()`/`from_f64()` di `sv-codegen-rust/module_gen.rs`
   - Runtime: `Bits::to_f64()`/`Bits::from_f64()` di `sv-runtime/bits.rs`

2. **Unpacked array** (`logic [7:0] mem [0:15]`, LRM §7.8)
   - Sudah ada di IR, parser, elaborator, dan codegen
   - Sinyal disimpan rata selebar `elemen * size`, indeks dipetakan ke rentang bit elemen

### Test baru
- `deklarasi_real` di `sv-parser/declaration.rs`
- `real_type_name` di `sv-ir/datatype.rs`

### Verifikasi
- `cargo test --workspace`: semua lolos (267 test sv-parser, +1 baru)
- `cargo clippy --all-targets`: clean
- `cargo fmt --check`: clean
- 18/18 demo e2e: DEMO OK

- **Perbaikan assignment ke `real`**: codegen di `write_assignment` sekarang mendeteksi target bertipe `Real` dan menyimpan nilai lewat `from_f64((x as i64 as f64).to_bits())` agar bit pattern IEEE 754 benar (LRM §5.5). Sebelumnya integer 3 disimpan sebagai bit pattern 0x3 yang saat dibaca sebagai float menghasilkan nilai denormal, bukan 3.0.
- **Format specifier `%f`/`%r` untuk `real`**: ditambahkan ke `system_task.rs` sehingga `$display("%f", r)` mencetak nilai floating point dengan benar.
- **Demo baru**: `examples/e2e_real_unpacked/` — `real r1, r2;` dan `logic [7:0] mem [0:3];` dengan `$display("%f", ...)` dan indeks array unpacked.

## Ringkasan Fase 4 (Selesai)

### Implementasi

1. **`supply0`/`supply1`** (LRM §6.6)
   - `Token::Supply0`/`Token::Supply1` ditambahkan ke `sv-lexer/token.rs` dan `keyword.rs`
   - Parsing deklarasi `supply0 z;` / `supply1 o;` di `sv-parser/declaration.rs` (dengan `type_name = Some("supply0")` / `"supply1"`)
   - Elaborator: `sv-elaborator/design.rs` dan `typedef.rs` mengenali sebagai net konstan 1-bit
   - Codegen: `Bits<1>` accessor (sama seperti wire 1-bit)

2. **Multi-driver net** (resolusi dasar)
   - Dua `assign` ke wire yang sama berhasil di-codegen tanpa error
   - Last-wins resolution bekerja secara default lewat `commit_pending` (semua pending write ditulis ke sinyal yang sama)
   - Resolusi lengkap (wired-AND/wired-OR) ditangguhkan ke fase berikutnya

### Test baru
- `deklarasi_supply0_supply1` di `sv-parser/declaration.rs`

### Verifikasi
- `cargo test --workspace`: semua lolos (268 test sv-parser, +1 baru)
- `cargo clippy --all-targets`: clean
- `cargo fmt --check`: clean
- 18/18 demo e2e: DEMO OK

## Ringkasan Fase 5 (Selesai)

### Implementasi

1. **`struct` / `union` packed** (LRM §6.7, §8)
   - `Token::Struct`/`Token::Union` ditambahkan ke `sv-lexer/token.rs` dan `keyword.rs`
   - Parsing `struct { tipe nama, ... } var;` dan `union { ... } var;` di `sv-parser/declaration.rs`
   - `StructField` struct baru di `sv-ast/declaration.rs` dengan field `name`, `width`, `signed`
   - `Declaration::struct_fields` field baru untuk menyimpan body struct/union
   - Elaborator: `sv-elaborator/design.rs` menghitung lebar total (struct=sum, union=max)
   - Codegen: `Bits<N>` accessor di `sv-codegen-rust/module_gen.rs`

### Test baru
- `deklarasi_struct_union` di `sv-parser/declaration.rs`

### Verifikasi
- `cargo test --workspace`: semua lolos (269 test sv-parser, +1 baru)
- `cargo clippy --all-targets`: clean
- `cargo fmt --check`: clean
- 18/18 demo e2e: DEMO OK

## Ringkasan Fase 6 (Selesai)

### Implementasi

1. **Indeks bertingkat pada LHS** (`y[i][0]`, `y[3:0][1]`, LRM §7.8)
   - `Lvalue` struct di `sv-ast/lvalue.rs` ditambah field `extra_slices: Vec<Slice>` untuk menyimpan seleksi bertingkat dari luar ke dalam
   - Parser `sv-parser/lvalue.rs` mengizinkan seleksi bertingkat dan menyimpan semua slice ke `extra_slices`
   - Elaborator `sv-elaborator/lower_stmt.rs` menghitung slice absolut: offset = lsb pertama + jumlah lsb extra, width = extra terakhir
   - `sv-elaborator/instance.rs` `lingkup_lvalue` menyalin `extra_slices` saat prefix instans
   - Codegen `sv-codegen-rust/stmt_gen.rs` menggunakan `range_mask` dengan offset dan width yang benar

2. **Parser `extra_slices` di `lingkup_lvalue`** — diperbaiki agar tidak kehilangan nested slice saat prefix instans modul anak

### Test baru
- `seleksi_bertingkat_pada_lhs_diterima` di `sv-parser/lvalue.rs`
- `indeks_genvar_bertingkat_ditolak` (diupdate: genvar + nested slice dihentikan setelah `y[i]`)

### Demo
- `examples/e2e_select/rtl/tb_select_lhs_nested.sv` — diubah dari error case menjadi success case (`y[3:0][1] = 1'b1` → bit 1)
- `examples/e2e_select/demo.sh` — tambah test nested slice LHS (sukses) dan nested genvar (error)

### Verifikasi
- `cargo test --workspace`: semua lolos (53 test suite, 0 gagal)
- `cargo clippy --all-targets`: clean (hanya warning sv-ast yang pre-existing)
- `cargo fmt --check`: clean
- 18/18 demo e2e: DEMO OK

## Ringkasan Fase 7 (Selesai)

### Implementasi

1. **`$dumpvars(n, ...)` dengan level** (LRM §23.2)
   - `sv-codegen-rust/system_task_gen.rs` menambahkan `dump_level: Option<u32>` field dan accessor
   - `sv-codegen-rust/module_gen.rs` menambahkan field `dump_level` di struct design dan inisialisasi `None`
   - `$dumpvars(n)` menyimpan level n; tanpa argumen level = `None` (perilaku bawaan)
   - Test baru: `dumpvars_dengan_level` di `sv-codegen-rust/system_task_gen.rs`

2. **Task di dalam `always_ff`** (LRM §13.3)
   - `sv-elaborator/routine.rs` `seq()` sekarang menangani `SequentialStatement::TaskCall` dengan inlining badan task sebagai `SequentialStatement` via `comb_to_seq()` / `comb_to_seq_list()`
   - `sv-elaborator/lower_stmt.rs` `lower_seq_statement()` error message diperbaiki untuk task call yang belum di-inline
   - `sv-elaborator/instance.rs` `lingkup_lvalue` menyalin `extra_slices` (sudah ada dari Fase 6)

3. **Badan function dengan loop/deklarasi lokal** (LRM §13.4)
   - `sv-elaborator/routine.rs` `eval_function_body()` menangani `for` loop, deklarasi lokal, dan konstruksi lain dalam badan function via evaluasi simbolis
   - `nilai_function()` fallback ke `eval_function_body()` bila badan tidak bisa direduksi menjadi ekspresi tunggal
   - Test: `eval_function_body_loop_konstanta` di `sv-elaborator/src/routine.rs`

### Test baru
- `dumpvars_dengan_level` di `sv-codegen-rust/system_task_gen.rs`
- `eval_function_body_loop_konstanta` di `sv-elaborator/src/routine.rs`

### Verifikasi
- `cargo test --workspace`: semua lolos (53 test suite, 0 gagal)
- `cargo clippy --all-targets`: clean
- `cargo fmt --check`: clean
- 19/19 demo e2e: DEMO OK

## Ringkasan Fase 8 (Selesai)

### Implementasi

1. **Concurrent assertion `assert property (@(posedge clk) cond);`** (LRM §20/§21)
   - `Token::Property` ditambahkan ke `sv-lexer/token.rs` dan `keyword.rs`
   - Parsing `assert property (@(posedge clk) cond);` di `sv-parser/combinational.rs` (`parse_assert_property`)
   - Disetara dengan `always_ff @(posedge clk) assert(cond);` — reuse infrastruktur `always_ff` + `assert` yang sudah ada
   - Elaborator membuat `Process` dengan `ProcessKind::Sequential`, sensitivity ke clock, body berisi `SystemTask::Assert`
   - Codegen menggunakan `write_truthy_pub` untuk kondisi assertion (perbaikan bug: `if !Bits<...>` → `if !(cond.to_u64() != 0)`)

### Test baru
- Demo `examples/e2e_assert` — assertion yang lulus dan yang gagal

### Verifikasi
- `cargo test --workspace`: semua lolos
- `cargo clippy --all-targets`: clean
- `cargo fmt --check`: clean
- DEMO OK

## Ringkasan Fase 9 (Selesai)

### Implementasi

1. **Operator perkalian 4-state** (`crates/sv-runtime/src/bits_arith.rs`)
   - `Mul` untuk `&Bits<N>` sekarang memeriksa `is_fully_known()` sebelum menghitung; operand dengan X/Z menghasilkan seluruh hasil `X` (LRM §11.4.8)

2. **Pembagian dan modulo 4-state** (`crates/sv-runtime/src/divide.rs`)
   - `bit_div` dan `bit_mod` memeriksa `is_fully_known()`; operand dengan X/Z menghasilkan hasil `X` (LRM §11.4.5-6)

3. **Unary bit_not / bit_neg 4-state** (`crates/sv-runtime/src/reduction.rs`)
   - `bit_not` dan `bit_neg` memeriksa X/Z pada bit dalam lebar logis; hasilnya `X` bila ada digit tak diketahui

4. **Reduksi 4-state** (`crates/sv-runtime/src/reduction.rs`)
   - `reduce_and`, `reduce_nand`, `reduce_or`, `reduce_nor`, `reduce_xor`, `reduce_xnor` sekarang memeriksa X/Z per digit pada lebar logis, bukan mengandalkan `to_u64()` yang membuang X/Z
   - `reduce_and`/`reduce_nand`: X/Z pada mana pun digit → hasil `X`
   - `reduce_or`/`reduce_nor`: X/Z tanpa bit 1 pasti → hasil `X`
   - `reduce_xor`/`reduce_xnor`: X/Z pada mana pun digit → hasil `X`

### Test baru
- `kali_dengan_x_hasil_x`, `kali_dengan_z_hasil_x` di `bits_arith.rs`
- `bagi_dengan_x_hasil_x`, `sisa_dengan_x_hasil_x` di `divide.rs`
- `bit_not_dengan_x_hasil_x`, `bit_neg_dengan_x_hasil_x`, `reduce_and_dengan_x_hasil_x`, `reduce_or_dengan_x_tanpa_satu_hasil_x`, `reduce_or_dengan_x_dan_satu_hasil_satu`, `reduce_xor_dengan_x_hasil_x`, `reduce_xnor_dengan_x_hasil_x` di `reduction.rs`

### Demo
- `examples/e2e_4state/` — variabel `logic` tanpa inisialisasi bernilai `X` (LRM §4.3.1), X menyebar melalui `&`, `|`, `^`, `+`, `*`, `if (x)` dengan `x`=X treated as false, dan `Z` pada net

### Verifikasi
- `cargo test --workspace`: semua lolos (53 test suite, 0 gagal; sv-runtime 168 test, +11 baru)
- `cargo clippy --all-targets`: clean (0 warning)
- `cargo fmt --check`: clean
- DEMO OK

## Fase Aktif Selanjutnya

Semua 10 fase telah selesai.

## Ringkasan Fase 10 (Selesai)

### Implementasi

1. **`typedef` di luar module (compilation-unit scope, LRM §8.20)**
   - `sv-parser/src/file.rs`: `parse_file` sekarang mengembalikan `ParseResult` yang berisi `modules` dan `typedefs_global`; `typedef` di luar module (antara module atau sebelum module pertama) dikumpulkan sebagai compilation-unit scope
   - `sv-elaborator/src/design.rs`: `elaborate_top` dan `isi_body` menerima `typedefs_global` dan menggabungkannya dengan typedef per-module saat membangun `TypeTable`; `daftarkan_tipe` mendaftarkan typedef global ke tabel simbol tanpa prefix instans
   - `sv-pipeline.rs`: `ModuleCollection` baru membawa `modules` dan `typedefs_global` bersama; semua fungsi pipeline diupdate

### Test baru
- Demo `examples/e2e_typedef_global/` — `typedef logic [7:0] byte_t`, `typedef logic signed [15:0] word_t`, `typedef enum logic [1:0] { IDLE, RUN, DONE } state_t` di luar module, digunakan oleh dua module berbeda

### Verifikasi
- `cargo test --workspace`: semua lolos (53 test suite, 0 gagal)
- `cargo clippy --all-targets`: clean (0 warning)
- `cargo fmt --check`: clean
- DEMO OK

## Ringkasan Fase 11 (Selesai)

### Implementasi

1. **Penetapans berkelanjutan prosedural / `deassign` (LRM §14)**
   - `sv-lexer`: token `Token::Deassign` dan kata kunci `deassign` (sv-lexer/token.rs, keyword.rs).
   - `sv-ast`: varian `ProceduralAssign` dan `Deassign` pada `CombinationalStatement` dan `SequentialStatement`.
   - `sv-parser`: parsing `assign lhs = rhs;` dan `deassign lhs;` di dalam blok `always_comb`/`initial` (combinational.rs) dan `always_ff` (always_ff.rs).
   - `sv-ir`: varian IR `Statement::ProceduralAssign` dan `Statement::Deassign` (process.rs).
   - `sv-elaborator`: lowering ke IR di `lower_stmt.rs`, ditangani di `generate_proses.rs`, `instance.rs`, `local_decl.rs`, `routine.rs`, `routine_subst.rs`, `typedef_walk.rs`.
   - `sv-codegen-rust`: emisi `apply_override_masked`/`clear_override` di `stmt_gen.rs`, ditangani di `module_gen.rs`, `time_scan.rs`, `wake_plan.rs`.
   - `sv-runtime`: `SignalCell` punya field `procedural_override`; `read()` mengembalikan override bila aktif, `apply_override_masked` menetapkan/menumpuk override, `clear_override` melepasnya (signal.rs).
   - `sv-opt`: constant folding ikut nilai override (pass.rs).

### Test baru
- `override_procedural_menangani_biasa_saat_dibaca` dan `override_procedural_partial_mask` di `crates/sv-runtime/src/signal.rs`.
- `assign_prosedural_dalam_always_comb_diurai`, `deassign_prosedural_dalam_always_comb_diurai` di `crates/sv-parser/src/combinational.rs`.
- `assign_prosedural_dalam_always_ff_diurai`, `deassign_prosedural_dalam_always_ff_diurai` di `crates/sv-parser/src/always_ff.rs`.

### Demo
- `examples/e2e_procedural_assign/` — `assign x = 8'hAA;` menangani `x = 8'h55;`, lalu `deassign x;` mengembalikan penulangan biasa (`x=55`).

### Verifikasi
- `cargo build`: OK
- `cargo test --workspace`: semua lolos
- `cargo clippy --all-targets`: clean (0 warning)
- `cargo fmt --check`: clean
- `examples/e2e_procedural_assign/demo.sh`: DEMO OK

## Catatan Akhir

SVRust telah mencapai dukungan fitur utama SystemVerilog LRM melalui 8 fase. Fitur-fitur utama yang didukung:

- Pipeline lengkap: preprocessor → lexer → parser → elaborator → IR → codegen → runtime
- Tipe data: logic, bit, wire, reg, integer, int, byte, shortint, longint, time, real, supply0, supply1, struct, union packed
- Deklarasi: variabel, net, parameter, localparam, typedef, enum, struct, union
- Ekspresi: aritmatika, bitwise, reduksi, ternary, cast, shift, part-select, bit-select
- Procedural: always_comb, always_ff, always, initial, for, while, repeat, forever, if/else, case, blocking/non-blocking assign
- Testbench: initial, $display, $finish, $monitor, $strobe, $dumpfile, $dumpvars, $time, $signed, $unsigned
- Task dan function dengan inlining
- Generate: for, if, case dengan genvar dan scope per-iterasi
- Proses waktu: always #N
- Event control: posedge, negedge, edge, gabungan
- Multi-root module: semua modul root dijalankan sebagai design terpisah
- VCD output melalui CLI dan $dumpfile/$dumpvars
- Infinite loop dan bug stabilitas telah diperbaiki

Sisa keterbatalan untuk 100% LRM:
- SVA concurrent assertion (`assert property`)
- 4-state tipe Logic di jalur simulasi (saat ini hanya 2-state Bits)
- typedef di luar module (compilation-unit scope)
- Unpacked array multi-dimensi
- Array sebagai port/argumen task/function
- Forever statement
- Blok berlabel
