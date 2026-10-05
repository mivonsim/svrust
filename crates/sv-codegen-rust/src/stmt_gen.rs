// Tanggung jawab: generate kode Rust untuk statement IR.
use crate::expr_gen::{write_expr, write_truthy_pub as write_truthy};
use crate::indent::Indent;
use crate::system_task_gen::write_system_task;
use sv_ir::process::{AssignStyle, Assignment, CaseArm, CaseKind, Statement};

/// Tulis `casez`/`casex` sebagai rantai if-else dengan perbandingan bermasker.
///
/// LRM §12.5: bit yang ditulis `z`/`?` (`casez`) atau `x`/`z`/`?` (`casex`) pada
/// label tidak ikut dibandingkan, sehingga dicoret dengan `!mask`.
fn write_case_wildcard(
    selector: &sv_ir::Expr,
    arms: &[CaseArm],
    kind: CaseKind,
    out: &mut String,
    indent: &Indent,
) {
    let sel = format!("_selw{}", selector_hash(selector));

    indent.push(out);
    out.push_str(&format!("let {} = ", sel));
    write_expr(selector, out, indent);
    out.push_str(";\n");

    // Kumpulkan seluruh cabang lebih dulu supaya bisa tahu mana yang terakhir.
    let mut cabang: Vec<Branch> = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    let mut sudah_default = false;
    for arm in arms {
        if arm.is_default {
            if sudah_default {
                continue;
            }
            sudah_default = true;
            cabang.push(Branch {
                test: None,
                body: &arm.body,
            });
            continue;
        }
        for label in &arm.labels {
            // Label non-konstan tidak bisa dibandingkan di waktu kompilasi.
            // Eligator sudah menolaknya; kalau sampai sini, lewati agar tidak
            // became label `0` yang salah.
            let Some((nilai, unknown_mask, zmask)) = label_bits(label) else {
                continue;
            };
            // LRM §12.5: `casex` menjadikan `x` juga wildcard, `casez` tidak.
            let x_wildcard = if kind.is_casex() { unknown_mask } else { 0 };
            let key = format!("{nilai:#x}:{x_wildcard:#x}:{zmask:#x}");
            if seen.contains(&key) {
                continue;
            }
            seen.push(key);
            cabang.push(Branch {
                test: Some(TestCase {
                    nilai,
                    unknown_mask: x_wildcard,
                    zmask,
                }),
                body: &arm.body,
            });
        }
    }

    for (i, branch) in cabang.iter().enumerate() {
        let terakhir = i + 1 == cabang.len();
        let pertama = i == 0;
        // `default` di posisi pertama tidak butuh blok pengaman sama sekali.
        let membuka_blok = !(branch.test.is_none() && pertama);

        if membuka_blok {
            indent.push(out);
            // Perbandingan per digit lewat helper runtime: `to_u64()` membuang
            // bit di atas 63 sehingga pada selektor lebar semua bit tinggi
            // hilang dari perbandingan dan cabang yang salah ikut diambil.
            out.push_str(&match (branch.test.as_ref(), pertama) {
                // Cabang berikutnya menutup kurung sebelumnya lewat awalan `}`.
                // Perbandingan per digit lewat helper runtime — `to_u64()`
                // membuang bit di atas 63 sehingga bit tinggi selektor hilang
                // dari perbandingan dan cabang yang salah ikut diambil.
                (Some(t), true) => format!("if {} {{\n", emit_test(t, &sel, selector)),
                (Some(t), false) => format!("}} else if {} {{\n", emit_test(t, &sel, selector)),
                (None, false) => "} else {\n".to_string(),
                (None, true) => unreachable!("default pertama tidak membuka blok"),
            });
        }
        write_statements(branch.body, out, &indent.child());
        // Hanya cabang terakhir yang menutup, karena cabang sebelumnya sudah
        // ditutup oleh awalan `} else` dari cabang berikutnya.
        if terakhir && membuka_blok {
            indent.push(out);
            out.push_str("}\n");
        }
    }
}

/// Satu cabang `casez`/`casex` beserta kondisi pencocokannya.
struct Branch<'a> {
    /// `None` berarti `default`.
    test: Option<TestCase>,
    body: &'a [Statement],
}

/// Satu label `case` yang sudah diurai jadi nilai + masker wildcard.
///
/// Ketiganya `u64`, jadi lebar label dibatasi 64 bit — elaborator menolak
/// selektor dan label lebih lebar dengan pesan jelas.
struct TestCase {
    nilai: u64,
    unknown_mask: u64,
    zmask: u64,
}

/// Emit ekspresi boolean pembanding label dengan selektor.
///
/// Perbandingan dilakukan per digit oleh `sv_runtime::case_eq`, bukan lewat
/// `to_u64()`: `to_u64()` hanya melihat 64 bit LSB dan membuang digit `x`/`z`,
/// sehingga pada selektor lebar semua bit di atas 63 hilang dari perbandingan
/// dan cabang yang salah ikut diambil.
fn emit_test(t: &TestCase, sel: &str, selector: &sv_ir::Expr) -> String {
    let w = selector.data_type().width as usize;
    format!(
        "sv_runtime::case_eq::<{w}, MAX_WIDTH>(&{sel}, {:#x}, {:#x}, {:#x})",
        t.nilai, t.unknown_mask, t.zmask
    )
}

/// `(nilai, unknown_mask, zmask)` dari label konstanta, atau `None` bila label
/// bukan konstanta.
///
/// Label non-konstan (`case (sel) sig: ...`) tidak bisa dibandingkan saat
/// codegen; eligator menolak bentuk itu. Kalau sampai di sini, `None` membuat
/// lengannya dilewati — lebih baik daripada diam-diam memperlakukannya sebagai
/// label `0`, yang menghasilkan cabang yang salah tanpa pesan.
fn label_bits(label: &sv_ir::Expr) -> Option<(u64, u64, u64)> {
    let sv_ir::Expr::Const {
        value,
        unknown_mask,
        zmask,
        ..
    } = label
    else {
        return None;
    };
    Some((*value, *unknown_mask, *zmask))
}

/// Tulis rangkaian statement IR.
pub fn write_statements(statements: &[Statement], out: &mut String, indent: &Indent) {
    for statement in statements {
        write_statement(statement, out, indent);
    }
}

fn write_statement(statement: &Statement, out: &mut String, indent: &Indent) {
    match statement {
        Statement::Assign { assignment, .. } => write_assignment(assignment, out, indent),
        Statement::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            // LRM §9: kondisi adalah kebenaran 4-state. `write_truthy`
            // sudah memakai bentuk yang benar untuk node logika dan `Bits`.
            indent.push(out);
            out.push_str("if ");
            write_truthy(condition, out, indent);
            out.push_str(" {\n");
            write_statements(then_branch, out, &indent.child());
            indent.push(out);
            if else_branch.is_empty() {
                out.push_str("}\n");
            } else {
                out.push_str("} else {\n");
                write_statements(else_branch, out, &indent.child());
                indent.push(out);
                out.push_str("}\n");
            }
        }
        Statement::For {
            init,
            condition,
            step,
            body,
            ..
        } => {
            // LRM §12.7.1: init dievaluasi sekali sebelum loop.
            write_assignment(init, out, indent);
            indent.push(out);
            out.push_str("loop {\n");
            let inner = indent.child();
            // Kondisi SV adalah tes kebenaran, jadi `!kondisi` berarti berhenti.
            inner.push(out);
            out.push_str("if !");
            write_truthy(condition, out, &inner);
            out.push_str(" {\n");
            let guard = inner.child();
            guard.push(out);
            out.push_str("break;\n");
            inner.push(out);
            out.push_str("}\n");
            write_statements(body, out, &inner);
            write_assignment(step, out, &inner);
            indent.push(out);
            out.push_str("}\n");
        }
        Statement::Repeat { count, body, .. } => {
            // LRM §12.8.1: jumlah iterasi dievaluasi sekali saat loop masuk,
            // bukan tiap iterasi, jadi disimpan di variable lokal dulu.
            indent.push(out);
            out.push_str("let __sv_repeat_n = ");
            write_expr(count, out, indent);
            out.push_str(".to_u64();\n");
            out.push_str("for _ in 0..__sv_repeat_n {\n");
            let inner = indent.child();
            write_statements(body, out, &inner);
            indent.push(out);
            out.push_str("}\n");
        }
        Statement::While {
            condition, body, ..
        } => {
            // LRM §12.7.2: kondisi diuji sebelum tiap iterasi.
            indent.push(out);
            out.push_str("loop {\n");
            let inner = indent.child();
            inner.push(out);
            out.push_str("if !");
            write_truthy(condition, out, &inner);
            out.push_str(" {\n");
            let guard = inner.child();
            guard.push(out);
            out.push_str("break;\n");
            inner.push(out);
            out.push_str("}\n");
            write_statements(body, out, &inner);
            indent.push(out);
            out.push_str("}\n");
        }
        Statement::Case {
            selector,
            arms,
            kind,
            ..
        } => write_case(selector, arms, *kind, out, indent),
        Statement::Block { body, .. } => {
            indent.push(out);
            out.push_str("{\n");
            write_statements(body, out, &indent.child());
            indent.push(out);
            out.push_str("}\n");
        }
        Statement::SystemTask { kind, args, .. } => write_system_task(*kind, args, out, indent),
        // LRM §11.2: `#n` menggeser waktu simulasi sebelum body dijalankan.
        // Penundaan di sini bersifat sinkron pada tahap codegen: proses lain
        // belum sempat dievaluasi pada waktu yang sama, jadi urutan
        // antar-proses belum dimodelkan sebagai event queue.
        Statement::Delay {
            amount, unit, body, ..
        } => {
            // LRM §3.3: satuan `#n` menentukan konversi ke satuan internal
            // femtosecond yang dipakai `SimTime`.
            let mut langkah = String::new();
            let eks_indent = Indent::new();
            write_expr(amount, &mut langkah, &eks_indent);
            indent.push(out);
            out.push_str(&format!(
                "self.time_now = self.time_now.saturating_add({});\n",
                time_step(*unit, &langkah)
            ));
            write_statement(body, out, indent);
        }
        // LRM §9.7: statement event control ditangani oleh driver yang memanggil
        // `eval_initial`; di sini hanya bodynya yang ditulis.
        Statement::EventControl { body, .. } => write_statements(body, out, indent),
        Statement::Noop { .. } => {}
    }
}

/// Bangun constructor `SimTime` untuk satuan `#n` dari kode nilai `n`.
///
/// Satuan besar dikalikan faktor femtosecond-nya karena `SimTime` menyimpan
/// femtosecond sebagai satuan internal terkecil.
fn time_step(unit: sv_ir::TimeUnit, kode_nilai: &str) -> String {
    let call = |fn_name: &str, arg: String| format!("sv_runtime::SimTime::{}({})", fn_name, arg);
    let n = || format!("{}.to_u64()", kode_nilai);
    match unit {
        sv_ir::TimeUnit::Seconds => call(
            "from_femtos",
            format!("{}.saturating_mul(sv_runtime::FS_PER_SEC)", n()),
        ),
        sv_ir::TimeUnit::MilliSeconds => call("from_millis", n()),
        sv_ir::TimeUnit::MicroSeconds => call(
            "from_femtos",
            format!("{}.saturating_mul(sv_runtime::FS_PER_US)", n()),
        ),
        sv_ir::TimeUnit::NanoSeconds => call("from_nanos", n()),
        sv_ir::TimeUnit::PicoSeconds => call("from_picos", n()),
        sv_ir::TimeUnit::FectoSeconds => call("from_femtos", n()),
    }
}

/// Nilai RHS dievaluasi ke temporary dulu supaya tidak ada
/// dua borrow `self.signals` sekaligus pada ekspresi yang sama.
/// LRM: hasil perbandingan/logika adalah 1-bit; konversi ke Bits agar
/// `write()`/`PendingWrite` terima tipe yang sama.
fn write_assignment(assignment: &Assignment, out: &mut String, indent: &Indent) {
    let temp = format!("_v{}", assignment.target);

    indent.push(out);
    out.push_str(&format!("let {} = ", temp));
    // `write_expr` selalu menghasilkan `Bits<MAX_WIDTH>` (lihat
    // `expr_gen::write_expr`), jadi perbandingan dan logika tidak perlu
    // dibungkus lagi di sini — membungkusnya dua kali menghasilkan
    // `from_u64(from_u64(..) as u64)` yang tidak bisa dikompilasi.
    write_expr(&assignment.value, out, indent);
    out.push_str(";\n");

    let mask = match assignment.slice {
        None => "Bits::<MAX_WIDTH>::satu()".to_string(),
        // Masker dihitung saat runtime (`range_mask`) bukan jadi literal `u64`:
        // irisan pada sinyal lebih dari 64 bit tidak bisa diwakili `u64` —
        // `1u64 << lsb` panic di debug dan wrap diam-diam di release.
        Some(slice) => {
            let (lsb, width) = slice.mask_range();
            format!("sv_runtime::range_mask::<MAX_WIDTH>({lsb}, {width})")
        }
    };
    let seluruh = assignment.slice.is_none();
    // LRM §10.10.1: nilai harus berada pada posisi bit yang dituju irisan,
    // bukan di bit terendah. Tanpa pergeseran ini `y[3] = b` menulis ke bit 0
    // dan bukan ke bit 3.
    let nilai = match assignment.slice {
        Some(slice) if slice.lsb > 0 => format!("({} << {})", temp, slice.lsb),
        _ => temp,
    };
    match assignment.style {
        AssignStyle::Blocking => {
            indent.push(out);
            if seluruh {
                out.push_str(&format!(
                    "self.signals[{}].write({});\n",
                    assignment.target, nilai
                ));
            } else {
                // LRM §10.10.1: hanya bit di dalam irisan yang berubah.
                out.push_str(&format!(
                    "self.signals[{}].write_masked({}, {});\n",
                    assignment.target, nilai, mask
                ));
            }
        }
        AssignStyle::NonBlocking => {
            indent.push(out);
            if seluruh {
                out.push_str(&format!(
                    "self.pending.push(PendingWrite::new({}, {}));\n",
                    assignment.target, nilai
                ));
            } else {
                out.push_str(&format!(
                    "self.pending.push(PendingWrite::new_masked({}, {}, {}));\n",
                    assignment.target, nilai, mask
                ));
            }
        }
    }
}

fn write_case(
    selector: &sv_ir::Expr,
    arms: &[CaseArm],
    kind: CaseKind,
    out: &mut String,
    indent: &Indent,
) {
    // Wildcard butuh perbandingan bermasker; `match` tidak bisa Calculus itu.
    if kind.is_wildcard() {
        write_case_wildcard(selector, arms, kind, out, indent);
        return;
    }

    let value = format!("_sel{}", selector_hash(selector));

    indent.push(out);
    out.push_str(&format!("let {} = ", value));
    write_expr(selector, out, indent);
    out.push_str(";\n");

    // Rantai if/else dengan pembanding per digit (`case_eq`), bukan `match`
    // atas `to_u64()`: `to_u64()` hanya melihat 64 bit LSB sehingga pada
    // selektor lebih dari 64 bit semua bit di atas 63 hilang dari
    // perbandingan dan cabang yang salah ikut diambil.
    let mut cabang: Vec<(Option<TestCase>, &[Statement])> = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    let mut sudah_default = false;
    for arm in arms {
        if arm.is_default {
            if sudah_default {
                continue;
            }
            sudah_default = true;
            cabang.push((None, &arm.body));
            continue;
        }
        for label in &arm.labels {
            // Label non-konstan tidak bisa dibandingkan. Eligator menolaknya;
            // kalau sampai sini, lewati — jangan diperlakukan sebagai label 0
            // yang membuat cabang yang salah terpilih tanpa pesan.
            let Some((nilai, unknown_mask, zmask)) = label_bits(label) else {
                continue;
            };
            let key = format!("{nilai:#x}:{unknown_mask:#x}:{zmask:#x}");
            if seen.contains(&key) {
                continue;
            }
            seen.push(key);
            cabang.push((
                Some(TestCase {
                    nilai,
                    unknown_mask,
                    zmask,
                }),
                &arm.body,
            ));
        }
    }

    for (i, (test, body)) in cabang.iter().enumerate() {
        let pertama = i == 0;
        // `default` di posisi pertama tidak butuh blok pengaman sama sekali.
        let membuka_blok = !(test.is_none() && pertama);
        if membuka_blok {
            indent.push(out);
            out.push_str(&match (test, pertama) {
                (Some(t), true) => format!("if {} {{\n", emit_test(t, &value, selector)),
                (Some(t), false) => {
                    format!("}} else if {} {{\n", emit_test(t, &value, selector))
                }
                (None, false) => "} else {\n".to_string(),
                (None, true) => unreachable!("default pertama tidak membuka blok"),
            });
        }
        write_statements(body, out, &indent.child());
        if i + 1 == cabang.len() {
            // Cabang terakhir menutup kurung yang dibuka semua cabang
            // sebelumnya.
            indent.push(out);
            out.push_str("}\n");
        }
    }
}

/// Hash stabil untuk nama temporary selector.
fn selector_hash(expr: &sv_ir::Expr) -> usize {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    expr.data_type().width.hash(&mut hasher);
    expr.data_type().signed.hash(&mut hasher);
    format!("{:?}", expr).hash(&mut hasher);
    hasher.finish() as usize % 1000
}
