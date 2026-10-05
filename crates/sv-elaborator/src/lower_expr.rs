// Tanggung jawab: lowering ekspresi AST menjadi ekspresi IR ter-resolve.
use crate::error::ElaborateError;
use crate::symbol::SymbolTable;
use crate::width::infer_binary;
use sv_ast::expression::{BinaryOp, Expr as AstExpr, UnaryOp as AstUnOp};
use sv_ir::{BinOp, DataType, Expr, SignalId, UnOp};

pub fn lower_expression(expr: &AstExpr, symbols: &SymbolTable) -> Result<Expr, ElaborateError> {
    lower_expression_konteks(expr, symbols, 0)
}

/// Lower ekspresi dengan lebar konteks dari luar.
///
/// `lebar_konteks` adalah ukuran yang diminta oleh konteks pemakaian — lebar
/// target pada batas assignment (LRM §11.6.1 Tabel 11-21) atau lebar ekspresi
/// induk untuk operand yang context-determined. Nilai `0` berarti "tanpa
/// konteks", jadi ukuran ekspresi sepenuhnya self-determined.
///
/// Kenapa harus meneruskan ke dalam, bukan hanya membungkus hasil: aturan
/// LRM §11.6.1 menyatakan ukuran ekspresi adalah max(ukuran konteks, ukuran
/// self-determined), lalu **operand** diprioritaskan ke ukuran itu. Kalau hanya
/// hasilnya yang dibungkus, `logic [31:0] z; logic [7:0] y; z = y + y;`
/// akan menjumlahkan dua operand 8-bit (0x4A) lalu Alive-kan ke 32 bit,
/// sedangkan LRM menjumlahkan pada 32 bit (0x14A).
pub fn lower_expression_konteks(
    expr: &AstExpr,
    symbols: &SymbolTable,
    lebar_konteks: u32,
) -> Result<Expr, ElaborateError> {
    match expr {
        AstExpr::Ident { name, span } => {
            let symbol = symbols
                .lookup(name)
                .ok_or_else(|| ElaborateError::undefined_signal(name, *span))?;
            Ok(Expr::signal_ref(symbol.signal_id, symbol.data_type))
        }
        // Literal desimal polos: LRM §5.7.1 menetapkan lebar 32 bit, dan
        // Tabel 5-22 menetapkan tipe `integer` yang **signed**. Signedness ini
        // bukan detail: `operand_signed` pada cast diambil dari sini, jadi
        // `word_t'(-1)` tanpa tanda akan salah zero-extend dan menghasilkan
        // 0x00000000FFFFFFFF alih-alih 0xFFFFFFFFFFFFFFFF.
        AstExpr::Number(value) => Ok(Expr::constant(*value, DataType::signed(32))),
        // Based literal memakai lebar eksplisitnya sendiri (LRM §5.7.1).
        // `x` dan `z` dibawa terpisah supaya nilainya benar-benar 4-state.
        AstExpr::Sized {
            value,
            width,
            signed,
            unknown_mask,
            zmask,
        } => Ok(Expr::constant_four_state(
            *value,
            DataType::logic(*width).with_signed(*signed),
            *unknown_mask,
            *zmask,
        )),
        AstExpr::Binary { op, lhs, rhs, .. } => {
            let bin_op = map_binary_op(*op);
            // LRM §11.6.1 Tabel 11-21: untuk `i << j` hanya `j` yang
            // self-determined; `i` ikut context-determined. Mematikan konteks
            // untuk keduanya membuat `sa << 4` dengan `sa` signed 8-bit
            // kehilangan sign-extension-nya.
            let (lowered_lhs, lowered_rhs) = if bin_op.is_shift() {
                (
                    lower_expression_konteks(lhs, symbols, lebar_konteks)?,
                    lower_expression_konteks(rhs, symbols, 0)?,
                )
            } else {
                (
                    lower_expression_konteks(lhs, symbols, lebar_konteks)?,
                    lower_expression_konteks(rhs, symbols, lebar_konteks)?,
                )
            };
            let tipe_kiri = lowered_lhs.data_type();
            let tipe_kanan = lowered_rhs.data_type();
            let data_type = infer_binary(bin_op, tipe_kiri, tipe_kanan);
            // Ukuran ekspresi = max(ukuran konteks, ukuran self-determined).
            // Konteks `0` berarti tidak ada konteks. Untuk geser lebarnya
            // mengikuti operand kiri; konteks luar tidak mengembangkannya.
            let lebar_ekspresi = match bin_op {
                op2 if op2.is_shift() => tipe_kiri.width.max(lebar_konteks),
                _ => lebar_konteks.max(data_type.width),
            };
            // LRM §11.6.1 langkah 3: setiap operand context-determined harus
            // bertipe dan selebar sama dengan hasil operator. Jadi operand
            // diprioritaskan ke lebar **dan signedness hasil**, bukan
            // signedness-nya sendiri: `sa + 16'd1` hasilnya unsigned
            // (Tabel 11-22), jadi `sa` di-zero-extend. Memakai signedness
            // operand menghasilkan 0xFFFFFFFE, bukan 0x000000FE.
            //
            // Perbandingan dan geser tidak butuh wrapper: keduanya sudah
            // membaca nilai sepanjang lebar logis masing-masing operand di
            // codegen, jadi membungkus operand hanya menambah node mati —
            // dan karena `max_width` ikut menghitung lebar tipe node itu,
            // menaikkan lebar penyimpanan design tanpa alasan (`y == 0` dengan
            // `y` 8-bit memaksa `MAX_WIDTH` jadi 32 hanya karena literal `0`).
            // Geser tetap perlu resize operand kiri; hanya operand kanannya
            // yang self-determined.
            // LRM §11.6.1 langkah 3: operand context-determined dikonversi ke
            // tipe HASIL, jadi arah perluasannya ikut signedness hasil —
            // `sa + 16'd1` hasilnya unsigned sehingga `sa` di-zero-extend
            // (0x000000FE, bukan 0xFFFFFFFE).
            //
            // Geser hanya operand kirinya yang context-determined; operand
            // kanannya self-determined (LRM §11.6.1 Tabel 11-21).
            let ke_tipe_hasil =
                |e: Expr| sesuaikan_lebar(e, lebar_ekspresi, data_type.signed, data_type.signed);
            let (lhs, rhs) = match bin_op {
                op2 if op2.is_shift() => (ke_tipe_hasil(lowered_lhs), lowered_rhs),
                op2 if op2.is_context_determined() && !op2.is_relational() => {
                    (ke_tipe_hasil(lowered_lhs), ke_tipe_hasil(lowered_rhs))
                }
                _ => (lowered_lhs, lowered_rhs),
            };
            Ok(Expr::Bin {
                op: bin_op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
                data_type: data_type.with_width(lebar_ekspresi),
            })
        }
        AstExpr::Unary { op, operand, .. } => {
            let lowered = lower_expression(operand, symbols)?;
            let un_op = map_unary_op(*op);
            // Reduksi menghasilkan 1 bit; operator lain mempertahankan lebar.
            let data_type = if un_op.is_reduction() {
                DataType::bit()
            } else {
                lowered.data_type()
            };
            Ok(Expr::Un {
                op: un_op,
                operand: Box::new(lowered),
                data_type,
            })
        }
        AstExpr::Ternary {
            condition,
            when_true,
            when_false,
            ..
        } => {
            let lowered_cond = lower_expression(condition, symbols)?;
            // LRM §11.6.1 Tabel 11-21: pada `i ? j : k` hanya `i` yang
            // self-determined; `j` dan `k` context-determined. Tanpa ini
            // cabang yang sempit dan signed kehilangan sign-extension-nya.
            let lowered_true = lower_expression_konteks(when_true, symbols, lebar_konteks)?;
            let lowered_false = lower_expression_konteks(when_false, symbols, lebar_konteks)?;
            // LRM §11.4.11: lebar hasil = max kedua cabang, signed hanya bila
            // keduanya signed.
            let data_type =
                crate::width::infer_ternary(lowered_true.data_type(), lowered_false.data_type());
            let lebar_ekspresi = lebar_konteks.max(data_type.width);
            // Sama seperti operand biner: cabang diprioritaskan ke tipe
            // *hasil*, bukan tipe cabangnya sendiri (LRM §11.6.1 langkah 3).
            // Cabang `? :` juga context-determined, jadi dikonversi ke tipe
            // hasil (LRM §11.6.1 langkah 3).
            let (ketika, salah) = (
                sesuaikan_lebar(
                    lowered_true,
                    lebar_ekspresi,
                    data_type.signed,
                    data_type.signed,
                ),
                sesuaikan_lebar(
                    lowered_false,
                    lebar_ekspresi,
                    data_type.signed,
                    data_type.signed,
                ),
            );
            Ok(Expr::Ternary {
                condition: Box::new(lowered_cond),
                when_true: Box::new(ketika),
                when_false: Box::new(salah),
                data_type: data_type.with_width(lebar_ekspresi),
            })
        }
        AstExpr::Select { base, msb, lsb, .. } => {
            // LRM §7.8: `mem[k]` pada array unpacked mengakses elemen ke-`k`.
            // Sinyalnya disimpan rata, jadi indeks dipetakan ke rentang bit
            // elemennya. Indeks di luar jangkauan adalah error, bukan dipotong
            // diam-diam.
            if let AstExpr::Ident { name, span } = &**base {
                if let Some(symbol) = symbols.lookup(name) {
                    if symbol.unpacked.is_some() {
                        // LRM §7.8: range select pada array unpacked — `mem[3:1]`
                        // — membaca 3 elemen berurutan, bukan hanya elemen `msb`.
                        // Jalur ini dulu mengabaikan `lsb` sepenuhnya sehingga
                        // hasilnya 8 bit (satu elemen) dengan lebar data_type
                        // yang keliru dan tanpa pesan apa pun.
                        if lsb != msb {
                            return Err(ElaborateError::new(
                                format!(
                                    "range select pada array unpacked belum didukung: \
                                     `{name}[{msb}:{lsb}]` pada {}",
                                    symbol
                                        .unpacked
                                        .as_ref()
                                        .map(|i| i.dim.describe())
                                        .unwrap_or_default()
                                ),
                                *span,
                            ));
                        }
                        let (msb_bit, lsb_bit) = symbol.elemen(u64::from(*msb)).ok_or_else(|| {
                            ElaborateError::new(
                                format!(
                                    "indeks {} di luar jangkauan array '{}' {}",
                                    msb,
                                    name,
                                    symbol
                                        .unpacked
                                        .as_ref()
                                        .map(|i| i.dim.describe())
                                        .unwrap_or_default()
                                ),
                                *span,
                            )
                        })?;
                        return Ok(Expr::Select {
                            base: Box::new(Expr::signal_ref(
                                symbol.signal_id,
                                symbol.data_type,
                            )),
                            msb: msb_bit,
                            lsb: lsb_bit,
                            data_type: DataType::logic(msb_bit - lsb_bit + 1)
                                .with_signed(symbol.data_type.signed),
                        });
                    }
                }
            }
            let lowered_base = lower_expression(base, symbols)?;
            let span = span_expr(base);
            if lsb > msb {
                return Err(ElaborateError::new(
                    format!("part-select tidak valid: [{}:{}]", msb, lsb),
                    span,
                ));
            }
            // LRM §7.8: seleksi di luar lebar operand menghasilkan `X`, bukan
            // nol. Helper runtime `select_kbits` menjaga terhadap kapasitas
            // vektor Rust (`MAX_WIDTH`), yang selalu lebih besar dari lebar
            // logis sinyal — jadi penjaga itu tidak pernah kena untuk indeks
            // konstan di luar jangkauan. sintesis keluar salah di sini secara
            // diam-diam, jadi indeks dicek di elaborasi dan ditolak.
            // Lebar base diambil dari hasil lowering, bukan dari bentuk AST-nya:
            // `mem[2]` pada array unpacked adalah `Select` dengan
            // `msb - lsb + 1` yang TIDAK sama dengan lebar elemennya, jadi
            // menghitung dari AST akan menolak kode sah. `lowered_base`
            // sudah membawa lebar yang benar untuk semua bentuk — sinyal,
            // seleksi, indeks dinamis, dan cast.
            let lebar = lowered_base.data_type().width;
            if u64::from(*msb) >= u64::from(lebar) {
                return Err(ElaborateError::new(
                    format!(
                        "bit/part-select [{}:{}] di luar lebar operand ({} bit)",
                        msb, lsb, lebar
                    ),
                    span,
                ));
            }
            let data_type = DataType::logic(msb - lsb + 1);
            Ok(Expr::Select {
                base: Box::new(lowered_base),
                msb: *msb,
                lsb: *lsb,
                data_type,
            })
        }
        // LRM §7.8: indeks dinamis `a[i]`. Semantiknya ditentukan dari base LANGSUNG,
        // bukan dari simbol yang dicari secara rekursif: `mem[2][k]` berarti baca
        // bit `k` dari elemen `mem[2]` — base-nya sudah diekstrak jadi skalar
        // 8 bit, jadi ini bit-select biasa. Kalau simbol `mem` (yang punya
        // `unpacked`) yang dicari rekursif, `flat_element` jadi `true` dan
        // hasilnya salah senyap.
        AstExpr::IndexDynamic { base, index, span } => {
            let lowered_base = lower_expression(base, symbols)?;
            let lowered_index = lower_expression(index, symbols)?;
            // LRM §7.8: dua semantik berbeda di balik sintaks yang sama.
            // Sinyal packed → bit-select, elemennya SATU bit di posisi `i`.
            // Array unpacked → elemen ke-`i` yang disimpan rata, lebarnya
            // lebar elemen. Dulu keduanya memakai lebar sinyal packed sehingga
            // `a[idx]` terbaca sebagai elemen array dan menggeser `idx * 8`
            // bit — selalu di luar jangkauan, hasilnya nol.
            let flat_element = match &**base {
                AstExpr::Ident { name, .. } => {
                    symbols.lookup(name).is_some_and(|s| s.unpacked.is_some())
                }
                _ => false,
            };
            // Lebar elemen: untuk array unpacked ini lebar SATU elemen, bukan
            // lebar total vektor rata (`lowered_base` sudah selebar
            // `elem * size`). Bit-select packed selalu 1 bit.
            let elem = match &**base {
                AstExpr::Ident { name, .. } => symbols
                    .lookup(name)
                    .filter(|s| s.unpacked.is_some())
                    .map_or(1, |s| s.bits_width()),
                _ => 1,
            };
            // Indeks yang bisa dilipat ke konstanta diperiksa di sini: `mem[-1]`
            // adalah error compile-time, bukan indeks yang menghasilkan `X` saat
            // simulasi berjalan.
            if let AstExpr::Ident { name, .. } = &**base {
                if let Some(symbol) = symbols.lookup(name) {
                    if let Ok(konstan) = crate::konst::konst(
                        index,
                        &crate::generate::GenvarEnv::kosong(),
                        &crate::param::ParamTable::baru(&[])?,
                        *span,
                    ) {
                        cek_indeks_konstan(symbol, konstan, *span)?;
                    }
                }
            }
            // LRM §11.8.1: hasil bit-select dan part-select SELALU unsigned, apa pun
            // signedness base-nya. Elemen array unpacked bukan seleksi — nilainya
            // adalah variabel bertipe yang diturunkan dari deklarasi, jadi
            // signedness-nya ikut terbawa.
            let data_type = if flat_element {
                DataType::logic(elem).with_signed(lowered_base.data_type().signed)
            } else {
                DataType::bit()
            };
            Ok(Expr::Index {
                base: Box::new(lowered_base),
                index: Box::new(lowered_index),
                data_type,
                flat_element,
            })
        }
        // LRM §11.8.1: lebar concatenation = jumlah lebar seluruh item.
        AstExpr::Concat { items, .. } => {
            let mut lowered = Vec::with_capacity(items.len());
            let mut lebar = 0u32;
            for item in items {
                let e = lower_expression(item, symbols)?;
                lebar = lebar.saturating_add(e.data_type().width);
                lowered.push(e);
            }
            if lowered.len() == 1 {
                // `{a}` sah tetapi tidak mengubah apa pun.
                let mut out = lowered;
                return Ok(out.remove(0));
            }
            Ok(Expr::Concat {
                items: lowered,
                data_type: DataType::logic(lebar),
            })
        }
        // LRM §11.8.2: lebar replication = jumlah x lebar operand.
        //
        // Count adalah ekspresi karena `{WIDTH{1'b1}}` dengan `WIDTH` parameter
        // modul adalah idiom RTL paling umum; standarnya hanya mensyaratkan
        // konstanta, jadi nilainya di-resolve di sini lewat tabel simbol
        // (nama parameter modul sudah jadi konstanta di `koneksi`).
        AstExpr::Replicate { count, value, span } => {
            let lowered = lower_expression(value, symbols)?;
            let count_nilai = crate::konst::konst(count, &crate::generate::GenvarEnv::kosong(), &crate::param::ParamTable::baru(&[])?, *span)?;
            let count_nilai = u32::try_from(count_nilai).map_err(|_| {
                ElaborateError::new("jumlah replikasi melebihi u32", *span)
            })?;
            if count_nilai == 0 {
                return Err(ElaborateError::new(
                    "jumlah replikasi harus >= 1",
                    *span,
                ));
            }
            let lebar = lowered
                .data_type()
                .width
                .checked_mul(count_nilai)
                .ok_or_else(|| ElaborateError::new("lebar replication meluap", *span))?;
            Ok(Expr::Replicate {
                count: count_nilai,
                value: Box::new(lowered),
                data_type: DataType::logic(lebar),
            })
        }
        // LRM §20: `$time` bertipe 64-bit karena waktu simulasi tidak
        // dibatasi lebar sinyal.
        AstExpr::SystemTime { .. } => Ok(Expr::SimTime {
            data_type: DataType::signed(64),
        }),
        // `expand_module` sudah mengembangkan setiap panggilan function sebelum
        // lowering. Node yang sampai ke sini berarti ada jalur yang melewatkan
        // pass tersebut, jadi dilaporkan sebagai error, bukan diabaikan diam-diam.
        AstExpr::FunctionCall { name, span, .. } => Err(ElaborateError::new(
            format!("function call '{name}' belum dikembangkan; badan function tidak bisa jadi ekspresi"),
            *span,
        )),
        // LRM §6.14: `nama_tipe'(ekspresi)` dievaluasi sebagai ekspresi sized
        // pada tipe tujuan — nilai operand dipertahankan sebanyak mungkin,
        // dipotong saat menyempit dan diisi (nol atau tanda) saat melebar.
        //
        // Arah perluasan ikut signedness OPERAND (`operand_signed`), bukan
        // signedness tipe tujuan: cast berarti "dievaluasi seolah operand
        // di-assign ke tipe tujuan", jadi yang berlaku adalah aturan
        // assignment LRM §6.2.1.
        AstExpr::Cast {
            type_name,
            operand,
            span,
        } => {
            let lowered = lower_expression(operand, symbols)?;
            let (width, signed) = symbols.type_info(type_name).ok_or_else(|| {
                // Nama tipe ter-prefix instans (`u0__byte_t`) adalah detail
                // internal hasil flattening. Pesan harus menyebut nama yang
                // benar-benar ditulis di source (AGENTS.md aturan 3).
                let ditulis = suffix_nama_tipe(type_name);
                ElaborateError::new(format!("undefined type '{ditulis}'"), *span)
            })?;
            Ok(Expr::Cast {
                operand_signed: lowered.data_type().signed,
                operand: Box::new(lowered),
                data_type: DataType::logic(width).with_signed(signed),
            })
        }
        // LRM §6.14: `signed'(x)` hanya mengubah signedness; lebarnya tetap,
        // jadi nilai bitnya sendiri tidak berubah — termasuk arah perluasan,
        // karena `operand_signed` tetap mengikuti operand asalnya.
        AstExpr::SignCast {
            signed,
            operand,
            span: _,
        } => {
            let lowered = lower_expression(operand, symbols)?;
            let width = lowered.data_type().width;
            Ok(Expr::Cast {
                operand_signed: lowered.data_type().signed,
                operand: Box::new(lowered),
                data_type: DataType::logic(width).with_signed(*signed),
            })
        }
        // LRM §6.14: cast ke tipe bawaan (`integer'(x)`, `byte'(x)`). Lebar dan
        // signedness sudah diketahui dari nama tipenya, jadi tidak perlu lookup
        // typedef; arah perluasan tetap mengikuti signedness OPERAND.
        AstExpr::BuiltinCast {
            width,
            signed,
            operand,
            ..
        } => {
            let lowered = lower_expression(operand, symbols)?;
            Ok(Expr::Cast {
                operand_signed: lowered.data_type().signed,
                operand: Box::new(lowered),
                data_type: DataType::logic(*width).with_signed(*signed),
            })
        }
        // LRM §6.14: `16'(x)` hanya mengubah lebar; signedness ikut operand.
        // Verifikasi iverilog: `16'(-8'sd3)` = 16'hFFFD, bukan 16'h00FD.
        AstExpr::SizeCast {
            width,
            operand,
            ..
        } => {
            let lowered = lower_expression(operand, symbols)?;
            let signed = lowered.data_type().signed;
            Ok(Expr::Cast {
                operand_signed: signed,
                operand: Box::new(lowered),
                data_type: DataType::logic(*width).with_signed(signed),
            })
        }
        // LRM §20: `$bits(x)` adalah konstanta waktu kompilasi, bukan nilai
        // runtime. Nilainya bertipe `integer` LRM, jadi 32 bit.
        AstExpr::Bits { operand, span } => {
            // BUG-5: LRM §20 mengizinkan `$bits(nama_tipe)` — dan justru itu
            // bentuk yang paling sering dipakai (`$bits(byte_t)`), karena nama
            // tipe bukan ekspresi nilai. Tanpa cabang ini, pesan errornya jadi
            // "undefined signal 'byte_t'" yang menyesatkan: yang salah nama
            // tipenya, bukan sinyalnya.
            if let AstExpr::Ident { name, .. } = &**operand {
                // LRM §20: `$bits` pada array unpacked mengembalikan lebar
                // ELEMEN, bukan lebar total. Sinyalnya sendiri disimpan rata
                // selebar `elemen * size`, jadi tanpa cabang ini hasilnya
                // kelihatan 8x lebih besar.
                if let Some(symbol) = symbols.lookup(name) {
                    let width = symbol.bits_width();
                    if width == 0 {
                        return Err(ElaborateError::invalid_width(
                            "$bits dari tipe tanpa bit",
                            *span,
                        ));
                    }
                    return Ok(Expr::constant(u64::from(width), DataType::logic(32)));
                }
                if let Some((width, _)) = symbols.type_info(name) {
                    if width == 0 {
                        return Err(ElaborateError::invalid_width(
                            "$bits dari tipe tanpa bit",
                            *span,
                        ));
                    }
                    return Ok(Expr::constant(u64::from(width), DataType::logic(32)));
                }
            }
            let lowered = lower_expression(operand, symbols)?;
            let width = lowered.data_type().width;
            if width == 0 {
                return Err(ElaborateError::invalid_width(
                    "$bits dari ekspresi tanpa bit",
                    *span,
                ));
            }
            Ok(Expr::constant(u64::from(width), DataType::logic(32)))
        }
    }
}

/// Nama tipe tanpa prefix instans, untuk pesan error.
///
/// Internal memakai `u0__byte_t`; user menulis `byte_t`. Prefix instans selalu
/// berakhir `__`, jadi memangkas sampai `__` terakhir mengembalikan nama asli.
fn suffix_nama_tipe(nama: &str) -> &str {
    match nama.rfind("__") {
        Some(pos) => &nama[pos + 2..],
        None => nama,
    }
}

/// Span untuk pesan error: pakai span base kalau ada (AGENTS.md aturan 3).
///
/// Match sengaja dibuat exhaustive: varian yang punya `span` tapi jatuh ke
/// `_` akan melaporkan `line 1 col 1` dan melanggar aturan 3.
pub fn span_expr(expr: &AstExpr) -> sv_lexer::span::Span {
    match expr {
        AstExpr::Ident { span, .. }
        | AstExpr::Binary { span, .. }
        | AstExpr::Unary { span, .. }
        | AstExpr::Ternary { span, .. }
        | AstExpr::Select { span, .. }
        | AstExpr::IndexDynamic { span, .. }
        | AstExpr::Concat { span, .. }
        | AstExpr::Replicate { span, .. }
        | AstExpr::FunctionCall { span, .. }
        | AstExpr::Cast { span, .. }
        | AstExpr::BuiltinCast { span, .. }
        | AstExpr::SizeCast { span, .. }
        | AstExpr::SignCast { span, .. }
        | AstExpr::Bits { span, .. } => *span,
        // `Number`, `Sized`, `SystemTime` tidak menyimpan span.
        AstExpr::Number(_) | AstExpr::Sized { .. } | AstExpr::SystemTime { .. } => {
            sv_lexer::Span::dummy()
        }
    }
}

/// Tolak indeks konstanta yang di luar jangkauan (LRM §7.8).
///
/// Bentuk ini bisa dicek waktu elaborasi; indeks yang benar-benar baru diketahui
/// saat simulasi berjalan ditangani helper runtime (hasilnya `X`).
fn cek_indeks_konstan(
    symbol: &crate::symbol::Symbol,
    index: u64,
    span: sv_lexer::span::Span,
) -> Result<(), ElaborateError> {
    let valid = match &symbol.unpacked {
        Some(info) => info.dim.offset(index).is_some(),
        None => index < u64::from(symbol.data_type.width),
    };
    if valid {
        return Ok(());
    }
    let jangkauan = match &symbol.unpacked {
        Some(info) => info.dim.describe(),
        None => format!("0..{}", symbol.data_type.width.saturating_sub(1)),
    };
    // `konst` menyimpan nilai dalam u64, jadi indeks negatif jadi angka
    // raksasa. Menampilkannya apa adanya akan membingungkan; sebut saja
    // bahwa indeksnya negatif.
    let teks = if index > u64::MAX / 2 {
        "indeks negatif".to_string()
    } else {
        format!("indeks {index}")
    };
    Err(ElaborateError::new(
        format!(
            "{teks} di luar jangkauan sinyal '{}' {jangkauan}",
            symbol.name
        ),
        span,
    ))
}

/// Sesuaikan lebar dan signedness nilai ke tipe konteks (LRM §11.6.1).
///
/// Dipakai di dua tempat dengan aturan yang sama:
///
/// - batas antar-operand operasi biner dan `?:`: operand di-prioritaskan ke
///   tipe hasil sebelum operasi dihitung;
/// - batas assignment: ekspresi kanan di-prioritaskan ke tipe target.
///
/// `signed_konteks` adalah signedness tipe tujuan — LRM §11.6.1 langkah 3
/// menyatakan operand context-determined harus bertipe sama dengan hasil
/// operator: `sa + 16'd1` menghasilkan tipe unsigned (Tabel 11-22), jadi
/// `sa` di-zero-extend; kalau tipe ikut operand hasilnya 0xFFFFFFFE dan keliru
/// dianggap negatif.
///
/// `signed_nilai` adalah signedness **nilai** itu sendiri, dan itulah yang
/// menentukan arah perluasan (LRM §6.2.1). Keduanya sengaja dipisah:
/// `assign y = a` dengan `a` signed 8-bit dan `y` unsigned 32-bit menghasilkan
/// `Cast { data_type: unsigned(32), operand_signed: true }` — tipe akhirnya
/// unsigned, tapi bitnya tetap sign-extend.
///
/// Lebar sama berarti tidak ada yang perlu diubah: `resize`/`sign_extend`
/// dengan lebar sumber = lebar tujuan adalah identitas.
pub fn sesuaikan_lebar(expr: Expr, lebar: u32, signed_konteks: bool, signed_nilai: bool) -> Expr {
    let tipe = expr.data_type();
    if tipe.width == lebar {
        return expr;
    }
    Expr::Cast {
        operand: Box::new(expr),
        data_type: tipe.with_width(lebar).with_signed(signed_konteks),
        operand_signed: signed_nilai,
    }
}

fn map_binary_op(op: BinaryOp) -> BinOp {
    match op {
        BinaryOp::And => BinOp::BitAnd,
        BinaryOp::Or => BinOp::BitOr,
        BinaryOp::Xor => BinOp::BitXor,
        BinaryOp::Add => BinOp::Add,
        BinaryOp::Sub => BinOp::Sub,
        BinaryOp::Mul => BinOp::Mul,
        BinaryOp::Div => BinOp::Div,
        BinaryOp::Mod => BinOp::Mod,
        BinaryOp::Shl => BinOp::Shl,
        BinaryOp::Shr => BinOp::Shr,
        BinaryOp::Sar => BinOp::Sar,
        BinaryOp::Eq => BinOp::Eq,
        BinaryOp::NotEq => BinOp::NotEq,
        BinaryOp::Lt => BinOp::Lt,
        BinaryOp::Gt => BinOp::Gt,
        BinaryOp::Le => BinOp::Le,
        BinaryOp::Ge => BinOp::Ge,
        BinaryOp::LogAnd => BinOp::LogAnd,
        BinaryOp::LogOr => BinOp::LogOr,
    }
}

fn map_unary_op(op: AstUnOp) -> UnOp {
    match op {
        AstUnOp::BitNot => UnOp::BitNot,
        AstUnOp::LogNot => UnOp::LogNot,
        AstUnOp::BitNeg => UnOp::BitNeg,
        AstUnOp::RedAnd => UnOp::RedAnd,
        AstUnOp::RedNand => UnOp::RedNand,
        AstUnOp::RedOr => UnOp::RedOr,
        AstUnOp::RedNor => UnOp::RedNor,
        AstUnOp::RedXor => UnOp::RedXor,
        AstUnOp::RedXnor => UnOp::RedXnor,
    }
}

/// Kumpulkan semua signal yang dibaca oleh ekspresi.
pub fn collect_signals(expr: &Expr, out: &mut Vec<SignalId>) {
    match expr {
        Expr::Const { .. } => {}
        Expr::SignalRef { signal, .. } => {
            if !out.contains(signal) {
                out.push(*signal);
            }
        }
        Expr::Bin { lhs, rhs, .. } => {
            collect_signals(lhs, out);
            collect_signals(rhs, out);
        }
        Expr::Un { operand, .. } => collect_signals(operand, out),
        Expr::Select { base, .. } => collect_signals(base, out),
        Expr::Ternary {
            condition,
            when_true,
            when_false,
            ..
        } => {
            collect_signals(condition, out);
            collect_signals(when_true, out);
            collect_signals(when_false, out);
        }
        Expr::Concat { items, .. } => {
            for item in items {
                collect_signals(item, out);
            }
        }
        Expr::Replicate { value, .. } => collect_signals(value, out),
        Expr::Index { base, index, .. } => {
            collect_signals(base, out);
            collect_signals(index, out);
        }
        Expr::Cast { operand, .. } => collect_signals(operand, out),
        // `$time` tidak membaca sinyal apa pun.
        Expr::SimTime { .. } => {}
    }
}
