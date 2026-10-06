# SVRust — Desain

## Visi

SystemVerilog → Rust native simulator. Bukan transpiler cosplay — runtime membawa
semantics HDL (event scheduling, delta cycles, NBA) ke dalam Rust.

## Arsitektur

```
SystemVerilog (.sv / .svh)
        │
        ▼
┌──────────────────┐
│  sv-preprocessor │  `define, `include, `ifdef, macro expansion
└────────┬─────────┘
         ▼
┌──────────────────┐
│    sv-lexer      │  token + span
└────────┬─────────┘
         ▼
┌──────────────────┐
│    sv-parser     │  AST
└────────┬─────────┘
         ▼
┌──────────────────┐
│  sv-elaborator  │  resolve instances, parameters, generate
└────────┬─────────┘
         ▼
┌──────────────────┐
│      sv-ir       │  HDL Semantic IR (types, signals, processes, timing)
└────────┬─────────┘
         │
    ┌────┴────┐
    ▼         ▼
sv-opt    debug metadata
    │         │
    └────┬────┘
         ▼
┌──────────────────┐
│ sv-codegen-rust  │  generated Rust source
└────────┬─────────┘
         ▼
    rustc / LLVM
         ▼
┌──────────────────┐
│   sv-runtime     │  scheduler, event queue, signal, trace
└──────────────────┘
```

## Tiga Lapisan Rust

| Lapisan | Sumber | Isi |
|---------|--------|-----|
| **Generated** | mesin | `design.rs`, `modules/`, `processes/`, `signals/` |
| **Runtime** | tulis tangan | `scheduler.rs`, `signal.rs`, `event.rs`, `time.rs`, `process.rs`, `memory.rs`, `trace.rs` |
| **User** | tulis tangan | `src/main.rs`, testbench |

## Type System

Width SV → type Rust:

```rust
Bits<32>       // logic [31:0]
Signed<32>     // signed arithmetic
Unsigned<32>    // unsigned arithmetic
Logic<32>      // 4-state (0, 1, X, Z)
Wire<32>       // continuous assignment
Reg<32>        // procedural assignment
```

Compiler Rust menangkap width mismatch saat compile time.

## Event Scheduler

```rust
pub struct Scheduler {
    current_time: SimTime,
    active: EventQueue,      // blocking assignments, #delay
    nba: EventQueue,         // non-blocking assignments
    postponed: EventQueue,   // $monitor, $strobe
}
```

Semantics: active → NBA → postponed, per delta cycle.

## CLI

```
cargo sv check      // parse + elaborate semua modul root
cargo sv build      // SV → Rust → native binary
cargo sv run        // build + execute
cargo sv elaborate  // statistik + daftar proses tiap modul root
cargo sv codegen    // Rust source saja (satu berkas per design)
cargo sv inspect    // statistik design
cargo sv clean      // clean target/sv/
cargo sv help       // bantuan
cargo sv version    // versi
```

Opsi: `--rtl <BERKAS>` (boleh diulang), `--top <NAMA>` (batasi modul top),
`--steps <N>` (jumlah langkah simulasi), `--vcd <BERKAS>` (rekam waveform).

Tanpa `--top`, semua modul yang tidak diinstansiasi modul lain dijalankan
sebagai modul root (LRM §23.1) — masing-masing sebagai design sendiri.

## Output

```
target/sv/
├── ast/
├── ir/
├── generated/
│   ├── lib.rs
│   ├── <module>.rs
│   └── ...
├── metadata/
└── source-map/

target/debug/    ← domain Cargo
target/release/  ← domain Cargo
```

## Source Mapping

Error Rust/engine → source map → lokasi SV original:

```
rtl/cpu.sv:128:17
```

Bukan alamat generated Rust yang tidak manusiawi.

## Milestone

| Stage | Fitur |
|-------|-------|
| 0 | `module hello; endmodule` |
| 1 | `assign y = a & b;` |
| 2 | `always_ff @(posedge clk)` |
| 3 | `always_comb` + `case` |
| 4 | `module #(parameter WIDTH)` |
| 5 | multi-module, multi-file, include, generate |
| 6 | testbench, clock, reset, assertion, VCD |
| 7 | real RTL |

## Aturan Arsitektur

**1 file = 1 tanggung jawab.** Tidak boleh lebih.

Setiap file crate punya komentar header:
```rust
// Tanggung jawab: <deskripsi satu kalimat>
```

