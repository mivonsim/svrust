// Tanggung jawab: generate kode Rust untuk ekspresi IR.
use crate::indent::Indent;
use sv_ir::{BinOp, DataType, Expr, UnOp};

/// Tulis ekspresi IR sebagai Rust ke writer.
///
/// **Selalu** menghasilkan nilai bertipe `Bits<MAX_WIDTH>`, termasuk
/// perbandingan, operator logika, dan `!` yang secara semantik boolean.
///
/// Emission `bool` pernah dipakai untuk hasil perbandingan, tapi begitu nilai
/// itu masuk ke concat, cast, `$display`, atau `sign_extend` — yang semuanya
/// mengharapkan `Bits` — kodenya gagal dikompilasi. Membungkus sekali di sini
/// jauh lebih murah daripada membuat setiap konsumen memeriksa tipenya.
///
/// Lebar logis tiap ekspresi tetap dibawa sebagai const generic ke helper
/// runtime.
pub fn write_expr(expr: &Expr, out: &mut String, indent: &Indent) {
    // Perbandingan, logika, dan `!` disimuulkan sebagai `bool` Rust supaya
    // `&&`/`||` dapat memakai short-circuit dan tidak membangun nilai yang
    // dibuang.
    if expr_is_bool(expr) {
        let mut bool_code = String::new();
        write_bool_expr(expr, &mut bool_code, indent);
        out.push_str("Bits::<MAX_WIDTH>::from_u64((");
        out.push_str(&bool_code);
        out.push_str(") as u64)");
        return;
    }
    write_bits_expr(expr, out, indent);
}

/// Tulis operand sebagai `&<ekspresi>` untuk helper yang menerima referensi.
///
/// Helper runtime `index_read` butuh `&Bits` supaya tidak menyalin vektor
/// penuh. Tanpa `&` di sini kode yang dihasilkan gagal dikompilasi Rust dengan
/// pesan "expected `&Bits<64>`, found `Bits<64>`".
fn write_operand_ref(expr: &Expr, out: &mut String, indent: &Indent) {
    out.push('&');
    write_expr(expr, out, indent);
}

/// Tulis ekspresi yang hasilnya `Bits<MAX_WIDTH>`.
///
/// Dipanggil [`write_expr`] setelah expr_is_bool dipastikan salah, jadi tidak
/// ada lengan boolean di sini.
fn write_bits_expr(expr: &Expr, out: &mut String, indent: &Indent) {
    match expr {
        // LRM §5.7.1: digit `x`/`z` pada literal adalah bagian nilai, bukan
        // sekadar tidak ada. Tanpa masker di-emit, `8'hxA` akan tercetak `0a`
        // dan `casez`/`casex` kehilangan wildcard-nya.
        Expr::Const {
            value,
            unknown_mask,
            zmask,
            data_type,
        } if *unknown_mask != 0 || *zmask != 0 => {
            out.push_str(&format!(
                "Bits::<MAX_WIDTH>::from_unknown({value}, {unknown_mask}, {zmask}, {})",
                data_type.width
            ));
        }
        Expr::Const { value, .. } => {
            out.push_str(&format!("Bits::<MAX_WIDTH>::from_u64({value})"));
        }
        Expr::SignalRef { signal, .. } => {
            out.push_str(&format!("self.signals[{}].read()", signal));
        }
        Expr::Bin { op, lhs, rhs, data_type } => {
            match *op {
                // Pembagian/sisa lewat helper runtime agar pembagi nol aman.
                BinOp::Div => {
                    write_helper_binary("bit_div", expr_width(expr), lhs, rhs, out, indent)
                }
                BinOp::Mod => {
                    write_helper_binary("bit_mod", expr_width(expr), lhs, rhs, out, indent)
                }
                // Geser lewat helper runtime: operator Rust `<<`/`>>` bekerja
                // pada vektor penuh `MAX_WIDTH`, sedangkan LRM §11.4.10
                // menentukan lebar dari operand kiri dan meminta jumlah
                // geser `x`/`z` menghasilkan unknown.
                BinOp::Shl => {
                    write_shift("geser_kiri", lhs.data_type().width, lhs, rhs, None, out, indent)
                }
                BinOp::Shr => write_shift(
                    "geser_kanan_logis",
                    lhs.data_type().width,
                    lhs,
                    rhs,
                    None,
                    out,
                    indent,
                ),
                BinOp::Sar => write_shift(
                    "geser_kanan_aritmetik",
                    lhs.data_type().width,
                    lhs,
                    rhs,
                    Some(data_type.signed),
                    out,
                    indent,
                ),
                _ => {
                    out.push('(');
                    write_expr(lhs, out, indent);
                    out.push_str(&format!(" {} ", bin_op_token(*op)));
                    write_expr(rhs, out, indent);
                    out.push(')');
                }
            }
        }
        Expr::Un { op, operand, .. } => write_unary(*op, operand, out, indent),
        // Part-select lewat helper runtime, bukan `to_u64`: digit `X`/`Z`
        // ikut terbawa ke posisi yang sama, sedangkan `to_u64` membuangnya
        // sehingga `a[7:4]` pada `8'hxA` akan hilang wildcard-nya.
        Expr::Select { base, lsb, .. } => {
            // Lebar logis base ikut diteruskan supaya penjaga out-of-bounds
            // di helper membandingkan terhadap lebar sinyal, bukan `MAX_WIDTH`.
            let w = base.data_type().width as usize;
            out.push_str(&format!("sv_runtime::select_kbits::<{w}, MAX_WIDTH>("));
            write_expr(base, out, indent);
            out.push_str(&format!(", {lsb}, {}", expr_width(expr)));
            out.push(')');
        }
        Expr::Ternary {
            condition,
            when_true,
            when_false,
            ..
        } => {
            // Cabang dipilih sebagai nilai utuh, bukan lewat `to_u64` —
            // LRM §11.4.11 tidak mengubah nilai cabang, dan `X`/`Z` di
            // cabang terpilih harus tetap terlihat.
            out.push_str("(if ");
            write_truthy(condition, out, indent);
            out.push_str(" { ");
            write_expr(when_true, out, indent);
            out.push_str(" } else { ");
            write_expr(when_false, out, indent);
            out.push_str(" })");
        }
        // LRM §11.8.1: item pertama menjadi MSB, digeser sesuai lebar di kanannya.
        Expr::Concat { items, .. } => write_concat(items, out, indent),
        // LRM §11.8.2: operand digandakan sebanyak `count` kali.
        //
        // Replikasi lewat pengali integer (`v.to_u64() * multiplier`) membuang
        // `X`/`Z`, jadi tiap salinan digeser sebagai digit lewat helper runtime.
        Expr::Replicate { count, value, .. } => {
            let width = value.data_type().width as usize;
            let mut expr = String::new();
            write_expr(value, &mut expr, indent);
            if width == 0 || *count == 0 {
                out.push_str("Bits::<MAX_WIDTH>::zero()");
                return;
            }
            out.push_str("sv_runtime::concat_kbits::<MAX_WIDTH>(&[");
            for _ in 0..*count {
                out.push_str(&expr);
                out.push_str(", ");
            }
            out.push_str("], &[");
            for _ in 0..*count {
                out.push_str(&format!("{width}, "));
            }
            out.push(']');
            out.push(')');
        }
        // LRM §20: `$time` membaca waktu simulasi yang berjalan di design.
        Expr::SimTime { .. } => {
            out.push_str("Bits::<MAX_WIDTH>::from_u64(self.time_now.nanos())");
        }
        // LRM §7.8: indeks dinamis lewat helper runtime; hasilnya `Bits` dengan
        // lebar elemen di bit rendah. Dua semantik berbeda memilih helper berbeda:
        // array unpacked memakai `index_read` (elemen pada `i * lebar`), sinyal
        // packed memakai `select_bit` (satu bit pada `i`).
        //
        // Lebar **logis** base ikut diteruskan sebagai const generic `W` supaya
        // penjaga out-of-bounds di helper membandingkan terhadap lebar sinyal,
        // bukan terhadap `MAX_WIDTH`. Tanpa itu indeks di atas lebar logis
        // membaca bit yang tidak ada dan hasilnya bergantung pada konfigurasi
        // build (LENGKAP/MAX_WIDTH) alih-alih `X` (LRM §7.8).
        Expr::Index {
            base,
            index,
            data_type,
            flat_element,
        } => {
            let w = base.data_type().width as usize;
            // Ketiga helper menerima `&Bits`, jadi operand ditulis sebagai
            // referensi; tanpa itu kode yang dihasilkan gagal dikompilasi.
            if *flat_element {
                let elem = data_type.width as usize;
                out.push_str(&format!(
                    "sv_runtime::index_read::<{elem}, {w}, MAX_WIDTH>("
                ));
            } else {
                out.push_str(&format!("sv_runtime::select_bit::<{w}, MAX_WIDTH>("));
            }
            write_operand_ref(base, out, indent);
            out.push_str(", ");
            write_operand_ref(index, out, indent);
            out.push(')');
        }
        // LRM §6.14: cast dievaluasi seolah operand di-assign ke tipe tujuan,
        // sehingga arah perluasan ikut aturan assignment — ditentukan
        // signedness OPERAND, bukan tipe tujuan. Operand signed di-sign-extend,
        // operand unsigned di-zero-extend.
        //
        // Lebar sumber penting, bukan lebar tujuan: sign-extend menyalin bit
        // MSB *sumber*, bukan bit MSB tujuan yang belum terisi.
        //
        // Hasil helper dibiarkan sebagai `Bits` apa adanya, tanpa
        // `from_u64(..to_u64())` seperti helper lain: `to_u64` hanya membaca
        // 64 bit LSB dan membuang X/Z, jadi setiap cast yang lebarnya di atas
        // 64 bit akan kehilangan separuh nilainya.
        Expr::Cast {
            operand,
            data_type,
            operand_signed,
        } => {
            let src = operand.data_type().width as usize;
            let dst = data_type.width as usize;
            let helper = if *operand_signed {
                "sign_extend"
            } else {
                "resize"
            };
            out.push_str(&format!("sv_runtime::{helper}::<{src}, {dst}, MAX_WIDTH>("));
            write_expr(operand, out, indent);
            out.push(')');
        }
    }
}

/// Tulis satu sisi perbandingan, sudah dikonversi ke integer.
///
/// Sisi yang signed dibungkus `to_signed::<lebar_logis>` supaya `to_u64` tidak
/// dipakai — `to_u64` membaca bit mentah sehingga `-1` pada lebar 32 terbaca
/// sebagai 4294967295 dan `-1 < 0` jadi false.
fn write_comparison_side(operand: &Expr, signed: bool, out: &mut String, indent: &Indent) {
    if signed {
        out.push_str(&format!(
            "sv_runtime::to_signed::<{}, MAX_WIDTH>(",
            operand.data_type().width
        ));
        write_expr(operand, out, indent);
        // Hanya kurung penutup argumen: `to_signed` sudah dipanggil pada
        // token sebelumnya dan hasilnya `i64`, jadi tidak perlu `()` lagi —
        // `)()` akan dibaca Rust sebagai pemanggilan pada nilai `i64`.
        out.push(')');
    } else {
        write_expr(operand, out, indent);
        out.push_str(".to_u64()");
    }
}

/// Tulis concatenation dengan menggeser tiap item ke posisi bitnya.
///
/// LRM §11.8.1: item paling kiri menjadi MSB, jadi pergeseran dihitung dari
/// lebar total ke bawah — bukan naik dari nol (yang akan membalik urutan).
fn write_concat(items: &[Expr], out: &mut String, indent: &Indent) {
    // LRM §11.8.1: item paling kiri menjadi MSB, jadi digeser sesuai lebar di
    // kanannya.
    //
    // Perakitan lewat `to_u64` akan membuang `X`/`Z`, sehingga
    // `{8'hxA, 8'h5Z}` tercetak `0a50` bukan `xa5z`. Helper runtime bekerja
    // pada digit, bukan integer.
    out.push_str("sv_runtime::concat_kbits::<MAX_WIDTH>(&[");
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        write_expr(item, out, indent);
    }
    out.push_str("], &[");
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&format!("{}", item.data_type().width));
    }
    out.push_str("])");
}

/// Tulis unary dan reduction operator.
fn write_unary(op: UnOp, operand: &Expr, out: &mut String, indent: &Indent) {
    let width = operand.data_type().width;
    match op {
        // `~a` — nolkan tiap bit operand pada lebar logisnya.
        UnOp::BitNot => write_helper("bit_not", width, operand, out, indent),
        // `-a` — negasi aritmetika dua's complement pada lebar logisnya.
        UnOp::BitNeg => write_helper("bit_neg", width, operand, out, indent),
        // `!a` hasilnya boolean, jadi tidak pernah sampai ke sini: `write_expr`
        // mencegahnya lewat `expr_is_bool`. Lengan ini menjaga match exhaustif.
        UnOp::LogNot => write_bool_expr(
            &sv_ir::Expr::Un {
                op: UnOp::LogNot,
                operand: Box::new(operand.clone()),
                data_type: sv_ir::DataType::bit(),
            },
            out,
            indent,
        ),
        // Reduksi: satukan seluruh bit operand menjadi 1 bit (LRM §11.4.9).
        UnOp::RedAnd
        | UnOp::RedNand
        | UnOp::RedOr
        | UnOp::RedNor
        | UnOp::RedXor
        | UnOp::RedXnor => {
            out.push_str(&format!(
                "Bits::<MAX_WIDTH>::from_u64(sv_runtime::reduce_{}::<{}, MAX_WIDTH>(",
                reduction_fold(op),
                width
            ));
            write_expr(operand, out, indent);
            out.push_str(").to_u64())");
        }
    }
}

/// Panggil helper unary runtime dengan lebar logis dan lebar penyimpanan.
fn write_helper(nama: &str, width: u32, operand: &Expr, out: &mut String, indent: &Indent) {
    out.push_str(&format!("sv_runtime::{}::<{}, MAX_WIDTH>(", nama, width));
    write_expr(operand, out, indent);
    out.push(')');
}

/// Panggil helper biner runtime dengan lebar logis dan lebar penyimpanan.
///
/// Dipakai `bit_div`/`bit_mod` yang harus aman terhadap pembagi nol.
fn write_helper_binary(
    nama: &str,
    width: u32,
    lhs: &Expr,
    rhs: &Expr,
    out: &mut String,
    indent: &Indent,
) {
    out.push_str(&format!("sv_runtime::{}::<{}, MAX_WIDTH>(", nama, width));
    write_expr(lhs, out, indent);
    out.push_str(", ");
    write_expr(rhs, out, indent);
    out.push(')');
}

/// Panggil helper geser runtime (LRM §11.4.10).
///
/// `signed` hanya diisi untuk `geser_kanan_aritmetik`: LRM menetapkan bit
/// pengisi dari signedness tipe HASIL, bukan dari operator yang dipakai.
/// Helper geser logis tidak punya parameter itu sama sekali.
fn write_shift(
    nama: &str,
    width: u32,
    lhs: &Expr,
    rhs: &Expr,
    signed: Option<bool>,
    out: &mut String,
    indent: &Indent,
) {
    out.push_str(&format!("sv_runtime::{nama}::<{}, MAX_WIDTH>(", width));
    write_operand_ref(lhs, out, indent);
    out.push_str(", ");
    write_operand_ref(rhs, out, indent);
    if let Some(signed) = signed {
        out.push_str(&format!(", {signed}"));
    }
    out.push(')');
}

/// Nama fold reduksi di runtime: `and`/`or`/`xor` dengan flag negasi.
fn reduction_fold(op: UnOp) -> &'static str {
    match op {
        UnOp::RedAnd => "and",
        UnOp::RedNand => "nand",
        UnOp::RedOr => "or",
        UnOp::RedNor => "nor",
        UnOp::RedXor => "xor",
        _ => "xnor",
    }
}

fn expr_width(expr: &Expr) -> u32 {
    expr.data_type().width
}

fn bin_op_token(op: BinOp) -> &'static str {
    match op {
        BinOp::BitAnd => "&",
        BinOp::BitOr => "|",
        BinOp::BitXor => "^",
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        BinOp::Div => "/",
        BinOp::Mod => "%",
        BinOp::Shl => "<<",
        BinOp::Shr => ">>",
        // Ditulis helper runtime, bukan token Rust; lengan ini hanya menjaga
        // match exhaustif.
        BinOp::Sar => ">>>",
        BinOp::Eq => "==",
        BinOp::NotEq => "!=",
        BinOp::Lt => "<",
        BinOp::Gt => ">",
        BinOp::Le => "<=",
        BinOp::Ge => ">=",
        BinOp::LogAnd => "&&",
        BinOp::LogOr => "||",
    }
}

/// True bila operator hasilkan bool Rust (LRM: perbandingan dan logika).
fn bin_op_bool(op: BinOp) -> bool {
    matches!(
        op,
        BinOp::Eq
            | BinOp::NotEq
            | BinOp::Lt
            | BinOp::Gt
            | BinOp::Le
            | BinOp::Ge
            | BinOp::LogAnd
            | BinOp::LogOr
    )
}

/// Tulis kebenaran operand sebagai `bool` Rust.
///
/// Semua ekspresi sudah `Bits` (lihat [`write_expr`]), jadi kebenaran operand
/// selalu `.to_u64() != 0`. Kurung hanya perlu bila operandnya majemuk —
/// perbandingan atau logika — karena `&&`/`||` mengikat lebih kuat daripada
/// `!=`, jadi tanpa kurung urutannya berubah.
fn write_truthy(expr: &Expr, out: &mut String, indent: &Indent) {
    if expr_is_majemuk(expr) {
        write_bool_expr(expr, out, indent);
    } else {
        write_expr(expr, out, indent);
        out.push_str(".to_u64() != 0");
    }
}

/// Tulis ekspresi yang hasilnya `bool` Rust.
///
/// Hanya perbandingan, logika, dan `!` yang masuk sini; sisanya lewat
/// [`write_expr`] yang selalu menghasilkan `Bits`.
fn write_bool_expr(expr: &Expr, out: &mut String, indent: &Indent) {
    match expr {
        Expr::Bin { op, lhs, rhs, .. } if *op == BinOp::LogAnd => {
            out.push('(');
            write_truthy(lhs, out, indent);
            out.push_str(" && ");
            write_truthy(rhs, out, indent);
            out.push(')');
        }
        Expr::Bin { op, lhs, rhs, .. } if *op == BinOp::LogOr => {
            out.push('(');
            write_truthy(lhs, out, indent);
            out.push_str(" || ");
            write_truthy(rhs, out, indent);
            out.push(')');
        }
        Expr::Bin { op, lhs, rhs, .. }
            if matches!(
                op,
                BinOp::Eq | BinOp::NotEq | BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge
            ) =>
        {
            // LRM §11.4.5: perbandingan bertanda hanya bila kedua operand
            // signed. Tanpa ini `-1 < 0` membandingkan 0xFFFFFFFF dengan 0
            // sebagai unsigned dan hasilnya false.
            //
            // Lebar logis masing-masing operand dipakai sebagai const generic ke
            // `to_signed`, supaya `-1` pada lebar 32 dibaca sebagai -1 dan
            // bukan 4294967295. X/Z jadi 0, sama seperti `to_u64`.
            let signed = op.relational_is_signed(lhs.data_type(), rhs.data_type());
            out.push('(');
            write_comparison_side(lhs, signed, out, indent);
            out.push_str(&format!(" {} ", bin_op_token(*op)));
            write_comparison_side(rhs, signed, out, indent);
            out.push(')');
        }
        // `!a` beroperandasi pada kebenaran `a`, jadi bentuknya
        // `..to_u64() == 0` — `!Bits` tidak ada di Rust.
        Expr::Un {
            op: UnOp::LogNot,
            operand,
            ..
        } => {
            out.push_str("!(");
            write_truthy(operand, out, indent);
            out.push(')');
        }
        // Node lain sudah `Bits`; `write_truthy` yang memanggilnya yang
        // menambahkan `.to_u64() != 0`.
        other => {
            write_expr(other, out, indent);
            out.push_str(".to_u64() != 0");
        }
    }
}

/// True bila ekspresi majemuk, misal `(a == b) && c`.
fn expr_is_majemuk(expr: &Expr) -> bool {
    matches!(expr, Expr::Bin { .. } | Expr::Ternary { .. })
}

/// Versi publik `write_truthy` untuk dipakai generator statement.
pub fn write_truthy_pub(expr: &Expr, out: &mut String, indent: &Indent) {
    write_truthy(expr, out, indent);
}

/// True bila ekspresi hasilkan bool Rust (bukan Bits).
pub fn expr_is_bool(expr: &Expr) -> bool {
    match expr {
        Expr::Bin { op, .. } => bin_op_bool(*op),
        Expr::Un { op, .. } => matches!(op, UnOp::LogNot),
        _ => false,
    }
}

/// Tulis baris `let <nama> = <ekspresi>;`.
pub fn write_binding(name: &str, expr: &Expr, out: &mut String, indent: &Indent) {
    indent.push(out);
    out.push_str(&format!("let {} = ", name));
    write_expr(expr, out, indent);
    out.push_str(";\n");
}

pub fn type_name(data_type: DataType) -> String {
    format!("Bits<{}>", data_type.width)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sv_ir::DataType;

    fn bit_ref(sinyal: u32) -> Expr {
        Expr::signal_ref(sinyal, DataType::logic(1))
    }

    fn wide_ref(sinyal: u32, width: u32) -> Expr {
        Expr::signal_ref(sinyal, DataType::logic(width))
    }

    fn render(ekspresi: &Expr) -> String {
        let mut keluar = String::new();
        write_expr(ekspresi, &mut keluar, &Indent::new());
        keluar
    }

    #[test]
    fn banding_emit_u64_bukan_bits() {
        let kiri = bit_ref(0);
        let kanan = Expr::constant(1, DataType::logic(1));
        let ekspresi = Expr::Bin {
            op: BinOp::Eq,
            lhs: Box::new(kiri),
            rhs: Box::new(kanan),
            data_type: DataType::logic(1),
        };
        let hasil = render(&ekspresi);
        assert!(hasil.contains(".to_u64() =="), "hasil: {hasil}");
        assert!(expr_is_bool(&ekspresi));
    }

    #[test]
    fn logika_emit_truthy() {
        let ekspresi = Expr::Bin {
            op: BinOp::LogAnd,
            lhs: Box::new(bit_ref(0)),
            rhs: Box::new(bit_ref(1)),
            data_type: DataType::logic(1),
        };
        let hasil = render(&ekspresi);
        assert!(hasil.contains(".to_u64() != 0 && "), "hasil: {hasil}");
        assert!(expr_is_bool(&ekspresi));
    }

    #[test]
    fn logika_menjaga_kurung_untuk_operand_majemuk() {
        // `(a == b) && c` harus tetap berkurung agar urutannya tidak berubah.
        let ekspresi = Expr::Bin {
            op: BinOp::LogAnd,
            lhs: Box::new(Expr::Bin {
                op: BinOp::Eq,
                lhs: Box::new(bit_ref(0)),
                rhs: Box::new(bit_ref(1)),
                data_type: DataType::logic(1),
            }),
            rhs: Box::new(bit_ref(2)),
            data_type: DataType::logic(1),
        };
        let hasil = render(&ekspresi);
        assert!(hasil.contains("("), "hasil: {hasil}");
        assert!(hasil.contains(") && "), "hasil: {hasil}");
    }

    #[test]
    fn signal_sederhana_tidak_dikurung() {
        // `&a` pada sinyal tunggal tidak perlu kurung; SignalCell::read()
        // sudah Bits sehingga `.to_u64() != 0` suffice.
        let ekspresi = Expr::Un {
            op: UnOp::RedAnd,
            operand: Box::new(wide_ref(0, 8)),
            data_type: DataType::bit(),
        };
        let hasil = render(&ekspresi);
        assert!(hasil.contains(".to_u64())"), "hasil: {hasil}");
        assert!(!hasil.contains("(!"), "hasil: {hasil}");
    }

    #[test]
    fn semua_operator_hasil_kurung_seimbang() {
        // Setiap bentuk ekspresi wajib menghasilkan Rust yang bisa diparse.
        let a = wide_ref(0, 8);
        let b = wide_ref(1, 8);
        let cases = [
            Expr::constant(5, DataType::logic(8)),
            a.clone(),
            Expr::Bin {
                op: BinOp::Add,
                lhs: Box::new(a.clone()),
                rhs: Box::new(b.clone()),
                data_type: DataType::logic(8),
            },
            Expr::Un {
                op: UnOp::BitNot,
                operand: Box::new(a.clone()),
                data_type: DataType::logic(8),
            },
            Expr::Un {
                op: UnOp::RedXor,
                operand: Box::new(a.clone()),
                data_type: DataType::bit(),
            },
            Expr::Select {
                base: Box::new(a.clone()),
                msb: 3,
                lsb: 0,
                data_type: DataType::logic(4),
            },
            Expr::Ternary {
                condition: Box::new(a.clone()),
                when_true: Box::new(a.clone()),
                when_false: Box::new(b.clone()),
                data_type: DataType::logic(8),
            },
            // BUG: `Cast` belum pernah ikut kasus ini, padahal lengan
            // `Expr::Cast` punya kurung `(` dan `)` sendiri.
            Expr::Cast {
                operand: Box::new(a.clone()),
                data_type: DataType::logic(16),
                operand_signed: false,
            },
            Expr::Concat {
                items: vec![a.clone(), b.clone()],
                data_type: DataType::logic(16),
            },
            Expr::Replicate {
                count: 4,
                value: Box::new(bit_ref(0)),
                data_type: DataType::logic(4),
            },
        ];
        for (i, ekspresi) in cases.iter().enumerate() {
            let hasil = render(ekspresi);
            let buka = hasil.matches('(').count();
            let tutup = hasil.matches(')').count();
            assert_eq!(buka, tutup, "kasus {i} kurung tak seimbang: {hasil}");
        }
    }

    #[test]
    fn negasi_logika_dibalut_sebagai_bits() {
        // BUG-1: `!a` dulu di-emit sebagai `bool` mentah, sehingga langsung
        // rusak begitu masuk ke concat/cast/`$display` yang mengharapkan
        // `Bits`. Sekarang `write_expr` selalu membungkusnya jadi `Bits`.
        let ekspresi = Expr::Un {
            op: UnOp::LogNot,
            operand: Box::new(bit_ref(0)),
            data_type: DataType::logic(1),
        };
        let hasil = render(&ekspresi);
        assert!(
            hasil.starts_with("Bits::<MAX_WIDTH>::from_u64(("),
            "hasil: {hasil}"
        );
        // `!a` berarti `a` salah, jadi bentuknya `!(.. != 0)`.
        assert!(
            hasil.contains("!(self.signals[0].read().to_u64() != 0)"),
            "hasil: {hasil}"
        );
        assert!(expr_is_bool(&ekspresi));
    }

    #[test]
    fn perbandingan_selalu_dibalut_sebagai_bits() {
        // BUG-11: perbandingan yang dipakai sebagai operand concat/cast
        // gagal dikompilasi karena emission-nya `bool`. Semua ekspresi harus
        // bertipe `Bits`, apa pun tipe logikanya.
        let ekspresi = Expr::Bin {
            op: BinOp::Eq,
            lhs: Box::new(bit_ref(0)),
            rhs: Box::new(bit_ref(1)),
            data_type: DataType::bit(),
        };
        let hasil = render(&ekspresi);
        assert!(
            hasil.starts_with("Bits::<MAX_WIDTH>::from_u64(("),
            "hasil: {hasil}"
        );
    }

    #[test]
    fn logika_selalu_dibalut_sebagai_bits() {
        let ekspresi = Expr::Bin {
            op: BinOp::LogAnd,
            lhs: Box::new(bit_ref(0)),
            rhs: Box::new(bit_ref(1)),
            data_type: DataType::bit(),
        };
        let hasil = render(&ekspresi);
        assert!(
            hasil.starts_with("Bits::<MAX_WIDTH>::from_u64(("),
            "hasil: {hasil}"
        );
        // Short-circuit tetap dipakai di dalam bool.
        assert!(hasil.contains(" && "), "hasil: {hasil}");
    }

    #[test]
    fn seleksi_memakai_helper_yang_mempertahankan_xz() {
        // BUG: part-select dulu digeser lewat `to_u64()`, yang membuang digit
        // `X`/`Z` — jadi `a[7:4]` pada `8'hxA` kehilangan wildcard-nya.
        // Sekarang emission memanggil helper runtime yang bekerja pada digit.
        let ekspresi = Expr::Select {
            base: Box::new(bit_ref(0)),
            msb: 5,
            lsb: 2,
            data_type: DataType::logic(4),
        };
        let hasil = render(&ekspresi);
        assert!(
            hasil.contains("sv_runtime::select_kbits::<1, MAX_WIDTH>("),
            "hasil: {hasil}"
        );
        assert!(hasil.contains(", 2, 4)"), "hasil: {hasil}");
        assert!(!hasil.contains("to_u64"), "hasil: {hasil}");
        assert!(!expr_is_bool(&ekspresi));
    }

    #[test]
    fn seleksi_memiliki_kurung_seimbang() {
        // BUG: `index_read` menerima `&Bits`, tapi emission menulis operand
        // sebagai nilai. Kode yang dihasilkan gagal dikompilasi Rust dengan
        // "expected `&Bits<64>`, found `Bits<64>`" — runtime crash saat build
        // binary simulasi, bukan error runtime.
        for flat_element in [true, false] {
            let ekspresi = Expr::Index {
                base: Box::new(wide_ref(0, 8)),
                index: Box::new(wide_ref(1, 3)),
                data_type: if flat_element {
                    DataType::logic(8)
                } else {
                    DataType::bit()
                },
                flat_element,
            };
            let hasil = render(&ekspresi);
            assert!(
                hasil.contains("&self.signals[0].read()"),
                "base harus ditulis sebagai referensi: {hasil}"
            );
            assert!(
                hasil.contains(", &self.signals[1].read())"),
                "index harus ditulis sebagai referensi: {hasil}"
            );
        }
    }

    // BUG: `a[i]` pada sinyal packed (bit-select) dan `mem[i]` pada array
    // unpacked punya sintaks sama tapi semantik geser berbeda. Keduanya dulu
    // memakai `index_read` yang menggeser `i * lebar`, sehingga bit-select
    // packed selalu membaca di luar jangkauan dan hasilnya nol.
    #[test]
    fn indeks_packed_dan_unpacked_memakai_helper_berbeda() {
        let packed = render(&Expr::Index {
            base: Box::new(wide_ref(0, 8)),
            index: Box::new(wide_ref(1, 3)),
            data_type: DataType::bit(),
            flat_element: false,
        });
        assert!(
            packed.contains("sv_runtime::select_bit::<8, MAX_WIDTH>("),
            "bit-select packed harus pakai select_bit: {packed}"
        );
        assert!(
            !packed.contains("index_read"),
            "packed tidak boleh pakai helper array: {packed}"
        );

        let unpacked = render(&Expr::Index {
            base: Box::new(wide_ref(0, 32)),
            index: Box::new(wide_ref(1, 3)),
            data_type: DataType::logic(8),
            flat_element: true,
        });
        assert!(
            unpacked.contains("sv_runtime::index_read::<8, 32, MAX_WIDTH>("),
            "elemen array harus pakai index_read: {unpacked}"
        );
        assert!(
            !unpacked.contains("select_bit"),
            "array tidak boleh pakai select_bit: {unpacked}"
        );
    }

    #[test]
    fn seleksi_memiliki_kurung_seimbang_lama() {
        // Kurung berlebih membuat Rust gagal parse dengan "unexpected
        // closing delimiter", jadi jumlah kurung wajib seimbang.
        for lsb in [0u32, 4] {
            let ekspresi = Expr::Select {
                base: Box::new(wide_ref(0, 8)),
                msb: 7,
                lsb,
                data_type: DataType::logic(8 - lsb),
            };
            let hasil = render(&ekspresi);
            let buka = hasil.matches('(').count();
            let tutup = hasil.matches(')').count();
            assert_eq!(buka, tutup, "kurung tak seimbang untuk {hasil}");
        }
    }

    // --- BUG-3: unary ---

    #[test]
    fn bitwise_not_meng_mask_ke_lebar_operand() {
        let ekspresi = Expr::Un {
            op: UnOp::BitNot,
            operand: Box::new(wide_ref(0, 4)),
            data_type: DataType::logic(4),
        };
        let hasil = render(&ekspresi);
        // Helper runtime, lebar logis + lebar penyimpanan sebagai const generic.
        assert!(
            hasil.contains("sv_runtime::bit_not::<4, MAX_WIDTH>("),
            "hasil: {hasil}"
        );
        assert!(!expr_is_bool(&ekspresi));
    }

    #[test]
    fn negasi_aritmetika_memakai_helper_lebar() {
        let ekspresi = Expr::Un {
            op: UnOp::BitNeg,
            operand: Box::new(wide_ref(0, 8)),
            data_type: DataType::logic(8),
        };
        let hasil = render(&ekspresi);
        assert!(
            hasil.contains("sv_runtime::bit_neg::<8, MAX_WIDTH>("),
            "hasil: {hasil}"
        );
        assert!(!expr_is_bool(&ekspresi));
    }

    // --- BUG-4: reduction ---

    #[test]
    fn reduksi_and_menjadi_satu_bit() {
        let ekspresi = Expr::Un {
            op: UnOp::RedAnd,
            operand: Box::new(wide_ref(0, 8)),
            data_type: DataType::bit(),
        };
        let hasil = render(&ekspresi);
        assert!(
            hasil.contains("sv_runtime::reduce_and::<8, MAX_WIDTH>("),
            "hasil: {hasil}"
        );
        assert!(hasil.contains("Bits::<MAX_WIDTH>"), "hasil: {hasil}");
    }

    #[test]
    fn reduksi_or_menggunakan_fold_or() {
        let ekspresi = Expr::Un {
            op: UnOp::RedOr,
            operand: Box::new(wide_ref(0, 4)),
            data_type: DataType::bit(),
        };
        let hasil = render(&ekspresi);
        assert!(
            hasil.contains("sv_runtime::reduce_or::<4, MAX_WIDTH>("),
            "hasil: {hasil}"
        );
    }

    #[test]
    fn reduksi_xor_menggunakan_fold_xor() {
        let ekspresi = Expr::Un {
            op: UnOp::RedXor,
            operand: Box::new(wide_ref(0, 4)),
            data_type: DataType::bit(),
        };
        let hasil = render(&ekspresi);
        assert!(
            hasil.contains("sv_runtime::reduce_xor::<4, MAX_WIDTH>("),
            "hasil: {hasil}"
        );
    }

    #[test]
    fn reduksi_nand_memakai_helper_sendiri() {
        let ekspresi = Expr::Un {
            op: UnOp::RedNand,
            operand: Box::new(wide_ref(0, 4)),
            data_type: DataType::bit(),
        };
        let hasil = render(&ekspresi);
        assert!(
            hasil.contains("sv_runtime::reduce_nand::<4, MAX_WIDTH>("),
            "hasil: {hasil}"
        );
    }

    #[test]
    fn reduksi_xnor_memakai_helper_sendiri() {
        let ekspresi = Expr::Un {
            op: UnOp::RedXnor,
            operand: Box::new(wide_ref(0, 4)),
            data_type: DataType::bit(),
        };
        let hasil = render(&ekspresi);
        assert!(
            hasil.contains("sv_runtime::reduce_xnor::<4, MAX_WIDTH>("),
            "hasil: {hasil}"
        );
    }

    // --- BUG-5: ternary ---

    #[test]
    fn ternary_emit_if_else_rust() {
        let ekspresi = Expr::Ternary {
            condition: Box::new(bit_ref(0)),
            when_true: Box::new(wide_ref(1, 8)),
            when_false: Box::new(Expr::constant(0, DataType::logic(8))),
            data_type: DataType::logic(8),
        };
        let hasil = render(&ekspresi);
        assert!(hasil.contains("if "), "hasil: {hasil}");
        assert!(hasil.contains("} else {"), "hasil: {hasil}");
        assert!(!expr_is_bool(&ekspresi));
    }

    #[test]
    fn ternary_kondisi_bits_dijadi_truthy() {
        let ekspresi = Expr::Ternary {
            condition: Box::new(wide_ref(0, 8)),
            when_true: Box::new(wide_ref(1, 8)),
            when_false: Box::new(wide_ref(2, 8)),
            data_type: DataType::logic(8),
        };
        let hasil = render(&ekspresi);
        assert!(hasil.contains(".to_u64() != 0 {"), "hasil: {hasil}");
    }
}
