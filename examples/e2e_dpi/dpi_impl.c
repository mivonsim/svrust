// Tanggung jawab: implementasi fungsi C yang dipanggil lewat DPI-C import/export (LRM §35).
#include <stdint.h>
#include <stdio.h>

// --- DPI-C import: fungsi C yang dipanggil dari SystemVerilog ---

// `set_data(input int nilai)` — simpan nilai ke variabel global.
static int32_t data_global = 0;
void set_data(int32_t nilai) {
    data_global = nilai;
    printf("[C] set_data dipanggil dengan nilai=%d\n", nilai);
}

// `hitung_paritas(input bit [7:0] data) -> int` — hitung paritas.
int hitung_paritas(uint8_t data) {
    int p = 0;
    uint8_t d = data;
    while (d != 0) {
        p ^= (d & 1);
        d >>= 1;
    }
    printf("[C] hitung_paritas(0x%02x) = %d\n", data, p);
    return p;
}

// --- DPI-C export: fungsi C yang memanggil fungsi SV ---

// Fungsi ini akan dipanggil dari Rust wrapper yang di-generate.
// Menerima pointer ke instance design (untuk sekarang diabaikan).
void c_tampilkan_hasil(void* __instance, int32_t nilai) {
    printf("[C] c_tampilkan_hasil dipanggil dengan nilai=%d (dari SV export)\n", nilai);
}
