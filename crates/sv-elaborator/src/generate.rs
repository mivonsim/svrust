// Tanggung jawab: elaborasi region `generate` menjadi design flat (LRM §27).
use crate::error::ElaborateError;
use crate::lower_stmt::lower_module_statement;
use crate::param::ParamTable;
use crate::symbol::{Symbol, SymbolTable};
use sv_ast::expression::Expr as AstExpr;
use sv_ast::generate::{ForGenerate, GenerateItem, GenerateRegion};
use sv_ast::lvalue::Lvalue;
use sv_ast::module::Module;
use sv_ast::statement::Statement as AstStatement;
use sv_ir::datatype::DataType;
use sv_ir::scope::ScopePath;
use sv_ir::variable::{VarDecl, VarKind};
use sv_ir::Design;

/// Batas aman iterasi supaya loopgenerate rusak tidak menguras memori.
const MAKS_ITERASI: u64 = 1 << 20;

/// Nilai genvar yang aktif pada satu titik elaborasi.
///
/// Scope anak menutupi scope induk, jadi pencarian dilakukan dari belakang.
#[derive(Debug, Clone, Default)]
pub struct GenvarEnv {
    pasangan: Vec<(String, u64)>,
}

impl GenvarEnv {
    pub fn kosong() -> Self {
        Self::default()
    }

    /// Turunkan satu tingkat: `var = nilai` berlaku pada badan loop.
    pub fn anak(&self, var: &str, nilai: u64) -> Self {
        let mut hasil = self.clone();
        hasil.pasangan.push((var.to_string(), nilai));
        hasil
    }

    /// Nilai genvar, atau `None` bila nama itu bukan genvar aktif.
    pub fn cari(&self, nama: &str) -> Option<u64> {
        self.pasangan
            .iter()
            .rev()
            .find(|(n, _)| n == nama)
            .map(|(_, nilai)| *nilai)
    }
}

/// Konteks scope blok generate yang sedang dielaborasi.
///
/// LRM §27.4: tiap iterasi loop adalah blok hierarki tersendiri, sehingga
/// sinyal yang dideklarasikan di dalamnya tidak boleh bentrok antar iterasi.
/// `prefix` ditambahkan ke nama sinyal, sedangkan `lokal` berisi nama yang
/// berasal dari scope blok ini — hanya nama itu yang perlu di-prefix saat
/// dirujuk; sinyal dari modul induk harus tetap apa adanya.
///
/// `koneksi` hanya diisi saat mengelaborasi region milik modul anak, agar
/// port yang tersambung ke sinyal parent ikut dipetakan.
#[derive(Debug, Clone, Default)]
pub(crate) struct ScopeBlok<'a> {
    prefix: String,
    lokal: Vec<String>,
    /// Prefix instans; berlaku untuk nama yang bukan lokal dan bukan port.
    pub(crate) inst_prefix: &'a str,
    /// Peta port anak ke sinyal parent; `None` pada modul top.
    koneksi: Option<&'a crate::instance::KoneksiMap>,
}

impl<'a> ScopeBlok<'a> {
    /// Ekspresi hasil qualify untuk `nama`, atau `None` bila tak perlu diubah.
    pub(crate) fn qualify(&self, nama: &str, span: sv_lexer::span::Span) -> Option<AstExpr> {
        // Nama lokal blok generate selalu milik scope ini.
        if self.lokal.iter().any(|n| n == nama) {
            return Some(AstExpr::ident(format!("{}{}", self.prefix, nama), span));
        }
        // Port anak mengikuti sinyal parent; parameter di-inline jadi konstanta.
        let koneksi = self.koneksi?;
        koneksi.get(nama).map(|k| k.ekspresi(span))
    }

    /// Nama hasil qualify untuk target assignment.
    ///
    /// Parameter tidak pernah jadi target, jadi tidak punya nama pengganti.
    pub(crate) fn qualify_nama(&self, nama: &str) -> Option<String> {
        if self.lokal.iter().any(|n| n == nama) {
            return Some(format!("{}{}", self.prefix, nama));
        }
        let koneksi = self.koneksi?;
        match koneksi.get(nama)? {
            crate::instance::Target::Sinyal { signal, .. } => Some(signal.clone()),
            crate::instance::Target::Param { .. } => None,
        }
    }

    /// Nama akhir untuk identifier yang bukan bagian dari scope blok ini.
    pub(crate) fn nama_luar(&self, nama: &str, span: sv_lexer::span::Span) -> String {
        match self.qualify(nama, span) {
            Some(AstExpr::Ident { name: baru, .. }) => baru,
            // Parameter sudah jadi konstanta; nama aslinya tak lagi dipakai.
            Some(_) => nama.to_string(),
            None => format!("{}{}", self.inst_prefix, nama),
        }
    }

    /// Turunkan satu tingkat untuk iterasi loop generate berikutnya.
    fn anak(&self, label: &str, nilai: u64, lokal: Vec<String>) -> Self {
        Self {
            // Nilai genvar ikut masuk nama supaya tiap iterasi punya nama
            // sinyal yang benar-benar berbeda.
            prefix: format!("{}{}_{}_", self.prefix, label, nilai),
            lokal,
            inst_prefix: self.inst_prefix,
            koneksi: self.koneksi,
        }
    }

    /// Turunkan satu tingkat untuk cabang `if`/`case` yang berlabel.
    ///
    /// Cabang hanya satu yang dielaborasi (LRM §27.1), jadi tidak perlu nilai
    /// iterasi — tapi deklarasi di dalamnya tetap harus punya nama sendiri dan
    /// terdaftar sebagai lokal. Tanpa itu `assign tag = ...` di dalam
    /// `if (...) begin : g` cari `u0__tag` sementara deklarasinya terdaftar
    /// sebagai `tag`, dan hasilnya "undefined signal".
    fn cabang(&self, label: &str, lokal: Vec<String>) -> Self {
        Self {
            prefix: format!("{}{}_", self.prefix, label),
            lokal,
            inst_prefix: self.inst_prefix,
            koneksi: self.koneksi,
        }
    }
}

/// Kumpulkan nama yang dideklarasikan di dalam badan loop generate.
///
/// Termasuk deklarasi di dalam proses, karena satu blok iterasi hanya punya
/// satu prefix sehingga semuanya harus ikut terbawa.
fn nama_lokal(for_gen: &ForGenerate) -> Vec<String> {
    crate::generate_proses::kumpulkan_deklarasi(&for_gen.body)
        .into_iter()
        .map(|d| d.name)
        .collect()
}

/// Nama seluruh deklarasi di dalam satu list item generate.
fn nama_lokal_item(items: &[GenerateItem]) -> Vec<String> {
    crate::generate_proses::kumpulkan_deklarasi(items)
        .into_iter()
        .map(|d| d.name)
        .collect()
}

/// Label blok untuk cabang `if`/`case`.
///
/// LRM §27.4 memberi hierarki nama per blok berlabel; dipakai sebagai prefix
/// supaya deklarasi di dalam cabang tidak bentrok dengan信号的 di luar
/// cabang. Tanpa label, prefix kosong tapi `lokal` tetap mendaftarkan namanya
/// sehingga lookup tetap benar.
fn label_cabang(items: &[GenerateItem]) -> String {
    // Label disimpan di `ForGenerate`/blok berlabel; untuk cabang `if`/`case`
    // yang tidak berlabel, prefix kosong sudah cukup karena hanya satu cabang
    // yang dielaborasi.
    let _ = items;
    String::new()
}

/// Elaborate seluruh region generate milik satu module.
///
/// Harus dipanggil setelah deklarasi dan statement modul biasa selesai,
/// supaya sinyal yang dirujuk generate sudah terdaftar di tabel simbol.
pub fn lower_regions(
    design: &mut Design,
    symbols: &mut SymbolTable,
    module: &Module,
    params: &ParamTable,
    modules: &[Module],
) -> Result<(), ElaborateError> {
    // LRM §6.20: parameter modul adalah konstanta yang juga terlihat di dalam
    // `generate`. Tanpa peta ini `if (WIDTH > 8)` gagal dengan "undefined
    // signal 'WIDTH'".
    let parameter = crate::instance::peta_parameter(params);
    let scope = ScopeBlok {
        prefix: String::new(),
        lokal: Vec::new(),
        inst_prefix: "",
        koneksi: Some(&parameter),
    };
    for region in &module.generates {
        lower_region(
            design,
            symbols,
            region,
            params,
            modules,
            &GenvarEnv::kosong(),
            &scope,
        )?;
    }
    Ok(())
}

/// Sama seperti `lower_regions`, tetapi dengan prefix instans.
///
/// Dipanggil dari `lower_instance` supaya region generate milik modul anak
/// ikut terelaborasi; tanpa ini generate di anak akan diabaikan diam-diam.
pub(crate) fn lower_regions_dalam_instans<'a>(
    design: &mut Design,
    symbols: &mut SymbolTable,
    module: &Module,
    params: &ParamTable,
    modules: &[Module],
    env: &GenvarEnv,
    instan: &'a crate::instance::InstanInfo<'a>,
) -> Result<(), ElaborateError> {
    let scope = ScopeBlok {
        prefix: String::new(),
        lokal: Vec::new(),
        inst_prefix: instan.prefix,
        // Koneksi port anak DAN parameter modul anak keduanya perlu terlihat
        // di dalam generate anak, jadi keduanya dipakai.
        koneksi: Some(instan.koneksi),
    };
    for region in &module.generates {
        lower_region(design, symbols, region, params, modules, env, &scope)?;
    }
    Ok(())
}

/// Elaborate satu region generate pada konteks genvar dan scope tertentu.
fn lower_region(
    design: &mut Design,
    symbols: &mut SymbolTable,
    region: &GenerateRegion,
    params: &ParamTable,
    modules: &[Module],
    env: &GenvarEnv,
    scope: &ScopeBlok,
) -> Result<(), ElaborateError> {
    for item in &region.items {
        lower_item(design, symbols, item, params, modules, env, scope)?;
    }
    Ok(())
}

/// Elaborate satu item generate pada prefix hierarki tertentu.
fn lower_item(
    design: &mut Design,
    symbols: &mut SymbolTable,
    item: &GenerateItem,
    params: &ParamTable,
    modules: &[Module],
    env: &GenvarEnv,
    scope: &ScopeBlok,
) -> Result<(), ElaborateError> {
    match item {
        GenerateItem::For(for_gen) => {
            let lokal = nama_lokal(for_gen);
            for nilai in nilai_iterasi(for_gen)? {
                // LRM §27.4: tiap iterasi menjadi blok hierarki tersendiri,
                // jadi sinyal lokalnya harus berbeda antar iterasi.
                let env_anak = env.anak(&for_gen.var, nilai);
                let scope_anak = scope.anak(&for_gen.label, nilai, lokal.clone());
                for item_anak in &for_gen.body {
                    lower_item(
                        design,
                        symbols,
                        item_anak,
                        params,
                        modules,
                        &env_anak,
                        &scope_anak,
                    )?;
                }
            }
            Ok(())
        }
        GenerateItem::If(if_gen) => {
            // LRM §27.1: hanya cabang terpilih yang dielaborasi, sehingga
            // sinyal di cabang lain tidak muncul di design.
            let syarat = crate::konst::konst(&if_gen.condition, env, params, if_gen.span)?;
            if syarat != 0 {
                let label = label_cabang(&if_gen.then_branch);
                let lokal = nama_lokal_item(&if_gen.then_branch);
                let scope_anak = scope.cabang(&label, lokal);
                for item_anak in &if_gen.then_branch {
                    lower_item(
                        design,
                        symbols,
                        item_anak,
                        params,
                        modules,
                        env,
                        &scope_anak,
                    )?;
                }
            } else if let Some(else_branch) = &if_gen.else_branch {
                let label = label_cabang(else_branch);
                let lokal = nama_lokal_item(else_branch);
                let scope_anak = scope.cabang(&label, lokal);
                for item_anak in else_branch {
                    lower_item(
                        design,
                        symbols,
                        item_anak,
                        params,
                        modules,
                        env,
                        &scope_anak,
                    )?;
                }
            }
            Ok(())
        }
        GenerateItem::Case(case_gen) => {
            let nilai = crate::konst::konst(&case_gen.selector, env, params, case_gen.span)?;
            // LRM §27.1: lengan pertama yang cocok dipakai; `default` bertindak sebagai
            // jaring pengaman bila tidak ada yang cocok.
            let terpilih = case_gen
                .arms
                .iter()
                .find(|arm| {
                    if arm.is_default {
                        return false;
                    }
                    arm.labels.iter().any(|label| {
                        crate::konst::konst(label, env, params, arm.span).map(|v| v == nilai)
                            == Ok(true)
                    })
                })
                .or_else(|| case_gen.arms.iter().find(|arm| arm.is_default));
            let Some(arm) = terpilih else {
                return Err(ElaborateError::new(
                    "case-generate tidak punya lengan yang cocok",
                    case_gen.span,
                ));
            };
            let scope_anak = scope.cabang(&label_cabang(&arm.body), nama_lokal_item(&arm.body));
            for item_anak in &arm.body {
                lower_item(
                    design,
                    symbols,
                    item_anak,
                    params,
                    modules,
                    env,
                    &scope_anak,
                )?;
            }
            Ok(())
        }
        GenerateItem::Decl(decls) => {
            for decl in decls {
                let bit = params.lebar(&decl.width, decl.span)?;
                let tipe = DataType::logic(bit as u32).with_signed(decl.signed);
                let nama = format!("{}{}", scope.prefix, decl.name);
                let id = symbols.insert(Symbol {
                    name: nama.clone(),
                    signal_id: 0,
                    data_type: tipe,
                    kind: VarKind::Variable,
                    unpacked: None,
                    span: decl.span,
                })?;
                design.add_variable(VarDecl::new(
                    nama.clone(),
                    ScopePath::root().child(nama),
                    tipe,
                    VarKind::Variable,
                    id,
                ));
            }
            Ok(())
        }
        GenerateItem::Assign { lhs, rhs, span } => {
            let stmt = AstStatement::ContinuousAssign {
                lhs: qualify_lvalue(lhs, scope, env)?,
                rhs: subst_expr(rhs, env, scope),
                span: *span,
            };
            design
                .processes
                .push(lower_module_statement(&stmt, symbols)?);
            Ok(())
        }
        GenerateItem::Instance(inst) => {
            // Koneksi port di dalam generate boleh menunjuk sinyal lokal
            // blok iterasi, jadi namanya harus di-prefix seperti LHS lain.
            let mut inst = inst.clone();
            for conn in &mut inst.port_conns {
                // Sinyal lokal blok generate ikut dipetakan, lalu port
                // tersambung ke parent, lalu sisanya bawa prefix instans.
                match scope.qualify_nama(&conn.signal) {
                    Some(baru) => conn.signal = baru,
                    None => {
                        let Some(koneksi) = scope.koneksi else {
                            conn.signal = scope.nama_luar(&conn.signal, conn.span);
                            continue;
                        };
                        if matches!(
                            koneksi.get(&conn.signal),
                            Some(crate::instance::Target::Param { .. })
                        ) {
                            return Err(ElaborateError::new(
                                format!(
                                    "parameter '{}' tidak bisa jadi sinyal koneksi port",
                                    conn.signal
                                ),
                                conn.span,
                            ));
                        }
                        conn.signal = scope.nama_luar(&conn.signal, conn.span);
                    }
                }
            }
            crate::instance::lower_instance(design, symbols, &inst, modules, env)?;
            Ok(())
        }
        GenerateItem::Process(stmt) => {
            // LRM §27.1: genvar dan scope blok disubstitusi pada badan proses
            // agar tiap iterasi menghasilkan proses yang benar-benar berbeda.
            crate::generate_proses::daftarkan(design, symbols, stmt, scope, params)?;
            let sub = crate::generate_proses::subst_statement(stmt, env, scope);
            design
                .processes
                .push(lower_module_statement(&sub, symbols)?);
            Ok(())
        }
    }
}

/// Hitung daftar nilai genvar untuk satu loop generate.
fn nilai_iterasi(for_gen: &ForGenerate) -> Result<Vec<u64>, ElaborateError> {
    let mut out = Vec::new();
    let step = for_gen.step;
    if step == 0 {
        // Parser sudah menolak langkah nol; ini jaga-jaga bila AST
        // dibangun langsung tanpa lewat parser.
        return Err(ElaborateError::invalid_width(
            "langkah loop generate tidak boleh nol",
            for_gen.span,
        ));
    }
    let mut nilai = for_gen.init;
    while for_gen.compare.holds(nilai, for_gen.bound) {
        if out.len() as u64 >= MAKS_ITERASI {
            return Err(ElaborateError::invalid_width(
                "loop generate melebihi batas iterasi",
                for_gen.span,
            ));
        }
        out.push(nilai);
        nilai = if step > 0 {
            nilai.saturating_add(step as u64)
        } else {
            nilai.saturating_sub(step.unsigned_abs())
        };
    }
    Ok(out)
}

/// Ganti `Ident(nama)` genvar dengan konstantanya dan qualify nama scope lokal.
fn subst_expr(expr: &AstExpr, env: &GenvarEnv, scope: &ScopeBlok) -> AstExpr {
    match expr {
        AstExpr::Ident { name: nama, span } => {
            // Genvar lebih dulu: nilainya diketahui dan bukan sinyal.
            if let Some(nilai) = env.cari(nama) {
                return AstExpr::Number(nilai);
            }
            match scope.qualify(nama, *span) {
                Some(baru) => baru,
                None if scope.inst_prefix.is_empty() => expr.clone(),
                // Nama di luar scope blok tetap perlu prefix instans supaya
                // rujukan dari modul anak tidak tertukar dengan induknya.
                None => AstExpr::ident(scope.nama_luar(nama, *span), *span),
            }
        }
        // Panggilan function sudah ter-expand sebelum elaborasi, tapi nama
        // argumennya tetap perlu disubstitusi bila node ini lolos ke sini.
        AstExpr::FunctionCall { name, args, span } => AstExpr::FunctionCall {
            name: name.clone(),
            args: args.iter().map(|a| subst_expr(a, env, scope)).collect(),
            span: *span,
        },
        AstExpr::Binary { op, lhs, rhs, span } => AstExpr::Binary {
            op: *op,
            lhs: Box::new(subst_expr(lhs, env, scope)),
            rhs: Box::new(subst_expr(rhs, env, scope)),
            span: *span,
        },
        AstExpr::Unary { op, operand, span } => AstExpr::Unary {
            op: *op,
            operand: Box::new(subst_expr(operand, env, scope)),
            span: *span,
        },
        AstExpr::Ternary {
            condition,
            when_true,
            when_false,
            span,
        } => AstExpr::Ternary {
            condition: Box::new(subst_expr(condition, env, scope)),
            when_true: Box::new(subst_expr(when_true, env, scope)),
            when_false: Box::new(subst_expr(when_false, env, scope)),
            span: *span,
        },
        AstExpr::Select {
            base,
            msb,
            lsb,
            span,
        } => AstExpr::Select {
            base: Box::new(subst_expr(base, env, scope)),
            msb: *msb,
            lsb: *lsb,
            span: *span,
        },
        AstExpr::IndexDynamic { base, index, span } => AstExpr::IndexDynamic {
            base: Box::new(subst_expr(base, env, scope)),
            index: Box::new(subst_expr(index, env, scope)),
            span: *span,
        },
        AstExpr::Concat { items, span } => AstExpr::Concat {
            items: items.iter().map(|i| subst_expr(i, env, scope)).collect(),
            span: *span,
        },
        AstExpr::Replicate { count, value, span } => AstExpr::Replicate {
            count: Box::new(subst_expr(count, env, scope)),
            value: Box::new(subst_expr(value, env, scope)),
            span: *span,
        },
        AstExpr::Sized {
            value,
            width,
            signed,
            unknown_mask,
            zmask,
        } => AstExpr::Sized {
            value: *value,
            width: *width,
            signed: *signed,
            unknown_mask: *unknown_mask,
            zmask: *zmask,
        },
        AstExpr::Number(v) => AstExpr::Number(*v),
        AstExpr::SystemTime { unit, span } => AstExpr::SystemTime {
            unit: *unit,
            span: *span,
        },
        // LRM §6.14 + §8.20: nama tipe bukan sinyal lokal scope ini, jadi tidak
        // ikut prefix iterasi — tapi typedef module-scoped, jadi tetap perlu
        // prefix instans supaya tidak tertukar dengan typedef modul lain.
        // LRM §6.14: cast ke tipe bawaan dan size cast tidak punya nama tipe
        // yang perlu di-prefix, jadi hanya operandnya yang ikut substitusi.
        AstExpr::BuiltinCast {
            type_name,
            width,
            signed,
            operand,
            span,
        } => AstExpr::BuiltinCast {
            type_name: type_name.clone(),
            width: *width,
            signed: *signed,
            operand: Box::new(subst_expr(operand, env, scope)),
            span: *span,
        },
        AstExpr::SizeCast {
            width,
            operand,
            span,
        } => AstExpr::SizeCast {
            width: *width,
            operand: Box::new(subst_expr(operand, env, scope)),
            span: *span,
        },
        AstExpr::Cast {
            type_name,
            operand,
            span,
        } => AstExpr::Cast {
            type_name: format!("{}{}", scope.inst_prefix, type_name),
            operand: Box::new(subst_expr(operand, env, scope)),
            span: *span,
        },
        AstExpr::SignCast {
            signed,
            operand,
            span,
        } => AstExpr::SignCast {
            signed: *signed,
            operand: Box::new(subst_expr(operand, env, scope)),
            span: *span,
        },
        AstExpr::Bits { operand, span } => AstExpr::Bits {
            operand: Box::new(subst_expr(operand, env, scope)),
            span: *span,
        },
    }
}

/// Beri prefix scope pada LHS dan selesaikan indeks genvar.
///
/// Nilai genvar sudah diketahui saat loop generate dibuka, jadi `y[i]`
/// menjadi irisan bit konkret sebelum masuk IR (LRM §27.3).
fn qualify_lvalue(
    lhs: &Lvalue,
    scope: &ScopeBlok,
    env: &GenvarEnv,
) -> Result<Lvalue, ElaborateError> {
    let mut hasil = match scope.qualify_nama(&lhs.name) {
        Some(baru) => lhs.dengan_nama(baru),
        None if scope.inst_prefix.is_empty() => lhs.clone(),
        None => lhs.dengan_nama(scope.nama_luar(&lhs.name, lhs.span)),
    };
    if let Some(genvar) = hasil.genvar_index.clone() {
        let nilai = env.cari(&genvar).ok_or_else(|| {
            ElaborateError::new(
                format!("indeks genvar '{}' tak aktif pada blok ini", genvar),
                hasil.span,
            )
        })?;
        let bit = u32::try_from(nilai).map_err(|_| {
            ElaborateError::invalid_width(
                format!("nilai genvar '{}' melebihi lebar sinyal", genvar),
                hasil.span,
            )
        })?;
        hasil.resolve_genvar(bit);
    }
    Ok(hasil)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env_dalam() -> GenvarEnv {
        GenvarEnv::kosong().anak("i", 3)
    }

    fn scope_kosong() -> ScopeBlok<'static> {
        ScopeBlok {
            prefix: String::new(),
            lokal: Vec::new(),
            inst_prefix: "",
            koneksi: None,
        }
    }

    fn ident(nama: &str) -> AstExpr {
        AstExpr::ident(nama, sv_lexer::span::Span::dummy())
    }

    /// Scope uji untuk modul top: prefix iterasi tanpa prefix instans.
    fn scope_uji(prefix: &str, lokal: &[&str]) -> ScopeBlok<'static> {
        ScopeBlok {
            prefix: prefix.to_string(),
            lokal: lokal.iter().map(|s| s.to_string()).collect(),
            inst_prefix: "",
            koneksi: None,
        }
    }

    #[test]
    fn genvar_mengganti_ekspresi_ident() {
        let hasil = subst_expr(&ident("i"), &env_dalam(), &scope_kosong());
        assert_eq!(hasil, AstExpr::Number(3));
    }

    #[test]
    fn nama_bukan_genvar_tetap_dipertahankan() {
        let expr = ident("y");
        assert_eq!(subst_expr(&expr, &env_dalam(), &scope_kosong()), expr);
    }

    #[test]
    fn scope_anak_menutupi_scope_induk() {
        let env = GenvarEnv::kosong().anak("i", 1).anak("i", 9);
        assert_eq!(env.cari("i"), Some(9));
    }

    #[test]
    fn scope_induk_tetap_terlihat_dari_anak() {
        let env = GenvarEnv::kosong().anak("i", 4).anak("j", 7);
        assert_eq!(env.cari("i"), Some(4));
        assert_eq!(env.cari("j"), Some(7));
    }

    #[test]
    fn substitusi_masuk_ke_seluruh_cabang_ekspresi() {
        // `i + 1` pada kedua sisi harus terganti.
        let expr = AstExpr::Binary {
            op: sv_ast::expression::BinaryOp::Add,
            lhs: Box::new(ident("i")),
            rhs: Box::new(AstExpr::Number(1)),
            span: sv_lexer::span::Span::dummy(),
        };
        let hasil = subst_expr(&expr, &env_dalam(), &scope_kosong());
        match hasil {
            AstExpr::Binary { lhs, .. } => assert_eq!(*lhs, AstExpr::Number(3)),
            other => panic!("harus binary, dapat {:?}", other),
        }
    }

    #[test]
    fn substitusi_masuk_ke_konkatinasi_dan_replikasi() {
        let concat = AstExpr::Concat {
            items: vec![ident("i"), ident("k")],
            span: sv_lexer::span::Span::dummy(),
        };
        match subst_expr(&concat, &env_dalam(), &scope_kosong()) {
            AstExpr::Concat { items, .. } => {
                assert_eq!(items[0], AstExpr::Number(3));
                assert_eq!(items[1], ident("k"));
            }
            other => panic!("harus concat, dapat {:?}", other),
        }
    }

    // BUG: deklarasi di dalam loop generate tidak diberi prefix per iterasi,
    // sehingga iterasi kedua bentrok nama dengan iterasi pertama.
    #[test]
    fn nama_lokal_blok_generate_diprefix() {
        let scope = scope_uji("g_", &["t"]);
        assert_eq!(
            scope.qualify("t", sv_lexer::span::Span::dummy()),
            Some(ident("g_t"))
        );
        // Sinyal dari modul induk tidak boleh di-prefix.
        assert_eq!(scope.qualify("y", sv_lexer::span::Span::dummy()), None);
    }

    #[test]
    fn qualify_lvalue_hanya_menyentuh_nama_lokal() {
        let scope = scope_uji("g_", &["t"]);
        let lokal = Lvalue::simple("t", sv_lexer::span::Span::dummy());
        assert_eq!(
            qualify_lvalue(&lokal, &scope, &GenvarEnv::kosong())
                .unwrap()
                .name,
            "g_t"
        );

        let modul = Lvalue::simple("y", sv_lexer::span::Span::dummy());
        assert_eq!(
            qualify_lvalue(&modul, &scope, &GenvarEnv::kosong())
                .unwrap()
                .name,
            "y"
        );
    }

    // BUG: `y[i]` di dalam loop generate ditolak karena `Slice` hanya
    // menyimpan indeks literal.
    #[test]
    fn indeks_genvar_pada_lhs_jadi_irisan_konkret() {
        let scope = scope_kosong();
        let lhs = Lvalue::dengan_genvar("y", "i", sv_lexer::span::Span::dummy());
        let hasil = qualify_lvalue(&lhs, &scope, &env_dalam()).expect("resolve");
        assert_eq!(hasil.genvar_index, None);
        let slice = hasil.slice.expect("irisan harus terisi");
        assert_eq!(slice.msb, 3);
        assert_eq!(slice.lsb, 3);
    }

    #[test]
    fn indeks_genvar_tak_aktif_ditolak() {
        let scope = scope_kosong();
        let lhs = Lvalue::dengan_genvar("y", "k", sv_lexer::span::Span::dummy());
        let err = qualify_lvalue(&lhs, &scope, &env_dalam()).unwrap_err();
        assert!(err.to_string().contains("tak aktif"), "{}", err);
    }

    #[test]
    fn qualify_lvalue_menyelesaikan_genvar_dan_prefix_bersamaan() {
        let scope = scope_uji("g_", &["t"]);
        let lhs = Lvalue::dengan_genvar("t", "i", sv_lexer::span::Span::dummy());
        let hasil = qualify_lvalue(&lhs, &scope, &env_dalam()).expect("resolve");
        assert_eq!(hasil.name, "g_t");
        assert_eq!(hasil.slice.unwrap().msb, 3);
    }

    #[test]
    fn ekspresi_referensi_sinyal_lokal_diprefix() {
        let scope = scope_uji("g_", &["t"]);
        let hasil = subst_expr(&ident("t"), &env_dalam(), &scope);
        assert_eq!(hasil, ident("g_t"));
    }

    #[test]
    fn genvar_menang_atas_nama_lokal() {
        // Bila nama lokal sama dengan genvar, genvar yang menentukan.
        let scope = scope_uji("g_", &["i"]);
        let hasil = subst_expr(&ident("i"), &env_dalam(), &scope);
        assert_eq!(hasil, AstExpr::Number(3));
    }

    #[test]
    fn nama_lokal_mengumpulkan_seluruh_deklarasi_badan() {
        let mut decl = sv_ast::declaration::Declaration {
            name: "t".to_string(),
            width: sv_ast::width::WidthExpr::Literal(1),
            signed: false,
            unpacked: None,
            span: sv_lexer::span::Span::dummy(),
            init: None,
            type_name: None,
        };
        let loop_gen = ForGenerate {
            var: "i".to_string(),
            init: 0,
            bound: 2,
            compare: sv_ast::generate::CompareOp::Lt,
            step: 1,
            label: "g".to_string(),
            body: vec![GenerateItem::Decl(vec![decl.clone()])],
            span: sv_lexer::span::Span::dummy(),
        };
        assert_eq!(nama_lokal(&loop_gen), vec!["t".to_string()]);
        decl.name = "u".to_string();
        let _ = decl;
    }

    fn for_gen(
        init: u64,
        bound: u64,
        compare: sv_ast::generate::CompareOp,
        step: i64,
    ) -> ForGenerate {
        ForGenerate {
            var: "i".to_string(),
            init,
            bound,
            compare,
            step,
            label: "blk".to_string(),
            body: Vec::new(),
            span: sv_lexer::span::Span::dummy(),
        }
    }

    #[test]
    fn iterasi_naik_dihitung_benar() {
        let nilai = nilai_iterasi(&for_gen(0, 4, sv_ast::generate::CompareOp::Lt, 1)).unwrap();
        assert_eq!(nilai, vec![0, 1, 2, 3]);
    }

    #[test]
    fn iterasi_turun_dihitung_benar() {
        let nilai = nilai_iterasi(&for_gen(3, 0, sv_ast::generate::CompareOp::Gt, -1)).unwrap();
        assert_eq!(nilai, vec![3, 2, 1]);
    }

    #[test]
    fn langkah_lebih_dari_satu() {
        let nilai = nilai_iterasi(&for_gen(0, 10, sv_ast::generate::CompareOp::Lt, 3)).unwrap();
        assert_eq!(nilai, vec![0, 3, 6, 9]);
    }

    #[test]
    fn langkah_nol_ditolak() {
        let err = nilai_iterasi(&for_gen(0, 4, sv_ast::generate::CompareOp::Lt, 0)).unwrap_err();
        assert!(err.to_string().contains("tidak boleh nol"), "{}", err);
    }

    #[test]
    fn batas_iterasi_dijaga() {
        // `Lt` dengan langkah 1 dari 0 ke u64::MAX akan melewati batas aman.
        let err =
            nilai_iterasi(&for_gen(0, u64::MAX, sv_ast::generate::CompareOp::Lt, 1)).unwrap_err();
        assert!(
            err.to_string().contains("melebihi batas iterasi"),
            "{}",
            err
        );
    }
}
