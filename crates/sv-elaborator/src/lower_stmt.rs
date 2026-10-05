// Tanggung jawab: lowering statement AST menjadi statement IR.
use crate::error::ElaborateError;
use crate::lower_expr::{collect_signals, lower_expression};
use crate::symbol::{Symbol, SymbolTable};
use crate::width::infer_binary;
use sv_ast::combinational::CombinationalStatement;
use sv_ast::lvalue::Lvalue;
use sv_ast::statement::{SequentialStatement, Statement as AstStatement};
use sv_ir::expr::SignalId;
use sv_ir::process::{
    AssignStyle, Assignment, CaseArm, CaseKind, EdgeSensitivity, Process, ProcessKind,
    SensitivityItem, Slice, Statement,
};
use sv_ir::system_task::{SystemArg, SystemTaskKind};
use sv_ir::{BinOp, Expr};

/// Lower blok statement combinational.
pub fn lower_comb_block(
    body: &[CombinationalStatement],
    symbols: &SymbolTable,
) -> Result<Vec<Statement>, ElaborateError> {
    let mut out = Vec::new();
    for item in body {
        // Deklarasi lokal sudah didaftarkan sebagai variable design,
        // jadi tidak menghasilkan statement runtime.
        if matches!(item, CombinationalStatement::Decl(_)) {
            continue;
        }
        out.push(lower_comb_statement(item, symbols)?);
    }
    Ok(out)
}

fn lower_comb_statement(
    item: &CombinationalStatement,
    symbols: &SymbolTable,
) -> Result<Statement, ElaborateError> {
    match item {
        // Difilter oleh `lower_comb_block`; jika sampai sini berarti ada jalur
        // lowering lain yang belum menyaring deklarasi lokal.
        CombinationalStatement::Decl(decl) => Err(ElaborateError::new(
            "deklarasi lokal tidak boleh dilower sebagai statement",
            decl.span,
        )),
        CombinationalStatement::BlockingAssign { lhs, rhs, span } => {
            let (target, slice) = lookup_lvalue(lhs, symbols)?;
            let value = sesuaikan_ke_target(rhs, lhs, slice, symbols)?;
            Ok(Statement::Assign {
                assignment: Assignment {
                    target,
                    slice,
                    value,
                    style: AssignStyle::Blocking,
                },
                span: to_ir_span(*span),
            })
        }
        // LRM §10.4: `<=` di dalam `initial` testbench sah dan tetap non-blocking.
        CombinationalStatement::NonBlockingAssign { lhs, rhs, span } => {
            let (target, slice) = lookup_lvalue(lhs, symbols)?;
            let value = sesuaikan_ke_target(rhs, lhs, slice, symbols)?;
            Ok(Statement::Assign {
                assignment: Assignment {
                    target,
                    slice,
                    value,
                    style: AssignStyle::NonBlocking,
                },
                span: to_ir_span(*span),
            })
        }
        CombinationalStatement::Block { body, span } => Ok(Statement::Block {
            body: lower_comb_block(body, symbols)?,
            span: to_ir_span(*span),
        }),
        // LRM §9.7: `@(posedge clk)` menangguhkan proses sampai edge terjadi.
        CombinationalStatement::EventControl { events, body, span } => {
            let mut lowered = Vec::new();
            for item in events {
                // Span item menunjuk token sinyal, bukan `@`-nya.
                let signal_id = lookup_signal(&item.signal, symbols, item.span)?;
                lowered.push(sv_ir::process::EventItem {
                    edge: map_event_edge(item.edge),
                    signal: signal_id,
                });
            }
            Ok(Statement::EventControl {
                events: lowered,
                body: lower_comb_block(body, symbols)?,
                span: to_ir_span(*span),
            })
        }
        // LRM §11.3: `y += a` setara `y = y + a` memakai nilai `y` lama.
        CombinationalStatement::CompoundAssign { lhs, op, rhs, span } => {
            let (target, slice) = lookup_lvalue(lhs, symbols)?;
            let symbol = symbols
                .lookup(&lhs.name)
                .ok_or_else(|| ElaborateError::undefined_signal(&lhs.name, *span))?;
            let target_type = symbol.data_type;
            let lebar = lebar_tujuan(symbol, slice);
            // Nilai `y` yang dibaca harus **irisan** LHS-nya kalau ada:
            // `y[15:8] += x` membaca `y[15:8]`, bukan seluruh `y`. Membaca
            // penuh membuat penambahan terjadi pada 16 bit dan bit bawahnya
            // ikut berubah tanpa pernah ditulis.
            let current = match slice {
                None => Expr::signal_ref(target, target_type),
                Some(s) => Expr::Select {
                    base: Box::new(Expr::signal_ref(target, target_type)),
                    msb: s.msb,
                    lsb: s.lsb,
                    data_type: sv_ir::DataType::logic(s.width()),
                },
            };
            // `x` adalah operand context-determined dari `y + x` (dan dari
            // boundary assignment), jadi lebarnya disesuaikan ke `y`.
            let bin_op = map_compound_binop(*op);
            let value = crate::lower_expr::lower_expression_konteks(rhs, symbols, lebar)?;
            let data_type = infer_binary(bin_op, current.data_type(), target_type);
            let lebar_ekspresi = lebar.max(data_type.width);
            let nilai = Expr::Bin {
                op: bin_op,
                lhs: Box::new(crate::lower_expr::sesuaikan_lebar(
                    current,
                    lebar_ekspresi,
                    data_type.signed,
                    data_type.signed,
                )),
                rhs: Box::new(crate::lower_expr::sesuaikan_lebar(
                    value,
                    lebar_ekspresi,
                    data_type.signed,
                    data_type.signed,
                )),
                data_type: data_type.with_width(lebar_ekspresi),
            };
            let signed_nilai = nilai.data_type().signed;
            Ok(Statement::Assign {
                assignment: Assignment {
                    target,
                    slice,
                    value: crate::lower_expr::sesuaikan_lebar(
                        nilai,
                        lebar,
                        target_type.signed,
                        signed_nilai,
                    ),
                    style: AssignStyle::Blocking,
                },
                span: to_ir_span(*span),
            })
        }
        CombinationalStatement::For {
            init,
            condition,
            step,
            body,
            span,
        } => {
            let lowered_condition = lower_expression(condition, symbols)?;
            let lowered_body = lower_comb_block(body, symbols)?;
            // Sensitivitas mencakup kondisi, langkah, dan seluruh body.
            let mut reads = Vec::new();
            collect_signals(&lowered_condition, &mut reads);
            for statement in &lowered_body {
                collect_statement_reads(statement, &mut reads);
            }
            Ok(Statement::For {
                init: lower_for_step(init, symbols)?,
                condition: lowered_condition,
                step: lower_for_step(step, symbols)?,
                body: lowered_body,
                span: to_ir_span(*span),
            })
        }
        CombinationalStatement::SystemTask(task) => {
            let mut args = Vec::new();
            for arg in &task.args {
                match arg {
                    sv_ast::system_task::SystemArg::Format(teks) => {
                        args.push(SystemArg::Format(teks.clone()));
                    }
                    sv_ast::system_task::SystemArg::Value(expr) => {
                        args.push(SystemArg::Value(lower_expression(expr, symbols)?));
                    }
                }
            }
            // Syarat `$monitor if (kondisi)` ikut diturunkan agar bisa dievaluasi runtime.
            let condition = match &task.condition {
                Some(expr) => Some(lower_expression(expr, symbols)?),
                None => None,
            };
            Ok(Statement::SystemTask {
                kind: map_system_task_kind(task.kind.clone()),
                args,
                condition,
                time_scale: map_time_scale(task.time_scale),
                span: to_ir_span(task.span),
            })
        }
        CombinationalStatement::Repeat { count, body, span } => {
            let lowered_count = lower_expression(count, symbols)?;
            let lowered_body = lower_comb_block(body, symbols)?;
            // Hitungan ikut sensitivitas karena jumlah iterasi bisa berubah.
            let mut reads = Vec::new();
            collect_signals(&lowered_count, &mut reads);
            for statement in &lowered_body {
                collect_statement_reads(statement, &mut reads);
            }
            Ok(Statement::Repeat {
                count: lowered_count,
                body: lowered_body,
                span: to_ir_span(*span),
            })
        }
        CombinationalStatement::While {
            condition,
            body,
            span,
        } => {
            let lowered_condition = lower_expression(condition, symbols)?;
            let lowered_body = lower_comb_block(body, symbols)?;
            // Sensitivitas mencakup kondisi dan seluruh body.
            let mut reads = Vec::new();
            collect_signals(&lowered_condition, &mut reads);
            for statement in &lowered_body {
                collect_statement_reads(statement, &mut reads);
            }
            Ok(Statement::While {
                condition: lowered_condition,
                body: lowered_body,
                span: to_ir_span(*span),
            })
        }
        // LRM §11.2: `#n <stmt>` menggeser waktu lalu menjalankan statement.
        CombinationalStatement::Delay {
            amount,
            unit,
            body,
            span,
        } => {
            let lowered_amount = lower_expression(amount, symbols)?;
            let lowered_body = lower_comb_statement(body, symbols)?;
            let mut reads = Vec::new();
            collect_signals(&lowered_amount, &mut reads);
            collect_statement_reads(&lowered_body, &mut reads);
            Ok(Statement::Delay {
                amount: lowered_amount,
                unit: map_time_unit(*unit),
                body: Box::new(lowered_body),
                span: to_ir_span(*span),
            })
        }
        CombinationalStatement::IfElse {
            condition,
            then_branch,
            else_branch,
            span,
        } => Ok(Statement::If {
            condition: lower_expression(condition, symbols)?,
            then_branch: lower_comb_block(then_branch, symbols)?,
            else_branch: match else_branch {
                Some(branch) => lower_comb_block(branch, symbols)?,
                None => Vec::new(),
            },
            span: to_ir_span(*span),
        }),
        CombinationalStatement::Case {
            selector,
            arms,
            kind,
            span,
        } => {
            // LRM §12.5: label `case` diuji satu per satu dan nilai label
            // disimpan sebagai konstanta. Lebar di atas 64 bit belum bisa
            // ditampung, jadi ditolak di sini — lebih baik daripada
            // membandingkan hanya 64 bit bawah dan mengambil cabang yang salah.
            let lebar = lower_expression(selector, symbols)?.data_type().width;
            if lebar > 64 {
                return Err(ElaborateError::new(
                    format!("lebar selektor `case` {lebar} bit belum didukung (maksimum 64)"),
                    *span,
                ));
            }
            Ok(Statement::Case {
                selector: lower_expression(selector, symbols)?,
                arms: lower_case_arms(arms, symbols)?,
                kind: map_case_kind(*kind),
                span: to_ir_span(*span),
            })
        }
        // LRM §13.3/§13.4: `expand_module` sudah mengembangkan task call dan
        // `return` sebelum lowering. Node yang lolos berarti ada jalur yang
        // melewatkan pass tersebut, jadi error, bukan diabaikan diam-diam.
        CombinationalStatement::TaskCall(call) => Err(ElaborateError::new(
            format!("task call '{}' belum dikembangkan", call.name),
            call.span,
        )),
        CombinationalStatement::Return { span, .. } => Err(ElaborateError::new(
            "`return` hanya sah di dalam task/function",
            *span,
        )),
    }
}

fn lower_case_arms(
    arms: &[sv_ast::combinational::CaseArm],
    symbols: &SymbolTable,
) -> Result<Vec<CaseArm>, ElaborateError> {
    let mut out = Vec::new();
    for arm in arms {
        let mut labels = Vec::new();
        for label in &arm.labels {
            // LRM §12.5: label `case` adalah `constant_expression`. Label
            // non-konstan tidak bisa dibandingkan saat codegen; tanpa
            // penjagaan, `case (sel) sig: ...` diam-diam diperlakukan sebagai
            // label `0` (atau lengannya dibuang sama sekali pada `casez`)
            // sehingga cabang yang salah dipilih tanpa pesan.
            //
            // Cast konstan seperti `16'(16'h01F2)` tetap sah, jadi statusnya
            // diperiksa lewat evaluator `konst`, bukan dari bentuk AST.
            let span = crate::lower_expr::span_expr(label);
            crate::konst::konst(
                label,
                &crate::generate::GenvarEnv::kosong(),
                &crate::param::ParamTable::baru(&[])?,
                span,
            )
            .map_err(|_| {
                ElaborateError::new(
                    "label `case` harus konstanta; label non-konstan belum didukung",
                    span,
                )
            })?;
            labels.push(lower_expression(label, symbols)?);
        }
        out.push(CaseArm {
            labels,
            is_default: arm.is_default,
            body: lower_comb_block(&arm.body, symbols)?,
        });
    }
    Ok(out)
}

/// Lower statement level module: continuous assign, always_ff, always_comb.
pub fn lower_module_statement(
    statement: &AstStatement,
    symbols: &SymbolTable,
) -> Result<Process, ElaborateError> {
    match statement {
        AstStatement::ContinuousAssign { lhs, rhs, span } => {
            let (target, slice) = lookup_lvalue(lhs, symbols)?;
            let value = sesuaikan_ke_target(rhs, lhs, slice, symbols)?;

            let mut reads = Vec::new();
            collect_signals(&value, &mut reads);

            Ok(Process {
                name: format!("assign_{}", lhs.name),
                kind: ProcessKind::Combinational,
                span: to_ir_span(*span),
                sensitivity: reads
                    .into_iter()
                    .map(|signal| SensitivityItem {
                        signal,
                        edge: EdgeSensitivity::AnyChange,
                    })
                    .collect(),
                body: vec![Statement::Assign {
                    assignment: Assignment {
                        target,
                        slice,
                        value,
                        style: AssignStyle::Blocking,
                    },
                    span: to_ir_span(*span),
                }],
            })
        }
        AstStatement::AlwaysComb { body, span } => {
            let lowered = lower_comb_block(body, symbols)?;
            let mut reads = Vec::new();
            for statement in &lowered {
                collect_statement_reads(statement, &mut reads);
            }
            Ok(Process {
                name: "always_comb".to_string(),
                kind: ProcessKind::Combinational,
                sensitivity: reads
                    .into_iter()
                    .map(|signal| SensitivityItem {
                        signal,
                        edge: EdgeSensitivity::AnyChange,
                    })
                    .collect(),
                body: lowered,
                span: to_ir_span(*span),
            })
        }
        // Blok initial berjalan sekali pada waktu nol (LRM §15.2).
        AstStatement::Initial { body, span } => {
            let lowered = lower_comb_block(body, symbols)?;
            Ok(Process {
                name: "initial".to_string(),
                kind: ProcessKind::Initial,
                sensitivity: vec![],
                body: lowered,
                span: to_ir_span(*span),
            })
        }
        AstStatement::AlwaysFf { events, body, span } => {
            // LRM §9.7: seluruh item event jadi sensitivitas proses, misalnya
            // `@(posedge clk or posedge rst)` → dua item dengan edge masing-masing.
            let sensitivity = events
                .iter()
                .map(|item| {
                    Ok(SensitivityItem {
                        signal: lookup_signal(&item.signal, symbols, item.span)?,
                        edge: map_event_edge(item.edge),
                    })
                })
                .collect::<Result<Vec<_>, ElaborateError>>()?;
            let nama_proses = events
                .first()
                .map(|item| item.signal.as_str())
                .unwrap_or("empty");
            let mut lowered = Vec::new();
            for item in body {
                // Deklarasi lokal sudah jadi variable design, tanpa efek runtime.
                if matches!(item, SequentialStatement::Decl(_)) {
                    continue;
                }
                lowered.push(lower_seq_statement(item, symbols)?);
            }
            Ok(Process {
                name: format!("always_ff_{}", nama_proses),
                kind: ProcessKind::Sequential,
                sensitivity,
                body: lowered,
                span: to_ir_span(*span),
            })
        }
    }
}

fn lower_seq_statement(
    item: &SequentialStatement,
    symbols: &SymbolTable,
) -> Result<Statement, ElaborateError> {
    match item {
        // Difilter oleh `lower_seq_block`; lihat catatan di `lower_comb_statement`.
        SequentialStatement::Decl(decl) => Err(ElaborateError::new(
            "deklarasi lokal tidak boleh dilower sebagai statement",
            decl.span,
        )),
        SequentialStatement::NonBlockingAssign { lhs, rhs, span } => {
            let (target, slice) = lookup_lvalue(lhs, symbols)?;
            let value = sesuaikan_ke_target(rhs, lhs, slice, symbols)?;
            Ok(Statement::Assign {
                assignment: Assignment {
                    target,
                    slice,
                    value,
                    style: AssignStyle::NonBlocking,
                },
                span: to_ir_span(*span),
            })
        }
        SequentialStatement::BlockingAssign { lhs, rhs, span } => {
            let (target, slice) = lookup_lvalue(lhs, symbols)?;
            let value = sesuaikan_ke_target(rhs, lhs, slice, symbols)?;
            Ok(Statement::Assign {
                assignment: Assignment {
                    target,
                    slice,
                    value,
                    style: AssignStyle::Blocking,
                },
                span: to_ir_span(*span),
            })
        }
        // LRM §12.4: `if (syarat) ... else ...` di dalam blok sekuensial.
        SequentialStatement::IfElse {
            condition,
            then_branch,
            else_branch,
            span,
        } => {
            let cond = lower_expression(condition, symbols)?;
            let then_ir = lower_seq_list(then_branch, symbols)?;
            // IR memakai daftar kosong untuk `if` tanpa `else` (LRM §12.4).
            let else_ir = match else_branch {
                Some(branch) => lower_seq_list(branch, symbols)?,
                None => Vec::new(),
            };
            Ok(Statement::If {
                condition: cond,
                then_branch: then_ir,
                else_branch: else_ir,
                span: to_ir_span(*span),
            })
        }
        SequentialStatement::SystemTask(task) => {
            let mut args = Vec::new();
            for arg in &task.args {
                match arg {
                    sv_ast::system_task::SystemArg::Format(t) => {
                        args.push(SystemArg::Format(t.clone()));
                    }
                    sv_ast::system_task::SystemArg::Value(e) => {
                        args.push(SystemArg::Value(lower_expression(e, symbols)?));
                    }
                }
            }
            Ok(Statement::SystemTask {
                kind: map_system_task_kind(task.kind.clone()),
                args,
                condition: match &task.condition {
                    Some(expr) => Some(lower_expression(expr, symbols)?),
                    None => None,
                },
                time_scale: map_time_scale(task.time_scale),
                span: to_ir_span(task.span),
            })
        }
        // Lihat catatan pada `lower_comb_statement`: node ini sudah ter-expand.
        SequentialStatement::TaskCall(call) => Err(ElaborateError::new(
            format!("task call '{}' belum dikembangkan", call.name),
            call.span,
        )),
        SequentialStatement::Return { span, .. } => Err(ElaborateError::new(
            "`return` hanya sah di dalam task/function",
            *span,
        )),
    }
}

/// Lower rangkaian statement sekuensial; deklarasi lokal dilewati karena
/// sudah menjadi variable design tanpa efek runtime.
fn lower_seq_list(
    body: &[SequentialStatement],
    symbols: &SymbolTable,
) -> Result<Vec<Statement>, ElaborateError> {
    let mut out = Vec::new();
    for item in body {
        if matches!(item, SequentialStatement::Decl(_)) {
            continue;
        }
        out.push(lower_seq_statement(item, symbols)?);
    }
    Ok(out)
}

/// Kumpulkan signal yang dibaca sebuah statement IR.
pub fn collect_statement_reads(statement: &Statement, out: &mut Vec<SignalId>) {
    match statement {
        Statement::Assign { assignment, .. } => collect_signals(&assignment.value, out),
        Statement::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            collect_signals(condition, out);
            for s in then_branch.iter().chain(else_branch.iter()) {
                collect_statement_reads(s, out);
            }
        }
        Statement::Case { selector, arms, .. } => {
            collect_signals(selector, out);
            for arm in arms {
                for s in &arm.body {
                    collect_statement_reads(s, out);
                }
            }
        }
        Statement::Block { body, .. } => {
            for s in body {
                collect_statement_reads(s, out);
            }
        }
        Statement::For {
            condition, body, ..
        }
        | Statement::While {
            condition, body, ..
        } => {
            collect_signals(condition, out);
            for s in body {
                collect_statement_reads(s, out);
            }
        }
        Statement::Repeat { count, body, .. } => {
            collect_signals(count, out);
            for s in body {
                collect_statement_reads(s, out);
            }
        }
        // Argumen nilai pada system task dibaca seperti ekspresi biasa.
        Statement::SystemTask {
            args, condition, ..
        } => {
            for arg in args {
                if let sv_ir::system_task::SystemArg::Value(expr) = arg {
                    collect_signals(expr, out);
                }
            }
            // Syarat `$monitor if` juga boleh berubah antar timestep.
            if let Some(expr) = condition {
                collect_signals(expr, out);
            }
        }
        Statement::Delay { amount, body, .. } => {
            collect_signals(amount, out);
            collect_statement_reads(body, out);
        }
        // Sinyal yang diawasi event control dibaca tiap langkah, jadi masuk
        // daftar bacaan supaya proses initial dievaluasi saat berubah.
        Statement::EventControl { events, body, .. } => {
            for item in events {
                out.push(item.signal);
            }
            for s in body {
                collect_statement_reads(s, out);
            }
        }
        Statement::Noop { .. } => {}
    }
}

/// Resolve nama sinyal menjadi id, dengan pesan error bila tidak dikenal.
fn lookup_signal(
    name: &str,
    symbols: &SymbolTable,
    span: sv_lexer::span::Span,
) -> Result<SignalId, ElaborateError> {
    symbols
        .lookup(name)
        .map(|symbol| symbol.signal_id)
        .ok_or_else(|| ElaborateError::undefined_signal(name, span))
}

/// Resolve Lvalue menjadi id sinyal beserta irisan bit opsionalnya.
/// Lebar irisan harus muat di lebar sinyal (LRM §10.10.1).
/// Lebar yang harus diisi oleh nilai ekspresi pada batas assignment.
///
/// LRM §11.6.1 Tabel 11-21: ekspresi kanan adalah context-determined, jadi
/// lebarnya disesuaikan dengan target. Untuk LHS teriris (`y[3:0] = x`) lebar
/// tujuannya hanya lebar irisan, bukan lebar penuh sinyal — nilai lalu digeser
/// ke posisi bitnya oleh codegen.
fn lebar_tujuan(symbol: &Symbol, slice: Option<Slice>) -> u32 {
    match slice {
        None => symbol.data_type.width,
        Some(s) => s.width(),
    }
}

/// Lower ekspresi kanan assignment lalu sesuaikan lebarnya ke target.
///
/// Ini titik penting untuk signedness: `logic [31:0] o = sa` dengan `sa`
/// signed 8-bit harus sign-extend (LRM §11.6.1), bukan zero-extend. Tanpa
/// langkah ini `signed'(x)` menjadi no-op di setiap assignment karena nilai
/// signed apa pun ikut terpotong jadi bentuk unsigned saat ditulis.
fn sesuaikan_ke_target(
    expr: &sv_ast::expression::Expr,
    lhs: &Lvalue,
    slice: Option<Slice>,
    symbols: &SymbolTable,
) -> Result<Expr, ElaborateError> {
    let symbol = symbols
        .lookup(&lhs.name)
        .ok_or_else(|| ElaborateError::undefined_signal(&lhs.name, lhs.span))?;
    let target = lebar_tujuan(symbol, slice);
    // Lebar konteks diteruskan ke dalam ekspresi (LRM §11.6.1 Tabel 11-21),
    // bukan hanya ke hasilnya: `logic [31:0] z; logic [7:0] y; z = y + y;`
    // harus menjumlahkan pada 32 bit, bukan 8 bit lalu dilebarkan.
    let lowered = crate::lower_expr::lower_expression_konteks(expr, symbols, target)?;
    // Tipe hasilnya mengikuti target (LRM §11.6.1 Tabel 11-21), tapi arah
    // perluasan mengikuti signedness ekspresi aslinya (LRM §6.2.1): `assign y
    // = a` dengan `a` signed 8-bit dan `y` unsigned 32-bit menghasilkan
    // `Cast { data_type: unsigned(32), operand_signed: true }`.
    let signed_nilai = lowered.data_type().signed;
    Ok(crate::lower_expr::sesuaikan_lebar(
        lowered,
        target,
        symbol.data_type.signed,
        signed_nilai,
    ))
}

fn lookup_lvalue(
    lhs: &Lvalue,
    symbols: &SymbolTable,
) -> Result<(SignalId, Option<Slice>), ElaborateError> {
    let symbol = symbols
        .lookup(&lhs.name)
        .ok_or_else(|| ElaborateError::undefined_signal(&lhs.name, lhs.span))?;
    // Indeks variabel pada LHS belum didukung — baik `y[i]` (bit-select) maupun
    // `mem[k]` (elemen array unpacked, LRM §7.8). Parser menyimpan identifier
    // tunggal sebagai `genvar_index`, jadi pesan sebelumnya selalu menyebut
    // "genvar" walau penulis kode tidak menulis genvar sama sekali; sekarang
    // keduanya dijelaskan apa adanya, plus discriminate sinyal array.
    if let Some(genvar) = &lhs.genvar_index {
        let apa = if symbol.unpacked.is_some() {
            format!(
                "penulisan indeks variabel ke array unpacked '{}' belum didukung",
                lhs.name
            )
        } else {
            "penulisan dengan indeks variabel pada LHS belum didukung".to_string()
        };
        return Err(ElaborateError::new(
            format!("{apa} (indeks '{}'); pakai indeks konstan", genvar),
            lhs.span,
        ));
    }
    let Some(ast_slice) = lhs.slice else {
        return Ok((symbol.signal_id, None));
    };
    // LRM §7.8: `mem[k]` pada array unpacked menulis ELEMEN ke-k, yang pada
    // sinyal rata menempati rentang bit `[k*W +: W]`. Tanpa pemetaan ini nilai
    // 8-bit hanya menulis bit paling rendah elemen dan kehilangan sisanya.
    if symbol.unpacked.is_some() {
        let msb_bit = ast_slice.msb;
        let (msb, lsb) = symbol.elemen(u64::from(ast_slice.lsb)).ok_or_else(|| {
            ElaborateError::new(
                format!(
                    "indeks {} di luar jangkauan array '{}' {}",
                    ast_slice.lsb,
                    lhs.name,
                    symbol
                        .unpacked
                        .map(|i| i.dim.describe())
                        .unwrap_or_default()
                ),
                lhs.span,
            )
        })?;
        debug_assert!(msb_bit == ast_slice.msb);
        return Ok((symbol.signal_id, Some(Slice { msb, lsb })));
    }
    let slice = Slice {
        msb: ast_slice.msb,
        lsb: ast_slice.lsb,
    };
    if slice.msb >= symbol.data_type.width {
        return Err(ElaborateError::new(
            format!(
                "part-select [{}:{}] di luar lebar sinyal '{}' ({} bit)",
                slice.msb, slice.lsb, lhs.name, symbol.data_type.width
            ),
            lhs.span,
        ));
    }
    Ok((symbol.signal_id, Some(slice)))
}

/// Konversi span lexer ke span IR.
fn to_ir_span(span: sv_lexer::span::Span) -> sv_ir::process::Span {
    sv_ir::process::Span {
        file_id: 0,
        line: span.line as u32,
        col: span.col as u32,
    }
}

/// Petakan jenis case AST ke IR.
/// Petakan `timescale` AST ke bentuk IR.
pub(crate) fn map_time_scale(scale: sv_ast::time_scale::TimeScale) -> sv_ir::time_scale::TimeScale {
    sv_ir::time_scale::TimeScale::new(map_time_unit(scale.unit), map_time_unit(scale.precision))
}

/// Petakan satuan waktu AST ke enum IR.
pub(crate) fn map_time_unit(unit: sv_ast::time_unit::TimeUnit) -> sv_ir::time_unit::TimeUnit {
    use sv_ast::time_unit::TimeUnit as Ast;
    use sv_ir::time_unit::TimeUnit as Ir;
    match unit {
        // `Bawaan` sudah diselesaikan parser lewat
        // `sv_ast::delay_unit::terapkan_module`. Kalau sampai ke sini, modul
        // dibangun tanpa `parse_file`, jadi tidak ada `timescale` — nanosecond
        // dipakai agar delay tetap punya satuan yang pasti.
        Ast::Bawaan => Ir::NanoSeconds,
        Ast::Seconds => Ir::Seconds,
        Ast::MilliSeconds => Ir::MilliSeconds,
        Ast::MicroSeconds => Ir::MicroSeconds,
        Ast::NanoSeconds => Ir::NanoSeconds,
        Ast::PicoSeconds => Ir::PicoSeconds,
        Ast::FectoSeconds => Ir::FectoSeconds,
    }
}

/// Petakan jenis system task AST ke enum IR.
fn map_system_task_kind(kind: sv_ast::system_task::SystemTaskKind) -> SystemTaskKind {
    use sv_ast::system_task::SystemTaskKind as Ast;
    match kind {
        Ast::Display => SystemTaskKind::Display,
        Ast::Finish => SystemTaskKind::Finish,
        Ast::Monitor => SystemTaskKind::Monitor,
        Ast::MonitorOn => SystemTaskKind::MonitorOn,
        Ast::MonitorOff => SystemTaskKind::MonitorOff,
        Ast::Strobe => SystemTaskKind::Strobe,
        Ast::DumpFile => SystemTaskKind::DumpFile,
        Ast::DumpVars => SystemTaskKind::DumpVars,
    }
}

/// Petakan sisi edge AST ke enum sensitivitas IR (LRM §9.7).
fn map_event_edge(edge: sv_ast::event_edge::EventEdge) -> EdgeSensitivity {
    use sv_ast::event_edge::EventEdge as Ast;
    match edge {
        Ast::Posedge => EdgeSensitivity::Posedge,
        Ast::Negedge => EdgeSensitivity::Negedge,
        Ast::AnyChange => EdgeSensitivity::AnyChange,
    }
}

fn map_case_kind(kind: sv_ast::combinational::CaseKind) -> CaseKind {
    match kind {
        sv_ast::combinational::CaseKind::Exact => CaseKind::Exact,
        sv_ast::combinational::CaseKind::Casez => CaseKind::Casez,
        sv_ast::combinational::CaseKind::Casex => CaseKind::Casex,
    }
}

/// Petakan operator compound AST ke operator biner IR.
fn map_compound_binop(op: sv_ast::combinational::CompoundOp) -> BinOp {
    use sv_ast::combinational::CompoundOp as C;
    match op {
        C::Add => BinOp::Add,
        C::Sub => BinOp::Sub,
        C::Mul => BinOp::Mul,
        C::Div => BinOp::Div,
        C::Mod => BinOp::Mod,
        C::BitAnd => BinOp::BitAnd,
        C::BitOr => BinOp::BitOr,
        C::BitXor => BinOp::BitXor,
        C::Shl => BinOp::Shl,
        C::Shr => BinOp::Shr,
        C::Sar => BinOp::Sar,
    }
}

/// Lower satu langkah `for` menjadi assignment blocking.
///
/// Init dan step `for` adalah assignment biasa, jadi ekspresi kanannya
/// context-determined (LRM §11.6.1). Tanpa itu `for (si = -8'sd1; si > 0;
/// si = si - 1)` dengan `si` signed 32-bit membuat inisialisasi terpotong
/// menjadi positif dan loop berjalan 255 kali.
fn lower_for_step(
    step: &sv_ast::combinational::ForStep,
    symbols: &SymbolTable,
) -> Result<Assignment, ElaborateError> {
    let (target, slice) = lookup_lvalue(&step.lhs, symbols)?;
    let value = sesuaikan_ke_target(&step.rhs, &step.lhs, slice, symbols)?;
    Ok(Assignment {
        target,
        slice,
        value,
        style: AssignStyle::Blocking,
    })
}
