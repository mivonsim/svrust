// Tanggung jawab: satu fuzz target untuk seluruh mode oracle.
//
// Mode dipilih dari byte pertama; byte sisanya adalah badan program. Satu
// bin untuk semua mode supaya:
//   - hanya satu korpus, jadi cakupan oracle berbeda terakumulasi pada input
//     yang sama,
//   - hanya satu perintah build dan satu perintah jalan,
//   - program yang penting untuk `elaborate` otomatis dicoba juga oleh
//     `codegen`, `determinisme`, dan `metamorphic`.
#![no_main]

#[path = "../src/oracle.rs"]
mod oracle;

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Some(pelanggaran) = oracle::jalankan(data) {
        panic!("{pelanggaran}");
    }
});