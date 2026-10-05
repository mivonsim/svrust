// Tanggung jawab: makro bersama untuk demo e2e function-like & default arg.
// `define dengan argumen formal, nilai default, dan body bersarang.
`define WIDTH 8
`define ADD(a, b) ((a) + (b))
`define CLAMP(v, hi = `WIDTH) ((v) > (hi) ? (hi) : (v))
`define DOUBLE(x) `ADD(x, x)
// Komentar berikut memuat direktif palsu yang tidak boleh diproses:
// `define HOAX 999
// `ifdef TIDAK_ADA
