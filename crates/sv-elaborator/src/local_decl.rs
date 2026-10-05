// Tanggung jawab: mengumpulkan deklarasi variabel lokal di dalam blok prosedural.
use crate::error::ElaborateError;
use crate::param::ParamTable;
use crate::symbol::{Symbol, SymbolTable};
use sv_ast::combinational::{CaseArm, CombinationalStatement};
use sv_ast::declaration::Declaration;
use sv_ast::module::Module;
use sv_ast::statement::{SequentialStatement, Statement};
use sv_ir::datatype::DataType;
use sv_ir::scope::ScopePath;
use sv_ir::variable::{VarDecl, VarKind};
use sv_ir::Design;

/// Daftarkan seluruh variabel lokal yang muncul di dalam blok prosedural.
///
/// Variabel lokal pada `always_comb`/`always_ff` bersifat statis di SV, jadi
/// cukup diubah menjadi variable biasa pada design. Pendaftaran harus terjadi
/// sebelum statement dielaborasi, karena ekspresi di dalam blok sudah
/// merujuk nama tersebut.
pub fn daftarkan(
    module: &Module,
    design: &mut Design,
    symbols: &mut SymbolTable,
    params: &ParamTable,
    types: &crate::typedef::TypeTable,
) -> Result<(), ElaborateError> {
    for decl in kumpulkan(module) {
        daftarkan_satu(decl, design, symbols, params, types)?;
    }
    Ok(())
}

/// Kumpulkan seluruh deklarasi lokal yang ada di dalam module.
pub fn kumpulkan(module: &Module) -> Vec<&Declaration> {
    let mut out = Vec::new();
    for statement in &module.statements {
        out.extend(kumpulkan_statement(statement));
    }
    out
}

/// Kumpulkan deklarasi lokal dari satu statement modul.
fn kumpulkan_statement(statement: &Statement) -> Vec<&Declaration> {
    match statement {
        Statement::AlwaysComb { body, .. } | Statement::Initial { body, .. } => {
            kumpulkan_comb(body)
        }
        Statement::AlwaysFf { body, .. } => kumpulkan_seq(body),
        Statement::ContinuousAssign { .. } => Vec::new(),
    }
}

/// Kumpulkan deklarasi lokal dari rangkaian statement combinational.
fn kumpulkan_comb(body: &[CombinationalStatement]) -> Vec<&Declaration> {
    let mut out = Vec::new();
    for stmt in body {
        match stmt {
            CombinationalStatement::Decl(decl) => out.push(decl),
            CombinationalStatement::Block { body, .. } => {
                out.extend(kumpulkan_comb(body));
            }
            CombinationalStatement::IfElse {
                then_branch,
                else_branch,
                ..
            } => {
                out.extend(kumpulkan_comb(then_branch));
                if let Some(branch) = else_branch {
                    out.extend(kumpulkan_comb(branch));
                }
            }
            CombinationalStatement::For { body, .. }
            | CombinationalStatement::While { body, .. }
            | CombinationalStatement::Repeat { body, .. }
            | CombinationalStatement::EventControl { body, .. } => out.extend(kumpulkan_comb(body)),
            CombinationalStatement::Delay { body, .. } => {
                out.extend(kumpulkan_comb(std::slice::from_ref(body.as_ref())))
            }
            CombinationalStatement::Case { arms, .. } => {
                for CaseArm { body, .. } in arms {
                    out.extend(kumpulkan_comb(body));
                }
            }
            // Task call dan `return` sudah ter-expand sebelum collected, jadi tidak
            // pernah menambah deklarasi lokal baru.
            CombinationalStatement::BlockingAssign { .. }
            | CombinationalStatement::NonBlockingAssign { .. }
            | CombinationalStatement::CompoundAssign { .. }
            | CombinationalStatement::SystemTask(_)
            | CombinationalStatement::TaskCall(_)
            | CombinationalStatement::Return { .. } => {}
        }
    }
    out
}

/// Kumpulkan deklarasi lokal dari rangkaian statement sekuensial.
fn kumpulkan_seq(body: &[SequentialStatement]) -> Vec<&Declaration> {
    let mut out = Vec::new();
    for stmt in body {
        match stmt {
            SequentialStatement::Decl(decl) => out.push(decl),
            SequentialStatement::IfElse {
                then_branch,
                else_branch,
                ..
            } => {
                out.extend(kumpulkan_seq(then_branch));
                if let Some(branch) = else_branch {
                    out.extend(kumpulkan_seq(branch));
                }
            }
            // Lihat catatan pada `kumpulan_comb`: node ini sudah ter-expand.
            SequentialStatement::NonBlockingAssign { .. }
            | SequentialStatement::BlockingAssign { .. }
            | SequentialStatement::SystemTask(_)
            | SequentialStatement::TaskCall(_)
            | SequentialStatement::Return { .. } => {}
        }
    }
    out
}

/// Daftarkan satu deklarasi lokal sebagai variable design.
fn daftarkan_satu(
    decl: &Declaration,
    design: &mut Design,
    symbols: &mut SymbolTable,
    params: &ParamTable,
    types: &crate::typedef::TypeTable,
) -> Result<(), ElaborateError> {
    let (bit, signed) = crate::typedef::lebar_deklarasi(
        &decl.width,
        decl.type_name.as_deref(),
        types,
        params,
        decl.span,
    )?;
    let data_type = DataType::logic(bit).with_signed(signed || decl.signed);
    let id = symbols.insert(Symbol {
        name: decl.name.clone(),
        signal_id: 0,
        data_type,
        kind: VarKind::Variable,
        unpacked: None,
        span: decl.span,
    })?;

    design.add_variable(VarDecl::new(
        decl.name.clone(),
        ScopePath::root().child(decl.name.clone()),
        data_type,
        VarKind::Variable,
        id,
    ));

    // Nilai awal waktu nol ikut diteruskan (LRM §6.2.2), dan karena itu juga
    // context-determined: ekspresinya sized ke lebar sinyal, dengan arah
    // perluasan mengikuti signedness ekspresi (LRM §11.6.1).
    if let Some(init) = &decl.init {
        let lowered = crate::lower_expr::lower_expression_konteks(init, symbols, data_type.width)?;
        let signed_nilai = lowered.data_type().signed;
        let lowered = crate::lower_expr::sesuaikan_lebar(
            lowered,
            data_type.width,
            data_type.signed,
            signed_nilai,
        );
        design.set_initial(id, lowered);
    }
    Ok(())
}
