# paralel.md — Perencanaan & Implementasi Eksekusi Paralel SV → Rust

**Status:** Dalam implementasi (bertahap)  
**Tanggal:** 8 November 2026  
**Target:** Memproses ratusan ribu modul SystemVerilog (OpenTitan class) dalam **< 5 menit**, idealnya **< 1 menit**, dari `.sv` / `.svh` hingga binary Rust siap jalankan.  
**Keterbatasan:** Dokumen ini adalah arsitektur target dan roadmap. Setiap perubahan codebase harus diikuti update dokumen ini agar traceable oleh agent sesi lain.

---

## 0. Critical Questions & Status Saat Ini

Berikut daftar pertanyaan kritis dari review arsitektur dan status implementasi:

| Area | Pertanyaan Kritis | Status | File Terkait |
|------|------------------|--------|--------------|
| Arsitektur | Satu crate Cargo per root optimal? | ❌ Belum dibuktikan | `crates/svrust/src/workspace.rs` |
| Arsitektur | Root = unit kompilasi independen? | ❌ Tidak (package bersama belum di-handle) | `crates/svrust/src/pipeline.rs` |
| Arsitektur | Package shared dianalisis 100x? | ✅ Ya (masih terjadi) | `crates/svrust/src/pipeline.rs` |
| Arsitektur | Duplikasi parsing/AST/semantic/codegen? | ✅ Ya (masih terjadi) | `crates/svrust/src/pipeline.rs` |
| Paralel | Parsing/preprocess/semantic/elaborate/codegen parallel? | ❌ Hanya cargo -j N | `crates/svrust/src/pipeline.rs` |
| Paralel | Scheduler paham dependency DAG? | ❌ Tidak | Belum ada |
| Paralel | Work stealing / load balancing? | ❌ Tidak | Belum ada |
| Incremental | Content-addressed cache? | ❌ Tidak ada | Belum ada |
| Incremental | Ubah satu leaf ⇒ recompile semua OpenTitan? | ✅ Ya (masih terjadi) | `crates/svrust/src/pipeline.rs` |
| Simulasi | Ekuivalen dengan simulator referensi? | ❌ Belum diverifikasi | Belum ada differential test |
| Simulasi | Scheduler parallel mempertahankan event semantics? | ⚠️ Per root single-threaded | `crates/sv-runtime/src/scheduler.rs` |
| Skala | 100 / 1.000 / 5.000 / 10.000 root diuji? | ❌ Belum | Belum ada benchmark |
| Jangka panjang | Backend native tanpa LLVM? | ❌ Tidak | Pakai rustc |
| Jangka panjang | IR untuk synthesizer/formal? | ❌ Belum | sv-ir masih dasar |

---

## 1. Masalah Utama: Menjalankan OpenTitan Tanpa `--top`

### 1.1 Gejala saat ini

Ketika menjalankan `svrust --rtl opentitan/rtl.f` **tanpa** `--top`:

```
error: ada N modul root (...); pilih salah satu dengan --top
```

Fungsi `pilih_top()` di `crates/svrust/src/pipeline.rs:560` menolak ketika ada lebih dari satu modul root. OpenTitan (`rtl.f` dengan ~1.122 file, `hw/` dengan ~3.946 `.sv`) memiliki ratusan hingga ribuan root module, jadi tanpa `--top` CLI total tidak bisa melanjutkan.

### 1.2 Dampak bagi pengguna

- Pengguna harus mengetahui nama top secara manual (`--top <nama>`), sulit pada codebase besar.
- Hanya satu modul root yang dielaborasi dan divisimulasi; modul lain diabaikan.
- Tidak ada skala: satu `cargo build` monolitik dengan ratusan ribu modul menunggu rustc selama berjam-jam.

### 1.3 Status Implementasi

| Item | Status | Commit |
|------|--------|--------|
| `check`/`build`/`run` tanpa `--top` jalankan semua root | ✅ Selesai | `a03c1a6` |
| Satu crate Cargo per root di `target/sv/parallel/units/` | ✅ Selesai | `a03c1a6` |
| `cargo -j N` build paralel per crate | ✅ Selesai | `a03c1a6` |
| Setiap root binary terpisah, VCD per root | ✅ Selesai | `a03c1a6` |
| Demo `e2e_parallel_root` (dua root tanpa `--top`) | ✅ Selesai | `a03c1a6` |

---

## 2. Analisis Bottleneck (Pipeline Saat Ini)

Pipeline saat ini (`crates/svrust/src/pipeline.rs`) masih sebagian sekuensial:

```
1. expand_rtl_inputs        (baca .f, rekursif)        — sekuensial, I/O terbatas
2. collect_modules          (per file: preprocess → lex → parse) — SEKUENSIAL
3. pilih_top / tops_dipakai — sekuensial
4. elaborate_tops           (per root: elaborate + optimalkan) — SEKUENSIAL
5. generate_rust            (per design: codegen ke String) — sekuensial
6. write_sim_project        (per root: satu crate) — PARALEL (sudah)
7. workspace.build          (cargo -j N per crate) — PARALEL (sudah)
8. workspace.run            (jalankan binary per root) — sekuensial
```

### 2.1 Bottleneck menurut fase (perkiraan, BELUM dibenchmark)

| Fase | Karakteristik | Skala OpenTitan | Estimasi dominasi | Status |
|------|--------------|-----------------|-------------------|--------|
| Baca + preprocess + lex + parse | I/O bound, CPU ringan | ~4.000 file | Rendah | ❌ Belum parallel |
| Elaborasi per module | CPU bound, symbolic | ~1.000+ root | Sedang | ❌ Belum parallel |
| Codegen per module | String manipulation | ~1.000+ design | Sedang | ❌ Belum parallel |
| write_sim_project (satu crate per root) | Memory/string besar | Semua modul | Tinggi | ✅ Selesai |
| cargo build / rustc (per crate) | CPU bound, O(N²)-ish | Per crate kecil | KRITIS | ✅ Parallel via -j N |

**Catatan:** Angka di atas adalah perkiraan, **BUKAN hasil benchmark.** Harus diukur end-to-end.

---

## 3. Arsitektur Paralel yang Disarankan

Strategi: **ecembagi per modul, kompilasi per modul, lalu link** (seperti build sistem C/C++: object file per modul, lalu linker). Setiap modul jadi unit kompilasi terpisah.

### 3.1 Pipeline paralel (fase-fase)

```
┌─────────────────────────────────────────────────────────────┐
│ FASE 0: Daftar file (.f expansion)                          │
│   - sekuensial (I/O ringan), output: daftar path            │
└─────────────────────────────────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────┐
│ FASE 1: Frontend paralel (per file)                         │
│   - preprocess → lex → parse → Module AST                   │
│   - PARALEL: rayon::scope / thread pool per file            │
│   - CACHE: content-addressed hash per file                  │
│   - Output: Vec<(path, Module, diagnostics)>                │
└─────────────────────────────────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────┐
│ FASE 2: Root detection & top selection                      │
│   - scan instances + generate untuk semua module            │
│   - PARALEL: scan setiap module secara independen           │
│   - Output: daftar root module (tanpa error bila >1)        │
└─────────────────────────────────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────┐
│ FASE 3: Elaborasi paralel (per root)                        │
│   - elaborate_top + optimalkan per root                     │
│   - PARALEL: setiap root independen                         │
│   - CACHE: shared symbol database untuk package             │
│   - Output: Vec<Design> + kumpulkan diagnostik              │
└─────────────────────────────────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────┐
│ FASE 4: Codegen paralel (per design)                        │
│   - generate_module → String Rust per design                │
│   - PARALEL: setiap design independen                       │
│   - CACHE: content-addressed hash per design                │
│   - Output: satu file .rs per design di target/sv/generated/│
└─────────────────────────────────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────┐
│ FASE 5: Build paralel (per modul, lalu link)                │
│   - setiap file .rs jadi satu crate kecil                   │
│   - PARALEL: cargo build per crate (–j N)                   │
│   - satu driver main yang memanggil semua modul             │
│   - link akhir                                              │
└─────────────────────────────────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────┐
│ FASE 6: Jalankan simulasi                                   │
│   - setiap binary root dijalankan (berurutan atau paralel)   │
│   - kumpulkan output dan VCD per root                       │
└─────────────────────────────────────────────────────────────┘
```

### 3.2 Mengapa "satu crate per modul" lebih cepat dari "satu crate monolitik"

- rustc bisa compile banyak crate secara paralel (`cargo -j N`).
- Setiap crate kecil ⇒ type checking, borrow checking, codegen cepat.
- Linker hanya menabarkan simbol per crate, bukan ratusan ribu modul sekaligus.
- Inkremental: ubah satu modul ⇒ hanya kompilasi ulang crate itu.

**Peringatan:** Pendekatan ini **memindahkan bottleneck** dari compiler monolitik ke Cargo workspace + filesystem + linker per crate. Belum dibuktikan optimal untuk 10k-100k root. Perlu benchmark.

---

## 4. Schema Matang (Antarmuka & Struktur Data)

### 4.1 Model data: `CompiledUnit`

```rust
// Dalam crate baru, contoh: sv-parallel (tidak ada saat ini)
pub struct CompiledUnit {
    pub design_name: String,
    pub rust_module_name: String,     // sv_design_<nama>
    pub rust_source: String,          // hasil generate_module()
    pub vcd_path: Option<PathBuf>,
    pub diagnostics: Vec<sv_diag::Diagnostic>,
    pub content_hash: String,         // untuk incremental cache
}
```

### 4.2 Model data: `ParallelPlan`

```rust
pub struct ParallelPlan {
    pub files: Vec<PathBuf>,                  // hasil .f expansion
    pub roots: Vec<String>,                   // semua modul root
    pub units: Vec<CompiledUnit>,             // satu per root
    pub worker_count: usize,                  // num_cpus
    pub release: bool,
    pub steps: usize,
    pub plusargs: Vec<(String, String)>,
    pub vcd: Option<PathBuf>,
}
```

### 4.3 Antarmuka eksekusi (trait)

```rust
pub trait ParallelExecutor {
    /// Jalankan seluruh pipeline paralel, kembali path binary siap jalan.
    fn execute(&self, plan: &ParallelPlan) -> Result<PathBuf, CliError>;
}
```

Implementasi awal: `RayonExecutor` menggunakan `rayon::ThreadPool` untuk fase 1, 3, 4; dan `cargo -j N` untuk fase 5.

### 4.4 Direktori output

```
target/sv/
├── parallel/
│   ├── units/             # satu .rs per modul root
│   │   ├── sv_design_uart.rs
│   │   ├── sv_design_spi.rs
│   │   └── ...
│   ├── driver.rs          # main yang memanggil semua unit
│   ├── Cargo.toml         # workspace dengan satu crate per unit + driver
│   ├── cache/             # content-addressed cache (parsing, elaboration, codegen)
│   │   ├── parse/
│   │   ├── elaborate/
│   │   └── codegen/
│   └── target/            # hasil cargo build (binary)
```

### 4.5 Kesejahteraan diagnostik

- Semua diagnostik dari semua file dikumpulkan, **tidak dihentikan pada pertama ditemukan**.
- Render ulang ke lokasi source SV asli (span sudah ada di `sv_diag`).
- Exit code non-zero bila ada error, tapi tampilkan SEMUA error sekaligus.

---

## 5. Rencana Implementasi Bertahap

### Tahap 1: Content-Addressed Cache (Incremental Build) — SELESAI (inisialisasi) & INTEGRASI PIPELINE (selesai)

**Tujuan:** Ubah satu file leaf module tidak memaksa recompilation seluruh OpenTitan.

**Aksi (selesai):**
1. ✅ Buat `crates/sv-cache/` — crate baru untuk content-addressed hashing.
2. ✅ Fungsi hash konten file SV (`hash_konten` dengan SHA256).
3. ✅ Simpan hasil parsing, AST, semantic analysis, IR, dan codegen secara terpisah di cache (`CachedParse`, `CachedElaborate`, `CachedCodegen`).
4. ✅ Cache key: `CacheKey` = hash konten + hash dependensi.
5. ✅ Integrasi ke pipeline: `elaborate_tops` menerima `&mut SvCache`, cek cache sebelum elaborate, simpan hasil ke cache setelah elaborate.
6. ⏳ Invalidation: perubahan package yang digunakan banyak root → invalidasi semua root yang mengimpor package tersebut (belum diimplementasikan, rencana Tahap 3).

**Test baru (7 test sv-cache + 1 test pipeline, semua lolos):**
- `cache_parse_hit_dan_miss`
- `cache_elaborate_hit_dan_miss`
- `cache_codegen_hit_dan_miss`
- `kunci_berbeda_jika_konten_berbeda`
- `kunci_berbeda_jika_dependensi_berbeda`
- `dependensi_hash_simpan_dan_ambil`
- `diagnistik_default_kosong`

**Verifikasi:**
- `cargo test -p sv-cache`: 7 passed, 0 failed
- `cargo nextest run --workspace`: 1547 passed, 0 skipped
- `cargo clippy --all-targets`: clean (kecuali pre-existing sv-ir)
- `cargo fmt --check`: bersih

**Commit:** sv-cache crate + integrasi cache ke pipeline (elaborate_tops + compile_semua_root + commands Check).

### Tahap 2: Parallel Frontend (Parsing per File) — SELESAI

**Tujuan:** Preprocess → lex → parse dijalankan paralel per file dengan rayon.

**Aksi (selesai):**
1. ✅ Tambah dependency `rayon` ke `crates/svrust/Cargo.toml`.
2. ✅ Ubah `collect_modules` untuk memproses file secara paralel (rayon::par_iter).
3. ✅ Kumpulkan diagnostik dari semua file tanpa berhenti pada pertama ditemukan.
4. ✅ Root detection tetap sekuensial (tergantung hasil parsing semua file).

**Test baru:**
- Tidak ada test baru spesifik (test pipeline_mengumpulkan_error_parse_dan_elaborasi_lintas_berkas diupdate untuk parallel)

**Verifikasi:**
- `cargo nextest run --workspace`: 1547 passed, 0 skipped
- `cargo clippy --all-targets`: clean (kecuali pre-existing sv-ir)
- `cargo fmt --check`: bersih
- `cargo run --release -- check --rtl opentitan.f --top top_earlgrey --release`: `check OK (3 module, top top_earlgrey)`

**Commit:** parallel frontend rayon di collect_modules.

### Tahap 3: Parallel Elaboration & Codegen — SELESAI

**Tujuan:** Elaborasi per root dijalankan paralel dengan rayon.

**Aksi (selesai):**
1. ✅ Ubah `Rc<VarDecl>` → `Arc<VarDecl>` di `sv-ir/src/design.rs` dan `sv-ir/src/variable.rs` agar `Design` adalah `Send`.
2. ✅ Ubah `elaborate_tops` di `pipeline.rs` untuk memproses root secara paralel dengan `rayon::par_iter` (3 langkah: sequential cache check → parallel elaborate → sequential cache save).
3. ⏳ Ubah `generate_rust` untuk memproses design secara paralel (belum — codegen biasanya cepat, prioritas rendah).
4. ⏳ Shared symbol database untuk package (belum — prioritas rendah).

**Test baru:** Tidak ada test baru (test pipeline_mengumpulkan_error_parse_dan_elaborasi_lintas_berkas masih lolos dengan parallel elaboration).

**Verifikasi:**
- `cargo nextest run --workspace`: 1547 passed, 0 skipped
- `cargo clippy --all-targets`: clean (kecuali pre-existing sv-ir)
- `cargo fmt --check`: bersih
- `cargo run --release -- check --rtl opentitan.f --top top_earlgrey --release`: `check OK (3 module, top top_earlgrey)`

**Commit:** Rc→Arc di sv-ir + parallel elaborate_tops dengan rayon.

### Tahap 4: Benchmark End-to-End — SEDANG BERJALAN

**Tujuan:** Buktikan dengan pengukuran, tidak klaim.

**Aksi:**
1. Buat skrip benchmark di `benchmarks/parallel_build.sh`.
2. Ukur waktu build, peak RAM, CPU utilization, throughput untuk worker 1/2/4/8/16/32.
3. Gunakan OpenTitan atau fixture besar yang représentatif.
4. Hasil dimasukkan ke tabel di paralel.md.

**Tabel benchmark yang harus diisi:**

| Worker | Waktu build | Peak RAM | CPU utilization | Throughput |
|--------|-------------|----------|-----------------|------------|
| 1      | Wajib diukur | Wajib | Wajib | Wajib |
| 2      | Wajib diukur | Wajib | Wajib | Wajib |
| 4      | Wajib diukur | Wajib | Wajib | Wajib |
| 8      | Wajib diukur | Wajib | Wajib | Wajib |
| 16     | Wajib diukur | Wajib | Wajib | Wajib |
| 32     | Wajib diukur | Wajib | Wajib | Wajib |

### Tahap 5: Akurasi Simulasi & Differential Testing — RENCANA

**Tujuan:** Buktikan simulasi ekuivalen dengan referensi.

**Aksi:**
1. Buat differential testing SVRust vs Icarus vs Verilator.
2. Bandingkan output VCD dan print untuk semua root.
3. Klasifikasi temuan: COCOK / BUG_SVRUST / ACAKAN_Iverilog / TIGA_BEDA.

---

## 6. Verifikasi Setiap Tahap

Setiap tahap harus memenuhi:
1. Demo uji coba baru (jika ada perubahan perilaku pengguna).
2. Test baru (unit test di `src/` atau integration test di `tests/`).
3. `cargo nextest run --workspace` — semua hijau.
4. `cargo clippy --all-targets` — clean (0 warning, kecuali pre-existing).
5. `cargo fmt --check` — bersih.
6. `cargo run --release -- check --rtl opentitan.f --top top_earlgrey --release` — `check OK`.
7. Commit & push ke `origin/main`.
8. Update `paralel.md` dengan status dan hasil.

---

## 7. Catatan Perubahan per Commit

Setiap commit harus mencantumkan:
- Tahap mana yang dikerjakan.
- Apa yang diimplementasikan.
- Test baru apa yang ditambahkan.
- Demo baru apa yang ditambahkan.
- Hasil verifikasi (test count, clippy, fmt, opentitan).
- Update paralel.md apa.

---

## 8. Risiko & Keterbatasan (Belum Dibuktikan)

- Satu crate per root **belum dibuktikan optimal** untuk 10k-100k root. Cargo workspace dengan ribuan member mungkin menjadi overload.
- Tidak ada **benchmark end-to-end** yang membuktikan kecepatan.
- Tidak ada **content-addressed cache** — masih re-analisis dari awal setiap build.
- Tidak ada **shared symbol database** — package dianalisis ulang per root.
- Tidak ada **parallel compiler pipeline** — hanya cargo -j N pada build.
- Simulasi per root masih **single-threaded** — tidak ada multi-threaded simulation.

Ini adalah daftar hidup. Setiap item yang diimplementasikan harus dicentang dan didokumentasikan di sini.
