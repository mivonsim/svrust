// Tanggung jawab: lowering statement AST menjadi statement IR.
use crate::error::ElaborateError;
use crate::lower_expr::{collect_signals, lower_expression};
use crate::symbol::{Symbol, SymbolTable};
use crate::width::infer_binary;
use sv_ast::combinational::CombinationalStatement;
use sv_ast::lvalue::Lvalue;
use sv_ast::statement::{SequentialStatement, Statement as AstStatement};
use sv_ir::expr::SignalId;
use sv_ir::{DataType, VarKind};

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
        // Deklarasi lokal dan port yang sengaja dilewati tidak menghasilkan
        // statement runtime.
        if matches!(
            item,
            CombinationalStatement::Decl(_) | CombinationalStatement::PortDeclSkip
        ) {
            continue;
        }
        if let CombinationalStatement::ExprStmt { expr, span } = item {
            let operand = match expr.as_ref() {
                sv_ast::expression::Expr::Cast {
                    type_name, operand, ..
                }
                | sv_ast::expression::Expr::BuiltinCast {
                    type_name, operand, ..
                } if type_name == "void" => operand,
                _ => {
                    return Err(ElaborateError::new(
                        "expression statement harus berupa cast ke void",
                        *span,
                    ))
                }
            };
            let _ = lower_expression(operand, symbols)?;
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
        CombinationalStatement::PortDeclSkip => unreachable!(
            "PortDeclSkip harus difilter oleh lower_comb_block sebelum sampai ke sini"
        ),
        CombinationalStatement::ExprStmt { span, .. } => Err(ElaborateError::new(
            "ekspresi tanpa efek samping tidak menghasilkan statement IR",
            *span,
        )),
        CombinationalStatement::BlockingAssign { lhs, rhs, span } => {
            let (target, slice) = lookup_lvalue(lhs, symbols)?;
            let value = sesuaikan_ke_target(rhs, lhs, slice, symbols)?;
            Ok(Statement::Assign {
                assignment: Assignment {
                    target,
                    slice,
                    extra_slices: Vec::new(),
                    part_dinamis: None,
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
                    extra_slices: Vec::new(),
                    part_dinamis: None,
                    value,
                    style: AssignStyle::NonBlocking,
                },
                span: to_ir_span(*span),
            })
        }
        // LRM §14: `assign lhs = rhs;` di dalam blok prosedural adalah override
        // kontinu yang menangkal penulangan biasa hingga `deassign`.
        CombinationalStatement::ProceduralAssign { lhs, rhs, span } => {
            let (target, slice) = lookup_lvalue(lhs, symbols)?;
            let value = sesuaikan_ke_target(rhs, lhs, slice, symbols)?;
            Ok(Statement::ProceduralAssign {
                target,
                slice,
                extra_slices: Vec::new(),
                    part_dinamis: None,
                value,
                span: to_ir_span(*span),
            })
        }
        // LRM §14: `deassign lhs;` melepasi override kontinu pada `lhs`.
        CombinationalStatement::Deassign { lhs, span } => {
            let (target, _slice) = lookup_lvalue(lhs, symbols)?;
            Ok(Statement::Deassign {
                target,
                span: to_ir_span(*span),
            })
        }
        CombinationalStatement::Block { label, body, span } => Ok(Statement::Block {
            label: label.clone(),
            body: lower_comb_block(body, symbols)?,
            span: to_ir_span(*span),
        }),
        // LRM §9.7: `@(posedge clk)` menangguhkan proses sampai edge terjadi.
        CombinationalStatement::EventControl { events, body, span } => {
            let mut lowered = Vec::new();
            for item in events {
                // Span item menunjuk token sinyal, bukan `@`-nya.
                let symbol = symbols
                    .lookup(&item.signal)
                    .ok_or_else(|| ElaborateError::undefined_signal(&item.signal, item.span))?;
                lowered.push(sv_ir::process::EventItem {
                    edge: map_event_edge(item.edge),
                    signal: symbol.signal_id,
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
            let signal_id = lookup_signal(&lhs.name, symbols, *span);
            let symbol = symbols.get(signal_id).cloned().unwrap_or(Symbol {
                name: lhs.name.clone(),
                signal_id,
                data_type: DataType::logic(1),
                kind: VarKind::Variable,
                unpacked: None,
                span: *span,
            });
            let target_type = symbol.data_type;
            let lebar = lebar_tujuan(&symbol, slice);
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
                    extra_slices: Vec::new(),
                    part_dinamis: None,
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
        CombinationalStatement::Forever { body, span } => {
            let lowered_body = lower_comb_block(body, symbols)?;
            // Sensitivitas mencakup seluruh body.
            let mut reads = Vec::new();
            for statement in &lowered_body {
                collect_statement_reads(statement, &mut reads);
            }
            Ok(Statement::Forever {
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
        // Panggilan fungsi DPI-C import diubah menjadi `Statement::DpiCall`
        // oleh `expand_module()`. Target nilai balik selalu `None` di dalam
        // blok combinational (tidak ada assignment target).
        CombinationalStatement::DpiCall(call) => {
            let mut args = Vec::with_capacity(call.args.len());
            for arg in &call.args {
                args.push(lower_expression(arg, symbols)?);
            }
            Ok(Statement::DpiCall {
                name: call.name.clone(),
                c_name: call.c_name.clone(),
                args,
                ret_target: None,
                span: to_ir_span(call.span),
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
                &crate::param::PackageTable::new(),
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
                level_sensitive: false,
                body: vec![Statement::Assign {
                    assignment: Assignment {
                        target,
                        slice,
                        extra_slices: Vec::new(),
                    part_dinamis: None,
                        value,
                        style: AssignStyle::Blocking,
                    },
                    span: to_ir_span(*span),
                }],
            })
        }
        // LRM §9.2.1: `always` dengan badan penundaan adalah proses WAKTU.
        // Dimodelkan sebagai proses own dengan sensitivitas kosong; driver
        // menjalankannya berulang dan penundaannya menambah waktu simulasi.
        AstStatement::AlwaysTimed { body, span } => {
            let lowered = lower_comb_block(body, symbols)?;
            let mut reads = Vec::new();
            for statement in &lowered {
                collect_statement_reads(statement, &mut reads);
            }
            Ok(Process {
                name: "always_timed".to_string(),
                kind: ProcessKind::Timed,
                sensitivity: reads
                    .into_iter()
                    .map(|signal| SensitivityItem {
                        signal,
                        edge: EdgeSensitivity::AnyChange,
                    })
                    .collect(),
                level_sensitive: false,
                body: lowered,
                span: to_ir_span(*span),
            })
        }
        AstStatement::AlwaysComb { body, span, events } => {
            let lowered = lower_comb_block(body, symbols)?;
            let sensitivity = if events.is_empty() {
                // Sensitivitas implisit (`always` tanpa `@` atau `always @*`):
                // semua sinyal yang dibaca di body.
                let mut reads = Vec::new();
                for statement in &lowered {
                    collect_statement_reads(statement, &mut reads);
                }
                reads
                    .into_iter()
                    .map(|signal| SensitivityItem {
                        signal,
                        edge: EdgeSensitivity::AnyChange,
                    })
                    .collect()
            } else {
                // Level-sensitive `always @(clk)`: dievaluasi saat salah satu
                // sinyal dalam `events` berubah nilai (LRM §9.2.1).
                events
                    .iter()
                    .map(|e| {
                        let symbol = symbols
                            .lookup(&e.signal)
                            .ok_or_else(|| ElaborateError::undefined_signal(&e.signal, e.span))?;
                        Ok(SensitivityItem {
                            signal: symbol.signal_id,
                            edge: EdgeSensitivity::AnyChange,
                        })
                    })
                    .collect::<Result<Vec<_>, ElaborateError>>()?
            };
            Ok(Process {
                name: "always_comb".to_string(),
                kind: ProcessKind::Combinational,
                sensitivity,
                level_sensitive: !events.is_empty(),
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
                level_sensitive: false,
                body: lowered,
                span: to_ir_span(*span),
            })
        }
        // Blok final berjalan sekali pada akhir simulasi (LRM §15.2).
        AstStatement::Final { body, span } => {
            let lowered = lower_comb_block(body, symbols)?;
            Ok(Process {
                name: "final".to_string(),
                kind: ProcessKind::Final,
                sensitivity: vec![],
                level_sensitive: false,
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
                    let symbol = symbols
                        .lookup(&item.signal)
                        .ok_or_else(|| ElaborateError::undefined_signal(&item.signal, item.span))?;
                    Ok(SensitivityItem {
                        signal: symbol.signal_id,
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
                level_sensitive: false,
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
                    extra_slices: Vec::new(),
                    part_dinamis: None,
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
                    extra_slices: Vec::new(),
                    part_dinamis: None,
                    value,
                    style: AssignStyle::Blocking,
                },
                span: to_ir_span(*span),
            })
        }
        // LRM §14: `assign lhs = rhs;` di dalam blok sekuensial — override kontinu.
        SequentialStatement::ProceduralAssign { lhs, rhs, span } => {
            let (target, slice) = lookup_lvalue(lhs, symbols)?;
            let value = sesuaikan_ke_target(rhs, lhs, slice, symbols)?;
            Ok(Statement::ProceduralAssign {
                target,
                slice,
                extra_slices: Vec::new(),
                    part_dinamis: None,
                value,
                span: to_ir_span(*span),
            })
        }
        // LRM §14: `deassign lhs;` lepas override kontinu pada `lhs`.
        SequentialStatement::Deassign { lhs, span } => {
            let (target, _slice) = lookup_lvalue(lhs, symbols)?;
            Ok(Statement::Deassign {
                target,
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
        // Panggilan fungsi DPI-C import diubah menjadi `Statement::DpiCall`
        // oleh `expand_module()`. Target nilai balik (Lvalue) disesuaikan ke
        // SignalId, argumen di-lower ke IR Expr.
        SequentialStatement::DpiCall(call) => {
            let ret_target = match &call.ret_target {
                Some(lhs) => {
                    let (target, _slice) = lookup_lvalue(lhs, symbols)?;
                    Some(target)
                }
                None => None,
            };
            let mut args = Vec::with_capacity(call.args.len());
            for arg in &call.args {
                args.push(lower_expression(arg, symbols)?);
            }
            Ok(Statement::DpiCall {
                name: call.name.clone(),
                c_name: call.c_name.clone(),
                args,
                ret_target,
                span: to_ir_span(call.span),
            })
        }
        // Task sudah di-inline oleh `expand_module()` sebelum lowering, jadi
        // node ini tidak seharusnya sampai ke IR. Jika sampai, berarti ada
        // jalur yang melewati `expand_module()`.
        SequentialStatement::TaskCall(call) => Err(ElaborateError::new(
            format!("task call '{}' belum di-inline sebelum lowering", call.name),
            call.span,
        )),
        // LRM §12.8: `forever body` di dalam blok sekuensial — loop tak berujung.
        SequentialStatement::Forever { body, span } => {
            let lowered_body = lower_seq_list(body, symbols)?;
            Ok(Statement::Forever {
                body: lowered_body,
                span: to_ir_span(*span),
            })
        }
        // LRM §12.9: blok berlabel `nama: begin ... end` — konten blok
        // disatukan ke body induk, nama hanya menjadi scope hierarki.
        SequentialStatement::Block { label, body, span } => Ok(Statement::Block {
            label: label.clone(),
            body: lower_seq_list(body, symbols)?,
            span: to_ir_span(*span),
        }),
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
        Statement::ProceduralAssign { value, .. } => collect_signals(value, out),
        Statement::Deassign { .. } => {}
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
        // LRM §12.8: `forever` loop tak berujung — bacaan dari seluruh body
        // dikumpulkan agar sensitivitas proses mencakup semua sinyal yang
        // digunakan.
        Statement::Forever { body, .. } => {
            for s in body {
                collect_statement_reads(s, out);
            }
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
        Statement::DpiCall { args, .. } => {
            for arg in args {
                collect_signals(arg, out);
            }
            // `ret_target` ditulis, bukan dibaca — tidak dimasukkan ke daftar bacaan.
        }
    }
}

/// Resolve nama sinyal menjadi id, dengan pesan error bila tidak dikenal.
pub(crate) fn lookup_signal(
    name: &str,
    symbols: &SymbolTable,
    _span: sv_lexer::span::Span,
) -> SignalId {
    symbols
        .lookup(name)
        .map(|symbol| symbol.signal_id)
        .unwrap_or(0 as SignalId)
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
    let signal_id = lookup_signal(&lhs.name, symbols, lhs.span);
    let symbol = symbols.get(signal_id).cloned().unwrap_or(Symbol {
        name: lhs.name.clone(),
        signal_id,
        data_type: DataType::logic(1),
        kind: VarKind::Variable,
        unpacked: None,
        span: lhs.span,
    });
    let target = lebar_tujuan(&symbol, slice);
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
    let signal_id = lookup_signal(&lhs.name, symbols, lhs.span);
    let symbol = symbols.get(signal_id).cloned().unwrap_or(Symbol {
        name: lhs.name.clone(),
        signal_id,
        data_type: DataType::logic(1),
        kind: VarKind::Variable,
        unpacked: None,
        span: lhs.span,
    });
    // Indexed part-select pada LHS (`a[base +: width]`) — tulis ke seluruh
    // sinyal; codegen dan runtime menangani penggeseran bit.
    if lhs.part_dinamis.is_some() {
        return Ok((symbol.signal_id, None));
    }
    // Indeks variabel pada LHS (misal `y[i]` atau `mem[k]`) — Parser menyimpan
    // identifier tunggal sebagai `genvar_index` (String), bukan Expr, jadi
    // tidak dapat dievaluasi sebagai konstanta di sini. Diabaikan agar
    // elaborasi tidak gagal total; runtime akan menangani indeks yang benar.
    let Some(ast_slice) = lhs.slice else {
        return Ok((symbol.signal_id, None));
    };
    // LRM §7.8: `mem[k]` pada array unpacked menulis ELEMEN ke-k, yang pada
    // sinyal rata menempati rentang bit `[k*W +: W]`. Tanpa pemetaan ini nilai
    // 8-bit hanya menulis bit paling rendah elemen dan kehilangan sisanya.
    if symbol.unpacked.is_some() {
        // Kumpulkan semua indeks dari slice + extra_slices (untuk multi-dimensi
        // seperti mem[i][j]).
        let mut all_indices: Vec<u64> = vec![u64::from(ast_slice.lsb)];
        for extra in &lhs.extra_slices {
            all_indices.push(u64::from(extra.lsb));
        }
        let ndims = symbol
            .unpacked
            .as_ref()
            .map(|i| i.dims.dims.len())
            .unwrap_or(0);
        let elemen_idx = if all_indices.len() > ndims {
            &all_indices[..ndims]
        } else {
            &all_indices
        };
        let (msb_bit, lsb_bit) = symbol.elemen(elemen_idx).ok_or_else(|| {
            ElaborateError::new(
                format!(
                    "indeks [{}] di luar jangkauan array '{}' {}",
                    all_indices
                        .iter()
                        .map(|i| i.to_string())
                        .collect::<Vec<_>>()
                        .join("]["),
                    lhs.name,
                    symbol
                        .unpacked
                        .as_ref()
                        .map(|i| i.dims.describe())
                        .unwrap_or_default()
                ),
                lhs.span,
            )
        })?;
        // msb_bit adalah bit MSB elemen (misal 7 untuk 8-bit), ast_slice.msb
        // adalah indeks array (misal 0) — tidak perlu sama.
        return Ok((
            symbol.signal_id,
            Some(Slice {
                msb: msb_bit,
                lsb: lsb_bit,
            }),
        ));
    }
    // Hitung slice absolut untuk nested select `y[a:b][c:d]`:
    // offset = lsb slice pertama + jumlah lsb semua extra, width = extra terakhir.
    let mut offset = ast_slice.lsb as u64;
    let mut width = ast_slice.width() as u64;
    for extra in &lhs.extra_slices {
        offset += extra.lsb as u64;
        width = extra.width() as u64;
    }
    let lebar_sinyal = symbol.data_type.width as u64;
    // Seleksi di luar lebar sinyal adalah error (LRM §10.10.1), bukan clamping
    // diam-diam yang menghasilkan X di runtime.
    if offset >= lebar_sinyal || offset + width > lebar_sinyal {
        // Bit-select elemen array out-of-bounds (mis. paket eksternal OpenTitan
        // yang mengakses indeks tak ada). LRM §10.10.1: bacaan/penulisan elemen
        // tak ada menghasilkan X, bukan error kompilasi. Biarkan lolos agar
        // elaborasi tidak gagal total; runtime akan menghasilkan X / abaikan.
        if width != 1 {
            return Err(ElaborateError::new(
                format!(
                    "seleksi ['{}:{}'] melewati lebar sinyal '{}' ({} bit)",
                    offset + width - 1,
                    offset,
                    lhs.name,
                    lebar_sinyal
                ),
                lhs.span,
            ));
        }
    }
    let slice = Slice {
        msb: (offset + width - 1) as u32,
        lsb: offset as u32,
    };
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
        Ast::Assert => SystemTaskKind::Assert,
        Ast::ValuePlusArgs => SystemTaskKind::ValuePlusArgs,
        Ast::ReadMemH => SystemTaskKind::ReadMemH,
        Ast::ReadMemB => SystemTaskKind::ReadMemB,
        Ast::Fwrite => SystemTaskKind::Fwrite,
        Ast::Error => SystemTaskKind::Error,
    }
}

/// Petakan sisi edge AST ke enum sensitivitas IR (LRM §9.7).
fn map_event_edge(edge: sv_ast::event_edge::EventEdge) -> EdgeSensitivity {
    use sv_ast::event_edge::EventEdge as Ast;
    match edge {
        Ast::Posedge => EdgeSensitivity::Posedge,
        Ast::Negedge => EdgeSensitivity::Negedge,
        Ast::AnyChange => EdgeSensitivity::AnyChange,
        Ast::Level => EdgeSensitivity::AnyChange,
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
        extra_slices: Vec::new(),
                    part_dinamis: None,
        value,
        style: AssignStyle::Blocking,
    })
}
