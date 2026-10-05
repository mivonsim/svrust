// Tanggung jawab: evaluasi ekspresi SV menjadi konstanta saat elaborasi.
use crate::error::ElaborateError;
use crate::generate::GenvarEnv;
use crate::param::ParamTable;
use sv_ast::expression::{BinaryOp, Expr, UnaryOp};
use sv_lexer::span::Span;

/// Evaluasi ekspresi yang wajib konstan pada waktu elaborasi.
///
/// Dipakai kondisi `if`-generate dan selektor/lengan `case`-generate
/// (LRM §27.1). Nilai yang boleh muncul hanyalah literal, parameter module,
/// genvar yang aktif, dan operasi aritmetika/logika atas semuanya — bukan
/// sinyal, karena sinyal belum punya nilai sebelum simulasi berjalan.
pub fn konst(
    expr: &Expr,
    env: &GenvarEnv,
    params: &ParamTable,
    span: Span,
) -> Result<u64, ElaborateError> {
    match expr {
        Expr::Number(n) => Ok(*n),
        Expr::Sized { value, .. } => Ok(*value),
        // Genvar lebih dulu karena nilainya dipaksa oleh loop, bukan parameter.
        Expr::Ident { name: nama, .. } => match env.cari(nama) {
            Some(nilai) => Ok(nilai),
            // Nama yang bukan genvar harus parameter modul. Bila tidak
            // ditemukan, kemungkinan besar itu sinyal — yang memang tidak
            // boleh jadi kondisi generate.
            None => params.nilai(nama, span).map_err(|_| {
                ElaborateError::new(
                    format!(
                        "ekspresi generate harus konstanta: '{}' bukan genvar atau parameter",
                        nama
                    ),
                    span,
                )
            }),
        },
        Expr::Unary { op, operand, span } => {
            let nilai = konst(operand, env, params, *span)?;
            unary(*op, nilai, *span)
        }
        Expr::Binary { op, lhs, rhs, span } => {
            let a = konst(lhs, env, params, *span)?;
            let b = konst(rhs, env, params, *span)?;
            binary(*op, a, b)
        }
        // LRM §6.14: type cast pada ekspresi konstanta. `if (16'(W) > 8)`
        // adalah pola yang lazim; tanpa cabang ini kondisinya ditolak padahal
        // operandnya konstanta sah.
        Expr::SignCast { operand, .. } => konst(operand, env, params, span),
        Expr::SizeCast { operand, .. } => konst(operand, env, params, span),
        Expr::Cast { operand, .. } => konst(operand, env, params, span),
        Expr::BuiltinCast { operand, .. } => konst(operand, env, params, span),
        // LRM §11.8.1: `{a, b}` — bagian atas lebih dulu, jadi `konst` harus
        // menggeser nilai sebelum menggabungkan.
        Expr::Concat { items, .. } => {
            let mut hasil = 0u64;
            for item in items {
                let lebar = lebar_statik(item);
                hasil = (hasil << lebar) | konst(item, env, params, span)?;
            }
            Ok(hasil)
        }
        // LRM §11.8.2: `{N{x}}` pada ekspresi konstanta.
        //
        // Nilai `x` diulang sebanyak N kali pada lebarnya, jadi hasilnya
        // `x * (1 + 2^w + 2^2w + ...)` — bukan `x * mask(w) * mask(N*w)`
        // seperti percobaan pertama, yang menghasilkan angka yang salah.
        Expr::Replicate { count, value, .. } => {
            let n = konst(count, env, params, span)? as u32;
            let lebar = lebar_statik(value);
            let satu = konst(value, env, params, span)?;
            // `x` diulang sebanyak N kali pada lebarnya, jadi hasilnya
            // `(x & mask(w)) * (1 + 2^w + 2^2w + ...)`. Mask harus
            // diterapkan pada OPERAND-nya dulu; kalau dikalikan setelah
            // hasilnya, angkanya jadi keliru.
            Ok((satu & mask(lebar)).wrapping_mul(rep_multiplier(n, lebar)))
        }
        // LRM §7.4: `a[hi:lo]` dan `a[i]` pada ekspresi konstanta.
        Expr::Select { base, msb, lsb, .. } => {
            let lebar = msb - lsb + 1;
            let nilai = konst(base, env, params, span)?;
            Ok((nilai >> lsb) & mask(lebar))
        }
        // Bentuk lain butuh nilai sinyal atau lebar dinamis, jadi tidak bisa
        // diputuskan pada waktu elaborasi.
        other => Err(ElaborateError::new(
            format!("ekspresi generate harus konstanta, ditemukan {:?}", other),
            span,
        )),
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
    span: Span,
) -> Result<u64, ElaborateError> {
    match expr {
        Expr::Binary { op, lhs, rhs, .. }
            if matches!(op, BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul) =>
        {
            let a = konst(lhs, env, params, span)?;
            let b = konst(rhs, env, params, span)?;
            let hasil = match op {
                BinaryOp::Add => a.checked_add(b),
                BinaryOp::Sub => a.checked_sub(b),
                BinaryOp::Mul => a.checked_mul(b),
                _ => unreachable!("dicek di atas"),
            };
            hasil.ok_or_else(|| {
                ElaborateError::invalid_width(
                    "ekspresi konstanta dimension meluap atau negatif",
                    span,
                )
            })
        }
        _ => konst(expr, env, params, span),
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

#[cfg(test)]
mod tests {
    use super::*;
    use sv_ast::width::ParamDecl;

    fn params_dengan(nilai: &[(&str, u64)]) -> ParamTable {
        let decls: Vec<ParamDecl> = nilai
            .iter()
            .map(|(nama, v)| ParamDecl {
                name: nama.to_string(),
                default: *v,
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

    #[test]
    fn genvar_mengalahkan_parameter() {
        let env = GenvarEnv::kosong().anak("W", 99);
        assert_eq!(
            konst(
                &ident("W"),
                &env,
                &params_dengan(&[("W", 8)]),
                Span::dummy()
            )
            .unwrap(),
            99
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
        // Pesan harus menyebut syarat konstanta, bukan sekadar "undefined
        // parameter", karena penyebab sebenarnya biasanya sinyal.
        let err = eval(&ident("UNKNOWN")).unwrap_err();
        assert!(err.to_string().contains("harus konstanta"), "{}", err);
    }

    #[test]
    fn ekspresi_non_konstan_ditolak() {
        // Select butuh nilai sinyal, tidak bisa diputuskan saat elaborasi.
        let expr = Expr::Select {
            base: Box::new(ident("a")),
            msb: 0,
            lsb: 0,
            span: Span::dummy(),
        };
        assert!(eval(&expr).is_err());
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
