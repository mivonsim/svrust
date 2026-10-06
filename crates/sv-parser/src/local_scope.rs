// Tanggung jawab: memberi nama unik pada deklarasi variabel lokal per scope blok.
use std::collections::HashMap;

use sv_ast::combinational::{CaseArm, CombinationalStatement};
use sv_ast::expression::Expr;
use sv_ast::lvalue::Lvalue;
use sv_ast::statement::{SequentialStatement, Statement};
use sv_ast::system_task::{SystemArg, SystemTask};

/// Konteks penamaan: penghitung unik dan tumpukan scope blok.
struct Ctx {
    /// Tiap deklarasi lokal mendapat nomor unik sendiri.
    counter: usize,
    /// Tumpukan scope; entri terakhir adalah scope paling dalam.
    scopes: Vec<HashMap<String, String>>,
}

impl Ctx {
    fn baru() -> Self {
        Self {
            counter: 0,
            scopes: vec![HashMap::new()],
        }
    }

    /// Daftarkan nama lokal baru dan kembalikan nama uniknya.
    fn daftarkan(&mut self, nama: &str) -> String {
        let unik = format!("{}__lokal{}", nama, self.counter);
        self.counter += 1;
        self.scopes
            .last_mut()
            .expect("selalu ada satu scope")
            .insert(nama.to_string(), unik.clone());
        unik
    }

    /// Cari nama yang aktif untuk `nama`, dari scope terdalam ke luar.
    fn cari(&self, nama: &str) -> Option<String> {
        self.scopes.iter().rev().find_map(|s| s.get(nama).cloned())
    }

    fn masuk(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn keluar(&mut self) {
        self.scopes.pop();
    }
}

/// Beri nama unik pada deklarasi lokal di seluruh module dan tulis ulang
/// seluruh rujukannya sesuai scope blok yang melingkupi.
///
/// Penghitung bersifat bersama untuk satu module, sehingga dua blok yang
/// sama-sama mendeklarasikan `t` tetap mendapat nama berbeda. Scope direset
/// per blok karena setiap blok prosedural adalah scope akar tersendiri.
pub fn unik_nama_modul(statements: &mut [Statement]) {
    let mut ctx = Ctx::baru();
    for statement in statements {
        ctx.scopes = vec![HashMap::new()];
        match statement {
            Statement::AlwaysTimed { body, .. }
            | Statement::AlwaysComb { body, .. }
            | Statement::Initial { body, .. } => {
                for stmt in body {
                    jalan_comb(stmt, &mut ctx);
                }
            }
            Statement::AlwaysFf { body, .. } => {
                for stmt in body {
                    jalan_seq(stmt, &mut ctx);
                }
            }
            Statement::ContinuousAssign { .. } => {}
        }
    }
}

fn jalan_comb(stmt: &mut CombinationalStatement, ctx: &mut Ctx) {
    match stmt {
        CombinationalStatement::Decl(decl) => decl.name = ctx.daftarkan(&decl.name),
        CombinationalStatement::BlockingAssign { lhs, rhs, .. }
        | CombinationalStatement::NonBlockingAssign { lhs, rhs, .. }
        | CombinationalStatement::CompoundAssign { lhs, rhs, .. } => {
            nama_lvalue(lhs, ctx);
            nama_expr(rhs, ctx);
        }
        CombinationalStatement::Block { body, .. } => {
            // Blok baru membuka scope sendiri.
            ctx.masuk();
            for s in body {
                jalan_comb(s, ctx);
            }
            ctx.keluar();
        }
        CombinationalStatement::IfElse {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            nama_expr(condition, ctx);
            for branch in [Some(then_branch), else_branch.as_mut()]
                .into_iter()
                .flatten()
            {
                ctx.masuk();
                for s in branch {
                    jalan_comb(s, ctx);
                }
                ctx.keluar();
            }
        }
        CombinationalStatement::Repeat { count, body, .. } => {
            nama_expr(count, ctx);
            ctx.masuk();
            for s in body {
                jalan_comb(s, ctx);
            }
            ctx.keluar();
        }
        // Sinyal event control merujuk sinyal yang sudah ada, jadi hanya perlu
        // mencari nama unik bila terdeklarasi sebagai variabel lokal.
        CombinationalStatement::EventControl { events, body, .. } => {
            for item in events {
                if let Some(unik) = ctx.cari(&item.signal) {
                    item.signal = unik;
                }
            }
            ctx.masuk();
            for s in body {
                jalan_comb(s, ctx);
            }
            ctx.keluar();
        }
        CombinationalStatement::While {
            condition, body, ..
        } => {
            nama_expr(condition, ctx);
            ctx.masuk();
            for s in body {
                jalan_comb(s, ctx);
            }
            ctx.keluar();
        }
        CombinationalStatement::For {
            init,
            condition,
            step,
            body,
            ..
        } => {
            nama_lvalue(&mut init.lhs, ctx);
            nama_expr(&mut init.rhs, ctx);
            nama_expr(condition, ctx);
            nama_lvalue(&mut step.lhs, ctx);
            nama_expr(&mut step.rhs, ctx);
            ctx.masuk();
            for s in body {
                jalan_comb(s, ctx);
            }
            ctx.keluar();
        }
        CombinationalStatement::Case { selector, arms, .. } => {
            nama_expr(selector, ctx);
            for CaseArm { labels, body, .. } in arms {
                for label in labels {
                    nama_expr(label, ctx);
                }
                ctx.masuk();
                for s in body {
                    jalan_comb(s, ctx);
                }
                ctx.keluar();
            }
        }
        // Argumen nilai pada system task juga bisa merujuk variabel lokal.
        CombinationalStatement::SystemTask(task) => nama_task(task, ctx),
        // `#n <stmt>` punya dua ekspresi yang bisa merujuk variabel lokal.
        CombinationalStatement::Delay { amount, body, .. } => {
            nama_expr(amount, ctx);
            jalan_comb(body, ctx);
        }
        // Argumen panggilan task bisa merujuk variabel lokal.
        CombinationalStatement::TaskCall(call) => {
            for arg in &mut call.args {
                nama_expr(arg, ctx);
            }
        }
        // LRM §13.4: nilai yang dibawa `return expr;`.
        CombinationalStatement::Return { value, .. } => {
            if let Some(expr) = value.as_mut() {
                nama_expr(expr, ctx);
            }
        }
    }
}

/// Tulis ulang nama pada argumen system task yang berupa nilai.
fn nama_task(task: &mut SystemTask, ctx: &mut Ctx) {
    for arg in &mut task.args {
        if let SystemArg::Value(expr) = arg {
            nama_expr(expr, ctx);
        }
    }
    if let Some(cond) = task.condition.as_mut() {
        nama_expr(cond, ctx);
    }
}

fn jalan_seq(stmt: &mut SequentialStatement, ctx: &mut Ctx) {
    match stmt {
        SequentialStatement::Decl(decl) => decl.name = ctx.daftarkan(&decl.name),
        SequentialStatement::NonBlockingAssign { lhs, rhs, .. }
        | SequentialStatement::BlockingAssign { lhs, rhs, .. } => {
            nama_lvalue(lhs, ctx);
            nama_expr(rhs, ctx);
        }
        // LRM §12.4: tiap cabang `if`/`else` adalah scope blok tersendiri,
        // sama seperti cabang pada `always_comb`.
        SequentialStatement::IfElse {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            nama_expr(condition, ctx);
            for branch in [Some(then_branch), else_branch.as_mut()]
                .into_iter()
                .flatten()
            {
                ctx.masuk();
                for s in branch {
                    jalan_seq(s, ctx);
                }
                ctx.keluar();
            }
        }
        SequentialStatement::SystemTask(task) => nama_task(task, ctx),
        // Argumen panggilan task bisa merujuk variabel lokal.
        SequentialStatement::TaskCall(call) => {
            for arg in &mut call.args {
                nama_expr(arg, ctx);
            }
        }
        SequentialStatement::Return { value, .. } => {
            if let Some(expr) = value.as_mut() {
                nama_expr(expr, ctx);
            }
        }
    }
}

/// Tulis ulang nama pada target assignment bila ia lokal.
fn nama_lvalue(lhs: &mut Lvalue, ctx: &Ctx) {
    if let Some(unik) = ctx.cari(&lhs.name) {
        lhs.name = unik;
    }
}

/// Tulis ulang nama pada seluruh ekspresi beserta sub-ekspresinya.
fn nama_expr(expr: &mut Expr, ctx: &Ctx) {
    match expr {
        Expr::Ident { name, .. } => {
            if let Some(unik) = ctx.cari(name) {
                *name = unik;
            }
        }
        Expr::Binary { lhs, rhs, .. } => {
            nama_expr(lhs, ctx);
            nama_expr(rhs, ctx);
        }
        Expr::Unary { operand, .. } => nama_expr(operand, ctx),
        Expr::Select { base, .. } => nama_expr(base, ctx),
        Expr::Ternary {
            condition,
            when_true,
            when_false,
            ..
        } => {
            nama_expr(condition, ctx);
            nama_expr(when_true, ctx);
            nama_expr(when_false, ctx);
        }
        Expr::Concat { items, .. } => {
            for item in items {
                nama_expr(item, ctx);
            }
        }
        // `count` ikut ditulis ulang: kalau tidak, nama variabel lokal di dalam
        // count replikasi bocor ke ekspresi generate tanpa pernah didaftarkan.
        Expr::Replicate { value, count, .. } => {
            nama_expr(count, ctx);
            nama_expr(value, ctx);
        }
        // Argumen pemanggilan function bisa merujuk variabel lokal.
        Expr::FunctionCall { args, .. } => {
            for arg in args {
                nama_expr(arg, ctx);
            }
        }
        // Operand cast dan `$bits` bisa merujuk variabel lokal.
        Expr::Cast { operand, .. }
        | Expr::SignCast { operand, .. }
        | Expr::Bits { operand, .. }
        | Expr::BuiltinCast { operand, .. }
        | Expr::SizeCast { operand, .. } => nama_expr(operand, ctx),
        // Indeks dinamis juga bisa merujuk variabel lokal.
        Expr::IndexDynamic { base, index, .. } => {
            nama_expr(base, ctx);
            nama_expr(index, ctx);
        }
        Expr::Number(_) | Expr::Sized { .. } | Expr::SystemTime { .. } => {}
    }
}
