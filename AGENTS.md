# AGENTS.md — Panduan Agent

## Aturan Wajib

### 1. Satu File = Satu Tanggung Jawab

Setiap file `.rs` punya tepat satu tanggung jawab. Header wajib:

```rust
// Tanggung jawab: <deskripsi satu kalimat>
```

Contoh:
- `token.rs` → definisi jenis token
- `lexer.rs` → mesin leksikal
- `error.rs` → tipe error
- `span.rs` → posisi sumber

Dilarang: file yang berisi beberapa konsep tidak terkait.

### 2. Crate Boundary

| Crate | Tanggung Jawab |
|-------|----------------|
| `sv-lexer` | token, span, error leksikal |
| `sv-preprocessor` | `define`, `include`, `ifdef`, macro |
| `sv-ast` | definisi AST |
| `sv-parser` | token → AST |
| `sv-ir` | HDL semantic IR |
| `sv-elaborator` | resolve instances, parameters, generate |
| `sv-opt` | optimisasi IR |
| `sv-codegen-rust` | IR → Rust source |
| `sv-runtime` | scheduler, event, signal, time, trace |
| `sv-trace` | VCD / waveform output |
| `cargo-sv` | CLI binary |

Tidak boleh crate mengakses internal crate lain secara langsung. Gunakan pub API.

### 3. Error Handling

Semua error membawa `Span` (line, col, file). Error message harus bisa di-trace ke source SV original, bukan generated Rust.

### 4. Testing

Setiap crate punya unit test di `src/` (inline `#[cfg(test)]`). Integration test di `tests/<crate-name>/`.

### 5. Style

- Bahasa Indonesia untuk komentar dan doc
- Bahasa English untuk identifier
- `cargo clippy` harus clean
- `cargo fmt` sebelum commit

## Alur Kerja

1. Baca `DESIGN.md` dulu
2. Cek milestone aktif
1 file = 1 tanggung jawab. Jangan buat file "utilitas" yang berisi 5 hal tidak terkait.
3. Implementasi sesuai stage
4. Test harus pass sebelum lanjut
5.  1 bug = 1 new test setelah bug di perbaiki 
# Milestone

Lihat `DESIGN.md` bagian Milestone. Jangan lompat stage.

## Dilarang

- File > 5000 baris (split jika perlu)
- `pub mod` yang expose internal tanpa alasan
- Copy-paste code antar crate (extract ke shared crate jika perlu)
- Ignore error dengan `unwrap()` di production code
