// Tanggung jawab: pelipatan konstanta pada ekspresi IR.
use sv_ir::{BinOp, DataType, Expr, UnOp};

/// Masker seluruh bit pada lebar `n`, sama dengan `sv_runtime`.
fn mask(n: u32) -> u64 {
    if n == 0 {
        0
    } else if n >= 64 {
        u64::MAX
    } else {
        (1u64 << n) - 1
    }
}

/// Nilai konstanta beserta lebarnya, bila node adalah `Const` polos.
fn sebagai_konst(e: &Expr) -> Option<u64> {
    match e {
        Expr::Const {
            value,
            unknown_mask,
            ..
        } if *unknown_mask == 0 => Some(*value),
        _ => None,
    }
}

/// Nilai konstanta termasuk wildcard `x`/`z`; dipakai untuk `Select` dan
/// `Concat` yang tidak boleh dilipat bila ada bit tak diketahui.
fn sebagai_konst_known(e: &Expr) -> Option<(u64, u64)> {
    match e {
        Expr::Const {
            value,
            unknown_mask,
            ..
        } => Some((*value, *unknown_mask)),
        _ => None,
    }
}

/// Nilai konstanta beserta masker `x` dan `z`, bila node adalah `Const`.
///
/// `z` dipisah dari `x` karena LRM §12.5 memperlakukannya berbeda pada
/// `casez` (hanya `z` yang wildcard) dan `casex` (keduanya wildcard).
fn sebagai_konst_nukil(e: &Expr) -> Option<(u64, u64, u64)> {
    match e {
        Expr::Const {
            value,
            unknown_mask,
            zmask,
            ..
        } => Some((*value, *unknown_mask, *zmask)),
        _ => None,
    }
}

/// Hitung nilai `Bin` dari dua konstanta, meniru `sv-codegen-rust::expr_gen`
/// persis: pembagian/modulo di-mask ke lebar hasil, perbandingan memakai
/// nilai mentah kecuali yang bertanda, operator lain tidak di-mask sama sekali.
///
/// `None` berarti jangan dilipat karena runtime akan berperilaku lain —
/// operator geser dengan besaran `>= 64` panik di build debug, jadi melipatnya
/// jadi konstanta justru mengubah perilaku yang diamati pengguna.
///
/// `signed` menentukan perbandingan bertanda (LRM §11.4.5). Tanpa ini
/// `-1 < 0` terlipat jadi false karena 0xFFFFFFFF dibandingkan sebagai
/// unsigned — persis perilaku runtime setelah perbandingan signed diperbaiki.
fn nilai_bin(op: BinOp, a: u64, b: u64, lebar: u32, signed: bool) -> Option<u64> {
    Some(match op {
        BinOp::BitAnd => a & b,
        BinOp::BitOr => a | b,
        BinOp::BitXor => a ^ b,
        BinOp::Add => a.wrapping_add(b),
        BinOp::Sub => a.wrapping_sub(b),
        BinOp::Mul => a.wrapping_mul(b),
        // LRM §11.4.5: pembagi nol menghasilkan 0 pada engine 2-state.
        BinOp::Div => (a & mask(lebar)).checked_div(b & mask(lebar)).unwrap_or(0),
        BinOp::Mod => (a & mask(lebar)).checked_rem(b & mask(lebar)).unwrap_or(0),
        // Geser lewat helper runtime yang sudah menjaga jumlah geser melebihi
        // lebar logis (hasilnya nol, atau seluruh sign bit untuk `>>>`), jadi
        // penjaga `b >= 64` yang tadinya ada tidak diperlukan lagi.
        BinOp::Shl => geser_logis(a, b, lebar, true),
        BinOp::Shr => geser_logis(a, b, lebar, false),
        // LRM §11.4.10: `>>>` mengisi bit sign bila tipe hasil signed.
        BinOp::Sar => geser_logis(a, b, lebar, false) | sign_fill(a, b, lebar, signed),
        // Perbandingan bertanda membaca nilai sepanjang lebar operand, bukan
        // sebagai u64 mentah. Helper-nya sama dengan yang dipanggil codegen.
        BinOp::Eq if signed => u64::from(bertanda(a, lebar) == bertanda(b, lebar)),
        BinOp::NotEq if signed => u64::from(bertanda(a, lebar) != bertanda(b, lebar)),
        BinOp::Lt if signed => u64::from(bertanda(a, lebar) < bertanda(b, lebar)),
        BinOp::Gt if signed => u64::from(bertanda(a, lebar) > bertanda(b, lebar)),
        BinOp::Le if signed => u64::from(bertanda(a, lebar) <= bertanda(b, lebar)),
        BinOp::Ge if signed => u64::from(bertanda(a, lebar) >= bertanda(b, lebar)),
        BinOp::Eq => u64::from(a == b),
        BinOp::NotEq => u64::from(a != b),
        BinOp::Lt => u64::from(a < b),
        BinOp::Gt => u64::from(a > b),
        BinOp::Le => u64::from(a <= b),
        BinOp::Ge => u64::from(a >= b),
        // `truthy` pada konstanta selalu `nilai != 0`.
        BinOp::LogAnd => u64::from(a != 0 && b != 0),
        BinOp::LogOr => u64::from(a != 0 || b != 0),
    })
}

/// Geser logis sepanjang lebar logis `lebar`, dengan pengisi nol.
///
/// Meniru `sv_runtime::geser_kiri`/`geser_kanan_logis`, termasuk jumlah geser
/// yang melebihi lebar: hasilnya nol, bukan `u64` yang bergeser melewati
/// kapasitas dan membungkus dengan diam-diam.
fn geser_logis(a: u64, b: u64, lebar: u32, kiri: bool) -> u64 {
    let w = (lebar as usize).min(64);
    let jumlah = usize::try_from(b).unwrap_or(usize::MAX);
    if w == 0 {
        return 0;
    }
    let mut out = 0u64;
    for i in 0..w {
        let src = if kiri {
            i.checked_sub(jumlah)
        } else {
            i.checked_add(jumlah)
        };
        if let Some(s) = src.filter(|s| *s < w) {
            out |= ((a >> s) & 1) << i;
        }
    }
    out
}

/// Bit yang `>>>` isi dengan nilai sign.
///
/// Posisi vacated pada geser kanan adalah bit-**atas** sebanyak jumlah geser,
/// bukan bit bawah: `8'shF0 >>> 1` mengisi bit 7 saja (0xf8), sedangkan mengisi
/// bit 1..7 memberi 0xfe.
fn sign_fill(a: u64, b: u64, lebar: u32, signed: bool) -> u64 {
    let w = (lebar as usize).min(64);
    if !signed || w == 0 {
        return 0;
    }
    let jumlah = usize::try_from(b).unwrap_or(usize::MAX);
    if (a >> (w - 1)) & 1 == 0 {
        return 0;
    }
    let tinggi = jumlah.clamp(1, w);
    if tinggi >= 64 {
        return u64::MAX;
    }
    mask(tinggi as u32) << (w - tinggi)
}

/// Nilai bertanda sepanjang lebar logis `lebar`.
///
/// Meniru `sv_runtime::to_signed`, termasuk cap 64 supaya tidak menggeser
/// berlebihan dan tidak panicked pada lebar di atas 64.
fn bertanda(nilai: u64, lebar: u32) -> i64 {
    let w = (lebar as usize).min(64);
    if w == 0 {
        return 0;
    }
    let mentah = nilai & mask(w as u32);
    let tanda = 1u64 << (w - 1);
    if mentah & tanda == 0 || w == 64 {
        return mentah as i64;
    }
    (mentah as i64) - (1i64 << w)
}

/// Hitung nilai `Un` dari satu konstanta.
///
/// Penting: `lebar_operand` adalah lebar **operand**, bukan lebar hasil.
/// `sv-codegen-rust::expr_gen::write_unary` memanggil helper runtime dengan
/// lebar operand, jadi `|4'b1010` menyatukan keempat bit dan menghasilkan 1,
/// bukan `mask(1)` yang akan menghasilkan 0.
fn nilai_un(op: UnOp, a: u64, lebar_operand: u32) -> u64 {
    let m = mask(lebar_operand);
    match op {
        UnOp::BitNot => !a & m,
        UnOp::LogNot => u64::from(a == 0),
        UnOp::BitNeg => (a & m).wrapping_neg() & m,
        UnOp::RedAnd => u64::from((a & m) == m),
        UnOp::RedNand => u64::from((a & m) != m),
        UnOp::RedOr => u64::from((a & m) != 0),
        UnOp::RedNor => u64::from((a & m) == 0),
        UnOp::RedXor => u64::from((a & m).count_ones() % 2 == 1),
        UnOp::RedXnor => u64::from((a & m).count_ones() % 2 == 0),
    }
}

/// Bentuk konstanta baru hanya bila nilainya muat dalam lebar tipenya.
///
/// Syarat ini adalah pengaman, bukan syarat yang sedang terlihat efeknya:
/// pada jalur simulasi sekarang, nilai yang meluber akan dimask oleh
/// pemformat `$display` maupun penulisan sinyal, sehingga hasilnya sama
/// terlipat atau tidak. Syarat tetap dipertahankan agar konstanta hasil fold
/// selalu berbentuk konstanta yang sah pada lebar tipenya.
fn konstanta_jika_muat(nilai: u64, dt: DataType) -> Option<Expr> {
    if nilai <= mask(dt.width) {
        Some(Expr::constant(nilai, dt))
    } else {
        None
    }
}

/// Lipat ekspresi IR bila seluruh operand-nya konstanta.
///
/// Mengembalikan jumlah ekspresi yang berubah menjadi konstanta.
pub fn fold_expr(expr: &mut Expr) -> u32 {
    let mut n = 0;
    fold_children(expr, &mut n);
    if let Some(hasil) = hitung(expr) {
        *expr = hasil;
        n += 1;
    }
    n
}

/// Lipat anak-anak lebih dulu agar konstanta di dalam bisa ikut terpakai.
fn fold_children(expr: &mut Expr, n: &mut u32) {
    match expr {
        Expr::Bin { lhs, rhs, .. } => {
            *n += fold_expr(lhs);
            *n += fold_expr(rhs);
        }
        Expr::Un { operand, .. } => *n += fold_expr(operand),
        Expr::Select { base, .. } => *n += fold_expr(base),
        Expr::Ternary {
            condition,
            when_true,
            when_false,
            ..
        } => {
            *n += fold_expr(condition);
            *n += fold_expr(when_true);
            *n += fold_expr(when_false);
        }
        Expr::Concat { items, .. } => {
            for item in items.iter_mut() {
                *n += fold_expr(item);
            }
        }
        Expr::Replicate { value, .. } => *n += fold_expr(value),
        Expr::Index { base, index, .. } => {
            *n += fold_expr(base);
            *n += fold_expr(index);
        }
        // LRM §6.14: konstanta di dalam cast harus ikut terlipat, dan cast
        // atas konstanta sendiri bisa dilipat jadi satu konstanta.
        Expr::Cast { operand, .. } => *n += fold_expr(operand),
        Expr::Const { .. } | Expr::SignalRef { .. } | Expr::SimTime { .. } => {}
    }
}

/// Hitung hasil lipatan untuk satu node yang anak-anaknya sudah terlipat.
fn hitung(expr: &Expr) -> Option<Expr> {
    match expr {
        Expr::Bin {
            op,
            lhs,
            rhs,
            data_type,
        } => {
            let (a, a_x, a_z) = sebagai_konst_nukil(lhs)?;
            let (b, b_x, b_z) = sebagai_konst_nukil(rhs)?;
            // LRM §11.4.10: jumlah geser `x`/`z` menghasilkan unknown, dan
            // digit `x`/`z` pada operand kiri ikut terbawa ke posisi yang
            // sama. `geser_logis`/`sign_fill` membaca bit mentah dari `u64`
            // sehingga keduanya kehilangan digit itu — lipatan harus dilewati
            // kalau salah satu operandnya belum pasti. Tanpa penjaga operand
            // KIRI, `8'b1010_xx01 >>> 2` terlipat jadi `00101000` (28)
            // sedangkan iverilog dan runtime helper memberi `00101xxx` (2x).
            if op.is_shift() && (a_x != 0 || a_z != 0 || b_x != 0 || b_z != 0) {
                return None;
            }
            // Perbandingan bertanda hanya sah bila kedua operand signed
            // (LRM §11.4.5), dan lebarnya lebar operand — bukan lebar hasil
            // yang cuma 1 bit.
            //
            // `>>>` juga butuh flag signed, tapi yang ia baca adalah signedness
            // tipe HASIL (LRM §11.4.10) — bukan signedness kedua operand
            // seperti perbandingan.
            let signed = if op.is_relational() {
                op.relational_is_signed(lhs.data_type(), rhs.data_type())
            } else {
                data_type.signed
            };
            let lebar = if op.is_relational() {
                lhs.data_type().width
            } else {
                data_type.width
            };
            let nilai = nilai_bin(*op, a, b, lebar, signed)?;
            konstanta_jika_muat(nilai, *data_type)
        }
        Expr::Un {
            op,
            operand,
            data_type,
        } => {
            let a = sebagai_konst(operand)?;
            let nilai = nilai_un(*op, a, operand.data_type().width);
            konstanta_jika_muat(nilai, *data_type)
        }
        // `a[i]` pada konstanta mengikuti runtime: geser lalu mask ke lebar hasil.
        Expr::Select {
            base,
            lsb,
            data_type,
            ..
        } => {
            let (a, unknown) = sebagai_konst_known(base)?;
            if unknown != 0 {
                return None;
            }
            let nilai = (a >> lsb) & mask(data_type.width);
            konstanta_jika_muat(nilai, *data_type)
        }
        // LRM §11.4.11: `cond ? a : b` hanya dipilih satu cabang, jadi cabang
        // yang tidak dipilih boleh beracuan sinyal.
        Expr::Ternary {
            condition,
            when_true,
            when_false,
            data_type,
        } => {
            let c = sebagai_konst(condition)?;
            let terpilih = if c != 0 { when_true } else { when_false };
            konstanta_jika_muat(sebagai_konst(terpilih)?, *data_type)
        }
        // LRM §6.14: cast hanya mengubah arti lebar, jadi konstantanya boleh
        // dilipat. Bit wildcard `x`/`z` **ikut** dibawa ke posisi yang sama —
        // jangan dilebur jadi angka biasa, karena `casez`/`casex` masih memakai
        // `unknown_mask` sebagai pola dan cast membuat label itu bukan lagi
        // `Expr::Const` sehingga lengannya terbuang diam-diam.
        Expr::Cast {
            operand,
            data_type,
            operand_signed,
        } => {
            let (a, unknown, zmask) = sebagai_konst_nukil(operand)?;
            let (nilai, unknown, zmask) = nilai_cast_dengan_masker(
                a,
                unknown,
                zmask,
                operand.data_type().width,
                *data_type,
                *operand_signed,
            )?;
            Some(Expr::Const {
                value: nilai,
                data_type: *data_type,
                unknown_mask: unknown,
                zmask,
            })
        }
        _ => None,
    }
}

/// Nilai konstanta setelah `tipe'(konstanta)` (LRM §6.14).
///
/// Meniru `sv_runtime::resize` dan `sv_runtime::sign_extend` persis: arah
/// perluasan ditentukan `operand_signed` (aturan assignment LRM §6.2.1), dan
/// bit MSB yang disalin adalah milik *sumber*, bukan tujuan.
///
/// `src` di-cap ke 64 karena nilai konstanta berkode u64; jalur `src > 64`
/// memang tidak terjangkau sekarang (lexer menolak literal lebih dari 64 bit
/// dan `konstanta_jika_muat` tidak pernah membuat lebar itu), tapi tanpa cap
/// `src - 1` akan menggeser berlebihan dan panic di build debug, sementara
/// `!mask(src)` akan jadi 0 — jadi fold menyimpang dari runtime justru di
/// skenario yang paling ingin dilindungi.
/// Nilai konstanta setelah `tipe'(konstanta)` beserta masker `x`/`z` hasilnya
/// (LRM §6.14).
///
/// Digit `x`/`z` mengikuti aturan yang sama dengan `sv_runtime::resize` dan
/// `sv_runtime::sign_extend`:
///
/// - bit di bawah lebar sumber tidak berubah;
/// - `resize` mengisi bit baru dengan nol, jadi `x`/`z` tidak merambat ke atas;
/// - `sign_extend` menyalin bit MSB **sumber**, jadi kalau MSB itu `x`/`z`,
///   seluruh bit baru ikut `x`/`z`.
///
/// Mengembalikan `None` kalau lebar sumber melebihi 64 bit: nilai dan masker
/// sumber tidak bisa diwakili, dan menebak akan menghasilkan pola `casez` yang
/// salah tanpa报错.
fn nilai_cast_dengan_masker(
    a: u64,
    unknown: u64,
    zmask: u64,
    src_lebar: u32,
    dt: DataType,
    operand_signed: bool,
) -> Option<(u64, u64, u64)> {
    if src_lebar > 64 {
        return None;
    }
    let nilai = nilai_cast(a, src_lebar, dt, operand_signed);
    // Bawa `x`/`z` yang masih di dalam lebar sumber apa adanya.
    let dalam = mask(src_lebar);
    let mut unknown_baru = unknown & dalam;
    let mut z_baru = zmask & dalam;
    // Bit yang baru dibuat: nol untuk `resize`, salinan MSB sumber untuk
    // `sign_extend`.
    if dt.width > src_lebar && src_lebar > 0 && operand_signed {
        let msb_unknown = unknown >> (src_lebar - 1) & 1 == 1;
        let msb_z = zmask >> (src_lebar - 1) & 1 == 1;
        if msb_unknown || msb_z {
            let baru = mask(dt.width) & !dalam;
            unknown_baru |= baru;
            if msb_z {
                z_baru |= baru;
            }
        }
    }
    Some((
        nilai,
        unknown_baru & mask(dt.width),
        z_baru & mask(dt.width),
    ))
}

fn nilai_cast(a: u64, src: u32, dt: DataType, operand_signed: bool) -> u64 {
    let src = src.min(64);
    // Bit di atas lebar sumber bukan bagian nilai: `sv_runtime::digit_dalam`
    // menggantinya dengan nol atau tanda, jadi fold harus membuangnya lebih
    // dulu. Tanpa ini, konstanta yang `value`-nya lebih lebar daripada
    // `data_type`-nya (mis. `Token::Number` tidak di-mask oleh lexer) akan
    // dibawa utuh ke hasil, sementara runtime memotongnya — pass yang
    // supposedly semantics-preserving jadi mengubah hasil observabel.
    let a = a & mask(src);
    let isi_atas = if operand_signed && src > 0 && (a >> (src - 1)) & 1 == 1 {
        !mask(src)
    } else {
        0
    };
    (a | isi_atas) & mask(dt.width)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kon(v: u64, w: u32) -> Expr {
        Expr::constant(v, DataType::logic(w))
    }

    fn bin(op: BinOp, a: Expr, b: Expr, w: u32) -> Expr {
        Expr::Bin {
            op,
            lhs: Box::new(a),
            rhs: Box::new(b),
            data_type: DataType::logic(w),
        }
    }

    #[test]
    fn bin_konstan_dilipat() {
        let mut e = bin(BinOp::Add, kon(3, 8), kon(4, 8), 8);
        assert_eq!(fold_expr(&mut e), 1);
        assert_eq!(e, kon(7, 8));
    }

    #[test]
    fn bin_dengan_sinyal_tidak_diubah() {
        let mut e = bin(
            BinOp::Add,
            kon(3, 8),
            Expr::signal_ref(0, DataType::logic(8)),
            8,
        );
        assert_eq!(fold_expr(&mut e), 0);
        assert!(matches!(e, Expr::Bin { .. }));
    }

    /// Bangun node `Cast` dengan flag signedness operand.
    ///
    /// LRM §6.14: arah perluasan ditentukan signedness OPERAND, jadi test
    /// harus menyebutnya sendiri — bukan menebak dari `dt.signed`.
    fn cast(operand: Expr, dt: DataType, operand_signed: bool) -> Expr {
        Expr::Cast {
            operand: Box::new(operand),
            data_type: dt,
            operand_signed,
        }
    }

    /// Konstanta bertipe signed; `kon` selalu unsigned, jadi hasil cast ke
    /// tipe signed harus dibandingkan lewat helper ini.
    fn kon_signed(v: u64, w: u32) -> Expr {
        Expr::constant(v, DataType::signed(w))
    }

    // BUG-2: `Expr::Cast` belum ada lengan di `fold_children`, jadi match
    // tidak lengkap dan konstanta di dalam cast tidak ikut terlipat.

    #[test]
    fn cast_konstan_melebar_ke_unsigned_dilipat() {
        // LRM §6.14: operand unsigned di-zero-extend saat cast melebar,
        // jadi 8'hA5 -> 16'h00A5 dan bukan 16'hFFA5.
        let mut e = cast(kon(0xA5, 8), DataType::logic(16), false);
        assert_eq!(fold_expr(&mut e), 1);
        assert_eq!(e, kon(0x00A5, 16));
    }

    #[test]
    fn cast_konstan_menyempit_memotong() {
        let mut e = cast(kon(0xBEEF, 16), DataType::logic(8), false);
        assert_eq!(fold_expr(&mut e), 1);
        assert_eq!(e, kon(0xEF, 8));
    }

    #[test]
    fn cast_konstan_menyalin_msb_sumber_bila_operand_signed() {
        // -8'sd3 = 8'hFD -> 16'hFFFD. Bit MSB yang disalin adalah milik
        // *sumber*; menyalin MSB tujuan (yang masih nol) akan menghasilkan
        // 0x00FD dan keliru dianggap positif.
        let mut e = cast(kon(0xFD, 8), DataType::signed(16), true);
        assert_eq!(fold_expr(&mut e), 1);
        assert_eq!(e, kon_signed(0xFFFD, 16));
    }

    #[test]
    fn cast_operand_signed_ke_tipe_unsigned_tetap_menyalin() {
        // BUG-1: cast adalah "dievaluasi seolah operand di-assign ke tipe
        // tujuan" (LRM §6.14), jadi yang menentukan arah perluasan adalah
        // signedness OPERAND, bukan tipe tujuan. `word_t'(a)` dengan `a`
        // signed 8 bit harus sign-extend walau `word_t` unsigned.
        // iverilog: -8'sd3 -> 16'hFFFD.
        let mut e = cast(kon(0xFD, 8), DataType::logic(16), true);
        assert_eq!(fold_expr(&mut e), 1);
        assert_eq!(e, kon(0xFFFD, 16));
    }

    #[test]
    fn cast_operand_unsigned_ke_tipe_signed_tidak_menyalin() {
        // BUG-1: kebalikannya. `sword_t'(u)` dengan `u` unsigned 8 bit harus
        // zero-extend walau `sword_t` signed — kalau tidak, 8'hFD akan jadi
        // 16'hFFFD dan terbaca sebagai -3.
        // iverilog: 8'hFD -> 16'h00FD.
        let mut e = cast(kon(0xFD, 8), DataType::signed(16), false);
        assert_eq!(fold_expr(&mut e), 1);
        assert_eq!(e, kon_signed(0x00FD, 16));
    }

    #[test]
    fn cast_menyempit_tidak_perlu_signedness() {
        // Saat menyempit tidak ada bit baru, jadi `operand_signed` tidak
        // berpengaruh: 16'hFFFD -> 8'hFD untuk kedua nilai flag.
        for flag in [true, false] {
            let mut e = cast(kon(0xFFFD, 16), DataType::signed(8), flag);
            assert_eq!(fold_expr(&mut e), 1);
            assert_eq!(e, kon_signed(0xFD, 8));
        }
    }

    #[test]
    fn cast_lebar_sumber_di_atas_64_tidak_panic() {
        // BUG-7: `a >> (src - 1)` dengan `src > 64` akan menggeser berlebihan
        // dan panic di build debug, dan `!mask(src)` jadi 0 sehingga fold
        // menyimpang dari runtime tepat di skenario yang paling ingin
        // dilindungi. Jalur ini belum terjangkau lewat lexer, jadi yang
        // diuji adalah `nilai_cast` langsung.
        //
        // `src` di-cap ke 64, jadi bit MSB yang dibaca adalah bit 63. Dengan
        // MSB sumber 1, semua bit di atas lebar sumber terisi 1.
        let hasil = nilai_cast(u64::MAX, 71, DataType::logic(128), true);
        // Semua 128 bit terisi: 71 bit dari sumber (yang semuanya 1) ditambah
        // bit baru 71..127 yang juga 1.
        assert_eq!(hasil, u64::MAX);

        // Tanpa tanda, bit di atas lebar sumber diisi nol — tidak panicked
        // walau `src > 64`.
        assert_eq!(
            nilai_cast(u64::MAX, 71, DataType::logic(128), false),
            u64::MAX
        );
    }

    #[test]
    fn cast_dengan_sinyal_tidak_diubah() {
        let mut e = cast(
            Expr::signal_ref(0, DataType::logic(8)),
            DataType::logic(16),
            false,
        );
        assert_eq!(fold_expr(&mut e), 0);
        assert!(matches!(e, Expr::Cast { .. }));
    }

    #[test]
    fn konstanta_di_dalam_cast_dilipat_terlebih_dulu() {
        // `word_t'(8'hA5 + 8'h0F)` -> `16'h00B4`: dua lipatan (penjumlahan di
        // dalam, lalu cast-nya).
        let mut e = cast(
            bin(BinOp::Add, kon(0xA5, 8), kon(0x0F, 8), 8),
            DataType::logic(16),
            false,
        );
        assert_eq!(fold_expr(&mut e), 2);
        assert_eq!(e, kon(0x00B4, 16));
    }

    #[test]
    fn cast_dengan_bit_wildcard_membawa_masker_lintas() {
        // BUG-3: cast wildcard dulu tidak terlipat sama sekali, sehingga label
        // `casez`/`casex` yang berupa cast tidak lagi jadi `Expr::Const` dan
        // lengannya terbuang diam-diam — lengan tidak pernah dieksekusi
        // tanpa pesan apa pun.
        //
        // Sekarang cast terlipat ke konstanta yang MEMBAWA `x`/`z` ke posisi
        // yang sama, karena `sv_runtime::resize` juga begitu.
        let mut e = cast(
            Expr::constant_unknown(0xF0, DataType::logic(8), 0x0F),
            DataType::logic(16),
            false,
        );
        assert_eq!(fold_expr(&mut e), 1);
        match e {
            Expr::Const {
                value,
                unknown_mask,
                zmask,
                data_type,
            } => {
                assert_eq!(value, 0x00F0);
                // `resize` mengisi bit baru dengan nol, jadi `x` tidak merambat.
                assert_eq!(unknown_mask, 0x0F, "wildcard tetap di posisi semula");
                assert_eq!(zmask, 0);
                assert_eq!(data_type.width, 16);
            }
            other => panic!("harus terlipat jadi Const, dapat {other:?}"),
        }
    }

    #[test]
    fn cast_wildcard_membawa_z_terpisah_dari_x() {
        // LRM §12.5: `casez` hanya memakai `z` sebagai wildcard, `casex`
        // memakai keduanya. Pemisahan harus bertahan melewati cast.
        let mut e = cast(
            Expr::constant_four_state(0x05, DataType::logic(8), 0x0A, 0x02),
            DataType::logic(16),
            false,
        );
        assert_eq!(fold_expr(&mut e), 1);
        match e {
            Expr::Const {
                unknown_mask,
                zmask,
                ..
            } => {
                assert_eq!(unknown_mask, 0x0A);
                assert_eq!(zmask, 0x02, "posisi z harus terpisah dari x");
            }
            other => panic!("harus terlipat jadi Const, dapat {other:?}"),
        }
    }

    #[test]
    fn cast_signed_menyalin_msb_wildcard_ke_bit_baru() {
        // LRM §6.14: `sign_extend` menyalin MSB sumber ke semua bit baru.
        // Kalau MSB itu `x`, bit baru ikut `x` — mengabaikannya membuat
        // `casez` mencocokkan pola yang seharusnya tidak cocok.
        // Sumber: 8'hx5 -> MSB (bit 7) `x`.
        let mut e = cast(
            Expr::constant_unknown(0x05, DataType::logic(8), 0x80),
            DataType::logic(16),
            true,
        );
        assert_eq!(fold_expr(&mut e), 1);
        match e {
            Expr::Const { unknown_mask, .. } => {
                // bit 7 dari sumber + bit 8..15 hasil salinan MSB.
                assert_eq!(unknown_mask, 0xFF80);
            }
            other => panic!("harus terlipat jadi Const, dapat {other:?}"),
        }
    }

    #[test]
    fn cast_lebar_sumber_di_atas_64_tidak_dilipat() {
        // Lebar sumber di atas 64 bit tidak bisa diwakili masker u64, jadi
        // fold harus menolak alih-alih menebak pola yang salah.
        let mut e = cast(
            Expr::constant_unknown(1, DataType::logic(71), 0),
            DataType::logic(128),
            false,
        );
        assert_eq!(fold_expr(&mut e), 0);
        assert!(matches!(e, Expr::Cast { .. }));
    }

    #[test]
    fn anak_konstan_dilipat_lebih_dulu() {
        // `(1 + 2) * 3` -> `9`: dua lipatan (masing-masing operasi binary).
        let mut e = bin(
            BinOp::Mul,
            bin(BinOp::Add, kon(1, 8), kon(2, 8), 8),
            kon(3, 8),
            8,
        );
        assert_eq!(fold_expr(&mut e), 2);
        assert_eq!(e, kon(9, 8));
    }

    #[test]
    fn hasil_melebihi_lebar_tidak_dilipat() {
        // `4'hF + 1` = 16, tak muat di 4 bit. Memakai konstanta 4-bit dengan
        // nilai 16 akan berbeda dari runtime yang tidak memask, jadi dilepas.
        let mut e = bin(BinOp::Add, kon(0xF, 4), kon(1, 4), 4);
        assert_eq!(fold_expr(&mut e), 0);
        assert!(matches!(e, Expr::Bin { .. }));
    }

    #[test]
    fn pembagian_dengan_pembagi_nol_menghasilkan_nol() {
        // LRM §11.4.5 pada engine 2-state: pembagi nol -> 0.
        let mut e = bin(BinOp::Div, kon(100, 8), kon(0, 8), 8);
        assert_eq!(fold_expr(&mut e), 1);
        assert_eq!(e, kon(0, 8));
    }

    #[test]
    fn pembagian_memakai_lebar_hasil_sebagai_masker() {
        // 8'b1111_1111 / 2 pada lebar 4 = 15 / 2 = 7.
        let mut e = bin(BinOp::Div, kon(0xFF, 8), kon(2, 8), 4);
        assert_eq!(fold_expr(&mut e), 1);
        assert_eq!(e, kon(7, 4));
    }

    #[test]
    fn perbandingan_menghasilkan_satu_bit() {
        let mut e = bin(BinOp::Lt, kon(3, 8), kon(9, 8), 1);
        assert_eq!(fold_expr(&mut e), 1);
        assert_eq!(e, kon(1, 1));
    }

    #[test]
    fn logika_menggunakan_kebenaran_nonzero() {
        let mut e = bin(BinOp::LogAnd, kon(0, 8), kon(5, 8), 1);
        assert_eq!(fold_expr(&mut e), 1);
        assert_eq!(e, kon(0, 1));
    }

    #[test]
    fn reduksi_menghitung_seluruh_bit_lebar_tipe() {
        // 4'b1010: `&` = 0, `~&` = 1, `|` = 1, `^` = 0.
        for (op, hope) in [
            (UnOp::RedAnd, 0),
            (UnOp::RedNand, 1),
            (UnOp::RedOr, 1),
            (UnOp::RedXnor, 1),
        ] {
            let mut e = Expr::Un {
                op,
                operand: Box::new(kon(0b1010, 4)),
                data_type: DataType::logic(1),
            };
            assert_eq!(fold_expr(&mut e), 1, "op {op:?}");
            assert_eq!(e, kon(hope, 1), "op {op:?}");
        }
    }

    #[test]
    fn bitnot_menjaga_lebar_tipe() {
        // 4'b1010 -> `~` = 4'b0101. Bit di atas lebar harus terpotong.
        let mut e = Expr::Un {
            op: UnOp::BitNot,
            operand: Box::new(kon(0b1010, 4)),
            data_type: DataType::logic(4),
        };
        assert_eq!(fold_expr(&mut e), 1);
        assert_eq!(e, kon(0b0101, 4));
    }

    #[test]
    fn select_pada_konstana_menggeser_dan_memask() {
        let mut e = Expr::Select {
            base: Box::new(kon(0b1011_0110, 8)),
            msb: 3,
            lsb: 2,
            data_type: DataType::logic(2),
        };
        assert_eq!(fold_expr(&mut e), 1);
        assert_eq!(e, kon(0b01, 2));
    }

    #[test]
    fn select_dengan_bit_x_tidak_diubah() {
        let mut e = Expr::Select {
            base: Box::new(Expr::constant_unknown(0xF0, DataType::logic(8), 0x0F)),
            msb: 3,
            lsb: 0,
            data_type: DataType::logic(4),
        };
        assert_eq!(fold_expr(&mut e), 0);
        assert!(matches!(e, Expr::Select { .. }));
    }

    #[test]
    fn ternary_memilih_satu_cabang_saja() {
        // Cabang yang dipilih boleh beracuan sinyal karena tak dievaluasi.
        let mut e = Expr::Ternary {
            condition: Box::new(kon(1, 1)),
            when_true: Box::new(kon(7, 8)),
            when_false: Box::new(Expr::signal_ref(3, DataType::logic(8))),
            data_type: DataType::logic(8),
        };
        assert_eq!(fold_expr(&mut e), 1);
        assert_eq!(e, kon(7, 8));
    }

    #[test]
    fn concat_tidak_dilipat_karena_bukan_target_pass_ini() {
        // Sengaja dibiarkan: butuh penggabungan lebar berurutan yang belum
        // divalidasi oleh test runtime.
        let mut e = Expr::Concat {
            items: vec![kon(1, 4), kon(2, 4)],
            data_type: DataType::logic(8),
        };
        assert_eq!(fold_expr(&mut e), 0);
        assert!(matches!(e, Expr::Concat { .. }));
    }

    #[test]
    fn signal_ref_tidak_pernah_dilipat() {
        let mut e = Expr::signal_ref(0, DataType::logic(8));
        assert_eq!(fold_expr(&mut e), 0);
        assert!(matches!(e, Expr::SignalRef { .. }));
    }

    #[test]
    fn geser_dengan_besaran_ge_lebar_dilipat_nol() {
        // Helper runtime menjaga jumlah geser yang melebihi lebar logis, jadi
        // lipatan boleh ikut-fold: hasilnya nol, sama seperti simulasi.
        for op in [BinOp::Shl, BinOp::Shr, BinOp::Sar] {
            let mut e = bin(op, kon(1, 8), kon(64, 8), 8);
            assert_eq!(fold_expr(&mut e), 1, "op {op:?}");
            assert_eq!(e, kon(0, 8), "op {op:?}");
        }
    }

    #[test]
    fn geser_aritmetik_mengisi_bit_tanda_bila_signed() {
        // LRM §11.4.10: `>>>` pada tipe signed mengisi bit sign.
        let mut e = bin(BinOp::Sar, kon(0x80, 8), kon(4, 8), 8);
        if let Expr::Bin { data_type, .. } = &mut e {
            data_type.signed = true;
        }
        assert_eq!(fold_expr(&mut e), 1);
        // Tipe hasil ikut terjaga: konstanta tetap signed seperti node asal.
        assert_eq!(e, Expr::constant(0xF8, DataType::signed(8)));
    }

    #[test]
    fn geser_aritmetik_mengisi_hanya_bit_atas_kosong() {
        // Posisi vacated pada geser kanan adalah bit ATAS sebanyak jumlah
        // geser. Mengisi bit 1..7 (seolah-olah posisi kosong ada di bawah)
        // menghasilkan 0xfe untuk `8'shF0 >>> 1`; iverilog dan LRM §11.4.10
        // memberi 0xf8.
        let mut e = bin(BinOp::Sar, kon(0xF0, 8), kon(1, 8), 8);
        if let Expr::Bin { data_type, .. } = &mut e {
            data_type.signed = true;
        }
        assert_eq!(fold_expr(&mut e), 1);
        assert_eq!(e, Expr::constant(0xF8, DataType::signed(8)));
    }

    #[test]
    fn geser_aritmetik_nol_bila_unsigned() {
        // LRM §11.4.10: pada tipe hasil unsigned `>>>` sama dengan `>>`.
        let mut e = bin(BinOp::Sar, kon(0x80, 8), kon(1, 8), 8);
        assert_eq!(fold_expr(&mut e), 1);
        assert_eq!(e, kon(0x40, 8));
    }

    #[test]
    fn geser_dengan_jumlah_x_tidak_dilipat() {
        // LRM §11.4.10: jumlah geser `x`/`z` menghasilkan unknown, jadi
        // lipatan tidak boleh mengubahnya menjadi angka.
        let jumlah_x = Expr::constant_unknown(0b0010, DataType::logic(4), 0b0010);
        for op in [BinOp::Shl, BinOp::Shr, BinOp::Sar] {
            let mut e = bin(op, kon(0xF0, 8), jumlah_x.clone(), 8);
            assert_eq!(fold_expr(&mut e), 0, "op {op:?}");
            assert!(matches!(e, Expr::Bin { .. }), "op {op:?}");
        }
    }

    #[test]
    fn geser_dengan_besaran_normal_dilipat() {
        let mut e = bin(BinOp::Shl, kon(1, 8), kon(3, 8), 8);
        assert_eq!(fold_expr(&mut e), 1);
        assert_eq!(e, kon(8, 8));
    }

    #[test]
    fn geser_operand_kiri_x_tidak_dilipat() {
        // BUG: `geser_logis`/`sign_fill` membaca bit dari `a` mentah, jadi
        // digit `x`/`z` pada operand KIRI hilang diam-diam: penjaga di
        // `fold_expr` hanya mengecek masker Operand kanan. `8'b1010_xx01 >>> 2`
        // jadi terlipat jadi `0x28`, sementara runtime (`geser` di sv-runtime)
        // memindahkan digit `x` ke posisi barunya dan iverilog mencetak `2X`.
        let kiri_x = Expr::constant_unknown(0b1010_0101, DataType::logic(8), 0b0000_1100);
        for op in [BinOp::Shl, BinOp::Shr, BinOp::Sar] {
            let mut e = bin(op, kiri_x.clone(), kon(2, 8), 8);
            assert_eq!(fold_expr(&mut e), 0, "op {op:?} harus tidak terlipat");
            assert!(matches!(e, Expr::Bin { .. }), "op {op:?}");
        }
    }
}
