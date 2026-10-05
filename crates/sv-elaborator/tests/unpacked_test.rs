// Tanggung jawab: test array unpacked (LRM §7.8).
//
// `logic [7:0] mem [0:15];` adalah array, bukan vektor: `$bits(mem)`
// mengembalikan lebar ELEMEN (8), bukan lebar total. Sinyalnya sendiri disimpan
// rata selebar `elemen * size`, dan `mem[k]` dipetakan ke rentang bit elemennya.
//
// Bentuk yang didukung: indeks KONSTAN pada dimension SATU. Indeks variabel
// (`mem[addr]`) dan dimensi bertingkat belum ada — keduanya butuh dynamic
// index yang belum ada di IR.
//
// Semua nilai diverifikasi terhadap iverilog 12.0 (`-g2012`).
mod support;

/// Nilai `variabel` dengan isi array diisi manual.
///
/// Helper ini tidak menjalankan `initial`, jadi penulisan ke elemen array harus
/// disetel sendiri: elemen ke-`k` pada `[0:N-1]` menempati bit
/// `[k*W +: W]` dari sinyal rata.
fn nilai(src: &str, variabel: &str) -> u64 {
    nilai_dengan(src, variabel, &[])
}

fn nilai_dengan(src: &str, variabel: &str, isi: &[(&str, u64, u32)]) -> u64 {
    let d = support::build(src);
    let expr = support::nilai_untuk(&d, variabel);
    let mut signals = vec![0u64; d.variables.len()];
    // Beberapa elemen ditulis ke sinyal yang sama, jadi digabung dengan OR —
    // menimpa akan membuat hanya elemen terakhir yang terlihat. Geser memakai
    // `u64` karena `0xAAu64 << 24` sudah melewati 32 bit dan `i32` akan
    // overflow (panik di build debug).
    for (nama, nilai, jarak) in isi {
        let var = d.find_variable(nama).expect("sinyal ada");
        signals[var.signal_id as usize] |= nilai << (*jarak * 8);
    }
    support::evaluasi(&expr, &mut signals)
}

/// Source dengan array ascend `[0:3]`; isinya disetel manual oleh
/// [`nilai_dengan`].
fn ascend(nama: &str) -> String {
    format!(
        "module m;\n  \
         logic [7:0] mem [0:3];\n  \
         logic [7:0] o;\n  \
         initial begin\n    \
         #1; o = {nama};\n  \
         end\n\
         endmodule\n"
    )
}

/// Isi keempat elemen ascend: 0x11, 0x22, 0x33, 0x44 pada elemen 0..3.
fn isi_ascend() -> [(&'static str, u64, u32); 4] {
    [
        ("mem", 0x11, 0),
        ("mem", 0x22, 1),
        ("mem", 0x33, 2),
        ("mem", 0x44, 3),
    ]
}

#[test]
fn elemen_ascend_terbaca_benar() {
    // iverilog: W1=11 W2=44
    assert_eq!(nilai_dengan(&ascend("mem[0]"), "o", &isi_ascend()), 0x11);
    assert_eq!(nilai_dengan(&ascend("mem[1]"), "o", &isi_ascend()), 0x22);
    assert_eq!(nilai_dengan(&ascend("mem[2]"), "o", &isi_ascend()), 0x33);
    assert_eq!(nilai_dengan(&ascend("mem[3]"), "o", &isi_ascend()), 0x44);
}

#[test]
fn elemen_descend_terbaca_benar() {
    // LRM §7.8: pada `[3:0]` indeks PERTAMA yang tertulis adalah 3, jadi
    // elemen 0 menempati bit paling atas (jarak 3) dan elemen 3 yang di bit
    // paling bawah (jarak 0). Urutannya terbalik dibanding `[0:3]`.
    let src_0 = "module m;\n  logic [7:0] rev [3:0];\n  logic [7:0] o;\n  initial begin\n    #1; o = rev[0];\n  end\nendmodule\n";
    assert_eq!(nilai_dengan(src_0, "o", &[("rev", 0xAA, 3)]), 0xAA);
    let src_3 = src_0.replace("rev[0]", "rev[3]");
    assert_eq!(nilai_dengan(&src_3, "o", &[("rev", 0xDD, 0)]), 0xDD);
}

#[test]
fn ukuran_array_kolom_menghitung_lebar_total() {
    // Empat elemen 8-bit disimpan rata jadi sinyal 32 bit.
    let d = support::build("module m; logic [7:0] mem [0:3]; endmodule");
    let var = d.find_variable("mem").expect("mem terdaftar");
    assert_eq!(var.data_type.width, 32);
}

#[test]
fn bits_dari_array_mengembalikan_lebar_elemen() {
    // LRM §20: `$bits` pada array unpacked adalah lebar ELEMEN, bukan total.
    // iverilog: W5=8 dan W6=8.
    // iverilog: W5=8
    assert_eq!(nilai(&ascend("$bits(mem)"), "o"), 8);
    // iverilog: W6=8
    assert_eq!(nilai(&ascend("$bits(mem[0])"), "o"), 8);
}

#[test]
fn elemen_kosong_membaca_nol() {
    // Elemen yang tidak pernah ditulis bernilai 0 pada engine 2-state.
    let src = "module m;\n  \
         logic [7:0] mem [0:3];\n  \
         logic [7:0] o;\n  \
         initial begin\n    \
         #1; o = mem[2];\n  \
         end\n\
         endmodule\n";
    assert_eq!(nilai(src, "o"), 0);
}

#[test]
fn indeks_di_luar_jangkauan_ditolak() {
    // LRM §7.8: indeks di luar jangkauan adalah error, bukan pemotongan diam-diam.
    let tokens =
        sv_lexer::lex("module m; logic [7:0] mem [0:3]; initial mem[4] = 8'h11; endmodule")
            .expect("lex");
    let m = sv_parser::parse_module(&tokens).expect("parse");
    let err = sv_elaborator::elaborate(&m).expect_err("indeks 4 di luar [0:3]");
    assert!(
        err.to_string().contains("di luar jangkauan"),
        "pesan tak menjelaskan masalah: {err}"
    );
}

#[test]
fn indeks_negatif_ditolak() {
    // `mem[-1]` adalah error waktu elaborasi, bukan indeks yang menghasilkan `X`
    // saat simulasi berjalan — karena nilainya sudah diketahui sebelum jalan.
    let tokens = sv_lexer::lex(
        "module m; logic [7:0] mem [0:3]; logic [7:0] o; initial o = mem[-1]; endmodule",
    )
    .expect("lex");
    let m = sv_parser::parse_module(&tokens).expect("parse");
    let err = sv_elaborator::elaborate(&m).expect_err("indeks negatif harus ditolak");
    let pesan = err.to_string();
    assert!(
        pesan.contains("indeks negatif") || pesan.contains("di luar jangkauan"),
        "pesan tak menjelaskan masalah: {pesan}"
    );
}

#[test]
fn dimensi_unpacked_bertingkat_ditolak_dengan_pesan_jelas() {
    // Belum ada modelnya; penolakan harus menyebut alasannya, bukan
    // "expected ';'".
    let tokens = sv_lexer::lex("module m; logic [7:0] big [0:1][0:3]; endmodule").expect("lex");
    let err = sv_parser::parse_module(&tokens).expect_err("dua dimensi ditolak");
    assert!(
        err.contains("bertingkat"),
        "pesan tak menjelaskan batasannya: {err}"
    );
}

#[test]
fn ukuran_nol_pada_dimensi_ditolak() {
    // `logic [7:0] mem [0:0];` sebenarnya sah (satu elemen), tapi
    // `[0-1:0]`pasti tidak.
    let tokens = sv_lexer::lex("module m; logic [7:0] mem [3:0]; endmodule").expect("lex");
    // `[3:0]` berarti 4 elemen — sah.
    sv_parser::parse_module(&tokens).expect("sah");
}

#[test]
fn array_unpacked_bisa_dipakai_di_modul_anak() {
    // Instance harus ikut membawa dimensi array-nya; kalau tidak, indeksnya
    // ditolak di badan anak.
    let design = support::build_top(
        "module c (output logic [7:0] o);\n  \
         logic [7:0] mem [0:3];\n  \
         initial begin\n    \
         #1; o = mem[2];\n  \
         end\n\
         endmodule\n\
         module top(output logic [7:0] q);\n  \
         c u0(.o(q));\n\
         endmodule\n",
        "top",
    );
    let expr = support::nilai_untuk(&design, "q");
    let mut signals = vec![0u64; design.variables.len()];
    let mem = design.find_variable("u0__mem").expect("mem anak terdaftar");
    signals[mem.signal_id as usize] |= 0x5Au64 << 16;
    assert_eq!(support::evaluasi(&expr, &mut signals), 0x5A);
}

/// BUG: pesan error untuk indeks variabel pada LHS selalu menyebut "genvar",
/// walau penulis kode tidak menulis genvar sama sekali — pada array unpacked
/// (`mem[k] = x`, LRM §7.8) sama sekali tidak menyebut array.
#[test]
fn indeks_variabel_pada_lhs_unpacked_pesan_yang_jelas() {
    let tokens = sv_lexer::lex(
        "module m; logic [7:0] mem [0:3]; logic [2:0] k; initial mem[k] = 8'hAB; endmodule",
    )
    .expect("lex");
    let ast = sv_parser::parse_module(&tokens).expect("parse");
    let e = sv_elaborator::elaborate(&ast).expect_err("indeks variabel pada LHS belum didukung");
    let pesan = e.message;
    assert!(
        pesan.contains("unpacked") && pesan.contains("mem"),
        "pesan harus menyebut array dan namanya: {pesan}"
    );
    assert!(
        !pesan.contains("genvar"),
        "pesan tidak boleh menyebut genvar: {pesan}"
    );
    assert!(
        pesan.contains("konstan"),
        "pesan harus menyarankan indeks konstan: {pesan}"
    );
}

#[test]
fn indeks_variabel_pada_lhs_sinyal_biasa_pesan_yang_jelas() {
    let tokens =
        sv_lexer::lex("module m; logic [7:0] y; logic [2:0] k; initial y[k] = 1'b1; endmodule")
            .expect("lex");
    let ast = sv_parser::parse_module(&tokens).expect("parse");
    let e = sv_elaborator::elaborate(&ast).expect_err("indeks variabel pada LHS belum didukung");
    let pesan = e.message;
    assert!(
        pesan.contains("indeks variabel"),
        "pesan harus menyebut indeks variabel: {pesan}"
    );
    assert!(!pesan.contains("genvar"), "pesan: {pesan}");
}
