// Tanggung jawab: test integrasi driver untuk proses waktu `always #N`
// (LRM §9.2.1 + §11.2): jam simulasi, edge clock, dan urutan evaluasi.
use cargo_sv::driver_gen::generate_driver;
use sv_ir::Design;

/// Elaborasi sumber SV menjadi design.
fn design(source: &str) -> Design {
    let tokens = sv_lexer::lex(source).expect("lex");
    let ast = sv_parser::parse_module(&tokens).expect("parse");
    sv_elaborator::elaborate(&ast).expect("elaborate")
}

/// Driver mode biasa (tanpa VCD).
fn driver(source: &str) -> String {
    generate_driver(&design(source), 40, None)
}

/// Kode struct design (tempat field waktu & proses waktu hidup).
fn struct_design(source: &str) -> String {
    sv_codegen_rust::generate_module(&design(source))
}

/// Driver mode VCD untuk berkas sementara.
fn driver_vcd(source: &str) -> String {
    generate_driver(
        &design(source),
        40,
        Some(std::path::Path::new("/tmp/x.vcd")),
    )
}

/// Testbench paling umum: clock `always #5`, penghitung `always_ff`, dan
/// `initial` yang menunggu lewat `#delay`.
const JAM_BASAH: &str = "module tb; \
     logic clk = 0; \
     logic [7:0] cnt = 0; \
     always #5 clk = ~clk; \
     always_ff @(posedge clk) cnt <= cnt + 1; \
     initial begin #52 $display(\"n=%0d\", cnt); $finish; end \
   endmodule";

/// BUG: `eval_timed` hanya dipanggil di dalam blok `if has_initial` pada driver
/// biasa dan di dalam loop VCD — TIDAK di loop "Clock simulation". Design
/// `always #5 clk = ~clk; always_ff @(posedge clk) ...;` tanpa blok `initial`
/// membuat clock membeku di t=0: tidak ada posedge yang pernah terjadi.
#[test]
fn loop_clock_jalan_bila_tanpa_blok_initial() {
    let src = "module tb(output logic [7:0] cnt); \
               logic clk = 0; \
               always #5 clk = ~clk; \
               always_ff @(posedge clk) cnt <= cnt + 1; \
             endmodule";
    let kode = driver(src);
    let loop_clock = kode
        .split("// Clock simulation")
        .nth(1)
        .expect("ada loop Clock simulation");
    assert!(
        loop_clock.contains("design.eval_timed()"),
        "loop Clock simulation harus menjalankan eval_timed:\n{loop_clock}"
    );
}

/// BUG: proses waktu harus maju jam SEBELUM segmen `initial` diperiksa. Urutan
/// terbalik membuat gerbang waktu di `eval_initial` membandingkan terhadap jam
/// satu periode di belakang, dan statement testbench berjalan lebih cepat dari
/// stimulusnya.
#[test]
fn proses_waktu_jalan_sebelum_segmen_initial() {
    let kode = driver(JAM_BASAH);
    let posisi_timed = kode
        .find("design.eval_timed()")
        .expect("driver memanggil eval_timed");
    let posisi_initial = kode
        .find("design.eval_initial(__step)")
        .expect("driver memanggil eval_initial");
    assert!(
        posisi_timed < posisi_initial,
        "eval_timed harus dipanggil sebelum eval_initial"
    );
}

/// BUG: driver men-toggle clock secara sintetis pada langkah demi langkah
/// (`set_clk(__step % 2)`) meskipun clock itu digerakkan `always #N`. Dua
/// penulis untuk satu sinyal: penulisan driver terjadi lebih dulu, lalu
/// `eval_timed` menimpanya — dan `@(posedge clk)` di blok `initial` jadi
/// terpenuhi pada langkah pertama (t=0) alih-alih t=5.
#[test]
fn driver_tidak_mentoggle_clock_yang_bergerak_sendiri() {
    let kode = driver(JAM_BASAH);
    assert!(
        !kode.contains("set_clk("),
        "clock yang digerakkan always #N tidak boleh di-toggle driver:\n{kode}"
    );
}

/// Aturan yang sama berlaku pada mode VCD — kalau tidak, waveform menampilkan
/// clock berayun pada langkah driver, bukan pada waktu simulasi.
#[test]
fn driver_vcd_tidak_mentoggle_clock_yang_bergerak_sendiri() {
    let kode = driver_vcd(JAM_BASAH);
    assert!(
        !kode.contains("set_clk("),
        "clock yang digerakkan always #N tidak boleh di-toggle driver (VCD):\n{kode}"
    );
}

/// Tanpa proses waktu, clock sintetis tetap dipakai — tanpa itu `always_ff`
/// pada RTL murni tidak pernah dievaluasi.
#[test]
fn clock_sintetis_tetap_dipakai_tanpa_proses_waktu() {
    let src = "module tb(input logic clk, output logic q); \
               always_ff @(posedge clk) q <= clk; \
             endmodule";
    let kode = driver_vcd(src);
    assert!(
        kode.contains("set_clk("),
        "tanpa proses waktu, driver harus tetap men-toggle clock:\n{kode}"
    );
}

/// BUG: proses sekuensial dievaluasi pada `if clock == {id}` — perbandingan id
/// sinyal dengan argumen driver yang nilainya id sinyal itu juga, jadi selalu
/// `true`. Dengan clock `always #N`, `always_ff @(posedge clk)` lalu berjalan
/// pada SETIAP langkah, bukan hanya saat posedge.
#[test]
fn proses_sekuensial_diuji_pada_edge_nyata_bila_ada_proses_waktu() {
    let kode = struct_design(JAM_BASAH);
    let eval_seq = kode.split("pub fn eval_seq").nth(1).expect("ada eval_seq");
    assert!(
        eval_seq.contains("edge_prev["),
        "eval_seq harus menguji edge nyata:\n{eval_seq}"
    );
    assert!(
        !eval_seq.contains("_clock =="),
        "syarat `clock == id` selalu benar, tidak bisa dipakai:\n{eval_seq}"
    );
}

/// Tanpa proses waktu, jalur lama `clock == {id}` dipertahankan supaya
/// perilaku RTL murni tidak berubah.
#[test]
fn jalur_clock_tetap_dipakai_tanpa_proses_waktu() {
    let src = "module tb(input logic clk, output logic q); \
               always_ff @(posedge clk) q <= clk; \
             endmodule";
    let kode = struct_design(src);
    let eval_seq = kode.split("pub fn eval_seq").nth(1).expect("ada eval_seq");
    assert!(
        eval_seq.contains("clock =="),
        "tanpa proses waktu, syarat clock id dipakai:\n{eval_seq}"
    );
}

/// BUG: `edge_prev` hanya diisi untuk sinyal sensitivitas gabungan
/// (`sensitivity.len() > 1`). Dengan clock `always #N` dan `always_ff
/// @(posedge clk)` biasa, `edge_prev` tidak pernah diemitter — perbandingan
/// edge jadi membandingkan `0` dengan nilai sekarang, dan posedge pertama
/// salah tafsir.
#[test]
fn edge_prev_menyimpan_clock_proses_waktu() {
    let kode = struct_design(JAM_BASAH);
    assert!(
        kode.contains("edge_prev: Vec<u64>"),
        "field edge_prev wajib ada:\n{kode}"
    );
    assert!(
        kode.contains("pub fn sample_edge(&mut self)"),
        "sample_edge wajib ada:\n{kode}"
    );
    // `clk` adalah sinyal pertama modul ini, jadi harus ikut disampel.
    assert!(
        kode.contains("self.edge_prev[0] = self.signals[0].read().to_u64() & 1;"),
        "clock harus disampel untuk deteksi posedge:\n{kode}"
    );
}

/// BUG: dua proses waktu ditulis di dalam satu loop `for __i in 0..N` dengan
/// indeks proses di-hardcode, sehingga badan keduanya dieksekusi N kali dan
/// `time_now` rusak oleh proses terakhir. `timed_time` per proses harus ditulis
/// tepat sekali masing-masing.
#[test]
fn dua_proses_waktu_tiap_tiap_menyimpan_waktu_sendiri() {
    let src = "module tb; logic a = 0; logic b = 0; \
               always #5 a = ~a; \
               always #3 b = ~b; \
             endmodule";
    let kode = struct_design(src);
    let eval_timed = kode
        .split("pub fn eval_timed")
        .nth(1)
        .expect("ada eval_timed");
    assert!(
        !eval_timed.contains("for __i in"),
        "proses waktu tidak boleh dibungkus loop bersama:\n{eval_timed}"
    );
    for indeks in 0..2 {
        assert!(
            eval_timed.contains(&format!("self.timed_time[{indeks}] = self.time_now;")),
            "proses waktu {indeks} tidak menyimpan waktunya:\n{eval_timed}"
        );
        assert!(
            eval_timed.contains(&format!("self.time_now = self.timed_time[{indeks}];")),
            "proses waktu {indeks} tidak memuat waktunya:\n{eval_timed}"
        );
    }
}

/// BUG: field `timed_maks` pernah ada tapi tidak pernah dipakai untuk apa pun —
/// `eval_timed` memakai variabel lokal sendiri, jadi field itu cuma state
/// sia-sia yang muncul di struct hasil generate.
#[test]
fn tidak_ada_field_waktu_yang_tidak_terpakai() {
    let kode = struct_design(JAM_BASAH);
    assert!(
        !kode.contains("timed_maks"),
        "field tak terpakai tidak boleh ada:\n{kode}"
    );
}

/// BUG: `INITIAL_STEPS` hanya menghitung jumlah SEGMENT, padahal segmen `#52`
/// butuh sepuluh iterasi clock sebelum waktunya benar-benar tiba. Driver lalu
/// salah menghitung langkah dan statement testbench membaca counter pada waktu
/// yang keliru.
#[test]
fn langkah_simulasi_memperhitungkan_waktu_bangun() {
    let kode = struct_design(JAM_BASAH);
    let baris = kode
        .lines()
        .find(|l| l.contains("INITIAL_STEPS"))
        .expect("ada INITIAL_STEPS");
    // `#52` dengan clock `#5` butuh sedikitnya 11 iterasi.
    let nilai: usize = baris
        .split("=")
        .nth(1)
        .expect("ada nilai")
        .trim()
        .trim_end_matches(';')
        .parse()
        .expect("angka");
    assert!(
        nilai >= 11,
        "INITIAL_STEPS = {nilai}, padahal `#52` butuh >= 11 iterasi clock"
    );
}

/// BUG: syarat "jatuh tempo" memakai `timed_time[i] < __t`, yaitu waktu bangun
/// SEBELUMNYA. Event berikutnya sebenarnya terjadi pada
/// `timed_time[i] + periode[i]`, sehingga proses yang bangun lebih lambat tapi
/// masih di bawah `__t` ikut jalan lebih awal.
///
/// Untuk `always #3` + `always #4` + `initial #12` (iverilog: `na=2 nb=1`),
/// proses `#4` ikut jalan pada t=3 lalu t=7 — bukan t=4 lalu t=8 — sehingga
/// `nb` terbaca `1`, bukan `2` setelah toggle pertama.
#[test]
fn syarat_jatuh_tempo_memakai_waktu_bangun_plus_periode() {
    let src = "module tb; logic a = 0; logic b = 0; \
               integer na = 0; integer nb = 0; \
               always #3 a = ~a; \
               always #4 b = ~b; \
               always @(posedge a) na = na + 1; \
               always @(posedge b) nb = nb + 1; \
               initial begin #12 $display(\"na=%0d nb=%0d\", na, nb); $finish; end \
             endmodule";
    let kode = struct_design(src);
    let eval_timed = kode
        .split("pub fn eval_timed")
        .nth(1)
        .expect("ada eval_timed");
    // Syarat harus menjumlahkan waktu bangun dengan periode konstan.
    assert!(
        eval_timed.contains(".saturating_add(sv_runtime::SimTime::from_femtos(3000000))"),
        "syarat jatuh tempo harus memakai periode #3:\n{eval_timed}"
    );
    assert!(
        eval_timed.contains(".saturating_add(sv_runtime::SimTime::from_femtos(4000000))"),
        "syarat jatuh tempo harus memakai periode #4:\n{eval_timed}"
    );
    // Perbandingan waktu bangun polos adalah bentuk yang salah: mengabaikan
    // periode, sehingga proses lambat ikut jalan lebih awal.
    assert!(
        !eval_timed.contains("if self.timed_time[0] < __t {"),
        "syarat `timed_time < __t` salah: mengabaikan periode\n{eval_timed}"
    );
}

/// BUG: satu pemanggilan `eval_timed` mengulang dalam `loop` sampai semua proses
/// mengejar waktu terdekat. Itu memproses BEBERAPA event sekaligus, sedangkan
/// `eval_seq` hanya dievaluasi sekali per langkah driver — edge yang saling
/// meniadakan di tengah hilang tanpa pernah terlihat, dan penghitung posedge
/// membaca nilai yang salah.
#[test]
fn eval_timed_memproses_satu_event_per_panggilan() {
    let src = "module tb; logic a = 0; logic b = 0; \
               always #3 a = ~a; always #4 b = ~b; \
             endmodule";
    let kode = struct_design(src);
    let eval_timed = kode
        .split("pub fn eval_timed")
        .nth(1)
        .expect("ada eval_timed");
    assert!(
        !eval_timed.contains("loop {"),
        "loop membuat satu panggilan memproses banyak event:\n{eval_timed}"
    );
}

/// BUG: jumlah langkah dihitung sebagai `terakhir / periode`, padahal event
/// terjadi di IRISAN seluruh periode — bukan kelipatan satu periode. Untuk
/// `#3` + `#4` + `initial #12` dibutuhkan 7 panggilan (event t=3,4,6,8,9,12
/// lalu t=12), sedangkan `12/4` hanya memberi 5 dan blok `initial` kehilangan
/// seluruh output-nya tanpa pesan.
#[test]
fn langkah_simulasi_menghitung_irisan_periode() {
    let src = "module tb; logic a = 0; logic b = 0; \
               always #3 a = ~a; always #4 b = ~b; \
               initial begin #12 $display(\"x\"); $finish; end \
             endmodule";
    let kode = struct_design(src);
    let baris = kode
        .lines()
        .find(|l| l.contains("INITIAL_STEPS"))
        .expect("ada INITIAL_STEPS");
    let nilai: usize = baris
        .split("=")
        .nth(1)
        .expect("ada nilai")
        .trim()
        .trim_end_matches(';')
        .parse()
        .expect("angka");
    // ceil(12/3) + ceil(12/4) + 2 = 4 + 3 + 2 = 9
    assert!(
        nilai >= 9,
        "INITIAL_STEPS = {nilai}, padahal irisan `#3`+`#4` sampai t=12 butuh >= 9"
    );
}

/// BUG: waktu simulasi besar (`#100000000`) dengan clock cepat (`#1`) membuat
/// `INITIAL_STEPS` bernilailimpah sehingga loop driver tidak pernah selesai.
/// Jumlah langkah harus dibatasi.
#[test]
fn langkah_simulasi_dibatasi_agar_tidak_hang() {
    let src = "module tb; logic clk = 0; \
               always #1 clk = ~clk; \
               initial begin #100000000 $finish; end \
             endmodule";
    let kode = struct_design(src);
    let baris = kode
        .lines()
        .find(|l| l.contains("INITIAL_STEPS"))
        .expect("ada INITIAL_STEPS");
    let nilai: usize = baris
        .split("=")
        .nth(1)
        .expect("ada nilai")
        .trim()
        .trim_end_matches(';')
        .parse()
        .expect("angka");
    // Batasnya `1 << 20` langkah, plus 2 langkah sisa. Tanpa batas, `#100000000`
// dengan clock `#1` meminta 100.000.002 langkah dan loop driver tidak pernah
// selesai.
assert!(
        nilai <= (1 << 20) + 2,
        "INITIAL_STEPS = {nilai} melewati batas — simulasi tidak akan selesai"
    );
}

/// Gerbang wake hanya boleh dipasang bila ada proses waktu. Tanpa itu, design
/// TANPA `always #N` tidak punya yang menggerakkan jam, `__sekarang` selalu 0,
/// dan setiap segmen `#5` tertahan selamanya pada syarat `0 >= 5000000`.
#[test]
fn gerbang_wake_tidak_dipasang_tanpa_proses_waktu() {
    let src = "module tb(output logic [7:0] a); \
               initial begin a = 1; #5 a = 2; #5 a = 3; end \
             endmodule";
    let kode = struct_design(src);
    assert!(
        !kode.contains("__sekarang"),
        "gerbang wake tidak boleh ada tanpa proses waktu:\n{kode}"
    );
}

/// Sebaliknya, dengan proses waktu gerbang WAJIB ada — tanpa itu segmen `#52`
/// langsung lolos pada langkah kedua.
#[test]
fn gerbang_wake_dipasang_bila_ada_proses_waktu() {
    let kode = struct_design(JAM_BASAH);
    assert!(
        kode.contains("__sekarang >= sv_runtime::SimTime::from_femtos(52000000)"),
        "segmen #52 harus punya gerbang waktu t=52:\n{kode}"
    );
}

/// BUG: segmen yang ditangguhkan `@(posedge clk)` memakai waktu bangun lamanya
/// yang sudah basi, sehingga `$time` di dalamnya melaporkan t=0 walau edge-nya
/// baru terjadi di t=5.
#[test]
fn segmen_yang_menunggu_edge_memakai_waktu_edge() {
    let src = "module tb; logic clk = 0; \
               always #5 clk = ~clk; \
               initial begin @(posedge clk); $display(\"t=%0t\", $time); $finish; end \
             endmodule";
    let kode = struct_design(src);
    let segmen = kode
        .split("initial[0] segmen 1")
        .nth(1)
        .expect("ada segmen 1");
    let baris = segmen
        .lines()
        .find(|l| l.contains("self.time_now ="))
        .expect("ada baris set time_now");
    assert!(
        baris.contains("__sekarang"),
        "segmen menunggu edge harus mulai dari waktu edge: {baris}"
    );
}
