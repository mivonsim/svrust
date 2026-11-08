// Tanggung jawab: implementasi fungsi C yang dipanggil lewat DPI-C import (LRM §35).
#include <stdint.h>

// `set_data(input int nilai)` — simpan nilai ke variabel global.
static int32_t data_global = 0;
void set_data(int32_t nilai) {
    data_global = nilai;
}

// `hitung_paritas(input bit [7:0] data) -> int` — hitung paritas (jumlah bit 1 mod 2).
int hitung_paritas(uint8_t data) {
    int p = 0;
    uint8_t d = data;
    while (d != 0) {
        p ^= (d & 1);
        d >>= 1;
    }
    return p;
}
