// Tanggung jawab: helper bersama untuk integration test sv-elaborator.
//
// Berkas ini di-`mod`-kan oleh setiap integration test, jadi setiap test
// binary hanya memakai sebagian helper. Tanpa `allow(dead_code)` Clippy
// melaporkan helper yang dipakai test lain sebagai tak terpakai di binary ini
// (AGENTS.md: `cargo clippy` harus clean).
#![allow(dead_code)]

use sv_ir::{Expr, UnOp};

pub fn build(source: &str) -> sv_ir::Design {
    let tokens = sv_lexer::lex(source).expect("lex");
    let ast = sv_parser::parse_module(&tokens).expect("parse");
    sv_elaborator::elaborate(&ast).expect("elaborate")
}

/// Error dari build top untuk beberapa module.
pub fn err_top(source: &str, top: &str) -> String {
    let tokens = sv_lexer::lex(source).expect("lex");
    let modules = sv_parser::parse_file(&tokens).expect("parse");
    sv_elaborator::elaborate_top(&modules, top)
        .expect_err("elaborate harus gagal")
        .to_string()
}

/// Build top dari beberapa module untuk menguji instansiasi.
pub fn build_top(source: &str, top: &str) -> sv_ir::Design {
    let tokens = sv_lexer::lex(source).expect("lex");
    let modules = sv_parser::parse_file(&tokens).expect("parse");
    sv_elaborator::elaborate_top(&modules, top).expect("elaborate")
}

/// Bangun satu module lalu jalankan pass optimisasi.
///
/// Setara dengan `build`, ditambah `sv-opt` — dipakai test yang memeriksa
/// bentuk IR setelah constant folding.
pub fn build_dengan_opt(source: &str) -> sv_ir::Design {
    let mut design = build(source);
    sv_opt::optimize(&mut design);
    design
}

/// Bangun top dari beberapa module, lalu jalankan pass optimisasi.
///
/// Optimisasi wajib ikut karena `sv-codegen-rust`-tekocek label `casez`/`casex`
/// dari `Expr::Const` — dan constant foldinglah yang membuat cast wildcard
/// jadi konstanta. Tanpa langkah ini test akan menguji kondisi yang tidak pernah
/// terjadi di pipeline sungguhan.
pub fn build_top_dengan_opt(source: &str, top: &str) -> sv_ir::Design {
    let mut design = build_top(source, top);
    sv_opt::optimize(&mut design);
    design
}

/// Ambil nilai ekspresi pada assignment pertama proses pertama.
pub fn nilai_rhs(design: &sv_ir::Design) -> Expr {
    match &design.processes[0].body[0] {
        sv_ir::Statement::Assign { assignment, .. } => assignment.value.clone(),
        other => panic!("expected assignment, dapat {other:?}"),
    }
}

/// Ambil ekspresi yang ditulis user, tanpa pembungkus resize batas assignment.
///
/// Batas assignment adalah context-determined (LRM §11.6.1), jadi nilainya
/// selalu sized ke lebar target dan sering dibungkus `Expr::Cast`. Test yang
/// memeriksa *bentuk* ekspresi (mis. "apakah ini `Select` atau `Const`")
/// memakai helper ini; test yang memeriksa *nilai* memakai [`nilai_rhs`].
pub fn nilai_rhs_mentah(design: &sv_ir::Design) -> Expr {
    peel_resize(&nilai_rhs(design))
}

/// Lepas satu lapisan `Cast` yang lebar tipenya berbeda dari lebarnya operand.
///
/// Hanya melepas Cast yang benar-benar mengubah lebar, jadi cast eksplisit
/// pengguna yang menyempitkan (`byte'(x)` dari 16 bit) ikut hilang — pemanggil
/// yang butuh keduanya harus memisahkan kasusnya sendiri.
fn peel_resize(expr: &Expr) -> Expr {
    match expr {
        Expr::Cast {
            operand, data_type, ..
        } if operand.data_type().width != data_type.width => (**operand).clone(),
        other => other.clone(),
    }
}

/// Ambil nilai ekspresi yang ditulis ke sinyal `nama`.
///
/// Berbeda dengan [`nilai_rhs`], yang selalu melihat assignment pertama —
/// itu tidak cukup kalau source punya beberapa net declaration assign, karena
/// prosesnya berurutan dan yang pertama bukan nilai yang dicari. Target
/// assignment di IR berupa `SignalId`, jadi nama dipetakan lewat
/// `design.variables`.
pub fn nilai_untuk(design: &sv_ir::Design, nama: &str) -> Expr {
    let var = design.find_variable(nama).expect("sinyal ada");
    for process in &design.processes {
        let mut ditemukan = None;
        cari_assign(&process.body, var.signal_id, &mut ditemukan);
        if let Some(expr) = ditemukan {
            return expr;
        }
    }
    panic!("tidak ada assignment ke '{nama}'");
}

/// Cari assignment pertama yang menulis ke `target`, termasuk yang terlindung
/// di dalam `Delay`, `Block`, dan `If`.
///
/// `initial begin #1; o = x; end` menurunkan `#1` menjadi `Statement::Delay`
/// dengan assignment di dalam `body`-nya, jadi penelusuran harus turun ke
/// statement anak. Tanpa itu test menyimpulkan "tidak ada assignment" padahal
/// ada — pesan yang menyesatkan dan menggoda test ditulis ulang.
fn cari_assign(body: &[sv_ir::Statement], target: u32, out: &mut Option<Expr>) {
    for stmt in body {
        match stmt {
            sv_ir::Statement::Assign { assignment, .. } => {
                if out.is_none() && assignment.target == target {
                    *out = Some(assignment.value.clone());
                }
            }
            sv_ir::Statement::Block { body, .. } => cari_assign(body, target, out),
            sv_ir::Statement::If {
                then_branch,
                else_branch,
                ..
            } => {
                cari_assign(then_branch, target, out);
                cari_assign(else_branch, target, out);
            }
            sv_ir::Statement::Delay { body, .. } => {
                cari_assign(std::slice::from_ref(&**body), target, out);
            }
            sv_ir::Statement::For {
                body, init, step, ..
            } => {
                if out.is_none() && init.target == target {
                    *out = Some(init.value.clone());
                }
                cari_assign(body, target, out);
                if out.is_none() && step.target == target {
                    *out = Some(step.value.clone());
                }
            }
            _ => {}
        }
        if out.is_some() {
            return;
        }
    }
}

/// Hitung nilai ekspresi IR dengan signal semua nol (input default nol di driver).
pub fn evaluasi(expr: &Expr, signals: &mut Vec<u64>) -> u64 {
    match expr {
        Expr::Const { value, .. } => *value,
        Expr::SignalRef { signal, .. } => signals[*signal as usize],
        Expr::Bin { op, lhs, rhs, .. } => {
            let a = evaluasi(lhs, signals);
            let b = evaluasi(rhs, signals);
            // Lebar logis ekspresi hasil; hasil aritmetika dipotong ke sini.
            let lebar = expr.data_type().width;
            use sv_ir::BinOp::*;
            match op {
                BitAnd => a & b,
                BitOr => a | b,
                BitXor => a ^ b,
                // LRM §11.4.4: hasil aritmetika dipotong ke ukuran ekspresi.
                // Tanpa mask ini `y * y` dengan `y` 8-bit tapi storage 64-bit
                // menyisakan bit di atas 8 yang tidak pernah ada di simulasi —
                // helper test harus meniru `sv_runtime::bits_arith` yang memotong
                // ke lebar vektornya.
                Add => a.wrapping_add(b) & lebar_mask(lebar),
                Sub => a.wrapping_sub(b) & lebar_mask(lebar),
                Mul => a.wrapping_mul(b) & lebar_mask(lebar),
                // Pembagi nol => 0, sama seperti helper runtime.
                Div => a.checked_div(b).unwrap_or(0),
                Mod => a.checked_rem(b).unwrap_or(0),
                // LRM §11.6.1: lebar hasil geser mengikuti operand KIRI, jadi
                // hasilnya dipotong ke lebar operand kiri — `8'hFF << 1`
                // adalah 8'hFE, bukan 0'h1FE.
                //
                // Operator Rust mentah `a << b` PANIC di build debug saat
                // `b >= 64`, sedangkan `sv_runtime::geser_kiri` mengembalikan
                // 0. Kedua jalur harus memakai helper yang sama supaya test
                // bisa menguji jumlah geser yang melebihi lebar logis.
                Shl => geser_kiri(a, b, lhs.data_type().width),
                Shr => geser_kanan_logis(a, b, lhs.data_type().width),
                // LRM §11.4.10: `>>>` mengisi bit sign tipe hasil. Lebar
                // mengikuti operand kiri seperti geser lain.
                Sar => geser_kanan_aritmetik(a, b, lhs.data_type().width, expr.data_type().signed),
                // LRM §11.4.5: perbandingan bertanda hanya bila kedua operand
                // signed, dan lebarnya lebar operand (bukan lebar hasil 1 bit).
                // Tanpa ini `-1 < 0` membandingkan 0xFFFFFFFF dengan 0 sebagai
                // unsigned dan menghasilkan false.
                op if op.relational_is_signed(lhs.data_type(), rhs.data_type()) => {
                    let lebar = lhs.data_type().width;
                    let (sa, sb) = (bertanda(a, lebar), bertanda(b, lebar));
                    u64::from(match op {
                        Eq => sa == sb,
                        NotEq => sa != sb,
                        Lt => sa < sb,
                        Gt => sa > sb,
                        Le => sa <= sb,
                        _ => sa >= sb,
                    })
                }
                Eq => u64::from(a == b),
                NotEq => u64::from(a != b),
                Lt => u64::from(a < b),
                Gt => u64::from(a > b),
                Le => u64::from(a <= b),
                Ge => u64::from(a >= b),
                LogAnd => u64::from(a != 0 && b != 0),
                LogOr => u64::from(a != 0 || b != 0),
            }
        }
        Expr::Un { op, operand, .. } => {
            let a = evaluasi(operand, signals);
            let m = lebar_mask(operand.data_type().width);
            match op {
                UnOp::BitNot => (!a) & m,
                UnOp::LogNot => u64::from(a == 0),
                UnOp::BitNeg => (a & m).wrapping_neg() & m,
                UnOp::RedAnd => u64::from((a & m) == m),
                UnOp::RedNand => u64::from((a & m) != m),
                UnOp::RedOr => u64::from((a & m) != 0),
                UnOp::RedNor => u64::from((a & m) == 0),
                // Paritas ganjil = 1; `% 2 == 0` berarti genap (MSRV 1.75,
                // lihat `clippy.toml`; `is_multiple_of` baru stabil di 1.87).
                UnOp::RedXor => u64::from((a & m).count_ones() % 2 == 1),
                UnOp::RedXnor => u64::from((a & m).count_ones() % 2 == 0),
            }
        }
        Expr::Select { base, msb, lsb, .. } => {
            let a = evaluasi(base, signals);
            (a >> lsb) & lebar_mask(msb - lsb + 1)
        }
        Expr::Ternary {
            condition,
            when_true,
            when_false,
            ..
        } => {
            if evaluasi(condition, signals) != 0 {
                evaluasi(when_true, signals)
            } else {
                evaluasi(when_false, signals)
            }
        }
        // LRM §11.8.1: item pertama menjadi MSB.
        Expr::Concat { items, .. } => {
            let mut hasil = 0u64;
            for item in items {
                let lebar = item.data_type().width;
                let nilai = evaluasi(item, signals) & lebar_mask(lebar);
                hasil = (hasil << lebar) | nilai;
            }
            hasil
        }
        // LRM §7.8: elemen ke-`index` pada vektor rata. Indeks di luar
        // jangkauan menghasilkan `X`, yang di evaluator ini dihitung 0.
        Expr::Index { base, index, .. } => {
            let v = evaluasi(base, signals);
            let i = evaluasi(index, signals);
            let geser = (i * expr.data_type().width as u64) as usize;
            (v >> geser) & lebar_mask(expr.data_type().width)
        }
        // LRM §11.8.2: operand diulang `count` kali.
        Expr::Replicate { count, value, .. } => {
            let lebar = value.data_type().width;
            let satu = evaluasi(value, signals) & lebar_mask(lebar);
            let mut hasil = 0u64;
            for _ in 0..*count {
                hasil = (hasil << lebar) | satu;
            }
            hasil
        }
        // LRM §6.14: cast memanggil helper runtime yang sama dengan yang
        // dipakai `sv-codegen-rust`, bukan implementasi ulang di sini.
        //
        // Meniru implementasi sendiri adalah sumber test yang salah: fold dan
        // codegen sama-sama bisa salah tanpa test ini clueless, karena
        // "kebetulan" menghasilkan angka yang sama. Dengan memanggil
        // `sv_runtime::resize`/`sign_extend`, test benar-benar menguji
        // behaviour yang dilihat simulasi.
        Expr::Cast {
            operand,
            data_type,
            operand_signed,
        } => {
            let nilai = evaluasi(operand, signals);
            cast_nilai(
                nilai,
                operand.data_type().width,
                data_type.width,
                *operand_signed,
            )
        }
        // Waktu simulasi tidak dihitung pada evaluator uji ini.
        Expr::SimTime { .. } => 0,
    }
}

/// Nilai bertanda sepanjang lebar logis `lebar`.
///
/// Meniru `sv_runtime::to_signed`, dipakai evaluator test agar hasilnya sama
/// dengan yang dilihat simulasi.
fn bertanda(nilai: u64, lebar: u32) -> i64 {
    let w = (lebar as usize).min(64);
    if w == 0 {
        return 0;
    }
    let mentah = nilai & lebar_mask(lebar);
    let tanda = 1u64 << (w - 1);
    if mentah & tanda == 0 || w == 64 {
        return mentah as i64;
    }
    (mentah as i64) - (1i64 << w)
}

/// Apakah statement `case` pada desain cocok dengan tiap stimulus.
///
/// Mengembalikan `true` bila ada lengan non-`default` yang cocok. Cukup untuk
/// menguji wildcard `casez`/`casex` tanpa harus menyimulasikan seluruh body
/// lengan.
pub fn case_cocok(design: &sv_ir::Design, stimuli: &mut [Vec<u64>]) -> Vec<bool> {
    let (selector, arms, kind) = design
        .processes
        .iter()
        .flat_map(|p| p.body.iter())
        .find_map(|s| match s {
            sv_ir::Statement::Case {
                selector,
                arms,
                kind,
                ..
            } => Some((selector, arms, *kind)),
            _ => None,
        })
        .expect("design harus punya statement case");

    stimuli
        .iter_mut()
        .map(|isi| {
            let sel = evaluasi(selector, isi);
            let m = lebar_mask(selector.data_type().width);
            arms.iter().filter(|a| !a.is_default).any(|arm| {
                arm.labels.iter().any(|label| {
                    let nilai = nilai_label(label);
                    let (unknown, zmask) = masker_label(label);
                    // LRM §12.5: `casez` hanya menjadikan `z`/`?` wildcard,
                    // `casex` menjadikan `x` dan `z`.
                    let wildcard = match kind {
                        sv_ir::process::CaseKind::Casez => zmask,
                        _ => unknown,
                    } & m;
                    (sel ^ nilai) & !wildcard & m == 0
                })
            })
        })
        .collect()
}

/// Nilai numerik sebuah label konstanta.
fn nilai_label(label: &sv_ir::Expr) -> u64 {
    match label {
        sv_ir::Expr::Const { value, .. } => *value,
        other => panic!("label case bukan konstanta: {other:?}"),
    }
}

/// Masker `x` dan `z` dari label konstanta, dipisah karena LRM §12.5
/// memperlakukannya berbeda pada `casez` dan `casex`.
fn masker_label(label: &sv_ir::Expr) -> (u64, u64) {
    match label {
        sv_ir::Expr::Const {
            unknown_mask,
            zmask,
            ..
        } => (*unknown_mask, *zmask),
        other => panic!("label case bukan konstanta: {other:?}"),
    }
}

/// Masker seluruh bit pada lebar tertentu.
pub fn lebar_mask(lebar: u32) -> u64 {
    if lebar == 0 {
        0
    } else if lebar >= 64 {
        u64::MAX
    } else {
        (1u64 << lebar) - 1
    }
}

/// `a << b` sepanjang lebar logis `lebar` (LRM §11.4.10).
///
/// Meniru `sv_runtime::geser_kiri` per digit: pengisi nol, dan jumlah geser
/// yang melebihi lebar mengosongkan seluruh bit — bukan `u64` yang bergeser
/// melewati kapasitas dan wraps atau panic.
fn geser_kiri(a: u64, b: u64, lebar: u32) -> u64 {
    geser_basis(a, b, lebar, true, false)
}

/// `a >> b` sepanjang lebar logis `lebar` (LRM §11.4.10).
///
/// Meniru `sv_runtime::geser_kanan_logis`: pengisi nol apa pun signedness-nya.
fn geser_kanan_logis(a: u64, b: u64, lebar: u32) -> u64 {
    geser_basis(a, b, lebar, false, false)
}

/// `a >>> b` pada lebar logis `lebar` (LRM §11.4.10).
///
/// Geser logis lebih dulu, lalu posisi vacated diisi bit sign hanya bila tipe
/// hasil signed. Jumlah gesar yang melebihi lebar membuat seluruh hasil sign
/// (atau nol untuk unsigned), sama seperti `sv_runtime::geser_kanan_aritmetik`.
fn geser_kanan_aritmetik(a: u64, b: u64, lebar: u32, signed: bool) -> u64 {
    geser_basis(a, b, lebar, false, signed)
}

/// Badan bersama ketiga operator geser, per digit.
///
/// Meniru `sv_runtime::shift::geser`. Bucle per digit (bukan `a << b`) penting
/// karena dua hal: jumlah geser `>= 64` tidak boleh panic di build debug, dan
/// `b.min(63)` akan menyisakan bit 63 pada lebar 64 alih-alih mengosongkan
/// seluruhnya.
fn geser_basis(a: u64, b: u64, lebar: u32, kiri: bool, signed: bool) -> u64 {
    let w = (lebar as usize).min(64);
    if w == 0 {
        return 0;
    }
    let nilai = a & lebar_mask(lebar);
    let jumlah = usize::try_from(b).unwrap_or(usize::MAX);
    let sign = signed && (nilai >> (w - 1)) & 1 == 1;
    let mut out = 0u64;
    for i in 0..w {
        let src = if kiri {
            i.checked_sub(jumlah)
        } else {
            i.checked_add(jumlah)
        };
        let satu = match src {
            Some(s) if s < w => (nilai >> s) & 1 == 1,
            // Bit yang keluar dari jangkauan: nol untuk logis, sign untuk
            // aritmetik.
            _ => sign,
        };
        if satu {
            out |= 1u64 << i;
        }
    }
    out
}

/// Nilai `nilai` setelah `tipe'(nilai)` (LRM §6.14).
///
/// Memanggil `sv_runtime::resize` dan `sv_runtime::sign_extend` — helper yang
/// sama dengan yang di-emit `sv-codegen-rust` — sehingga test menguji
/// behaviour yang benar-benar dilihat simulasi, bukan implementasi ulang yang
/// bisa menyimpang diam-diam.
///
/// Lebar penyimpanan 64 cukup: nilai sinyal pada evaluator ini juga u64, jadi
/// lebar di atas 64 tidak merepresentasikan apa pun lewat `evaluasi`.
///
/// Helper runtime memakai lebar sebagai const generic, jadi lebar sumber harus
/// dipilih lewat dispatch — cukup lebar *sumber*, karena lebar tujuan selalu
/// dipanggil sebagai 64 (lebar penyimpanan). Helper mengisi bit di atas
/// `src` sampai 64 dengan nol atau tanda; membaca hanya `dst` bit pertama
/// mereproduksi hasil cast untuk `dst` berapa pun:
/// - `dst > src`: bit baru sudah terisi benar oleh helper.
/// - `dst <= src`: tidak ada bit baru, jadi apa pun yang diisi helper di atas
///   `src` terpotong saat dibaca — sama dengan `resize` yang memotong.
fn cast_nilai(nilai: u64, src: u32, dst: u32, operand_signed: bool) -> u64 {
    use sv_runtime::{resize, sign_extend, Bits, Logic};
    let v = Bits::<64>::from_u64(nilai);
    let hasil = match (src, operand_signed) {
        (1, false) => resize::<1, 64, 64>(v),
        (1, true) => sign_extend::<1, 64, 64>(v),
        (4, false) => resize::<4, 64, 64>(v),
        (4, true) => sign_extend::<4, 64, 64>(v),
        (8, false) => resize::<8, 64, 64>(v),
        (8, true) => sign_extend::<8, 64, 64>(v),
        (16, false) => resize::<16, 64, 64>(v),
        (16, true) => sign_extend::<16, 64, 64>(v),
        (32, false) => resize::<32, 64, 64>(v),
        (32, true) => sign_extend::<32, 64, 64>(v),
        (_, false) => resize::<64, 64, 64>(v),
        (_, true) => sign_extend::<64, 64, 64>(v),
    };
    let mut out = 0u64;
    for i in 0..(dst.min(64) as usize) {
        if matches!(hasil.get(i), Logic::One) {
            out |= 1u64 << i;
        }
    }
    out
}

/// Set nilai sinyal tertentu lalu hitung ekspresi.
pub fn dengan_sinyal(source: &str, isi: &[(&str, u64)]) -> u64 {
    let design = build(source);
    let expr = nilai_rhs(&design);
    let mut signals = vec![0u64; design.variables.len()];
    for (nama, nilai) in isi {
        let var = design.find_variable(nama).expect("sinyal ada");
        signals[var.signal_id as usize] = *nilai;
    }
    evaluasi(&expr, &mut signals)
}
