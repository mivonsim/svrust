// Tanggung jawab: membangun IR Design dari AST Module.
use crate::error::ElaborateError;
use crate::generate::GenvarEnv;
use crate::instance::lower_instance;
use crate::konst;
use crate::lower_stmt::lower_module_statement;
use crate::param::{PackageTable, ParamTable};
use crate::symbol::{Symbol, SymbolTable};
use sv_ast::module::Module;
use sv_ast::package::PackageDecl;
use sv_ast::port::PortDirection;
use sv_ast::typedef::TypeDefDecl;
use sv_ir::datatype::{DataType, NetResolution};
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
    let params_awal = ParamTable::untuk_modul(module, &PackageTable::new())?;
    let types =
        crate::typedef::TypeTable::baru(&module.typedefs, &params_awal, &PackageTable::new())?;
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
    let params = ParamTable::untuk_modul(module, &PackageTable::new())?;

    for port in &module.ports {
        let bit = params.lebar(&port.width, &PackageTable::new(), port.span)?;
        let mut data_type = DataType::logic(bit as u32).with_signed(port.signed);
        // LRM §6.5: port tanpa tipe net/variabel eksplisit (`output [7:0] z`)
        // adalah implicit net, bukan variabel. Net tanpa driver bernilai `Z`
        // (iverilog `zZ`), bukan `X` seperti variabel. Wire default = last-wins
        // (NetResolution::None), wand = WiredAnd, wor/tri = WiredOr.
        let kind = if port.is_implicit_net() {
            data_type = data_type.with_net_resolution(NetResolution::None);
            VarKind::Net
        } else {
            VarKind::Port
        };
        let id = symbols.insert(Symbol {
            name: port.name.clone(),
            signal_id: 0,
            data_type,
            kind,
            unpacked: None,
            span: port.span,
        })?;

        design.add_variable(VarDecl::new(
            port.name.clone(),
            ScopePath::root().child(port.name.clone()),
            data_type,
            kind,
            id,
        ));
        design.ports.push(PortInfo {
            name: port.name.clone(),
            direction: map_direction(port.direction),
            data_type,
        });
    }

    isi_body(
        &mut design,
        &mut symbols,
        module,
        &[],
        &params,
        &PackageTable::new(),
    )?;

    // LRM §27: region generate tetap boleh dielabase tanpa modul lain, tetapi
    // instansiasi di dalamnya membutuhkan daftar module.
    if !module.instances.is_empty() {
        return Err(ElaborateError::new(
            "module berinstansi butuh elaborate_top",
            module.span,
        ));
    }
    crate::generate::lower_regions(
        &mut design,
        &mut symbols,
        module,
        &params,
        &[],
        &PackageTable::new(),
    )?;

    Ok(design)
}

/// Elaborate top dari daftar module seberkas; instansi di-flatten.
pub fn elaborate_top(
    modules: &[Module],
    typedefs_global: &[TypeDefDecl],
    packages: &[PackageDecl],
    top: &str,
) -> Result<Design, ElaborateError> {
    eprintln!("DEBUG elaborate_top: modules count = {}", modules.len());
    for m in modules {
        eprintln!("DEBUG elaborate_top: module = {}", m.name);
    }
    eprintln!("DEBUG elaborate_top: calling elaborate for top = {}", top);
    // Bangun tabel parameter per paket dari deklarasi paket yang dikumpulkan
    // saat parse. Parameter paket dievaluasi dalam urutan deklarasi karena
    // boleh merujuk parameter paket yang sebelumnya (LRM §26).
    // Beberapa paket boleh merujuk parameter paket yang dideklarasikan lebih
    // lambat (misal bus_params_pkg merujuk top_pkg), jadi evaluasi diulang
    // hingga tidak ada progress lagi.
    let mut package_tabel: PackageTable = PackageTable::new();
    // Ulangi hingga semua parameter paket stabil (tidak ada perubahan lagi).
    // Paket yang bergantung pada paket lain (mis. `alert_handler_pkg` →
    // `alert_handler_reg_pkg::NAlerts`) perlu diproses ulang setelah
    // dependensinya tersedia (LRM §26).
    let mut changed = true;
    let mut iterasi = 0;
    while changed && iterasi < 100 {
        changed = false;
        iterasi += 1;
        for pkg in packages.iter() {
            let mut tabel = ParamTable::baru(&[])?;
            let mut paket_ok = false;
            for local in &pkg.params {
                if let Ok(nilai) = konst::konst(
                    &local.value,
                    &GenvarEnv::kosong(),
                    &tabel,
                    &package_tabel,
                    local.span,
                ) {
                    // Duplikasi nama parameter di paket, lewati parameter ini.
                    if let Ok(()) = tabel.tambah(&local.name, nilai, local.span) {
                        paket_ok = true;
                    }
                }
                // Ekspresi parameter tak bisa dievaluasi (misal pemanggilan
                // fungsi yang tak didukung), lewati parameter ini.
            }
            // Simpan paket bila minimal satu parameter berhasil dievaluasi.
            // Jika tabel berubah (nilai parameter berisi karena dependensi
            // baru tersedia), perbarui dan tandai untuk iterasi berikutnya.
            let sudah_ada = package_tabel.get(&pkg.name);
            if paket_ok && (!sudah_ada.is_some() || *sudah_ada.unwrap() != tabel) {
                package_tabel.insert(pkg.name.clone(), tabel);
                changed = true;
            }
        }
    }

    // Resolusi referensi `pkg::nama` pada semua modul sebelum pemrosesan lebih
    // lanjut, sehingga `konst` dan `lower_expression` di dalam penurunan
    // ekspresi tidak perlu tahu soal paket.
    let modules_owned: Vec<Module> = modules
        .iter()
        .map(|m| {
            let mut m = m.clone();
            crate::konst::resolusi_paket_modul(&mut m, &package_tabel);
            m
        })
        .collect();
    let modules = &modules_owned;

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
            let params = ParamTable::untuk_modul(m, &package_tabel)?;
            let mut semua_typedef = typedefs_global.to_vec();
            semua_typedef.extend_from_slice(&m.typedefs);
            let types = crate::typedef::TypeTable::baru(&semua_typedef, &params, &package_tabel)?;
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
        let params = ParamTable::untuk_modul(module, &package_tabel)?;
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
    let params = ParamTable::untuk_modul(module, &package_tabel)?;

    for port in &module.ports {
        let bit = params.lebar(&port.width, &package_tabel, port.span)?;
        let mut data_type = DataType::logic(bit as u32).with_signed(port.signed);
        // LRM §6.5: port tanpa tipe net/variabel eksplisit (`output [7:0] z`)
        // adalah implicit net, bukan variabel. Net tanpa driver bernilai `Z`
        // (iverilog `zZ`), bukan `X` seperti variabel. Wire default = last-wins
        // (NetResolution::None).
        let kind = if port.is_implicit_net() {
            data_type = data_type.with_net_resolution(NetResolution::None);
            VarKind::Net
        } else {
            VarKind::Port
        };
        let id = symbols.insert(Symbol {
            name: port.name.clone(),
            signal_id: 0,
            data_type,
            kind,
            unpacked: None,
            span: port.span,
        })?;

        design.add_variable(VarDecl::new(
            port.name.clone(),
            ScopePath::root().child(port.name.clone()),
            data_type,
            kind,
            id,
        ));
        design.ports.push(PortInfo {
            name: port.name.clone(),
            direction: map_direction(port.direction),
            data_type,
        });
    }

    isi_body(
        &mut design,
        &mut symbols,
        module,
        typedefs_global,
        &params,
        &package_tabel,
    )?;

    for inst in &module.instances {
        lower_instance(
            &mut design,
            &mut symbols,
            inst,
            modules,
            &crate::generate::GenvarEnv::kosong(),
            &package_tabel,
            &params,
        )?;
    }

    // Region generate dielabore setelah instans modul biasa supaya tabel
    // simbol sudah memuat seluruh sinyal yang boleh dirujuknya.
    crate::generate::lower_regions(
        &mut design,
        &mut symbols,
        module,
        &params,
        modules,
        &package_tabel,
    )?;

    Ok(design)
}

/// Daftarkan nama `typedef` module dan `typedef` global ke tabel simbol
/// sebagai target type cast.
///
/// LRM §6.14: `nama_tipe'(ekspresi)` memakai typedef yang sama dengan deklarasi
/// variabel, jadi nama tipe harus sudah ada sebelum statement dielaborasi.
///
/// LRM §8.20: typedef di luar module (compilation-unit scope) tersedia untuk
/// semua module tanpa prefix instans. Typedef module-scoped di-prefix instans
/// seperti sinyal internal anak.
///
/// Dipanggil `prefix` kosong dari `isi_body` (modul top) dan dari
/// `lower_instance` dengan prefix instans.
pub(crate) fn daftarkan_tipe(
    symbols: &mut SymbolTable,
    module: &Module,
    typedefs_global: &[TypeDefDecl],
    params: &ParamTable,
    packages: &PackageTable,
    prefix: &str,
) -> Result<(), ElaborateError> {
    // Typedef global (compilation-unit scope) didaftarkan tanpa prefix instans,
    // karena tersedia untuk semua module.
    for decl in typedefs_global {
        let types = crate::typedef::TypeTable::baru(std::slice::from_ref(decl), params, packages)?;
        for (nama, info) in types.iter_tipe() {
            symbols.insert_type(nama, info.width, info.signed, decl.span)?;
        }
    }
    // Typedef module-scoped di-prefix instans.
    for decl in &module.typedefs {
        let types = crate::typedef::TypeTable::baru(std::slice::from_ref(decl), params, packages)?;
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
    typedefs_global: &[TypeDefDecl],
    params: &ParamTable,
    packages: &PackageTable,
) -> Result<(), ElaborateError> {
    // LRM §8.20 dan §6.7: typedef di luar module (compilation-unit scope)
    // tersedia untuk semua module, digabung dengan typedef milik module itu
    // sendiri. Typedef module-scoped memiliki prioritas lebih tinggi bila
    // nama sama (shadowing), tetapi `TypeTable::baru` akan menolak duplicate
    // nama dalam satu tabel — jadi kita gabungkan global dulu, lalu module
    // lokal, dan biarkan `baru` menangkap duplikasi yang benar-benar ilegal.
    let mut semua_typedef = typedefs_global.to_vec();
    semua_typedef.extend_from_slice(&module.typedefs);
    let types = crate::typedef::TypeTable::baru(&semua_typedef, params, packages)?;

    for decl in &module.declarations {
        // `struct`/`union` packed: lebar total = jumlah lebar semua field
        // (LRM §6.7). Tangani sebelum lebar_deklarasi agar tidak mencari
        // "struct"/"union" sebagai typedef di tabel tipe.
        if decl.type_name.as_deref() == Some("struct") || decl.type_name.as_deref() == Some("union")
        {
            let total: u32 = decl
                .struct_fields
                .as_ref()
                .map(|fields| {
                    let lebar_list: Vec<u32> = fields
                        .iter()
                        .map(|f| match f.width {
                            sv_ast::width::WidthExpr::Literal(w) => w as u32,
                            _ => 0,
                        })
                        .collect();
                    if decl.type_name.as_deref() == Some("union") {
                        // Union: lebar = maksimum semua field (LRM §6.7).
                        lebar_list.into_iter().max().unwrap_or(0)
                    } else {
                        // Struct: lebar = jumlah semua field.
                        lebar_list.into_iter().sum()
                    }
                })
                .unwrap_or(0);
            // struct/union packed adalah variable (bukan net), walaupun tipe-nya
            // diturunkan dari field-fieldnya.
            let data_type = DataType::logic(total);
            let symbol = Symbol {
                name: decl.name.clone(),
                signal_id: 0,
                data_type,
                kind: VarKind::Variable,
                unpacked: None,
                span: decl.span,
            };
            let id = symbols.insert(symbol)?;
            design.add_variable(VarDecl::new(
                decl.name.clone(),
                ScopePath::root().child(decl.name.clone()),
                data_type,
                VarKind::Variable,
                id,
            ));
            continue;
        }

        let (bit, signed) = crate::typedef::lebar_deklarasi(
            &decl.width,
            decl.type_name.as_deref(),
            &types,
            params,
            packages,
            decl.span,
        )?;
        // LRM §7.8: array unpacked disimpan sebagai satu sinyal rata selebar
        // `lebar elemen * size`, dan dimensinya ikut tersimpan supaya indeks
        // bisa dipetakan ke rentang bit elemennya.
        let (data_type, unpacked) = match &decl.unpacked {
            None => {
                // `real` adalah floating point 64-bit (LRM §5.5), bukan bit
                // vector. Lebar dan signedness diabaikan.
                if decl.type_name.as_deref() == Some("real") {
                    (DataType::real(), None)
                } else if decl.type_name.as_deref() == Some("supply0") {
                    // `supply0` — net konstan bernilai 0 (LRM §6.6).
                    (DataType::supply0(), None)
                } else if decl.type_name.as_deref() == Some("supply1") {
                    // `supply1` — net konstan bernilai 1 (LRM §6.6).
                    (DataType::supply1(), None)
                } else if decl.type_name.as_deref() == Some("wand") {
                    // `wand` — net wired-AND (LRM §6.6).
                    (DataType::wand(bit).with_signed(signed || decl.signed), None)
                } else if decl.type_name.as_deref() == Some("wor")
                    || decl.type_name.as_deref() == Some("tri")
                {
                    // `wor`/`wor`/`tri` — net wired-OR (LRM §6.6).
                    (DataType::wor(bit).with_signed(signed || decl.signed), None)
                } else {
                    (
                        DataType::logic(bit).with_signed(signed || decl.signed),
                        None,
                    )
                }
            }
            Some(dims) => {
                let total = dims
                    .jumlah_elemen()
                    .checked_mul(bit as u64)
                    .and_then(|t| u32::try_from(t).ok())
                    .ok_or_else(|| {
                        ElaborateError::invalid_width(
                            format!(
                                "lebar array unpacked {} x {} bit meluap",
                                dims.jumlah_elemen(),
                                bit
                            ),
                            decl.span,
                        )
                    })?;
                (
                    DataType::logic(total).with_signed(signed || decl.signed),
                    Some(dims.clone()),
                )
            }
        };
        // LRM §6.6: wire/tri/wor/wand/supply0/supply1 adalah net (dikontrol
        // oleh continuous assign), bukan variable (dikontrol oleh procedural
        // assign). Lebih lanjut di §6.4: logic/reg/bit/integer adalah variable.
        let is_net = matches!(
            decl.type_name.as_deref(),
            Some("wire")
                | Some("tri")
                | Some("wor")
                | Some("wand")
                | Some("supply0")
                | Some("supply1")
        );
        let var_kind = if is_net {
            VarKind::Net
        } else {
            VarKind::Variable
        };
        let mut symbol = Symbol {
            name: decl.name.clone(),
            signal_id: 0,
            data_type,
            kind: var_kind,
            unpacked: None,
            span: decl.span,
        };
        let mut pending_unpacked: Option<Vec<(u64, u64, bool)>> = None;
        if let Some(dim) = unpacked {
            pending_unpacked = Some(
                dim.dims
                    .iter()
                    .map(|d| (d.first, d.size, d.descending))
                    .collect(),
            );
            symbol = symbol.dengan_unpacked(dim.clone(), bit);
        }
        let id = symbols.insert(symbol)?;
        if let Some(dims_vec) = pending_unpacked {
            design.unpacked_dims.push((id, dims_vec));
        }

        design.add_variable(VarDecl::new(
            decl.name.clone(),
            ScopePath::root().child(decl.name.clone()),
            data_type,
            var_kind,
            id,
        ));
    }

    // Nama tipe didaftarkan setelah semua deklarasi, supaya bentrok nama dengan
    // variabel (LRM §8.20) benar-benar terlihat — `insert_type` menolak nama
    // yang sudah dipakai sinyal, dan hanya berlaku untuk nama polos (tanpa
    // prefix instans). Nilai awal di bawah memakai cast, jadi pendaftaran ini
    // harus sebelumnya.
    daftarkan_tipe(symbols, module, typedefs_global, params, packages, "")?;

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
    crate::local_decl::daftarkan(module, design, symbols, params, &types, packages)?;

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

/// Statistik design untuk perintah `svrust inspect`.
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
