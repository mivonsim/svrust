// Tanggung jawab: resolusi satuan penulisan waktu (`#delay` dan `$time`)
// memakai `timescale` modul.
//
// Parser menandai `#n` tanpa satuan dan `$time` sebagai `TimeUnit::Bawaan`
// karena `timescale` baru diketahui setelah modul diparse (LRM §21.8). Modul
// ini berjalan sekali per modul, sebelum elaborasi, sehingga statement
// lowering tidak perlu membawa konteks `timescale`.
//
// `$time` ikut di sini karena LRM §21.8 menyatakan skalanya juga `timeunit`
// modul **yang memuat `$time`**, bukan modul top: setelah flatten hanya
// modul top yang tercatat di `Design`, jadi skala yang hilang harus sudah
// tersimpan di node AST-nya.
use crate::combinational::{CaseArm, CombinationalStatement, ForStep};
use crate::declaration::Declaration;
use crate::expression::Expr;
use crate::generate::{GenerateItem, GenerateRegion};
use crate::module::Module;
use crate::routine::RoutineDecl;
use crate::statement::{SequentialStatement, Statement};
use crate::system_task::{SystemArg, SystemTask};
use crate::time_scale::TimeScale;
use crate::time_unit::TimeUnit;
use crate::width::WidthExpr;

/// Terapkan `timescale` modul pada `#delay`, `$time`, dan `%t`.
///
/// `Bawaan` pada `#delay` dan `$time` diganti `scale.unit`; setiap
/// `SystemTask` diberi `scale` penuh karena `%t` butuh `timeprecision`.
pub fn terapkan_module(module: &mut Module, scale: TimeScale) {
    for statement in &mut module.statements {
        statement_modul(statement, scale);
    }
    for region in &mut module.generates {
        region_generate(region, scale);
    }
    for routine in &mut module.routines {
        subrutin(routine, scale);
    }
    // Deklarasi level modul: nilai awal boleh memuat `$time`, misal
    // `time t0 = $time;`.
    for decl in &mut module.declarations {
        deklarasi(decl, scale);
    }
    for port in &mut module.ports {
        lebar_expr(&mut port.width, scale);
    }
    for localparam in &mut module.localparams {
        ekspresi(&mut localparam.value, scale);
    }
    for typedef in &mut module.typedefs {
        match &mut typedef.body {
            crate::typedef::TypeDefBody::Scalar { width, .. } => lebar_expr(width, scale),
            crate::typedef::TypeDefBody::Enum {
                base_width,
                members,
            } => {
                lebar_expr(base_width, scale);
                for member in members {
                    if let Some(value) = &mut member.value {
                        ekspresi(value, scale);
                    }
                }
            }
        }
    }
}

/// Statement level modul: `always_ff`, `always_comb`, dan `initial`.
fn statement_modul(statement: &mut Statement, scale: TimeScale) {
    match statement {
        Statement::ContinuousAssign { rhs, .. } => ekspresi(rhs, scale),
        Statement::AlwaysFf { body, .. } => {
            for stmt in body {
                statement_sekuensial(stmt, scale);
            }
        }
        Statement::AlwaysComb { body, .. } | Statement::Initial { body, .. } => {
            for stmt in body {
                statement_kombinasional(stmt, scale);
            }
        }
    }
}

/// Statement di dalam `always_ff` (LRM §9.2.1).
///
/// AST sekuensial belum punya `#delay`, jadi tidak ada satuan yang perlu
/// diisi di sini — tapi ekspresi di dalamnya bisa memuat `$time`, jadi
/// operandnya tetap ditelusuri.
fn statement_sekuensial(statement: &mut SequentialStatement, scale: TimeScale) {
    match statement {
        SequentialStatement::IfElse {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            ekspresi(condition, scale);
            for stmt in then_branch {
                statement_sekuensial(stmt, scale);
            }
            if let Some(branch) = else_branch {
                for stmt in branch {
                    statement_sekuensial(stmt, scale);
                }
            }
        }
        SequentialStatement::NonBlockingAssign { rhs, .. }
        | SequentialStatement::BlockingAssign { rhs, .. } => ekspresi(rhs, scale),
        SequentialStatement::Decl(decl) => deklarasi(decl, scale),
        SequentialStatement::SystemTask(task) => system_task(task, scale),
        SequentialStatement::TaskCall(call) => {
            for arg in &mut call.args {
                ekspresi(arg, scale);
            }
        }
        SequentialStatement::Return { value, .. } => {
            if let Some(value) = value {
                ekspresi(value, scale);
            }
        }
    }
}

/// Statement di dalam `always_comb`/`initial`, termasuk `#delay` bersarang.
fn statement_kombinasional(statement: &mut CombinationalStatement, scale: TimeScale) {
    match statement {
        CombinationalStatement::Delay {
            amount,
            unit: unit_delay,
            body,
            ..
        } => {
            if *unit_delay == TimeUnit::Bawaan {
                *unit_delay = scale.unit;
            }
            ekspresi(amount, scale);
            statement_kombinasional(body, scale);
        }
        CombinationalStatement::For {
            init,
            condition,
            step,
            body,
            ..
        } => {
            for_step(init, scale);
            ekspresi(condition, scale);
            for_step(step, scale);
            for stmt in body {
                statement_kombinasional(stmt, scale);
            }
        }
        CombinationalStatement::Repeat { count, body, .. } => {
            ekspresi(count, scale);
            for stmt in body {
                statement_kombinasional(stmt, scale);
            }
        }
        CombinationalStatement::While {
            condition, body, ..
        } => {
            ekspresi(condition, scale);
            for stmt in body {
                statement_kombinasional(stmt, scale);
            }
        }
        CombinationalStatement::Case { selector, arms, .. } => {
            ekspresi(selector, scale);
            for arm in arms {
                case_arm(arm, scale);
            }
        }
        CombinationalStatement::IfElse {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            ekspresi(condition, scale);
            for stmt in then_branch {
                statement_kombinasional(stmt, scale);
            }
            if let Some(branch) = else_branch {
                for stmt in branch {
                    statement_kombinasional(stmt, scale);
                }
            }
        }
        CombinationalStatement::Block { body, .. } => {
            for stmt in body {
                statement_kombinasional(stmt, scale);
            }
        }
        CombinationalStatement::EventControl { body, .. } => {
            for stmt in body {
                statement_kombinasional(stmt, scale);
            }
        }
        CombinationalStatement::BlockingAssign { rhs, .. }
        | CombinationalStatement::NonBlockingAssign { rhs, .. }
        | CombinationalStatement::CompoundAssign { rhs, .. } => ekspresi(rhs, scale),
        CombinationalStatement::Decl(decl) => deklarasi(decl, scale),
        CombinationalStatement::SystemTask(task) => system_task(task, scale),
        CombinationalStatement::TaskCall(call) => {
            for arg in &mut call.args {
                ekspresi(arg, scale);
            }
        }
        CombinationalStatement::Return { value, .. } => {
            if let Some(value) = value {
                ekspresi(value, scale);
            }
        }
    }
}

/// Satu lengan `case` beserta labelnya.
fn case_arm(arm: &mut CaseArm, scale: TimeScale) {
    for label in &mut arm.labels {
        ekspresi(label, scale);
    }
    for stmt in &mut arm.body {
        statement_kombinasional(stmt, scale);
    }
}

/// Satu langkah `for` — hanya sisi kanan yang berupa ekspresi.
fn for_step(step: &mut ForStep, scale: TimeScale) {
    ekspresi(&mut step.rhs, scale);
}

/// Deklarasi lokal: nilai awal dan ekspresi lebarnya bisa memuat `$time`.
fn deklarasi(decl: &mut Declaration, scale: TimeScale) {
    if let Some(init) = &mut decl.init {
        ekspresi(init, scale);
    }
    lebar_expr(&mut decl.width, scale);
}

/// Panggilan system task beserta seluruh argumen nilainya.
fn system_task(task: &mut SystemTask, scale: TimeScale) {
    task.time_scale = scale;
    if let Some(cond) = &mut task.condition {
        ekspresi(cond, scale);
    }
    for arg in &mut task.args {
        if let SystemArg::Value(value) = arg {
            ekspresi(value, scale);
        }
    }
}

/// Ekspresi pada batas lebar `[W+1:0]`.
fn lebar_expr(width: &mut WidthExpr, scale: TimeScale) {
    if let WidthExpr::Expr { msb, lsb } = width {
        ekspresi(msb, scale);
        if let Some(lsb) = lsb {
            ekspresi(lsb, scale);
        }
    }
}

/// Region `generate` beserta proses di dalamnya (LRM §27).
fn region_generate(region: &mut GenerateRegion, scale: TimeScale) {
    for item in &mut region.items {
        item_generate(item, scale);
    }
}

fn item_generate(item: &mut GenerateItem, scale: TimeScale) {
    match item {
        GenerateItem::For(g) => item_generate_list(&mut g.body, scale),
        GenerateItem::If(g) => {
            item_generate_list(&mut g.then_branch, scale);
            if let Some(branch) = g.else_branch.as_mut() {
                item_generate_list(branch, scale);
            }
        }
        GenerateItem::Case(g) => {
            for arm in &mut g.arms {
                item_generate_list(&mut arm.body, scale);
            }
        }
        GenerateItem::Process(p) => statement_modul(p, scale),
        GenerateItem::Instance(_) | GenerateItem::Decl(_) | GenerateItem::Assign { .. } => {}
    }
}

fn item_generate_list(items: &mut [GenerateItem], scale: TimeScale) {
    for item in items {
        item_generate(item, scale);
    }
}

/// Badan task/function, termasuk `#delay` di dalamnya.
fn subrutin(routine: &mut RoutineDecl, scale: TimeScale) {
    for stmt in &mut routine.body {
        statement_kombinasional(stmt, scale);
    }
}

/// Isi satuan `timeunit` modul pada setiap `$time` dalam ekspresi.
///
/// LRM §21.8: `$time` berskala ke `timeunit` modul **yang memuatnya**. Setelah
/// modul di-flatten ke satu design, modul top saja yang skalnya tersimpan di
/// `Design`, jadi nilai yang benar harus sudah ada di node ini — kalau tidak,
/// `$time` di modul anak diam-diam memakai satuan top dan seluruh simulasi
/// bergeser.
fn ekspresi(expr: &mut Expr, scale: TimeScale) {
    match expr {
        Expr::SystemTime {
            unit: unit_time, ..
        } => {
            if *unit_time == TimeUnit::Bawaan {
                *unit_time = scale.unit;
            }
        }
        Expr::Ident { .. } | Expr::Number(_) | Expr::Sized { .. } => {}
        Expr::Binary { lhs, rhs, .. } => {
            ekspresi(lhs, scale);
            ekspresi(rhs, scale);
        }
        Expr::Unary { operand, .. } => ekspresi(operand, scale),
        Expr::Ternary {
            condition,
            when_true,
            when_false,
            ..
        } => {
            ekspresi(condition, scale);
            ekspresi(when_true, scale);
            ekspresi(when_false, scale);
        }
        Expr::Select { base, .. } => ekspresi(base, scale),
        Expr::IndexDynamic { base, index, .. } => {
            ekspresi(base, scale);
            ekspresi(index, scale);
        }
        Expr::Concat { items, .. } => {
            for item in items {
                ekspresi(item, scale);
            }
        }
        Expr::Replicate { count, value, .. } => {
            ekspresi(count, scale);
            ekspresi(value, scale);
        }
        Expr::FunctionCall { args, .. } => {
            for arg in args {
                ekspresi(arg, scale);
            }
        }
        Expr::Cast { operand, .. }
        | Expr::BuiltinCast { operand, .. }
        | Expr::SizeCast { operand, .. }
        | Expr::SignCast { operand, .. }
        | Expr::Bits { operand, .. } => ekspresi(operand, scale),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expression::Expr;
    use sv_lexer::span::Span;

    fn delay(unit: TimeUnit) -> CombinationalStatement {
        CombinationalStatement::Delay {
            amount: Expr::Number(1),
            unit,
            body: Box::new(CombinationalStatement::Return {
                value: None,
                span: Span::dummy(),
            }),
            span: Span::dummy(),
        }
    }

    fn unit_di_delay(stmt: &CombinationalStatement) -> TimeUnit {
        match stmt {
            CombinationalStatement::Delay { unit, .. } => *unit,
            other => panic!("bukan delay: {other:?}"),
        }
    }

    #[test]
    fn delay_tanpa_satuan_diisi_dari_timescale() {
        let mut modul = Module {
            name: "m".into(),
            span: Span::dummy(),
            params: Vec::new(),
            ports: Vec::new(),
            declarations: Vec::new(),
            statements: vec![Statement::Initial {
                body: vec![delay(TimeUnit::Bawaan)],
                span: Span::dummy(),
            }],
            instances: Vec::new(),
            routines: Vec::new(),
            typedefs: Vec::new(),
            generates: Vec::new(),
            genvars: Vec::new(),
            localparams: Vec::new(),
            time_scale: crate::time_scale::TimeScale::default(),
        };
        terapkan_module(&mut modul, TimeScale::parse("1us/1ps").unwrap());
        let Statement::Initial { body, .. } = &modul.statements[0] else {
            panic!("harus initial");
        };
        assert_eq!(unit_di_delay(&body[0]), TimeUnit::MicroSeconds);
    }

    #[test]
    fn delay_dengan_satuan_tidak_diubah() {
        let mut modul = Module {
            name: "m".into(),
            span: Span::dummy(),
            params: Vec::new(),
            ports: Vec::new(),
            declarations: Vec::new(),
            statements: vec![Statement::Initial {
                body: vec![delay(TimeUnit::PicoSeconds)],
                span: Span::dummy(),
            }],
            instances: Vec::new(),
            routines: Vec::new(),
            typedefs: Vec::new(),
            generates: Vec::new(),
            genvars: Vec::new(),
            localparams: Vec::new(),
            time_scale: crate::time_scale::TimeScale::default(),
        };
        terapkan_module(&mut modul, TimeScale::parse("1us/1ps").unwrap());
        let Statement::Initial { body, .. } = &modul.statements[0] else {
            panic!("harus initial");
        };
        assert_eq!(unit_di_delay(&body[0]), TimeUnit::PicoSeconds);
    }

    #[test]
    fn delay_di_dalam_blok_nested_ikut_berubah() {
        // `#delay` bersarang tiga tingkat harus ikut berubah.
        let dalam = CombinationalStatement::Block {
            body: vec![CombinationalStatement::IfElse {
                condition: Expr::Number(1),
                then_branch: vec![delay(TimeUnit::Bawaan)],
                else_branch: Some(vec![CombinationalStatement::Block {
                    body: vec![delay(TimeUnit::Bawaan)],
                    span: Span::dummy(),
                }]),
                span: Span::dummy(),
            }],
            span: Span::dummy(),
        };
        let mut modul = Module {
            name: "m".into(),
            span: Span::dummy(),
            params: Vec::new(),
            ports: Vec::new(),
            declarations: Vec::new(),
            statements: vec![Statement::AlwaysComb {
                body: vec![dalam],
                span: Span::dummy(),
            }],
            instances: Vec::new(),
            routines: Vec::new(),
            typedefs: Vec::new(),
            generates: Vec::new(),
            genvars: Vec::new(),
            localparams: Vec::new(),
            time_scale: crate::time_scale::TimeScale::default(),
        };
        terapkan_module(&mut modul, TimeScale::parse("1ms/1us").unwrap());
        let Statement::AlwaysComb { body, .. } = &modul.statements[0] else {
            panic!("harus always_comb");
        };
        let CombinationalStatement::Block { body, .. } = &body[0] else {
            panic!("harus block");
        };
        let CombinationalStatement::IfElse {
            then_branch,
            else_branch,
            ..
        } = &body[0]
        else {
            panic!("harus if");
        };
        assert_eq!(unit_di_delay(&then_branch[0]), TimeUnit::MilliSeconds);
        let Some(else_branch) = else_branch else {
            panic!("harus ada else");
        };
        let CombinationalStatement::Block { body, .. } = &else_branch[0] else {
            panic!("harus block");
        };
        assert_eq!(unit_di_delay(&body[0]), TimeUnit::MilliSeconds);
    }
}
