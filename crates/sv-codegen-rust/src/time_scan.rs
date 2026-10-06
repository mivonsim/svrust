// Tanggung jawab: deteksi apakah design memakai waktu simulasi (`#delay`/`$time`).
use sv_ir::expr::Expr;
use sv_ir::process::Statement;
use sv_ir::Design;

/// True bila ada `#delay` atau `$time` di seluruh design.
///
/// Field waktu pada struct generated hanya dibuat bila perlu, supaya design
/// tanpa testbench tetap ramping.
///
/// Nilai awal variabel juga ikut diperiksa: `logic [63:0] stamp = $time;`
/// menghasilkan kode yang membaca `self.time_now`, jadi field itu wajib ada.
/// Kalau hanya statement proses yang dipindai, kode hasil generate gagal
/// dikompilasi dengan `no field 'time_now'`.
pub fn has_time(design: &Design) -> bool {
    design
        .variables
        .iter()
        .any(|var| var.initial.as_ref().is_some_and(expr_uses_time))
        || design
            .processes
            .iter()
            .any(|process| process.body.iter().any(statement_uses_time))
}

/// Jumlah proses waktu (`always #N ...`) pada design.
pub fn jumlah_proses_waktu(design: &Design) -> usize {
    design
        .processes
        .iter()
        .filter(|p| p.kind == sv_ir::ProcessKind::Timed)
        .count()
}

/// Kumpulkan task `$monitor` terakhir dari sebuah daftar statement.
///
/// LRM §20.2: panggilan `$monitor` berikutnya menggantikan yang sebelumnya,
/// jadi yang dipakai adalah yang terakhir.
pub fn kumpulkan_monitor(body: &[Statement], hasil: &mut Option<sv_ir::system_task::SystemTask>) {
    kumpulkan_task(body, sv_ir::system_task::SystemTaskKind::Monitor, hasil);
}

/// Kumpulkan task system task terakhir dengan `jenis` tertentu.
pub fn kumpulkan_task(
    body: &[Statement],
    jenis: sv_ir::system_task::SystemTaskKind,
    hasil: &mut Option<sv_ir::system_task::SystemTask>,
) {
    for statement in body {
        kumpulkan_monitor_stmt(statement, jenis, hasil);
    }
}

/// Jumlah proses waktu (`always #N ...`) pada design.
/// Kumpulkan task `$monitor` terakhir dari satu statement beserta turunannya.
pub fn kumpulkan_monitor_stmt(
    statement: &Statement,
    jenis: sv_ir::system_task::SystemTaskKind,
    hasil: &mut Option<sv_ir::system_task::SystemTask>,
) {
    match statement {
        Statement::SystemTask {
            kind,
            args,
            condition,
            time_scale,
            ..
        } => {
            if *kind == jenis {
                *hasil = Some(sv_ir::system_task::SystemTask {
                    kind: *kind,
                    args: args.clone(),
                    condition: condition.clone(),
                    time_scale: *time_scale,
                    span: sv_ir::process::Span::default(),
                });
            }
        }
        Statement::Delay { body, .. } => kumpulkan_task(std::slice::from_ref(body), jenis, hasil),
        Statement::EventControl { body, .. } => kumpulkan_task(body, jenis, hasil),
        Statement::If {
            then_branch,
            else_branch,
            ..
        } => {
            kumpulkan_task(then_branch, jenis, hasil);
            kumpulkan_task(else_branch, jenis, hasil);
        }
        Statement::Case { arms, .. } => {
            for arm in arms {
                kumpulkan_task(&arm.body, jenis, hasil);
            }
        }
        Statement::For { body, .. }
        | Statement::Repeat { body, .. }
        | Statement::While { body, .. }
        | Statement::Block { body, .. } => kumpulkan_task(body, jenis, hasil),
        Statement::Assign { .. } | Statement::Noop { .. } => {}
    }
}

/// True bila statement ini atau turunannya memakai waktu simulasi.
pub fn statement_uses_time(statement: &Statement) -> bool {
    match statement {
        // `#n` selalu menulis `time_now`, walau nilainya konstanta.
        Statement::Delay { .. } => true,
        Statement::SystemTask {
            args, condition, ..
        } => {
            let dari_argumen = args.iter().any(|arg| match arg {
                sv_ir::system_task::SystemArg::Value(expr) => expr_uses_time(expr),
                sv_ir::system_task::SystemArg::Format(_) => false,
            });
            // Syarat `$monitor if` bisa memuat `$time`.
            dari_argumen || condition.as_ref().is_some_and(expr_uses_time)
        }
        Statement::Assign { assignment, .. } => expr_uses_time(&assignment.value),
        Statement::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            expr_uses_time(condition)
                || then_branch.iter().any(statement_uses_time)
                || else_branch.iter().any(statement_uses_time)
        }
        Statement::Case { selector, arms, .. } => {
            expr_uses_time(selector)
                || arms.iter().any(|arm| {
                    arm.labels.iter().any(expr_uses_time)
                        || arm.body.iter().any(statement_uses_time)
                })
        }
        Statement::For {
            init,
            condition,
            step,
            body,
            ..
        } => {
            expr_uses_time(&init.value)
                || expr_uses_time(condition)
                || expr_uses_time(&step.value)
                || body.iter().any(statement_uses_time)
        }
        Statement::Repeat { count, body, .. } => {
            expr_uses_time(count) || body.iter().any(statement_uses_time)
        }
        Statement::While {
            condition, body, ..
        } => expr_uses_time(condition) || body.iter().any(statement_uses_time),
        Statement::Block { body, .. } => body.iter().any(statement_uses_time),
        Statement::EventControl { body, .. } => body.iter().any(statement_uses_time),
        Statement::Noop { .. } => false,
    }
}

/// True bila ekspresi ini atau sub-ekspresinya memakai waktu simulasi.
pub fn expr_uses_time(expr: &Expr) -> bool {
    match expr {
        Expr::SimTime { .. } => true,
        Expr::Bin { lhs, rhs, .. } => expr_uses_time(lhs) || expr_uses_time(rhs),
        Expr::Un { operand, .. } => expr_uses_time(operand),
        Expr::Select { base, .. } => expr_uses_time(base),
        Expr::Ternary {
            condition,
            when_true,
            when_false,
            ..
        } => expr_uses_time(condition) || expr_uses_time(when_true) || expr_uses_time(when_false),
        Expr::Concat { items, .. } => items.iter().any(expr_uses_time),
        Expr::Replicate { value, .. } => expr_uses_time(value),
        Expr::Index { base, index, .. } => expr_uses_time(base) || expr_uses_time(index),
        Expr::Cast { operand, .. } => expr_uses_time(operand),
        Expr::Const { .. } | Expr::SignalRef { .. } => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sv_ir::datatype::DataType;
    use sv_ir::process::Span;
    use sv_ir::process::{Process, ProcessKind};

    fn konstanta() -> Expr {
        Expr::constant(5, DataType::logic(8))
    }

    fn kosong() -> Expr {
        Expr::constant(0, DataType::logic(8))
    }

    #[test]
    fn design_kosong_tidak_pakai_waktu() {
        let design = Design::new("m");
        assert!(!has_time(&design));
    }

    #[test]
    fn delay_mendeteksi_waktu() {
        let statement = Statement::Delay {
            amount: konstanta(),
            unit: sv_ir::TimeUnit::NanoSeconds,
            body: Box::new(Statement::Noop {
                span: Span::default(),
            }),
            span: Span::default(),
        };
        assert!(statement_uses_time(&statement));
    }

    #[test]
    fn statement_tanpa_waktu_berbeda_false() {
        let statement = Statement::Noop {
            span: Span::default(),
        };
        assert!(!statement_uses_time(&statement));
    }

    #[test]
    fn sim_time_dalam_ekspresi_dideteksi() {
        let expr = Expr::Bin {
            op: sv_ir::BinOp::Add,
            lhs: Box::new(kosong()),
            rhs: Box::new(Expr::SimTime {
                data_type: DataType::signed(64),
                unit: sv_ir::TimeUnit::NanoSeconds,
            }),
            data_type: DataType::logic(64),
        };
        assert!(expr_uses_time(&expr));
    }

    // BUG-3: `Expr::Cast` belum ada lengan di `expr_uses_time`, sehingga
    // `$time` yang disembunyikan di dalam cast tidak terdeteksi dan field
    // `time_now` tidak pernah dibangkitkan di design hasil generate.

    #[test]
    fn sim_time_di_dalam_cast_dideteksi() {
        // `word_t'($time)` harus tetap terdeteksi memakai waktu simulasi.
        let expr = Expr::Cast {
            operand: Box::new(Expr::SimTime {
                data_type: DataType::signed(64),
                unit: sv_ir::TimeUnit::NanoSeconds,
            }),
            data_type: DataType::logic(16),
            operand_signed: false,
        };
        assert!(expr_uses_time(&expr));
    }

    #[test]
    fn cast_tanpa_sim_time_tidak_memerlukan_waktu() {
        let expr = Expr::Cast {
            operand: Box::new(kosong()),
            data_type: DataType::logic(16),
            operand_signed: false,
        };
        assert!(!expr_uses_time(&expr));
    }

    /// BUG: `has_time` hanya memindai body proses, sedangkan nilai awal
    /// variabel (`logic [63:0] stamp = $time;`) juga menghasilkan kode yang
    /// membaca `self.time_now`. Field `time_now` tidak pernah dibangkitkan,
    /// jadi kode hasil generate gagal dikompilasi dengan
    /// `no field 'time_now' on type ...`.
    #[test]
    fn bug_sim_time_di_nilai_awal_variabel_membutuhkan_field_waktu() {
        let var = sv_ir::variable::VarDecl::new(
            "stamp",
            sv_ir::scope::ScopePath::root(),
            DataType::signed(64),
            sv_ir::variable::VarKind::Variable,
            0,
        );
        let mut design = Design::new("tb");
        design.processes.push(Process {
            name: "kosong".to_string(),
            kind: ProcessKind::Combinational,
            sensitivity: Vec::new(),
            body: vec![Statement::Noop {
                span: Span::default(),
            }],
            span: Span::default(),
        });
        assert!(!has_time(&design), "tanpa nilai awal tidak perlu waktu");

        let mut dengan = var.clone();
        dengan.initial = Some(Expr::SimTime {
            data_type: DataType::signed(64),
            unit: sv_ir::TimeUnit::NanoSeconds,
        });
        let mut design = Design::new("tb");
        design.add_variable(dengan);
        assert!(has_time(&design));
    }

    #[test]
    fn delay_tanpa_sim_time_tetap_memerlukan_field_waktu() {
        // Amount-nya konstanta, jadi hanya keberadaan `Delay` yang membuat
        // design memakai waktu.
        let statement = Statement::Delay {
            amount: konstanta(),
            unit: sv_ir::TimeUnit::PicoSeconds,
            body: Box::new(Statement::Block {
                body: Vec::new(),
                span: Span::default(),
            }),
            span: Span::default(),
        };
        assert!(statement_uses_time(&statement));
    }

    #[test]
    fn delay_tersarang_di_dalam_blok_dideteksi() {
        let statement = Statement::Block {
            body: vec![Statement::Delay {
                amount: konstanta(),
                unit: sv_ir::TimeUnit::MicroSeconds,
                body: Box::new(Statement::Noop {
                    span: Span::default(),
                }),
                span: Span::default(),
            }],
            span: Span::default(),
        };
        assert!(statement_uses_time(&statement));
    }

    #[test]
    fn ekspresi_biasa_tidak_dideteksi() {
        assert!(!expr_uses_time(&konstanta()));
    }
}
