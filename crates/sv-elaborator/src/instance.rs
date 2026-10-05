// Tanggung jawab: lowering instansiasi module menjadi sinyal flat.
use crate::error::ElaborateError;
use crate::generate::GenvarEnv;
use crate::lower_stmt::lower_module_statement;
use crate::param::ParamTable;
use crate::symbol::{Symbol, SymbolTable};
use std::collections::HashMap;
use sv_ast::combinational::CombinationalStatement;
use sv_ast::expression::Expr as AstExpr;
use sv_ast::instance::Instance;
use sv_ast::lvalue::{Lvalue, Slice};
use sv_ast::module::Module;
use sv_ast::statement::{SequentialStatement, Statement as AstStatement};
use sv_ast::width::ParamDecl;
use sv_ir::datatype::DataType;
use sv_ir::scope::ScopePath;
use sv_ir::variable::{VarDecl, VarKind};
use sv_ir::Design;
use sv_lexer::span::Span;

/// Tujuan sebuah nama yang muncul di badan modul anak.
///
/// Port pemetaan sinyal sudah selesai. Parameter ikut dipetakan karena
/// parameter bukan sinyal: namanya harus tetap seperti tertulis, bukan
/// diberi prefix instans (LRM §23.10).
#[derive(Debug, Clone)]
pub(crate) enum Target {
    /// Sinyal parent dan bagiannya yang dipakai.
    ///
    /// `slice` hanya `None` bila seluruh sinyal dipakai. Indeks genvar sudah
    /// dipecah lebih dulu karena nilainya baru pasti di dalam loop generate.
    Sinyal {
        signal: String,
        slice: Option<Slice>,
    },
    /// Parameter modul anak. Nilai sudah diketahui saat instansiasi, jadi
    /// ekspresi yang memakainya disubstitusi menjadi konstanta.
    Param { nilai: u64 },
}

impl Target {
    /// Ekspresi hasil pemetaan untuk sebuah nama.
    ///
    /// Parameter menjadi konstanta 32-bit signed, mengikuti lebar bawaan
    /// parameter tak bertipe di SystemVerilog.
    pub(crate) fn ekspresi(&self, asal: Span) -> AstExpr {
        match self {
            Target::Sinyal { signal, .. } => AstExpr::ident(signal.clone(), asal),
            Target::Param { nilai } => AstExpr::Sized {
                value: *nilai,
                width: 32,
                signed: true,
                unknown_mask: 0,
                zmask: 0,
            },
        }
    }

    /// Nama hasil pemetaan; parameter tetap memakai nama aslinya.
    fn nama_pada(&self, nama: &str) -> String {
        match self {
            Target::Sinyal { signal, .. } => signal.clone(),
            Target::Param { .. } => nama.to_string(),
        }
    }
}

/// Peta nama modul anak ke targetnya; dipakai juga oleh elaborator generate.
pub(crate) type KoneksiMap = HashMap<String, Target>;

/// Konteks instans untuk elaborator generate.
///
/// Dipaketkan menjadi satu tipe agar pemanggilan tidak membawa banyak
/// parameter terpisah.
pub(crate) struct InstanInfo<'a> {
    /// Prefix nama sinyal internal anak, misal `u0__`.
    pub(crate) prefix: &'a str,
    /// Peta port anak ke sinyal parent.
    pub(crate) koneksi: &'a KoneksiMap,
}

/// Ubah indeks koneksi port menjadi rentang bit yang konkret.
///
/// Indeks genvar disubstitusi dengan nilai iterasi yang sudah diketahui;
/// bila nama itu bukan genvar aktif, ini error — bukan fallback diam-diam
/// ke koneksi seluruh sinyal yang akan salah secara semantik.
fn resolve_index(
    conn: &sv_ast::instance::PortConn,
    env: &GenvarEnv,
) -> Result<Option<Slice>, ElaborateError> {
    use sv_ast::instance::PortIndex;
    let Some(index) = conn.index.clone() else {
        return Ok(None);
    };
    let slice = match index {
        PortIndex::Bit(bit) => Slice { msb: bit, lsb: bit },
        PortIndex::Range(msb, lsb) => Slice { msb, lsb },
        PortIndex::Genvar(name) => {
            let Some(nilai) = env.cari(&name) else {
                return Err(ElaborateError::new(
                    format!("indeks genvar '{}' tak dikenal saat instansiasi", name),
                    conn.span,
                ));
            };
            let bit = u32::try_from(nilai).map_err(|_| {
                ElaborateError::invalid_width(
                    format!("nilai genvar '{}' melebihi lebar sinyal", name),
                    conn.span,
                )
            })?;
            Slice { msb: bit, lsb: bit }
        }
    };
    Ok(Some(slice))
}

/// Lower satu instansi: sinyal internal di-prefix, port jadi alias parent.
///
/// `env` menyimpan nilai genvar yang aktif; koneksi berindeks genvar seperti
/// `.y(q[i])` memakai nilai tersebut.
pub fn lower_instance(
    design: &mut Design,
    symbols: &mut SymbolTable,
    inst: &Instance,
    modules: &[Module],
    env: &GenvarEnv,
) -> Result<(), ElaborateError> {
    // Rantai instansiasi baru dimulai dari instans ini.
    let mut rantai = RantaiInstan::baru();
    lower_instance_dalam(design, symbols, inst, modules, env, &mut rantai)
}

/// Batas kedalaman instansiasi.
///
/// Tanpa batas, hierarki yang bersiklus (`a` menginstans `b`, `b` menginstans
/// `a`) menyebabkan `lower_instance` memanggil dirinya tanpa henti sampai
/// stack habis — `stack-overflow` yang tidak bisa ditangkap dan tidak memberi
/// pesan apa pun. Instansiasi siklus itu memang tidak sah (LRM §23.1.1: tak
/// ada rekursi tak terbatas pada hierarki), jadi harus ditolak dengan pesan.
pub const KEDALAM_INSTANS_MAKS: usize = 64;

/// Rantai modul yang sedang diturunkan, dipakai untuk mendeteksi siklus.
pub struct RantaiInstan {
    rantai: Vec<String>,
}

impl RantaiInstan {
    pub fn baru() -> Self {
        Self { rantai: Vec::new() }
    }
}

/// Naikkan rantai; `Err` kalau jadi siklus atau terlalu dalam.
fn masuk_rantai(
    rantai: &mut RantaiInstan,
    nama: &str,
    span: sv_lexer::span::Span,
) -> Result<(), ElaborateError> {
    if rantai.rantai.iter().any(|m| m == nama) {
        return Err(ElaborateError::new(
            format!(
                "hierarki instansiasi bersiklus: {} muncul lagi ({}); \
                 LRM §23.1.1 melarang rekursi tak terbatas",
                nama,
                rantai.rantai.join(" -> ")
            ),
            span,
        ));
    }
    if rantai.rantai.len() >= KEDALAM_INSTANS_MAKS {
        return Err(ElaborateError::new(
            format!(
                "kedalaman instansiasi melebihi {} tingkat; \
                 LRM §23.1.1 tidak membatasi kedalaman, tapi \
                 implementasi ini berhenti di batas itu",
                KEDALAM_INSTANS_MAKS
            ),
            span,
        ));
    }
    rantai.rantai.push(nama.to_string());
    Ok(())
}

fn keluar_rantai(rantai: &mut RantaiInstan) {
    rantai.rantai.pop();
}

#[allow(clippy::too_many_arguments)]
fn lower_instance_dalam(
    design: &mut Design,
    symbols: &mut SymbolTable,
    inst: &Instance,
    modules: &[Module],
    env: &GenvarEnv,
    rantai: &mut RantaiInstan,
) -> Result<(), ElaborateError> {
    let child = modules
        .iter()
        .find(|m| m.name == inst.module_name)
        .ok_or_else(|| {
            ElaborateError::new(
                format!("module tak dikenal: {}", inst.module_name),
                inst.span,
            )
        })?;
    masuk_rantai(rantai, &child.name, inst.span)?;
    let hasil = lower_instance_body(design, symbols, inst, child, modules, env, rantai);
    keluar_rantai(rantai);
    hasil
}

#[allow(clippy::too_many_arguments)]
fn lower_instance_body(
    design: &mut Design,
    symbols: &mut SymbolTable,
    inst: &Instance,
    child: &Module,
    modules: &[Module],
    env: &GenvarEnv,
    rantai: &mut RantaiInstan,
) -> Result<(), ElaborateError> {
    // Parameter efektif: default diganti override.
    let mut efektif: Vec<ParamDecl> = Vec::new();
    for p in &child.params {
        let nilai = inst
            .param_overrides
            .iter()
            .find(|o| o.name == p.name)
            .map(|o| o.value)
            .unwrap_or(p.default);
        efektif.push(ParamDecl {
            name: p.name.clone(),
            default: nilai,
            span: p.span,
        });
    }
    for o in &inst.param_overrides {
        if !child.params.iter().any(|p| p.name == o.name) {
            return Err(ElaborateError::undefined_param(&o.name, inst.span));
        }
    }
    // Parameter efektif anak (dengan override) lebih dulu, lalu `localparam`
    // anak. `localparam` tidak ikut di-override — `#(.NAMA(...))` untuknya
    // tetap error seperti padaSV aslinya.
    let mut params = ParamTable::baru(&efektif)?;
    for local in &child.localparams {
        let nilai = crate::konst::konst(
            &local.value,
            &crate::generate::GenvarEnv::kosong(),
            &params,
            local.span,
        )?;
        params.tambah(&local.name, nilai, local.span)?;
    }

    // Petakan port anak ke sinyal parent; cek lebar sama.
    let mut koneksi: KoneksiMap = HashMap::new();
    // Parameter anak di-inline nilainya saat instansiasi; mendaftarkannya
    // mencegah `lingkup_expr` memberi prefix instans dan menggantinya dengan
    // konstanta, bukan mencari sinyal bernama `u0__MODE`.
    for (p, nilai) in child.params.iter().zip(efektif.iter()) {
        koneksi.insert(
            p.name.clone(),
            Target::Param {
                nilai: nilai.default,
            },
        );
    }
    // `localparam` anak juga jadi konstanta saat instansiasi. Nilai diambil
    // dari tabel yang sudah dihitung dengan parameter EFEKTIF anak, jadi
    // `localparam MASK = (1 << W) - 1;` memakai nilai `W` hasil override dan
    // bukan default modul.
    for local in &child.localparams {
        let nilai = params.nilai(&local.name, local.span).unwrap_or(0);
        koneksi.insert(local.name.clone(), Target::Param { nilai });
    }
    for c in &inst.port_conns {
        let port_decl = child
            .ports
            .iter()
            .find(|p| p.name == c.port)
            .ok_or_else(|| {
                ElaborateError::new(
                    format!("port tak dikenal: {}.{}", child.name, c.port),
                    inst.span,
                )
            })?;
        let induk = symbols
            .lookup(&c.signal)
            .ok_or_else(|| ElaborateError::undefined_signal(&c.signal, c.span))?;
        let lebar_anak = params.lebar(&port_decl.width, inst.span)? as u32;
        let slice = resolve_index(c, env)?;
        let lebar_koneksi = match &slice {
            Some(s) => s.width(),
            None => induk.data_type.width,
        };
        if lebar_koneksi != lebar_anak {
            return Err(ElaborateError::invalid_width(
                format!(
                    "lebar koneksi beda: {}.{} {} bit vs {}{} {} bit",
                    child.name,
                    c.port,
                    lebar_anak,
                    c.signal,
                    slice_teks(slice.as_ref()),
                    lebar_koneksi
                ),
                c.span,
            ));
        }
        // Part-select di luar rentang sinyal parent adalah error, bukan
        // pemotongan diam-diam.
        if let Some(s) = &slice {
            if s.msb >= induk.data_type.width {
                return Err(ElaborateError::invalid_width(
                    format!(
                        "indeks {} di luar sinyal {} yang lebarnya {} bit",
                        slice_teks(Some(s)),
                        c.signal,
                        induk.data_type.width
                    ),
                    c.span,
                ));
            }
        }
        koneksi.insert(
            c.port.clone(),
            Target::Sinyal {
                signal: c.signal.clone(),
                slice,
            },
        );
    }
    for p in &child.ports {
        if !koneksi.contains_key(&p.name) {
            return Err(ElaborateError::new(
                format!("port tak tersambung: {}.{}", child.name, p.name),
                inst.span,
            ));
        }
    }

    // LRM §8.20: deklarasi boleh Qualified dengan typedef (`e_t q;`), jadi
    // lebar dan signedness harus ikut typedef anak — bukan hanya dimension
    // tertulis. Tanpa ini `e_t q;` dengan `typedef enum logic [1:0] e_t`
    // terdaftar selebar 1 bit dan nilainya terpotong.
    let tipe_anak = crate::typedef::TypeTable::baru(&child.typedefs, &params)?;

    // Daftarkan sinyal internal anak dengan prefix instansi.
    let prefix = format!("{}__", inst.inst_name);
    for d in &child.declarations {
        let (bit, signed) = crate::typedef::lebar_deklarasi(
            &d.width,
            d.type_name.as_deref(),
            &tipe_anak,
            &params,
            d.span,
        )?;
        // LRM §7.8: array unpacked di modul anak juga harus jadi sinyal rata
        // selebar `elemen * size` DAN membawa dimensinya, supaya indeksnya
        // dipetakan ke bit yang benar di badan anak. Tanpa ini deklarasinya
        // tetap jalan tapi hanya 8 bit dan `mem[2]` salah target.
        let tipe = match d.unpacked {
            None => DataType::logic(bit).with_signed(signed || d.signed),
            Some(dim) => {
                let total = (dim.size as u32).checked_mul(bit).ok_or_else(|| {
                    ElaborateError::invalid_width(
                        format!("lebar array unpacked {} x {} bit meluap", dim.size, bit),
                        d.span,
                    )
                })?;
                DataType::logic(total).with_signed(signed || d.signed)
            }
        };
        let nama = format!("{}{}", prefix, d.name);
        let mut symbol = Symbol {
            name: nama.clone(),
            signal_id: 0,
            data_type: tipe,
            kind: VarKind::Variable,
            unpacked: None,
            span: d.span,
        };
        if let Some(dim) = d.unpacked {
            symbol = symbol.dengan_unpacked(dim, bit);
        }
        let id = symbols.insert(symbol)?;
        design.add_variable(VarDecl::new(
            nama.clone(),
            ScopePath::root().child(nama),
            tipe,
            VarKind::Variable,
            id,
        ));
    }

    // Variabel lokal di dalam blok anak juga harus terdaftar, memakai
    // prefix yang sama karena rujukannya juga sudah di-prefix.
    for d in crate::local_decl::kumpulkan(child) {
        // LRM §7.8: array unpacked di modul anak juga harus jadi sinyal rata
        // selebar `elemen * size` DAN membawa dimensinya, supaya indeksnya
        // dipetakan ke bit yang benar di badan anak. Tanpa ini deklarasinya
        // tetap jalan tapi hanya 8 bit dan `mem[2]` salah target.
        let bit = params.lebar(&d.width, inst.span)?;
        let tipe = match d.unpacked {
            None => DataType::logic(bit as u32).with_signed(d.signed),
            Some(dim) => {
                let total = (dim.size as u32).checked_mul(bit as u32).ok_or_else(|| {
                    ElaborateError::invalid_width(
                        format!("lebar array unpacked {} x {} bit meluap", dim.size, bit),
                        d.span,
                    )
                })?;
                DataType::logic(total).with_signed(d.signed)
            }
        };
        let nama = format!("{}{}", prefix, d.name);
        let mut symbol = Symbol {
            name: nama.clone(),
            signal_id: 0,
            data_type: tipe,
            kind: VarKind::Variable,
            unpacked: None,
            span: d.span,
        };
        if let Some(dim) = d.unpacked {
            symbol = symbol.dengan_unpacked(dim, bit as u32);
        }
        let id = symbols.insert(symbol)?;
        design.add_variable(VarDecl::new(
            nama.clone(),
            ScopePath::root().child(nama),
            tipe,
            VarKind::Variable,
            id,
        ));
    }

    // LRM §8.20: typedef anak module-scoped, jadi harus didaftarkan dengan
    // prefix instans yang sama seperti sinyal internal anak. Tanpa ini cast
    // di badan anak selalu "undefined type", dan tanpa prefix dua instansi
    // dengan nama typedef sama akan saling menimpa.
    crate::design::daftarkan_tipe(symbols, child, &params, &prefix)?;

    // Qualify lalu lower statement anak ke dalam design top. `koneksi` sudah
    // memuat `localparam` anak karena `params` di atas sudah memuatnya, jadi
    // `localparam` ikut ter-substitusi seperti `parameter`.
    for s in &child.statements {
        design.processes.push(lower_module_statement(
            &lingkup_statement(s, &prefix, &koneksi),
            symbols,
        )?);
    }

    // Region generate milik anak ikut terelaborasi dengan parameter anak dan
    // prefix instans, supaya sinyalnya tidak bentrok antar instansi.
    // Instansi milik anak ikut menurunkan hierarki berikutnya. Tanpa ini
    // modul tingkat ketiga dan seterusnya hilang tanpa error.
    for inst_anak in &child.instances {
        let mut turunan = inst_anak.clone();
        turunan.inst_name = format!("{}{}", prefix, inst_anak.inst_name);
        for conn in &mut turunan.port_conns {
            conn.signal = petakan_nama(&conn.signal, &prefix, &koneksi);
            // Indeks genvar sudah diselesaikan oleh elaborator generate
            // ketika memproses region anak, sehingga di sini selalu konkret.
        }
        lower_instance_dalam(design, symbols, &turunan, modules, env, rantai)?;
    }

    crate::generate::lower_regions_dalam_instans(
        design,
        symbols,
        child,
        &params,
        modules,
        env,
        &crate::instance::InstanInfo {
            prefix: &prefix,
            koneksi: &koneksi,
        },
    )?;
    Ok(())
}

/// Teks irisan untuk pesan error, misal `[3]` atau `` (seluruh sinyal).
fn slice_teks(slice: Option<&Slice>) -> String {
    match slice {
        None => String::new(),
        Some(s) if s.msb == s.lsb => format!("[{}]", s.msb),
        Some(s) => format!("[{}:{}]", s.msb, s.lsb),
    }
}

/// Ganti nama: port ikut sinyal parent, sisanya di-prefix.
///
/// Diekspor agar elaborator generate bisa memetakan nama port yang sama
/// saat me-lower isi region generate milik modul anak.
pub(crate) fn petakan_nama(n: &str, prefix: &str, koneksi: &KoneksiMap) -> String {
    match koneksi.get(n) {
        Some(k) => k.nama_pada(n),
        None => format!("{}{}", prefix, n),
    }
}

/// Ganti nama: port ikut sinyal parent, sisanya di-prefix.
fn nama(n: &str, prefix: &str, koneksi: &KoneksiMap) -> String {
    petakan_nama(n, prefix, koneksi)
}

/// Sama seperti `nama`, tapi mempertahankan irisan bit pada LHS.
///
/// Port yang tersambung ke bagian sinyal lain menghasilkan LHS teriris,
/// sehingga penulisan di anak mengenai bagian yang tepat.
fn lingkup_lvalue(l: &Lvalue, prefix: &str, koneksi: &KoneksiMap) -> Lvalue {
    // Port anak yang tersambung lewat irisan: irisan koneksi menggantikan
    // irisan yang tertulis di sisi kiri statement anak.
    match koneksi.get(&l.name) {
        Some(Target::Sinyal { signal, slice }) => {
            return match slice {
                Some(s) => Lvalue::sliced(signal.clone(), *s, l.span),
                None => Lvalue::simple(signal.clone(), l.span),
            };
        }
        // Parameter tidak pernah jadi target assignment, tapi nama tetap
        // dibiarkan apa adanya agar konsisten dengan `petakan_nama`.
        Some(Target::Param { .. }) => return l.clone(),
        None => {}
    }
    let name = format!("{}{}", prefix, l.name);
    // `genvar_index` harus ikut dibawa. LHS dengan indeks genvar
    // (`assign y[i] = ...`) masih belum punya irisan konkret saat AST formed, dan
    // `lower_stmt::lookup_lvalue` menolaknya dengan pesan "hanya boleh dipakai di
    // dalam region generate" — jadi mappingsan di sini akan membatalkan deteksi
    // tersebut tanpa sengaja.
    let mut hasil = match l.slice {
        None => Lvalue::simple(name, l.span),
        Some(slice) => Lvalue::sliced(name, slice, l.span),
    };
    hasil.genvar_index = l.genvar_index.clone();
    hasil
}

/// Beri prefix instans pada ekspresi.
///
/// `asal` adalah lokasi sumber untuk node yang **disintesis** (mis. `Select`
/// dari koneksi berindeks). Identifier yang sudah ada di source memakai span
/// tokennya sendiri supaya error tetap menunjuk kolom yang benar.
fn lingkup_expr(e: &AstExpr, prefix: &str, koneksi: &KoneksiMap, _asal: Span) -> AstExpr {
    match e {
        // Span milik token identifier itu sendiri yang dipakai, bukan `asal`.
        // `asal` adalah span statement/ekspresi induk, jadi memakainya
        // menggeser kolom error: `q = a + din` akan melaporkan `a` di kolom
        // `q` dan bukan kolom `a` (AGENTS.md aturan 3).
        AstExpr::Ident { name: n, span } => {
            let sp = *span;
            match koneksi.get(n) {
                // Port anak yang tersambung ke bagian sinyal lain harus dibaca
                // sebagai bit/part-select, bukan seluruh sinyal.
                Some(Target::Sinyal { signal, slice }) => match slice {
                    Some(s) => AstExpr::Select {
                        base: Box::new(AstExpr::ident(signal.clone(), sp)),
                        msb: s.msb,
                        lsb: s.lsb,
                        span: sp,
                    },
                    None => AstExpr::ident(signal.clone(), sp),
                },
                // Parameter di-inline menjadi konstanta nilainya.
                Some(k @ Target::Param { .. }) => k.ekspresi(sp),
                None => AstExpr::ident(nama(n, prefix, koneksi), sp),
            }
        }
        // Nama fungsi bukan sinyal, jadi tidak di-prefix; argumennya yang perlu.
        AstExpr::FunctionCall { name, args, span } => AstExpr::FunctionCall {
            name: name.clone(),
            args: args
                .iter()
                .map(|a| lingkup_expr(a, prefix, koneksi, *span))
                .collect(),
            span: *span,
        },
        AstExpr::Number(v) => AstExpr::Number(*v),
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
        AstExpr::Binary { op, lhs, rhs, span } => AstExpr::Binary {
            op: *op,
            lhs: Box::new(lingkup_expr(lhs, prefix, koneksi, *span)),
            rhs: Box::new(lingkup_expr(rhs, prefix, koneksi, *span)),
            span: *span,
        },
        AstExpr::Unary { op, operand, span } => AstExpr::Unary {
            op: *op,
            operand: Box::new(lingkup_expr(operand, prefix, koneksi, *span)),
            span: *span,
        },
        AstExpr::Ternary {
            condition,
            when_true,
            when_false,
            span,
        } => AstExpr::Ternary {
            condition: Box::new(lingkup_expr(condition, prefix, koneksi, *span)),
            when_true: Box::new(lingkup_expr(when_true, prefix, koneksi, *span)),
            when_false: Box::new(lingkup_expr(when_false, prefix, koneksi, *span)),
            span: *span,
        },
        AstExpr::Select {
            base,
            msb,
            lsb,
            span,
        } => AstExpr::Select {
            base: Box::new(lingkup_expr(base, prefix, koneksi, *span)),
            msb: *msb,
            lsb: *lsb,
            span: *span,
        },
        AstExpr::IndexDynamic { base, index, span } => AstExpr::IndexDynamic {
            base: Box::new(lingkup_expr(base, prefix, koneksi, *span)),
            index: Box::new(lingkup_expr(index, prefix, koneksi, *span)),
            span: *span,
        },
        AstExpr::Concat { items, span } => AstExpr::Concat {
            items: items
                .iter()
                .map(|i| lingkup_expr(i, prefix, koneksi, *span))
                .collect(),
            span: *span,
        },
        AstExpr::Replicate { count, value, span } => AstExpr::Replicate {
            count: Box::new(lingkup_expr(count, prefix, koneksi, *span)),
            value: Box::new(lingkup_expr(value, prefix, koneksi, *span)),
            span: *span,
        },
        // `$time` tidak merujuk sinyal, jadi tidak perlu di-prefix.
        AstExpr::SystemTime { unit, span } => AstExpr::SystemTime {
            unit: *unit,
            span: *span,
        },
        // LRM §6.14 + §8.20: nama tipe bukan sinyal, jadi tidak dipetakan ke
        // koneksi port — tapi typedef module-scoped, jadi tetap perlu prefix
        // instans agar `w_t` di anak tidak tertukar dengan milik induk atau
        // instansi lain. Hanya operandnya yang mengikuti pemetaan koneksi.
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
            operand: Box::new(lingkup_expr(operand, prefix, koneksi, *span)),
            span: *span,
        },
        AstExpr::SizeCast {
            width,
            operand,
            span,
        } => AstExpr::SizeCast {
            width: *width,
            operand: Box::new(lingkup_expr(operand, prefix, koneksi, *span)),
            span: *span,
        },
        AstExpr::Cast {
            type_name,
            operand,
            span,
        } => AstExpr::Cast {
            type_name: format!("{prefix}{type_name}"),
            operand: Box::new(lingkup_expr(operand, prefix, koneksi, *span)),
            span: *span,
        },
        AstExpr::SignCast {
            signed,
            operand,
            span,
        } => AstExpr::SignCast {
            signed: *signed,
            operand: Box::new(lingkup_expr(operand, prefix, koneksi, *span)),
            span: *span,
        },
        AstExpr::Bits { operand, span } => AstExpr::Bits {
            operand: Box::new(lingkup_expr(operand, prefix, koneksi, *span)),
            span: *span,
        },
    }
}

/// Sama seperti `lingkup_lvalue`, tapi untuk langkah loop `for`.
fn lingkup_for_step(
    step: &sv_ast::combinational::ForStep,
    prefix: &str,
    koneksi: &KoneksiMap,
) -> sv_ast::combinational::ForStep {
    sv_ast::combinational::ForStep {
        lhs: lingkup_lvalue(&step.lhs, prefix, koneksi),
        rhs: lingkup_expr(&step.rhs, prefix, koneksi, step.span),
        span: step.span,
    }
}

/// Prefix satu statement tunggal; dipakai untuk badan `#n <stmt>`.
fn lingkup_satu(
    statement: &CombinationalStatement,
    prefix: &str,
    koneksi: &KoneksiMap,
) -> CombinationalStatement {
    let mut hasil = lingkup_comb(std::slice::from_ref(statement), prefix, koneksi);
    hasil
        .pop()
        .expect("satu statement menghasilkan satu statement")
}

fn lingkup_comb(
    body: &[CombinationalStatement],
    prefix: &str,
    koneksi: &KoneksiMap,
) -> Vec<CombinationalStatement> {
    body.iter()
        .map(|s| match s {
            // Deklarasi lokal juga diberi prefix instans, karena rujukan
            // di dalam blok juga di-prefix oleh `lingkup_expr`.
            CombinationalStatement::Decl(decl) => {
                let mut decl = decl.clone();
                decl.name = format!("{}{}", prefix, decl.name);
                CombinationalStatement::Decl(decl)
            }
            // Nama fungsi bukan sinyal port, jadi tidak di-prefix; argumennya yang perlu.
            CombinationalStatement::TaskCall(call) => {
                CombinationalStatement::TaskCall(sv_ast::routine::TaskCall {
                    name: call.name.clone(),
                    args: call
                        .args
                        .iter()
                        .map(|a| lingkup_expr(a, prefix, koneksi, call.span))
                        .collect(),
                    span: call.span,
                })
            }
            CombinationalStatement::Return { value, span } => CombinationalStatement::Return {
                value: value
                    .as_ref()
                    .map(|v| lingkup_expr(v, prefix, koneksi, *span)),
                span: *span,
            },
            CombinationalStatement::BlockingAssign { lhs, rhs, span } => {
                CombinationalStatement::BlockingAssign {
                    lhs: lingkup_lvalue(lhs, prefix, koneksi),
                    rhs: lingkup_expr(rhs, prefix, koneksi, *span),
                    span: *span,
                }
            }
            // NBA harus tetap NBA setelah prefix instans; kalau variannya
            // diturunkan ke `BlockingAssign`, `<=` jadi `=`.
            CombinationalStatement::NonBlockingAssign { lhs, rhs, span } => {
                CombinationalStatement::NonBlockingAssign {
                    lhs: lingkup_lvalue(lhs, prefix, koneksi),
                    rhs: lingkup_expr(rhs, prefix, koneksi, *span),
                    span: *span,
                }
            }
            CombinationalStatement::CompoundAssign { lhs, op, rhs, span } => {
                CombinationalStatement::CompoundAssign {
                    lhs: lingkup_lvalue(lhs, prefix, koneksi),
                    op: *op,
                    rhs: lingkup_expr(rhs, prefix, koneksi, *span),
                    span: *span,
                }
            }
            CombinationalStatement::For {
                init,
                condition,
                step,
                body,
                span,
            } => CombinationalStatement::For {
                init: lingkup_for_step(init, prefix, koneksi),
                condition: lingkup_expr(condition, prefix, koneksi, *span),
                step: lingkup_for_step(step, prefix, koneksi),
                body: lingkup_comb(body, prefix, koneksi),
                span: *span,
            },
            // Argumen system task ikut di-prefix agar sinyal yang dirujuk
            // tetap milik instans yang benar.
            CombinationalStatement::SystemTask(task) => {
                CombinationalStatement::SystemTask(sv_ast::system_task::SystemTask {
                    kind: task.kind.clone(),
                    args: task
                        .args
                        .iter()
                        .map(|a| match a {
                            sv_ast::system_task::SystemArg::Format(t) => {
                                sv_ast::system_task::SystemArg::Format(t.clone())
                            }
                            sv_ast::system_task::SystemArg::Value(e) => {
                                sv_ast::system_task::SystemArg::Value(lingkup_expr(
                                    e, prefix, koneksi, task.span,
                                ))
                            }
                        })
                        .collect(),
                    // Syarat `$monitor if` menunjuk sinyal instans yang sama.
                    condition: task
                        .condition
                        .as_ref()
                        .map(|c| lingkup_expr(c, prefix, koneksi, task.span)),
                    time_scale: task.time_scale,
                    span: task.span,
                })
            }
            CombinationalStatement::Repeat { count, body, span } => {
                CombinationalStatement::Repeat {
                    count: lingkup_expr(count, prefix, koneksi, *span),
                    body: lingkup_comb(body, prefix, koneksi),
                    span: *span,
                }
            }
            CombinationalStatement::Delay {
                amount,
                unit,
                body,
                span,
            } => CombinationalStatement::Delay {
                amount: lingkup_expr(amount, prefix, koneksi, *span),
                unit: *unit,
                body: Box::new(lingkup_satu(body, prefix, koneksi)),
                span: *span,
            },
            CombinationalStatement::While {
                condition,
                body,
                span,
            } => CombinationalStatement::While {
                condition: lingkup_expr(condition, prefix, koneksi, *span),
                body: lingkup_comb(body, prefix, koneksi),
                span: *span,
            },
            CombinationalStatement::Block { body, span } => CombinationalStatement::Block {
                body: lingkup_comb(body, prefix, koneksi),
                span: *span,
            },
            // Sinyal event control ikut di-prefix agar referring ke instans
            // yang benar.
            CombinationalStatement::EventControl { events, body, span } => {
                CombinationalStatement::EventControl {
                    events: events
                        .iter()
                        .map(|item| item.dengan_nama(nama(&item.signal, prefix, koneksi)))
                        .collect(),
                    body: lingkup_comb(body, prefix, koneksi),
                    span: *span,
                }
            }
            CombinationalStatement::IfElse {
                condition,
                then_branch,
                else_branch,
                span,
            } => CombinationalStatement::IfElse {
                condition: lingkup_expr(condition, prefix, koneksi, *span),
                then_branch: lingkup_comb(then_branch, prefix, koneksi),
                else_branch: else_branch
                    .as_ref()
                    .map(|b| lingkup_comb(b, prefix, koneksi)),
                span: *span,
            },
            CombinationalStatement::Case {
                selector,
                arms,
                kind,
                span,
            } => CombinationalStatement::Case {
                selector: lingkup_expr(selector, prefix, koneksi, *span),
                arms: arms
                    .iter()
                    .map(|a| sv_ast::combinational::CaseArm {
                        labels: a
                            .labels
                            .iter()
                            .map(|l| lingkup_expr(l, prefix, koneksi, *span))
                            .collect(),
                        body: lingkup_comb(&a.body, prefix, koneksi),
                        is_default: a.is_default,
                        span: a.span,
                    })
                    .collect(),
                kind: *kind,
                span: *span,
            },
        })
        .collect()
}

/// Beri prefix instans pada satu statement sekuensial beserta cabangnya.
fn lingkup_seq(i: &SequentialStatement, prefix: &str, koneksi: &KoneksiMap) -> SequentialStatement {
    match i {
        // Sama seperti combinational: lokal ikut di-prefix instans.
        SequentialStatement::Decl(decl) => {
            let mut decl = decl.clone();
            decl.name = format!("{}{}", prefix, decl.name);
            SequentialStatement::Decl(decl)
        }
        SequentialStatement::TaskCall(call) => {
            SequentialStatement::TaskCall(sv_ast::routine::TaskCall {
                name: call.name.clone(),
                args: call
                    .args
                    .iter()
                    .map(|a| lingkup_expr(a, prefix, koneksi, call.span))
                    .collect(),
                span: call.span,
            })
        }
        SequentialStatement::Return { value, span } => SequentialStatement::Return {
            value: value
                .as_ref()
                .map(|v| lingkup_expr(v, prefix, koneksi, *span)),
            span: *span,
        },
        SequentialStatement::NonBlockingAssign { lhs, rhs, span } => {
            SequentialStatement::NonBlockingAssign {
                lhs: lingkup_lvalue(lhs, prefix, koneksi),
                rhs: lingkup_expr(rhs, prefix, koneksi, *span),
                span: *span,
            }
        }
        SequentialStatement::BlockingAssign { lhs, rhs, span } => {
            SequentialStatement::BlockingAssign {
                lhs: lingkup_lvalue(lhs, prefix, koneksi),
                rhs: lingkup_expr(rhs, prefix, koneksi, *span),
                span: *span,
            }
        }
        // LRM §12.4: cabang `if`/`else` ikut di-prefix seperti statement biasa.
        SequentialStatement::IfElse {
            condition,
            then_branch,
            else_branch,
            span,
        } => SequentialStatement::IfElse {
            condition: lingkup_expr(condition, prefix, koneksi, *span),
            then_branch: lingkup_seq_list(then_branch, prefix, koneksi),
            else_branch: else_branch
                .as_ref()
                .map(|branch| lingkup_seq_list(branch, prefix, koneksi)),
            span: *span,
        },
        SequentialStatement::SystemTask(task) => {
            SequentialStatement::SystemTask(lingkup_task(task, prefix, koneksi))
        }
    }
}

/// Beri prefix instans pada argumen system task yang berupa nilai.
fn lingkup_task(
    task: &sv_ast::system_task::SystemTask,
    prefix: &str,
    koneksi: &KoneksiMap,
) -> sv_ast::system_task::SystemTask {
    let asal = task.span;
    sv_ast::system_task::SystemTask {
        kind: task.kind.clone(),
        args: task
            .args
            .iter()
            .map(|a| match a {
                sv_ast::system_task::SystemArg::Format(t) => {
                    sv_ast::system_task::SystemArg::Format(t.clone())
                }
                sv_ast::system_task::SystemArg::Value(e) => {
                    sv_ast::system_task::SystemArg::Value(lingkup_expr(e, prefix, koneksi, asal))
                }
            })
            .collect(),
        condition: task
            .condition
            .as_ref()
            .map(|c| lingkup_expr(c, prefix, koneksi, asal)),
        time_scale: task.time_scale,
        span: task.span,
    }
}

/// Beri prefix instans pada rangkaian statement sekuensial.
fn lingkup_seq_list(
    body: &[SequentialStatement],
    prefix: &str,
    koneksi: &KoneksiMap,
) -> Vec<SequentialStatement> {
    body.iter()
        .map(|i| lingkup_seq(i, prefix, koneksi))
        .collect()
}

/// Peta nama parameter modul ke nilai konstantanya.
///
/// Dipakai dua kali: sekali untuk menyubstitusi statement modul (lewat
/// [`substitusi_parameter`]) dan sekali lagi sebagai `koneksi` elaborator
/// generate, karena `lower_regions` berjalan terpisah dan tanpa peta ini
/// parameter tidak terlihat di dalam `generate`.
pub(crate) fn peta_parameter(params: &ParamTable) -> KoneksiMap {
    let mut koneksi: KoneksiMap = HashMap::new();
    for decl in params.decls() {
        let v = params.nilai(&decl.name, decl.span).unwrap_or(0);
        koneksi.insert(decl.name.clone(), Target::Param { nilai: v });
    }
    koneksi
}

/// Tolak nama yang dipakai ganda oleh parameter dan sinyal/port modul.
///
/// LRM §6.20: parameter dan variabel berbagi namespace di dalam modul, jadi
/// `parameter W` bersama `logic W` adalah deklarasi ganda. iverilog menolaknya
/// dengan pesan duplicate. Tanpa pemeriksaan ini `koneksi` lebih diprioritaskan
/// daripada simbol, sehingga `assign y = W;` diam-diam membaca parameter
/// padahal yang dimaksud variabel — kode yang salah lolos tanpa error.
fn cek_bentrok_nama(
    module: &Module,
    params: &ParamTable,
    span: Span,
) -> Result<(), ElaborateError> {
    let mut dipakai: Vec<&str> = Vec::new();
    for p in &module.ports {
        dipakai.push(&p.name);
    }
    for d in &module.declarations {
        dipakai.push(&d.name);
    }
    for decl in params.decls() {
        if dipakai.contains(&decl.name.as_str()) {
            return Err(ElaborateError::new(
                format!(
                    "nama '{}' dipakai ganda sebagai parameter dan sinyal modul",
                    decl.name
                ),
                span,
            ));
        }
    }
    Ok(())
}

/// Substitusi parameter modul ke konstanta di seluruh badan modul.
///
/// LRM §6.20: `parameter` adalah konstanta yang boleh dipakai di mana saja
/// dalam modul — body statement, nilai awal deklarasi, dan nilai awal
/// deklarasi lokal di dalam `begin...end`.
///
/// Parameternya modul anak sudah ter-substitusi lewat `koneksi` pada
/// `lower_instance`, tapi modul **top** tidak punya jalur itu sama sekali, jadi
/// `assign y = W;` gagal dengan "undefined signal 'W'" padahal kode-nya sah.
/// Fungsi ini menutup kedua sisi dengan satu implementasi.
///
/// `prefix` ikut diteruskan supaya deklarasi lokal di dalam instans tetap
///_di-prefix_, dan `nilai` menyediakan override parameter anak.
pub(crate) fn substitusi_parameter(
    module: &Module,
    params: &ParamTable,
    prefix: &str,
    nilai: Option<&HashMap<String, u64>>,
) -> Result<Module, ElaborateError> {
    cek_bentrok_nama(module, params, module.span)?;
    // Override dari instans menang; kalau tidak ada, pakai nilai default modul.
    let mut koneksi = peta_parameter(params);
    if let Some(override_nilai) = nilai {
        for decl in &module.params {
            if let Some(v) = override_nilai.get(&decl.name) {
                koneksi.insert(decl.name.clone(), Target::Param { nilai: *v });
            }
        }
    }
    let mut out = module.clone();
    // Nilai awal deklarasi module-level.
    for decl in &mut out.declarations {
        if let Some(init) = &decl.init {
            decl.init = Some(lingkup_expr(init, prefix, &koneksi, decl.span));
        }
    }
    // Deklarasi lokal di dalam blok: nilainya juga boleh memakai parameter.
    for decl in &mut out.statements {
        substitusi_local_decl_stmt(decl, prefix, &koneksi);
    }
    // Statement modul.
    out.statements = module
        .statements
        .iter()
        .map(|s| lingkup_statement(s, prefix, &koneksi))
        .collect();
    // Region generate SENGAJA tidak disentuh: `generate::lower_regions` sudah
    // menjalankan substitusi genvar + prefix instans sendiri, dan memanggilnya
    // dua kali akan merusak irisan LHS (`q[i]` kehilangan `i`).
    Ok(out)
}

/// Substitusi pada deklarasi lokal di dalam statement combinational.
fn substitusi_local_decl_stmt(s: &mut AstStatement, prefix: &str, koneksi: &KoneksiMap) {
    let (body, span) = match s {
        AstStatement::AlwaysComb { body, span } | AstStatement::Initial { body, span } => {
            (body, *span)
        }
        AstStatement::AlwaysFf { body, .. } => {
            for stmt in body.iter_mut() {
                substitusi_seq_local(stmt, prefix, koneksi);
            }
            return;
        }
        AstStatement::ContinuousAssign { .. } => return,
    };
    for stmt in body.iter_mut() {
        substitusi_comb_local(stmt, prefix, koneksi, span);
    }
}

fn substitusi_comb_local(
    s: &mut CombinationalStatement,
    prefix: &str,
    koneksi: &KoneksiMap,
    asal: sv_lexer::span::Span,
) {
    match s {
        CombinationalStatement::Decl(decl) => {
            if let Some(init) = &decl.init {
                decl.init = Some(lingkup_expr(init, prefix, koneksi, asal));
            }
        }
        CombinationalStatement::Block { body, .. } => {
            for s in body.iter_mut() {
                substitusi_comb_local(s, prefix, koneksi, asal);
            }
        }
        CombinationalStatement::IfElse {
            then_branch,
            else_branch,
            ..
        } => {
            for s in then_branch.iter_mut() {
                substitusi_comb_local(s, prefix, koneksi, asal);
            }
            if let Some(branch) = else_branch {
                for s in branch.iter_mut() {
                    substitusi_comb_local(s, prefix, koneksi, asal);
                }
            }
        }
        CombinationalStatement::For { body, .. } | CombinationalStatement::While { body, .. } => {
            for s in body.iter_mut() {
                substitusi_comb_local(s, prefix, koneksi, asal);
            }
        }
        CombinationalStatement::Repeat { body, .. } => {
            for s in body.iter_mut() {
                substitusi_comb_local(s, prefix, koneksi, asal);
            }
        }
        _ => {}
    }
}

fn substitusi_seq_local(
    s: &mut sv_ast::statement::SequentialStatement,
    prefix: &str,
    _koneksi: &KoneksiMap,
) {
    if let sv_ast::statement::SequentialStatement::Decl(decl) = s {
        if let Some(init) = &decl.init {
            decl.init = Some(lingkup_expr(init, prefix, &HashMap::new(), decl.span));
        }
    }
}

fn lingkup_statement(s: &AstStatement, prefix: &str, koneksi: &KoneksiMap) -> AstStatement {
    match s {
        AstStatement::ContinuousAssign { lhs, rhs, span } => AstStatement::ContinuousAssign {
            lhs: lingkup_lvalue(lhs, prefix, koneksi),
            rhs: lingkup_expr(rhs, prefix, koneksi, *span),
            span: *span,
        },
        AstStatement::AlwaysComb { body, span } => AstStatement::AlwaysComb {
            body: lingkup_comb(body, prefix, koneksi),
            span: *span,
        },
        // Blok initial milik instans juga ikut di-prefix.
        AstStatement::Initial { body, span } => AstStatement::Initial {
            body: lingkup_comb(body, prefix, koneksi),
            span: *span,
        },
        AstStatement::AlwaysFf { events, body, span } => AstStatement::AlwaysFf {
            // LRM §9.7: tiap item event ikut di-prefix, bukan hanya clock utama.
            events: events
                .iter()
                .map(|item| item.dengan_nama(nama(&item.signal, prefix, koneksi)))
                .collect(),
            body: body
                .iter()
                .map(|i| lingkup_seq(i, prefix, koneksi))
                .collect(),
            span: *span,
        },
    }
}
