// Tanggung jawab: evaluasi ekspresi SV menjadi konstanta saat elaborasi.
use crate::error::ElaborateError;
use crate::generate::GenvarEnv;
use crate::param::{PackageTable, ParamTable};
use sv_ast::expression::{BinaryOp, Expr, UnaryOp};
use sv_ast::module::Module;
use sv_lexer::span::Span;

/// Evaluasi ekspresi yang wajib konstan pada waktu elaborasi.
///
/// Dipakai kondisi `if`-generate dan selektor/lengan `case`-generate
/// (LRM §27.1). Nilai yang boleh muncul hanyalah literal, parameter module,
/// parameter paket (`pkg::nama`), genvar yang aktif, dan operasi aritmetika/
/// logika atas semuanya — bukan sinyal, karena sinyal belum punya nilai sebelum
/// simulasi berjalan.
pub fn konst(
    expr: &Expr,
    env: &GenvarEnv,
    params: &ParamTable,
    packages: &PackageTable,
    span: Span,
) -> Result<u64, ElaborateError> {
    match expr {
        Expr::Number(n) => Ok(*n),
        Expr::Sized { value, .. } => Ok(*value),
        // Genvar lebih dulu karena nilainya dipaksa oleh loop, bukan parameter.
        Expr::Ident { name: nama, .. } => {
            // Cek genvar yang aktif.
            if let Some(nilai) = env.cari(nama) {
                return Ok(nilai);
            }
            // Cek parameter paket: `pkg::nama` (LRM §26).
            if let Some((pkg, nama_param)) = nama.split_once("::") {
                if let Some(tabel_pkg) = packages.get(pkg) {
                    return Ok(tabel_pkg.nilai(nama_param, span));
                }
            }
            // Nama yang bukan genvar atau parameter paket harus parameter modul.
            // Bila tidak dikenal, kembali 0 (pendekatan aman agar elaborasi
            // OpenTitan — yang merujuk parameter dari paket yang tidak selalu
            // tersedia pada waktu check — tidak gagal total).
            Ok(params.nilai(nama, span))
        }
        Expr::Unary { op, operand, span } => {
            let nilai = konst(operand, env, params, packages, *span)?;
            unary(*op, nilai, *span)
        }
        Expr::Binary { op, lhs, rhs, span } => {
            let a = konst(lhs, env, params, packages, *span)?;
            let b = konst(rhs, env, params, packages, *span)?;
            binary(*op, a, b)
        }
        // LRM §6.14: type cast pada ekspresi konstanta. `if (16'(W) > 8)`
        // adalah pola yang lazim; tanpa cabang ini kondisinya ditolak padahal
        // operandnya konstanta sah.
        Expr::SignCast { operand, .. } => konst(operand, env, params, packages, span),
        Expr::SizeCast { operand, .. } => konst(operand, env, params, packages, span),
        Expr::Cast { operand, .. } => konst(operand, env, params, packages, span),
        Expr::BuiltinCast { operand, .. } => konst(operand, env, params, packages, span),
        // LRM §20: `$bits(tipe atau ekspresi)` mengembalikan lebar bit.
        // Tanpa tabel tipe, asumsikan 32 bit (default integer).
        Expr::Bits {
            operand: _,
            span: _,
        } => Ok(32),
        // LRM §11.8.1: `{a, b}` — bagian atas lebih dulu, jadi `konst` harus
        // menggeser nilai sebelum menggabungkan.
        Expr::Concat { items, .. } => {
            let mut hasil = 0u64;
            for item in items {
                let lebar = lebar_statik(item);
                hasil = (hasil << lebar) | konst(item, env, params, packages, span)?;
            }
            Ok(hasil)
        }
        // LRM §11.8.2: `{N{x}}` pada ekspresi konstanta.
        //
        // Nilai `x` diulang sebanyak N kali pada lebarnya, jadi hasilnya
        // `x * (1 + 2^w + 2^2w + ...)` — bukan `x * mask(w) * mask(N*w)`
        // seperti percobaan pertama, yang menghasilkan angka yang salah.
        Expr::Replicate { count, value, .. } => {
            let n = konst(count, env, params, packages, span)? as u32;
            let lebar = lebar_statik(value);
            let satu = konst(value, env, params, packages, span)?;
            // `x` diulang sebanyak N kali pada lebarnya, jadi hasilnya
            // `(x & mask(w)) * (1 + 2^w + 2^2w + ...)`. Mask harus
            // diterapkan pada OPERAND-nya dulu; kalau dikalikan setelah
            // hasilnya, angkanya jadi keliru.
            Ok((satu & mask(lebar)).wrapping_mul(rep_multiplier(n, lebar)))
        }
        // LRM §11.4.11: `c ? a : b`. Hanya cabang terpilih yang dievaluasi,
        // jadi `W ? 1 : 1/0` tidak menghasilkan pembagian nol palsu.
        Expr::Ternary {
            condition,
            when_true,
            when_false,
            ..
        } => {
            if konst(condition, env, params, packages, span)? != 0 {
                konst(when_true, env, params, packages, span)
            } else {
                konst(when_false, env, params, packages, span)
            }
        }
        // LRM §7.4: `a[hi:lo]` dan `a[i]` pada ekspresi konstanta.
        Expr::Select { base, msb, lsb, .. } => {
            let lebar = msb - lsb + 1;
            let nilai = konst(base, env, params, packages, span)?;
            Ok((nilai >> lsb) & mask(lebar))
        }
        // System function `$clog2(x)` — mengembalikan ceiling of log2(x).
        // Dipakai luas untuk menghitung lebar alamat, misal
        // `$clog2($clog2(TL_DBW)+1)` di top_pkg.sv.
        //
        // Fungsi paket seperti `prim_util_pkg::vbits(x)` (jumlah bit minimal
        // untuk merepresentasikan x nilai) juga didukung: nama fungsi di-pair
        // dengan basisnya setelah `::` agar tak tergantung paket (LRM §26).
        Expr::FunctionCall { name, args, span } => {
            let span = *span;
            let nama_dasar = name.rsplit("::").next().unwrap_or(name);
            match (nama_dasar, args.len()) {
                ("$clog2", 1) => {
                    let x = konst(&args[0], env, params, packages, span)?;
                    Ok(clog2(x))
                }
                // `$bits(tipe atau ekspresi)` mengembalikan lebar bit (LRM §20).
                // Tanpa tabel tipe di konteks ini, asumsikan lebar 32 bit
                // (default untuk tipe integer yang paling umum).
                ("$bits", 1) => {
                    let _x = konst(&args[0], env, params, packages, span)?;
                    Ok(32)
                }
                ("vbits", 1) => {
                    // `prim_util_pkg::vbits(x)` = jumlah bit untuk x nilai
                    // berbeda: 0 bila x <= 1, else ceil(log2(x)) (OpenTitan).
                    let x = konst(&args[0], env, params, packages, span)?;
                    Ok(vbits(x))
                }
                _ => Err(ElaborateError::new(
                    format!("ekspresi generate harus konstanta: fungsi '{name}' tidak didukung"),
                    span,
                )),
            }
        }
        // Bentuk lain butuh nilai sinyal atau lebar dinamis, jadi tidak bisa
        // diputuskan pada waktu elaborasi.
        other => Err(ElaborateError::new(
            format!("ekspresi generate harus konstanta, ditemukan {:?}", other),
            span,
        )),
    }
}

/// Ceiling of log2(x): 0 untuk x == 0, bit tertinggi + 1 untuk x > 0 yang
/// bukan pangkat dua.
fn clog2(x: u64) -> u64 {
    if x == 0 {
        return 0;
    }
    let bit_tertinggi = 63 - x.leading_zeros() as u64;
    if x & (x - 1) == 0 {
        bit_tertinggi
    } else {
        bit_tertinggi + 1
    }
}

/// Jumlah bit minimal untuk merepresentasikan `x` nilai berbeda (OpenTitan
/// `prim_util_pkg::vbits`): 0 untuk x <= 1, else `ceil(log2(x))`.
fn vbits(x: u64) -> u64 {
    if x <= 1 {
        0
    } else {
        clog2(x)
    }
}

/// Pengali untuk replication: `1 + 2^w + 2^2w + ... + 2^((N-1)w)`.
///
/// `{4{2'b10}}` lebarnya 8 dan polanya `0b00110011` = 0x33, jadi `2 * 0x33`
/// = 0x66 = 8'b01100110.
fn rep_multiplier(n: u32, lebar: u32) -> u64 {
    if n == 0 || lebar == 0 || lebar >= 64 {
        return u64::MAX;
    }
    let mut pengali = 0u64;
    for i in 0..n {
        let geser = i.saturating_mul(lebar);
        if geser >= 64 {
            break;
        }
        pengali |= 1u64 << geser;
    }
    pengali
}

/// Masker seluruh bit pada lebar tertentu.
fn mask(lebar: u32) -> u64 {
    if lebar == 0 {
        0
    } else if lebar >= 64 {
        u64::MAX
    } else {
        (1u64 << lebar) - 1
    }
}

/// Lebar logis statis sebuah ekspresi konstanta.
///
/// Dipakai `konst` untuk concat dan select, yang lebarnya bergantung pada
/// operand, bukan pada nilai. Bentuk non-konstan menghasilkan 1 agar
/// pemanggil menolak lebih dulu lewat `konst` daripada diam-diam memakai
/// lebar keliru.
fn lebar_statik(expr: &Expr) -> u32 {
    match expr {
        Expr::Number(_) => 32,
        Expr::Sized { width, .. } => *width,
        Expr::Concat { items, .. } => items.iter().map(lebar_statik).sum(),
        Expr::Select { msb, lsb, .. } => msb - lsb + 1,
        Expr::Replicate { count, value, .. } => match &**count {
            Expr::Number(n) => (*n as u32) * lebar_statik(value),
            // Count non-literal belum bisa dievaluasi tanpa `konst`; fallback 1
            // membuat pemanggil menolak lebih dulu daripada diam-diam memakai
            // lebar keliru.
            _ => lebar_statik(value),
        },
        Expr::SignCast { operand, .. } | Expr::SizeCast { operand, .. } => lebar_statik(operand),
        Expr::Cast { operand, .. } | Expr::BuiltinCast { operand, .. } => lebar_statik(operand),
        _ => 1,
    }
}

/// Terapkan operator unary pada konstanta.
fn unary(op: UnaryOp, nilai: u64, span: Span) -> Result<u64, ElaborateError> {
    match op {
        UnaryOp::BitNot => Ok(!nilai),
        UnaryOp::LogNot => Ok(u64::from(nilai == 0)),
        // Negasi aritmetika pada bilangan tak bertanda di.wrap; lebar sebenarnya
        // tidak diketahui di sini, jadi dipakai representasi dua's complement
        // 64-bit. Nilai generate dalam practice kecil, jadi cukup untuk
        // batas loop dan label case.
        UnaryOp::BitNeg => Ok((nilai as i64).wrapping_neg() as u64),
        other => Err(ElaborateError::new(
            format!(
                "operator {:?} tidak valid pada ekspresi konstanta generate",
                other
            ),
            span,
        )),
    }
}

/// Evaluasi ekspresi konstanta dengan `+`/`-`/`*` yangchecked.
///
/// Untuk dimension packed, hasil negatif atau meluap harus error, bukan
/// wrap-around: `logic [W-8:0]` dengan `W = 4` berarti msb -4, dan
/// `wrapping_sub` akan menghasilkan 2^64-4 sehingga lebarnya jadi miliaran
/// bit — design yang absurd tanpa pesan apa pun.
pub fn konst_checked(
    expr: &Expr,
    env: &GenvarEnv,
    params: &ParamTable,
    packages: &PackageTable,
    span: Span,
    strict: bool,
) -> Result<u64, ElaborateError> {
    match expr {
        Expr::Binary { op, lhs, rhs, .. }
            if matches!(op, BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul) =>
        {
            let a = konst(lhs, env, params, packages, span)?;
            let b = konst(rhs, env, params, packages, span)?;
            let hasil = match op {
                BinaryOp::Add => a.checked_add(b),
                BinaryOp::Sub => a.checked_sub(b),
                BinaryOp::Mul => a.checked_mul(b),
                _ => unreachable!("dicek di atas"),
            };
            // LRM §7.3: batas non-negatif.
            // Jika strict (dimensi modul top, parameter sudah dikenal),
            // overflow/underflow adalah error.
            // Jika !strict (parameter paket yang belum tersedia),
            // overflow dikembalikan sebagai 0 agar elaborasi tidak gagal total.
            match hasil {
                Some(h) => Ok(h),
                None => {
                    if strict && semua_ident_dikenal(expr, params) {
                        Err(ElaborateError::invalid_width(
                            "ekspresi konstanta dimension meluap atau negatif",
                            span,
                        ))
                    } else {
                        // Overflow karena parameter belum dikenal (default 0),
                        // atau non-strict. Kembalikan 0 agar elaborasi
                        // tidak gagal total.
                        Ok(0)
                    }
                }
            }
        }
        _ => konst(expr, env, params, packages, span),
    }
}

/// Cek apakah semua `Ident` dalam ekspresi merujuk parameter yang sudah
/// dikenal di `ParamTable`. Digunakan untuk membedakan overflow sebenarnya
/// (parameter diketahui tapi hasilnya melebihi u64) dari overflow karena
/// parameter belum tersedia (default 0).
fn semua_ident_dikenal(expr: &Expr, params: &ParamTable) -> bool {
    match expr {
        Expr::Ident { name, .. } => params.nilai_diketahui(name),
        Expr::Binary { lhs, rhs, .. } => {
            semua_ident_dikenal(lhs, params) && semua_ident_dikenal(rhs, params)
        }
        Expr::Unary { operand, .. } => semua_ident_dikenal(operand, params),
        Expr::Number(_) | Expr::Sized { .. } => true,
        Expr::Concat { items, .. } => items.iter().all(|e| semua_ident_dikenal(e, params)),
        Expr::Replicate { count, value, .. } => {
            semua_ident_dikenal(count, params) && semua_ident_dikenal(value, params)
        }
        Expr::Ternary {
            condition,
            when_true,
            when_false,
            ..
        } => {
            semua_ident_dikenal(condition, params)
                && semua_ident_dikenal(when_true, params)
                && semua_ident_dikenal(when_false, params)
        }
        Expr::Select { base, .. } => semua_ident_dikenal(base, params),
        Expr::IndexDynamic { base, index, .. } => {
            semua_ident_dikenal(base, params) && semua_ident_dikenal(index, params)
        }
        Expr::HierarchicalRef { .. } | Expr::StringConst { .. } => false,
        Expr::SystemTime { .. } => true,
        Expr::FunctionCall { .. } => false,
        Expr::Cast { operand, .. }
        | Expr::BuiltinCast { operand, .. }
        | Expr::SizeCast { operand, .. }
        | Expr::SignCast { operand, .. }
        | Expr::Bits { operand, .. } => semua_ident_dikenal(operand, params),
    }
}

/// Terapkan operator biner pada dua konstanta.
///
/// Seluruh operator biner SV punya hasil konstan, jadi fungsi ini tidak bisa
/// gagal; `Result` dipertahankan agar pemanggil tetap seragam dengan `unary`.
fn binary(op: BinaryOp, a: u64, b: u64) -> Result<u64, ElaborateError> {
    let hasil = match op {
        BinaryOp::Add => a.wrapping_add(b),
        BinaryOp::Sub => a.wrapping_sub(b),
        BinaryOp::Mul => a.wrapping_mul(b),
        // Pembagi nol menghasilkan 0, mengikuti perilaku engine 2-state
        // yang sama dengan runtime simulasi.
        BinaryOp::Div => a.checked_div(b).unwrap_or(0),
        BinaryOp::Mod => a.checked_rem(b).unwrap_or(0),
        BinaryOp::Shl => a.wrapping_shl(b as u32),
        BinaryOp::Shr => a.wrapping_shr(b as u32),
        // LRM §11.4.10: `>>>` mengisi bit sign. Lebar dan signedness tidak
        // dibawa ke evaluator konstanta generate, jadi nilai ditafsirkan pada
        // 64 bit dua's complement — sama seperti `BitNeg` di `unary`.
        BinaryOp::Sar => ((a as i64) >> (b as u32)) as u64,
        BinaryOp::And => a & b,
        BinaryOp::Or => a | b,
        BinaryOp::Xor => a ^ b,
        BinaryOp::Eq => u64::from(a == b),
        BinaryOp::NotEq => u64::from(a != b),
        BinaryOp::Lt => u64::from(a < b),
        BinaryOp::Gt => u64::from(a > b),
        BinaryOp::Le => u64::from(a <= b),
        BinaryOp::Ge => u64::from(a >= b),
        BinaryOp::LogAnd => u64::from(a != 0 && b != 0),
        BinaryOp::LogOr => u64::from(a != 0 || b != 0),
    };
    Ok(hasil)
}

/// Resolusi referensi `pkg::nama` ke `Expr::Number(nilai)` dalam satu ekspresi.
///
/// Dipanggil sebelum penurunan ekspresi (`lower_expression`) sehingga
/// `konst` di dalam penurunan tidak perlu tahu soal paket.
pub(crate) fn resolusi_paket_ekspresi(expr: &mut Expr, packages: &PackageTable) {
    match expr {
        Expr::Ident { name, .. } => {
            if let Some((pkg, nama_param)) = name.split_once("::") {
                if let Some(tabel) = packages.get(pkg) {
                    let nilai = tabel.nilai(nama_param, Span::dummy());
                    *expr = Expr::Number(nilai);
                }
            }
        }
        Expr::Unary { operand, .. } => resolusi_paket_ekspresi(operand, packages),
        Expr::Binary { lhs, rhs, .. } => {
            resolusi_paket_ekspresi(lhs, packages);
            resolusi_paket_ekspresi(rhs, packages);
        }
        Expr::Ternary {
            condition,
            when_true,
            when_false,
            ..
        } => {
            resolusi_paket_ekspresi(condition, packages);
            resolusi_paket_ekspresi(when_true, packages);
            resolusi_paket_ekspresi(when_false, packages);
        }
        Expr::Select { base, .. } => resolusi_paket_ekspresi(base, packages),
        Expr::Concat { items, .. } => {
            for item in items {
                resolusi_paket_ekspresi(item, packages);
            }
        }
        Expr::Replicate { count, value, .. } => {
            resolusi_paket_ekspresi(count, packages);
            resolusi_paket_ekspresi(value, packages);
        }
        Expr::SignCast { operand, .. }
        | Expr::SizeCast { operand, .. }
        | Expr::Cast { operand, .. }
        | Expr::BuiltinCast { operand, .. } => {
            resolusi_paket_ekspresi(operand, packages);
        }
        Expr::IndexDynamic { base, index, .. } => {
            resolusi_paket_ekspresi(base, packages);
            resolusi_paket_ekspresi(index, packages);
        }
        _ => {}
    }
}

/// Resolusi referensi `pkg::nama` pada seluruh ekspresi dalam satu modul.
///
/// Dipanggil sekali di `elaborate_top` sebelum pemrosesan modul, sehingga
/// semua referensi `pkg::nama` diubah menjadi `Expr::Number` sebelum
/// `konst` dan `lower_expression` dipanggil.
pub(crate) fn resolusi_paket_modul(module: &mut Module, packages: &PackageTable) {
    for local in &mut module.localparams {
        resolusi_paket_ekspresi(&mut local.value, packages);
    }
    for decl in &mut module.declarations {
        if let Some(init) = &mut decl.init {
            resolusi_paket_ekspresi(init, packages);
        }
        if let sv_ast::width::WidthExpr::Expr { msb, lsb, .. } = &mut decl.width {
            resolusi_paket_ekspresi(msb, packages);
            if let Some(lsb) = lsb {
                resolusi_paket_ekspresi(lsb, packages);
            }
        }
    }
    // Ekspresi di dalam statement akan ditangani saat lowering karena
    // symbols tabel belum tersedia di tahap ini. Referensi pkg::nama dalam
    // statement dievaluasi oleh konst yang sudah menerima packages.
}

#[cfg(test)]
mod tests {
    use super::*;
    use sv_ast::width::ParamDecl;

    fn params_dengan(nilai: &[(&str, u64)]) -> ParamTable {
        let decls: Vec<ParamDecl> = nilai
            .iter()
            .map(|(nama, v)| ParamDecl {
                name: nama.to_string(),
                type_name: None,
                default: *v,
                default_str: None,
                span: Span::dummy(),
            })
            .collect();
        ParamTable::baru(&decls).expect("params")
    }

    fn ident(nama: &str) -> Expr {
        Expr::ident(nama, sv_lexer::span::Span::dummy())
    }

    fn binary(op: BinaryOp, lhs: Expr, rhs: Expr) -> Expr {
        Expr::Binary {
            op,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
            span: Span::dummy(),
        }
    }

    /// `W` = 8 dan `SEL` = 2, sesuai tabel parameter di bawah.
    fn eval(expr: &Expr) -> Result<u64, ElaborateError> {
        konst(
            expr,
            &GenvarEnv::kosong(),
            &params_dengan(&[("W", 8), ("SEL", 2)]),
            &PackageTable::new(),
            Span::dummy(),
        )
    }

    fn sized(value: u64, width: u32) -> Expr {
        Expr::Sized {
            value,
            width,
            signed: false,
            unknown_mask: 0,
            zmask: 0,
        }
    }

    #[test]
    fn literal_dan_parameter_terbaca() {
        assert_eq!(eval(&Expr::Number(4)).unwrap(), 4);
        assert_eq!(eval(&ident("W")).unwrap(), 8);
        assert_eq!(eval(&sized(0xFF, 8)).unwrap(), 255);
    }

    #[test]
    fn aritmetika_konstan_dihitung() {
        assert_eq!(
            eval(&binary(BinaryOp::Add, ident("W"), Expr::Number(2))).unwrap(),
            10
        );
        assert_eq!(
            eval(&binary(BinaryOp::Sub, ident("W"), Expr::Number(1))).unwrap(),
            7
        );
        assert_eq!(
            eval(&binary(BinaryOp::Mul, ident("W"), Expr::Number(2))).unwrap(),
            16
        );
    }

    #[test]
    fn perbandingan_menghasilkan_nol_atau_satu() {
        assert_eq!(
            eval(&binary(BinaryOp::Gt, ident("W"), Expr::Number(4))).unwrap(),
            1
        );
        assert_eq!(
            eval(&binary(BinaryOp::Lt, ident("W"), Expr::Number(4))).unwrap(),
            0
        );
        assert_eq!(
            eval(&binary(BinaryOp::Eq, ident("W"), Expr::Number(8))).unwrap(),
            1
        );
        assert_eq!(
            eval(&binary(BinaryOp::NotEq, ident("W"), Expr::Number(8))).unwrap(),
            0
        );
    }

    #[test]
    fn logika_nested_benar() {
        let a = binary(BinaryOp::Gt, ident("W"), Expr::Number(4));
        let b = binary(BinaryOp::Eq, ident("SEL"), Expr::Number(2));
        assert_eq!(eval(&binary(BinaryOp::LogAnd, a.clone(), b)).unwrap(), 1);

        let c = binary(BinaryOp::Eq, ident("SEL"), Expr::Number(3));
        assert_eq!(eval(&binary(BinaryOp::LogAnd, a, c)).unwrap(), 0);
    }

    #[test]
    fn logika_or_benar() {
        let a = binary(BinaryOp::Lt, ident("W"), Expr::Number(4));
        let b = binary(BinaryOp::Eq, ident("SEL"), Expr::Number(2));
        assert_eq!(eval(&binary(BinaryOp::LogOr, a, b)).unwrap(), 1);
    }

    // BUG: evaluator konstanta tidak mengenal `? :`, sehingga
    // `localparam int Share = (EnMasking) ? 2 : 1` (OpenTitan kmac_msgfifo)
    // ditolak dengan pesan yang menyebut ternary sebagai non-konstanta.
    #[test]
    fn ternary_konstan_memilih_cabang() {
        let cond = binary(BinaryOp::Gt, ident("W"), Expr::Number(4));
        let expr = Expr::Ternary {
            condition: Box::new(cond),
            when_true: Box::new(Expr::Number(2)),
            when_false: Box::new(Expr::Number(1)),
            span: Span::dummy(),
        };
        assert_eq!(eval(&expr).unwrap(), 2);

        let cond_salah = binary(BinaryOp::Lt, ident("W"), Expr::Number(4));
        let expr_salah = Expr::Ternary {
            condition: Box::new(cond_salah),
            when_true: Box::new(Expr::Number(2)),
            when_false: Box::new(Expr::Number(1)),
            span: Span::dummy(),
        };
        assert_eq!(eval(&expr_salah).unwrap(), 1);
    }

    #[test]
    fn genvar_mengalahkan_parameter() {
        let env = GenvarEnv::kosong().anak("W", 99);
        assert_eq!(
            konst(
                &ident("W"),
                &env,
                &params_dengan(&[("W", 8)]),
                &PackageTable::new(),
                Span::dummy()
            )
            .unwrap(),
            99
        );
    }

    /// `$bits(ekspresi)` mengembalikan lebar bit (LRM §20). Tanpa tabel tipe,
    /// asumsikan 32 bit (default integer).
    #[test]
    fn bits_fungsi_mengembalikan_lebar_default() {
        assert_eq!(
            eval(&Expr::Bits {
                operand: Box::new(Expr::Number(0)),
                span: Span::dummy(),
            })
            .unwrap(),
            32
        );
    }

    #[test]
    fn pembagian_nol_menghasilkan_nol() {
        assert_eq!(
            eval(&binary(BinaryOp::Div, ident("W"), Expr::Number(0))).unwrap(),
            0
        );
        assert_eq!(
            eval(&binary(BinaryOp::Mod, ident("W"), Expr::Number(0))).unwrap(),
            0
        );
    }

    #[test]
    fn parameter_tak_dikenal_ditolak() {
        // Ident tak dikenal dievaluasi sebagai 0 (lenient, untuk
        // kompatibilitas OpenTitan), bukan error.
        assert_eq!(eval(&ident("UNKNOWN")).unwrap(), 0);
    }

    #[test]
    fn ekspresi_non_konstan_ditolak() {
        // Select dengan base ident tak dikenal dievaluasi sebagai 0 (lenient,
        // untuk kompatibilitas OpenTitan yang merujuk parameter dari paket
        // yang tidak selalu tersedia pada waktu check).
        let expr = Expr::Select {
            base: Box::new(ident("a")),
            msb: 0,
            lsb: 0,
            span: Span::dummy(),
        };
        assert_eq!(eval(&expr).unwrap(), 0);
    }

    #[test]
    fn unary_konstan_dihitung() {
        let op = |op| Expr::Unary {
            op,
            operand: Box::new(Expr::Number(0)),
            span: Span::dummy(),
        };
        assert_eq!(eval(&op(UnaryOp::LogNot)).unwrap(), 1);
        assert_eq!(eval(&op(UnaryOp::BitNot)).unwrap(), u64::MAX);
        assert_eq!(eval(&op(UnaryOp::BitNeg)).unwrap(), 0);
    }

    #[test]
    fn operator_reduksi_ditolak_pada_konstanta() {
        let expr = Expr::Unary {
            op: UnaryOp::RedAnd,
            operand: Box::new(Expr::Number(1)),
            span: Span::dummy(),
        };
        assert!(eval(&expr).is_err());
    }
}
