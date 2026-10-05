// Tanggung jawab: tabel tipe hasil `typedef` beserta nilai literal enum.
use crate::error::ElaborateError;
use crate::param::ParamTable;
use std::collections::HashMap;
use sv_ast::expression::Expr;
use sv_ast::typedef::{TypeDefBody, TypeDefDecl};
use sv_ast::width::WidthExpr;
use sv_lexer::span::Span;

/// Tipe hasil `typedef` yang sudah di-resolve ke lebar konkret.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TypeInfo {
    pub width: u32,
    pub signed: bool,
}

/// Tabel nama tipe beserta nilai literal enumerasi.
///
/// Dua hal yang diselesaikan di sini (LRM §8.20 dan §6.7):
/// nama tipe menjadi lebar/signedness konkret, dan nama anggota enum menjadi
/// nilai konstanta yang boleh dipakai di ekspresi.
pub struct TypeTable {
    tipe: HashMap<String, (TypeInfo, Span)>,
    /// Nilai konstanta tiap nama anggota enum beserta lebar tipe induknya.
    literal: HashMap<String, (u64, u32, Span)>,
}

impl TypeTable {
    /// Bangun tabel dari daftar `typedef`.
    ///
    /// Nilai anggota enum dihitung berurutan mulai dari 0 (LRM §6.7): anggota
    /// tanpa nilai eksplisit memakai nilai anggota sebelumnya ditambah satu.
    pub fn baru(decls: &[TypeDefDecl], params: &ParamTable) -> Result<Self, ElaborateError> {
        let mut tipe = HashMap::new();
        let mut literal: HashMap<String, (u64, u32, Span)> = HashMap::new();

        for decl in decls {
            if tipe.contains_key(&decl.name) {
                return Err(ElaborateError::new(
                    format!("duplicate typedef '{}'", decl.name),
                    decl.span,
                ));
            }
            match &decl.body {
                TypeDefBody::Scalar { width, signed } => {
                    let bit = params.lebar(width, decl.span)? as u32;
                    tipe.insert(
                        decl.name.clone(),
                        (
                            TypeInfo {
                                width: bit,
                                signed: *signed,
                            },
                            decl.span,
                        ),
                    );
                }
                TypeDefBody::Enum {
                    base_width,
                    members,
                } => {
                    let bit = params.lebar(base_width, decl.span)? as u32;
                    // LRM §6.7: nilai enum harus muat dalam lebar tipe dasar.
                    let maks = if bit >= 64 {
                        u64::MAX
                    } else {
                        (1u64 << bit) - 1
                    };
                    let mut berikut = 0u64;
                    for member in members {
                        let nilai = match &member.value {
                            Some(expr) => nilai_konstan(expr, member.span)?,
                            None => berikut,
                        };
                        if nilai > maks {
                            return Err(ElaborateError::invalid_width(
                                format!(
                                    "nilai enum '{}' = {} melebihi lebar tipe {} bit",
                                    member.name, nilai, bit
                                ),
                                member.span,
                            ));
                        }
                        if literal.contains_key(&member.name) {
                            return Err(ElaborateError::new(
                                format!("duplicate enum member '{}'", member.name),
                                member.span,
                            ));
                        }
                        literal.insert(member.name.clone(), (nilai, bit, member.span));
                        berikut = nilai.saturating_add(1);
                    }
                    tipe.insert(
                        decl.name.clone(),
                        (
                            TypeInfo {
                                width: bit,
                                signed: false,
                            },
                            decl.span,
                        ),
                    );
                }
            }
        }
        Ok(Self { tipe, literal })
    }

    /// True bila tabel kosong; pemanggil boleh melewati resolve tipe.
    pub fn is_empty(&self) -> bool {
        self.tipe.is_empty()
    }

    /// Ambil tipe hasil typedef; error bila nama tak dikenal.
    pub fn tipe(&self, nama: &str, span: Span) -> Result<TypeInfo, ElaborateError> {
        self.tipe
            .get(nama)
            .map(|(info, _)| *info)
            .ok_or_else(|| ElaborateError::new(format!("undefined type '{nama}'"), span))
    }

    /// Seluruh nama tipe beserta lebarnya; dipakai untuk mendaftarkan target
    /// type cast ke tabel simbol.
    pub fn iter_tipe(&self) -> impl Iterator<Item = (&str, TypeInfo)> + '_ {
        self.tipe
            .iter()
            .map(|(nama, (info, _))| (nama.as_str(), *info))
    }

    /// Ambil nilai literal enum beserta lebar tipe induknya.
    pub fn literal(&self, nama: &str) -> Option<(u64, u32)> {
        self.literal
            .get(nama)
            .map(|(nilai, lebar, _)| (*nilai, *lebar))
    }

    /// Substitusi literal enum pada ekspresi AST menjadi konstanta.
    ///
    /// Dipanggil sekali per module sebelum statement dielaborasi, sehingga
    /// `lower_expression` tidak perlu tahu soal enumerasi.
    pub fn ganti_literal(&self, expr: &Expr) -> Expr {
        match expr {
            // Nama enum yang tidak ada nilainya sendiri ikut disubstitusi,
            // mis. `IDLE` di dalam `[IDLE, RUN]` pada penugasan variabel.
            Expr::Ident { name, span: _ } => match self.literal(name) {
                Some((nilai, lebar)) => Expr::Sized {
                    value: nilai,
                    width: lebar,
                    signed: false,
                    unknown_mask: 0,
                    zmask: 0,
                },
                None => expr.clone(),
            },
            Expr::Number(_) | Expr::Sized { .. } | Expr::SystemTime { .. } => expr.clone(),
            // LRM §6.14: nama tipe sendiri bukan literal enum, jadi tidak
            // disentuh; hanya operandnya yang didalami ulang.
            Expr::Cast {
                type_name,
                operand,
                span,
            } => Expr::Cast {
                type_name: type_name.clone(),
                operand: Box::new(self.ganti_literal(operand)),
                span: *span,
            },
            Expr::SignCast {
                signed,
                operand,
                span,
            } => Expr::SignCast {
                signed: *signed,
                operand: Box::new(self.ganti_literal(operand)),
                span: *span,
            },
            Expr::Bits { operand, span } => Expr::Bits {
                operand: Box::new(self.ganti_literal(operand)),
                span: *span,
            },
            // LRM §6.14: cast ke tipe bawaan dan size cast hanya butuh operandnya
            // yang didalami ulang; nama tipe dan lebarnya sudah final.
            Expr::BuiltinCast {
                type_name,
                width,
                signed,
                operand,
                span,
            } => Expr::BuiltinCast {
                type_name: type_name.clone(),
                width: *width,
                signed: *signed,
                operand: Box::new(self.ganti_literal(operand)),
                span: *span,
            },
            Expr::SizeCast {
                width,
                operand,
                span,
            } => Expr::SizeCast {
                width: *width,
                operand: Box::new(self.ganti_literal(operand)),
                span: *span,
            },
            Expr::IndexDynamic { base, index, span } => Expr::IndexDynamic {
                base: Box::new(self.ganti_literal(base)),
                index: Box::new(self.ganti_literal(index)),
                span: *span,
            },
            Expr::Binary { op, lhs, rhs, span } => Expr::Binary {
                op: *op,
                lhs: Box::new(self.ganti_literal(lhs)),
                rhs: Box::new(self.ganti_literal(rhs)),
                span: *span,
            },
            Expr::Unary { op, operand, span } => Expr::Unary {
                op: *op,
                operand: Box::new(self.ganti_literal(operand)),
                span: *span,
            },
            Expr::Ternary {
                condition,
                when_true,
                when_false,
                span,
            } => Expr::Ternary {
                condition: Box::new(self.ganti_literal(condition)),
                when_true: Box::new(self.ganti_literal(when_true)),
                when_false: Box::new(self.ganti_literal(when_false)),
                span: *span,
            },
            Expr::Select {
                base,
                msb,
                lsb,
                span,
            } => Expr::Select {
                base: Box::new(self.ganti_literal(base)),
                msb: *msb,
                lsb: *lsb,
                span: *span,
            },
            Expr::Concat { items, span } => Expr::Concat {
                items: items.iter().map(|i| self.ganti_literal(i)).collect(),
                span: *span,
            },
            Expr::Replicate { count, value, span } => Expr::Replicate {
                count: Box::new(self.ganti_literal(count)),
                value: Box::new(self.ganti_literal(value)),
                span: *span,
            },
            Expr::FunctionCall { name, args, span } => Expr::FunctionCall {
                name: name.clone(),
                args: args.iter().map(|a| self.ganti_literal(a)).collect(),
                span: *span,
            },
        }
    }
}

/// Hitung nilai konstanta untuk anggota enum (LRM §6.7).
///
/// Hanya literal dan operasi aritmetika/bitwise yang diizinkan; rujukan
/// parameter atau sinyal ditolak karena nilainya belum tentu konstan.
fn nilai_konstan(expr: &Expr, span: Span) -> Result<u64, ElaborateError> {
    use sv_ast::expression::{BinaryOp, UnaryOp};
    let hitung = |e: &Expr| -> Result<u64, ElaborateError> { nilai_konstan(e, span) };
    Ok(match expr {
        Expr::Number(v) => *v,
        Expr::Sized { value, .. } => *value,
        Expr::Unary { op, operand, .. } => {
            let v = hitung(operand)?;
            match op {
                UnaryOp::BitNot => !v,
                UnaryOp::BitNeg => v.wrapping_neg(),
                UnaryOp::LogNot => u64::from(v == 0),
                UnaryOp::RedAnd
                | UnaryOp::RedOr
                | UnaryOp::RedXor
                | UnaryOp::RedNand
                | UnaryOp::RedNor
                | UnaryOp::RedXnor => {
                    return Err(ElaborateError::new(
                        "operator reduksi belum didukung pada nilai enum",
                        span,
                    ))
                }
            }
        }
        Expr::Binary { op, lhs, rhs, .. } => {
            let a = hitung(lhs)?;
            let b = hitung(rhs)?;
            match op {
                BinaryOp::Add => a.wrapping_add(b),
                BinaryOp::Sub => a.wrapping_sub(b),
                BinaryOp::Mul => a.wrapping_mul(b),
                BinaryOp::Div => a.checked_div(b).unwrap_or(0),
                BinaryOp::Mod => a.checked_rem(b).unwrap_or(0),
                BinaryOp::Shl => a.checked_shl(b as u32).unwrap_or(0),
                BinaryOp::Shr => a.checked_shr(b as u32).unwrap_or(0),
                // LRM §11.4.10: `>>>` mengisi bit sign.
                BinaryOp::Sar => ((a as i64) >> (b as u32)) as u64,
                BinaryOp::And => a & b,
                BinaryOp::Or => a | b,
                BinaryOp::Xor => a ^ b,
                BinaryOp::Eq => u64::from(a == b),
                BinaryOp::NotEq => u64::from(a != b),
                BinaryOp::Lt => u64::from(a < b),
                BinaryOp::Gt => u64::from(a > b),
                BinaryOp::Le => u64::from(a <= b),
                BinaryOp::Ge => u64::from(a >= b),
                BinaryOp::LogAnd => u64::from(a != 0 && b != 0),
                BinaryOp::LogOr => u64::from(a != 0 || b != 0),
            }
        }
        other => {
            return Err(ElaborateError::new(
                format!("nilai enum harus konstanta, dapat ekspresi {:?}", other),
                span,
            ))
        }
    })
}

/// Resolusi lebar deklarasi yang memakai tipe hasil typedef.
///
/// Lebar dari typedef bisa diperkecil oleh dimension eksplisit pada
/// deklarasi (`byte_t [3:0] nib;`). Width `Literal(1)` dari parser berarti
/// "tidak menulis dimension", jadi lebar typedef dipakai utuh.
pub fn lebar_deklarasi(
    width: &WidthExpr,
    type_name: Option<&str>,
    table: &TypeTable,
    params: &ParamTable,
    span: Span,
) -> Result<(u32, bool), ElaborateError> {
    let base = match type_name {
        Some(nama) => table.tipe(nama, span)?,
        None => {
            let bit = params.lebar(width, span)? as u32;
            return Ok((bit, false));
        }
    };
    // LRM §7.3: `t [N] w;` dengan `typedef logic [7:0] t;` adalah packed
    // array N elemen bertipe `t`, jadi lebarnya N x lebar tipe dasar — bukan
    // "persempit" dan bukan error saat N melebihi lebar dasar.
    //
    // Bentuk lama memperlakukannya sebagai penyempitan dan menolak N > lebar
    // dasar, sehingga kode SV yang sah gagal dielaborasi. `byte_t [3:0] nib;`
    // pun sebelumnya jadi 4 bit, padahal seharusnya 4 x 8 = 32 bit.
    let jumlah = match width {
        // `Literal(1)` berarti "tidak ada dimension": dipakai sebagai
        // default ketika parser tidak menemukan `[...]`.
        WidthExpr::Literal(1) => None,
        other => Some(params.lebar(other, span)? as u32),
    };
    match jumlah {
        None => Ok((base.width, base.signed)),
        Some(n) => {
            let total = base.width.checked_mul(n).ok_or_else(|| {
                ElaborateError::invalid_width(
                    format!("lebar packed array {} x {} bit meluap", n, base.width),
                    span,
                )
            })?;
            Ok((total, base.signed))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sv_ast::typedef::EnumMember;
    use sv_ast::width::ParamDecl;

    fn params() -> ParamTable {
        ParamTable::baru(&[ParamDecl {
            name: "W".to_string(),
            default: 8,
            span: Span::dummy(),
        }])
        .expect("params")
    }

    fn scalar(nama: &str, width: WidthExpr, signed: bool) -> TypeDefDecl {
        TypeDefDecl {
            name: nama.to_string(),
            body: TypeDefBody::Scalar { width, signed },
            span: Span::dummy(),
        }
    }

    fn enum_decl(nama: &str, base: WidthExpr, members: Vec<(&str, Option<Expr>)>) -> TypeDefDecl {
        TypeDefDecl {
            name: nama.to_string(),
            body: TypeDefBody::Enum {
                base_width: base,
                members: members
                    .into_iter()
                    .map(|(n, v)| EnumMember {
                        name: n.to_string(),
                        value: v,
                        span: Span::dummy(),
                    })
                    .collect(),
            },
            span: Span::dummy(),
        }
    }

    #[test]
    fn typedef_skalar_menyimpan_lebar() {
        let t = TypeTable::baru(&[scalar("byte_t", WidthExpr::Literal(8), false)], &params())
            .expect("tabel");
        let info = t.tipe("byte_t", Span::dummy()).expect("tipe");
        assert_eq!(info.width, 8);
        assert!(!info.signed);
    }

    #[test]
    fn typedef_berparameter_terresolve() {
        let t = TypeTable::baru(
            &[scalar(
                "w_t",
                WidthExpr::ParamMinusOne("W".to_string()),
                false,
            )],
            &params(),
        )
        .expect("tabel");
        assert_eq!(t.tipe("w_t", Span::dummy()).expect("tipe").width, 8);
    }

    #[test]
    fn typedef_tak_dikenal_ditolak() {
        let t = TypeTable::baru(&[], &params()).expect("tabel");
        assert!(t.tipe("hilang_t", Span::dummy()).is_err());
    }

    #[test]
    fn typedef_ganda_ditolak() {
        let decls = vec![
            scalar("a_t", WidthExpr::Literal(4), false),
            scalar("a_t", WidthExpr::Literal(8), false),
        ];
        assert!(TypeTable::baru(&decls, &params()).is_err());
    }

    #[test]
    fn enum_bernilai_urutan_mulai_nol() {
        let t = TypeTable::baru(
            &[enum_decl(
                "e_t",
                WidthExpr::Literal(3),
                vec![("A", None), ("B", None), ("C", None)],
            )],
            &params(),
        )
        .expect("tabel");
        assert_eq!(t.literal("A"), Some((0, 3)));
        assert_eq!(t.literal("B"), Some((1, 3)));
        assert_eq!(t.literal("C"), Some((2, 3)));
    }

    #[test]
    fn enum_nilai_eksplisit_menggeser_urutan_setelahnya() {
        let t = TypeTable::baru(
            &[enum_decl(
                "e_t",
                WidthExpr::Literal(4),
                vec![("A", None), ("B", Some(Expr::Number(5))), ("C", None)],
            )],
            &params(),
        )
        .expect("tabel");
        assert_eq!(t.literal("A"), Some((0, 4)));
        assert_eq!(t.literal("B"), Some((5, 4)));
        // LRM §6.7: anggota setelah nilai eksplisit melanjutkan dari situ.
        assert_eq!(t.literal("C"), Some((6, 4)));
    }

    #[test]
    fn enum_boleh_pakai_ekspresi_konstan() {
        let nilai = Expr::Binary {
            op: sv_ast::expression::BinaryOp::Shl,
            lhs: Box::new(Expr::Number(2)),
            rhs: Box::new(Expr::Number(3)),
            span: Span::dummy(),
        };
        let t = TypeTable::baru(
            // `2 << 3` = 16, jadi lebar 5 bit dipakai agar nilainya muat.
            &[enum_decl(
                "e_t",
                WidthExpr::Literal(5),
                vec![("A", Some(nilai))],
            )],
            &params(),
        )
        .expect("tabel");
        assert_eq!(t.literal("A"), Some((16, 5)));
    }

    #[test]
    fn enum_nilai_melebih_lebar_ditolak() {
        let t = TypeTable::baru(
            &[enum_decl(
                "e_t",
                WidthExpr::Literal(2),
                vec![("A", Some(Expr::Number(9)))],
            )],
            &params(),
        );
        assert!(t.is_err());
    }

    #[test]
    fn enum_anggota_ganda_ditolak() {
        let t = TypeTable::baru(
            &[enum_decl(
                "e_t",
                WidthExpr::Literal(2),
                vec![("A", None), ("A", None)],
            )],
            &params(),
        );
        assert!(t.is_err());
    }

    #[test]
    fn enum_nilai_bukan_konstan_ditolak() {
        let t = TypeTable::baru(
            &[enum_decl(
                "e_t",
                WidthExpr::Literal(2),
                vec![("A", Some(Expr::ident("x", Span::dummy())))],
            )],
            &params(),
        );
        assert!(t.is_err());
    }

    #[test]
    fn literal_enum_disubstitusi_menjadi_konstanta() {
        let t = TypeTable::baru(
            &[enum_decl(
                "e_t",
                WidthExpr::Literal(2),
                vec![("IDLE", None), ("RUN", None)],
            )],
            &params(),
        )
        .expect("tabel");
        let hasil = t.ganti_literal(&Expr::ident("RUN", Span::dummy()));
        assert_eq!(
            hasil,
            Expr::Sized {
                value: 1,
                width: 2,
                signed: false,
                unknown_mask: 0,
                zmask: 0,
            }
        );
    }

    #[test]
    fn literal_enum_substitusi_di_seluruh_sub_ekspresi() {
        let t = TypeTable::baru(
            &[enum_decl("e_t", WidthExpr::Literal(2), vec![("RUN", None)])],
            &params(),
        )
        .expect("tabel");
        let expr = Expr::Binary {
            op: sv_ast::expression::BinaryOp::Add,
            lhs: Box::new(Expr::ident("RUN", Span::dummy())),
            rhs: Box::new(Expr::Number(1)),
            span: Span::dummy(),
        };
        match t.ganti_literal(&expr) {
            Expr::Binary { lhs, .. } => {
                assert_eq!(
                    *lhs,
                    Expr::Sized {
                        value: 0,
                        width: 2,
                        signed: false,
                        unknown_mask: 0,
                        zmask: 0,
                    }
                );
            }
            other => panic!("harus binary, dapat {other:?}"),
        }
    }

    #[test]
    fn nama_bukan_literal_enum_tetap_dipertahankan() {
        let t = TypeTable::baru(&[], &params()).expect("tabel");
        let hasil = t.ganti_literal(&Expr::ident("sinyal", Span::dummy()));
        assert_eq!(hasil, Expr::ident("sinyal", Span::dummy()));
    }

    #[test]
    fn deklarasi_bertipe_tanpa_dimension_pakai_lebar_typedef() {
        let t = TypeTable::baru(&[scalar("byte_t", WidthExpr::Literal(8), false)], &params())
            .expect("tabel");
        let (lebar, signed) = lebar_deklarasi(
            &WidthExpr::Literal(1),
            Some("byte_t"),
            &t,
            &params(),
            Span::dummy(),
        )
        .expect("lebar");
        assert_eq!(lebar, 8);
        assert!(!signed);
    }

    #[test]
    fn deklarasi_bertipe_tanpa_dimension_pakai_lebar_typedef_baru() {
        let t = TypeTable::baru(&[scalar("byte_t", WidthExpr::Literal(8), false)], &params())
            .expect("tabel");
        // Tanpa `[...]` lebarnya persis lebar typedef.
        let (lebar, _) = lebar_deklarasi(
            &WidthExpr::Literal(1),
            Some("byte_t"),
            &t,
            &params(),
            Span::dummy(),
        )
        .expect("lebar");
        assert_eq!(lebar, 8);
    }

    #[test]
    fn deklarasi_bertipe_dengan_dimension_adalah_packed_array() {
        // LRM §7.3: `byte_t [3:0] nib;` adalah packed array 4 elemen bertipe
        // `byte_t`, jadi 4 x 8 = 32 bit — bukan "persempit" ke 4 bit seperti
        // implementasi lama, dan juga bukan error seperti pada N > lebar dasar.
        let t = TypeTable::baru(&[scalar("byte_t", WidthExpr::Literal(8), false)], &params())
            .expect("tabel");
        let (lebar, _) = lebar_deklarasi(
            &WidthExpr::Literal(4),
            Some("byte_t"),
            &t,
            &params(),
            Span::dummy(),
        )
        .expect("lebar");
        assert_eq!(lebar, 32);
    }

    #[test]
    fn deklarasi_bertipe_dimension_lebih_besar_dari_typedef_diperbolehkan() {
        // BUG-10: `nib_t [7:0] w;` dengan `typedef logic [3:0] nib_t` adalah
        // packed array 8 elemen = 32 bit dan sah secara LRM. Implementasi lama
        // menolaknya karenaley's Lebanon新浪娱乐../../ membandingkan langsung
        // dengan lebar tipe dasar, jadi desain RTL yang sah gagal total.
        let t = TypeTable::baru(&[scalar("nib_t", WidthExpr::Literal(4), false)], &params())
            .expect("tabel");
        let (lebar, _) = lebar_deklarasi(
            &WidthExpr::Literal(8),
            Some("nib_t"),
            &t,
            &params(),
            Span::dummy(),
        )
        .expect("packed array sah, tidak boleh error");
        assert_eq!(lebar, 32);
    }

    #[test]
    fn deklarasi_bertipe_dimension_nol_ditolak() {
        // `byte_t [0]` berarti packed array tanpa elemen. `ParamTable::lebar`
        // sudah menolak lebar di bawah 1 sebelum perkalian terjadi.
        let t = TypeTable::baru(&[scalar("byte_t", WidthExpr::Literal(8), false)], &params())
            .expect("tabel");
        let hasil = lebar_deklarasi(
            &WidthExpr::Literal(0),
            Some("byte_t"),
            &t,
            &params(),
            Span::dummy(),
        );
        assert!(hasil.is_err());
    }

    #[test]
    fn deklarasi_tanpa_typedef_pakai_lebar_tertulis() {
        let t = TypeTable::baru(&[], &params()).expect("tabel");
        let (lebar, _) =
            lebar_deklarasi(&WidthExpr::Literal(6), None, &t, &params(), Span::dummy())
                .expect("lebar");
        assert_eq!(lebar, 6);
    }
}
