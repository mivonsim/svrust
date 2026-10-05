// Tanggung jawab: membangun IR Design dari AST Module.
use crate::error::ElaborateError;
use crate::instance::lower_instance;
use crate::lower_stmt::lower_module_statement;
use crate::param::ParamTable;
use crate::symbol::{Symbol, SymbolTable};
use sv_ast::module::Module;
use sv_ast::port::PortDirection;
use sv_ir::datatype::DataType;
use sv_ir::scope::ScopePath;
use sv_ir::time_scale::TimeScale;
use sv_ir::variable::{VarDecl, VarKind};
use sv_ir::{Design, PortDirection as IrPortDirection, PortInfo};

/// Elaborate satu AST module menjadi IR design.
pub fn elaborate(module: &Module) -> Result<Design, ElaborateError> {
    // LRM §13.3/§13.4: badan task/function di-inline dulu, sehingga semua
    // statement yang dilihat helper di bawah tidak pernah menemukan panggilan
    // yang belum dikembangkan.
    let module = &crate::routine::expand_module(module)?;
    let params_awal = ParamTable::untuk_modul(module)?;
    let types = crate::typedef::TypeTable::baru(&module.typedefs, &params_awal)?;
    let module = &crate::typedef_walk::expand_literals(module, &types)?;
    let module = &crate::instance::substitusi_parameter(module, &params_awal, "", None)?;
    let mut symbols = SymbolTable::new();
    let mut design = Design::new(module.name.clone());
    // LRM §21.8: skala waktu top dipakai untuk membulatkan `$time` dan header
    // VCD. Modul anak bisa punya `timescale` sendiri; setelah flatten hanya
    // skala top yang tersisa, jadi `$time` global mengikuti presisi top.
    design.time_scale = Some(map_time_scale(module.time_scale));
    // Tabel lengkap (parameter port + `localparam`) dipakai lagi di sini:
    // lebar dimension dan tipe `typedef` boleh merujuk `localparam`.
    let params = ParamTable::untuk_modul(module)?;

    for port in &module.ports {
        let bit = params.lebar(&port.width, port.span)?;
        let data_type = DataType::logic(bit as u32).with_signed(port.signed);
        let id = symbols.insert(Symbol {
            name: port.name.clone(),
            signal_id: 0,
            data_type,
            kind: VarKind::Port,
            unpacked: None,
            span: port.span,
        })?;

        design.add_variable(VarDecl::new(
            port.name.clone(),
            ScopePath::root().child(port.name.clone()),
            data_type,
            VarKind::Port,
            id,
        ));
        design.ports.push(PortInfo {
            name: port.name.clone(),
            direction: map_direction(port.direction),
            data_type,
        });
    }

    isi_body(&mut design, &mut symbols, module, &params)?;

    // LRM §27: region generate tetap boleh dielabase tanpa modul lain, tetapi
    // instansiasi di dalamnya membutuhkan daftar module.
    if !module.instances.is_empty() {
        return Err(ElaborateError::new(
            "module berinstansi butuh elaborate_top",
            module.span,
        ));
    }
    crate::generate::lower_regions(&mut design, &mut symbols, module, &params, &[])?;

    Ok(design)
}

/// Elaborate top dari daftar module seberkas; instansi di-flatten.
pub fn elaborate_top(modules: &[Module], top: &str) -> Result<Design, ElaborateError> {
    // Setiap module di-expand sebelum instansiasi, sehingga modul anak ikut
    // memakai subrutin yang dideklarasikan di body-nya sendiri.
    let expanded = modules
        .iter()
        .map(crate::routine::expand_module)
        .collect::<Result<Vec<_>, ElaborateError>>()?;
    let modules = &expanded[..];
    // LRM §6.7: literal enum tiap modul disubstitusi terpisah, karena tabel
    // literal tidak boleh bocor antar modul.
    let modules = &modules
        .iter()
        .map(|m| {
            let params = ParamTable::untuk_modul(m)?;
            let types = crate::typedef::TypeTable::baru(&m.typedefs, &params)?;
            crate::typedef_walk::expand_literals(m, &types)
        })
        .collect::<Result<Vec<Module>, ElaborateError>>()?;
    // LRM §6.20: parameter modul top adalah konstanta yang boleh dipakai di
    // badan modulnya sendiri, begitu pula `localparam`. Tanpa substitusi ini
    // `assign y = W;` gagal dengan "undefined signal 'W'" padahal kodenya sah.
    //
    // Substitusi HANYA untuk modul top. Modul anak mendapat parameternya
    // di-inline saat instansiasi (`lower_instance`) memakai nilai EFFECTIF;
    // kalau di-substitusi di sini dengan nilai default, override `#(.W(8))`
    // akan hilang dan anak memakai lebar default.
    let module = modules.iter().find(|m| m.name == top).ok_or_else(|| {
        ElaborateError::new(
            format!("top module tak ditemukan: {}", top),
            sv_lexer::span::Span::dummy(),
        )
    })?;
    let module_owned;
    let module = {
        // `untuk_modul` memasukkan `localparam` (LRM §6.20); `ParamTable::baru`
        // hanya melihat daftar parameter port, jadi `localparam` akan hilang.
        let params = ParamTable::untuk_modul(module)?;
        module_owned = crate::instance::substitusi_parameter(module, &params, "", None)?;
        &module_owned
    };
    let mut symbols = SymbolTable::new();
    let mut design = Design::new(module.name.clone());
    // LRM §21.8: presisi simulasi adalah `timeprecision` **terkecil** di antara
    // seluruh modul, bukan milik modul top saja. Header VCD menulis nilai waktu
    // dalam satuan presisi itu, jadi memakai presisi top akan membuat pembaca VCD
    // salah menafsirkan setiap timestamp ketika ada modul anak yang lebih tajam.
    let scale = map_time_scale(module.time_scale);
    let presisi_paling_kecil = modules
        .iter()
        .map(|m| map_time_unit(m.time_scale.precision))
        .min_by_key(|u| u.femtos())
        .unwrap_or(scale.precision);
    design.time_scale = Some(TimeScale::new(scale.unit, presisi_paling_kecil));
    // Tabel yang sudah memuat `localparam` dipakai lagi di sini: lebar
    // dimension dan tipe `typedef` boleh merujuknya.
    let params = ParamTable::untuk_modul(module)?;

    for port in &module.ports {
        let bit = params.lebar(&port.width, port.span)?;
        let data_type = DataType::logic(bit as u32).with_signed(port.signed);
        let id = symbols.insert(Symbol {
            name: port.name.clone(),
            signal_id: 0,
            data_type,
            kind: VarKind::Port,
            unpacked: None,
            span: port.span,
        })?;

        design.add_variable(VarDecl::new(
            port.name.clone(),
            ScopePath::root().child(port.name.clone()),
            data_type,
            VarKind::Port,
            id,
        ));
        design.ports.push(PortInfo {
            name: port.name.clone(),
            direction: map_direction(port.direction),
            data_type,
        });
    }

    isi_body(&mut design, &mut symbols, module, &params)?;

    for inst in &module.instances {
        lower_instance(
            &mut design,
            &mut symbols,
            inst,
            modules,
            &crate::generate::GenvarEnv::kosong(),
        )?;
    }

    // Region generate dielabore setelah instans modul biasa supaya tabel
    // simbol sudah memuat seluruh sinyal yang boleh dirujuknya.
    crate::generate::lower_regions(&mut design, &mut symbols, module, &params, modules)?;

    Ok(design)
}

/// Daftarkan nama `typedef` module ke tabel simbol sebagai target type cast.
///
/// LRM §6.14: `nama_tipe'(ekspresi)` memakai typedef yang sama dengan deklarasi
/// variabel, jadi nama tipe harus sudah ada sebelum statement dielaborasi.
///
/// LRM §8.20: typedef module-scoped, jadi setiap nama di-prefix instans yang
/// sama seperti sinyal internal anak. Tanpa prefix, dua instansi modul yang
/// sama dengan `w_t` berbeda lebar akan saling menimpa, dan cast di badan anak
/// bisa diam-diam memakai lebar milik instansi lain.
///
/// Dipanggil `prefix` kosong dari `isi_body` (modul top, setelah port-nya ada
/// supaya bentrok nama dengan sinyal terdeteksi) dan dari `lower_instance`
/// dengan prefix instans.
pub(crate) fn daftarkan_tipe(
    symbols: &mut SymbolTable,
    module: &Module,
    params: &ParamTable,
    prefix: &str,
) -> Result<(), ElaborateError> {
    for decl in &module.typedefs {
        let types = crate::typedef::TypeTable::baru(std::slice::from_ref(decl), params)?;
        for (nama, info) in types.iter_tipe() {
            symbols.insert_type(
                &format!("{prefix}{nama}"),
                info.width,
                info.signed,
                decl.span,
            )?;
        }
    }
    Ok(())
}

/// Lower deklarasi dan statement satu module ke design.
fn isi_body(
    design: &mut Design,
    symbols: &mut SymbolTable,
    module: &Module,
    params: &ParamTable,
) -> Result<(), ElaborateError> {
    // LRM §8.20 dan §6.7: typedef menentukan lebar tipe, dan nama anggota
    // enum disubstitusi jadi konstanta sebelum statement dielaborasi.
    let types = crate::typedef::TypeTable::baru(&module.typedefs, params)?;

    for decl in &module.declarations {
        let (bit, signed) = crate::typedef::lebar_deklarasi(
            &decl.width,
            decl.type_name.as_deref(),
            &types,
            params,
            decl.span,
        )?;
        // LRM §7.8: array unpacked disimpan sebagai satu sinyal rata selebar
        // `lebar elemen * size`, dan dimensinya ikut tersimpan supaya indeks
        // bisa dipetakan ke rentang bit elemennya.
        let (data_type, unpacked) = match decl.unpacked {
            None => (
                DataType::logic(bit).with_signed(signed || decl.signed),
                None,
            ),
            Some(dim) => {
                let total = (dim.size as u32).checked_mul(bit).ok_or_else(|| {
                    ElaborateError::invalid_width(
                        format!("lebar array unpacked {} x {} bit meluap", dim.size, bit),
                        decl.span,
                    )
                })?;
                (
                    DataType::logic(total).with_signed(signed || decl.signed),
                    Some(dim),
                )
            }
        };
        let mut symbol = Symbol {
            name: decl.name.clone(),
            signal_id: 0,
            data_type,
            kind: VarKind::Variable,
            unpacked: None,
            span: decl.span,
        };
        if let Some(dim) = unpacked {
            symbol = symbol.dengan_unpacked(dim, bit);
        }
        let id = symbols.insert(symbol)?;

        design.add_variable(VarDecl::new(
            decl.name.clone(),
            ScopePath::root().child(decl.name.clone()),
            data_type,
            VarKind::Variable,
            id,
        ));
    }

    // Nama tipe didaftarkan setelah semua deklarasi, supaya bentrok nama dengan
    // variabel (LRM §8.20) benar-benar terlihat — `insert_type` menolak nama
    // yang sudah dipakai sinyal, dan hanya berlaku untuk nama polos (tanpa
    // prefix instans). Nilai awal di bawah memakai cast, jadi pendaftaran ini
    // harus sebelumnya.
    daftarkan_tipe(symbols, module, params, "")?;

    // Nilai awal variabel pada waktu nol (LRM §6.2.2).
    for decl in &module.declarations {
        if let Some(init) = &decl.init {
            let Some(var) = design.find_variable(&decl.name) else {
                continue;
            };
            // LRM §6.2.2: nilai awal di-assign ke sinyal, jadi ekspresinya
            // context-determined ke lebar sinyal. Tanpa itu `logic [7:0] s = -1;`
            // menyimpan 0xFFFFFFFFFFFFFFFF di sinyal 8-bit — bit di atas lebar
            // logis tidak pernah dibuang dan perbandingan_unsigned ikut salah.
            let lowered =
                crate::lower_expr::lower_expression_konteks(init, symbols, var.data_type.width)?;
            let signed_nilai = lowered.data_type().signed;
            let lowered = crate::lower_expr::sesuaikan_lebar(
                lowered,
                var.data_type.width,
                var.data_type.signed,
                signed_nilai,
            );
            design.set_initial(var.signal_id, lowered);
        }
    }

    // Variabel lokal harus terdaftar sebelum statement dielaborasi,
    // karena ekspresi di dalam blok sudah merujuk nama tersebut.
    crate::local_decl::daftarkan(module, design, symbols, params, &types)?;

    for statement in &module.statements {
        design
            .processes
            .push(lower_module_statement(statement, symbols)?);
    }

    Ok(())
}

fn map_direction(direction: PortDirection) -> IrPortDirection {
    match direction {
        PortDirection::Input => IrPortDirection::Input,
        PortDirection::Output => IrPortDirection::Output,
        PortDirection::Inout => IrPortDirection::Inout,
    }
}

/// Petakan `timescale` AST ke bentuk IR.
pub fn map_time_scale(scale: sv_ast::time_scale::TimeScale) -> sv_ir::time_scale::TimeScale {
    sv_ir::time_scale::TimeScale::new(map_time_unit(scale.unit), map_time_unit(scale.precision))
}

/// Petakan satuan waktu AST ke enum IR.
fn map_time_unit(unit: sv_ast::time_unit::TimeUnit) -> sv_ir::TimeUnit {
    use sv_ast::time_unit::TimeUnit as Ast;
    use sv_ir::TimeUnit as Ir;
    match unit {
        // `Bawaan` sudah diselesaikan parser lewat `terapkan_module`; bila
        // sampai ke sini, modul dibangun tanpa `parse_file` dan tidak ada
        // `timescale` yang bisa dipakai, jadi nanosecond adalah pilihan yang
        // tidak mungkin diam-diam menggeser waktu lebih jauh.
        Ast::Bawaan => Ir::NanoSeconds,
        Ast::Seconds => Ir::Seconds,
        Ast::MilliSeconds => Ir::MilliSeconds,
        Ast::MicroSeconds => Ir::MicroSeconds,
        Ast::NanoSeconds => Ir::NanoSeconds,
        Ast::PicoSeconds => Ir::PicoSeconds,
        Ast::FectoSeconds => Ir::FectoSeconds,
    }
}

/// Statistik design untuk perintah `cargo sv inspect`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesignStats {
    pub name: String,
    pub ports: usize,
    pub signals: usize,
    pub processes: usize,
    pub statements: usize,
}

impl DesignStats {
    pub fn of(design: &Design) -> Self {
        Self {
            name: design.name.clone(),
            ports: design.ports.len(),
            signals: design.variables.len(),
            processes: design.processes.len(),
            statements: design.processes.iter().map(|p| p.body.len()).sum(),
        }
    }
}

impl std::fmt::Display for DesignStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Design: {}", self.name)?;
        writeln!(f, "  Ports:      {}", self.ports)?;
        writeln!(f, "  Signals:    {}", self.signals)?;
        writeln!(f, "  Processes:  {}", self.processes)?;
        writeln!(f, "  Statements: {}", self.statements)?;
        Ok(())
    }
}
