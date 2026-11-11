// Tanggung jawab: node AST untuk deklarasi `typedef` dan `typedef enum`.
use serde::{Deserialize, Serialize};
use crate::expression::Expr;
use crate::width::WidthExpr;
use sv_lexer::span::Span;

/// Satu field dalam `struct packed` (LRM §6.7).
///
/// Lebar field dihitung dari `width` (dan `type_name` bila field memakai tipe
/// hasil typedef). SVRust meratakan `struct packed` menjadi sinyal bit-vector
/// selebar jumlah lebar semua field, sehingga yang penting adalah lebar total.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StructFieldDef {
    pub name: String,
    pub width: WidthExpr,
    pub signed: bool,
    /// Nama tipe hasil typedef bila field bukan tipe atom (mis. `byte_t f;`).
    /// `None` untuk tipe atom seperti `logic`/`bit`.
    pub type_name: Option<String>,
}

/// Bentuk tipe yang sedang di-`typedef`.
///
/// LRM §8.20 `typedef` menyalin tipe yang sudah ada; LRM §6.7 menambah
/// enumerasi; LRM §6.7/`struct packed` ditambahkan untuk dukungan paket
/// OpenTitan (mis. `prim_alert_pkg::alert_tx_t`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TypeDefBody {
    /// `typedef logic [7:0] byte_t;` — alias tipe biasa.
    Scalar { width: WidthExpr, signed: bool },
    /// `typedef enum logic [1:0] { IDLE, RUN } state_t;` (LRM §6.7).
    Enum {
        /// Lebar tipe dasar enumerasi; default 1 bit per LRM §6.7.
        base_width: WidthExpr,
        members: Vec<EnumMember>,
    },
    /// `typedef struct packed { logic a; logic b; } ab_t;` (LRM §6.7).
    ///
    /// Di-meratakan menjadi sinyal bit-vector selebar jumlah lebar field.
    Struct { fields: Vec<StructFieldDef> },
}

/// Satu anggota enumerasi (LRM §6.7).
///
/// Nilai kosong berarti mengikuti anggota sebelumnya, mulai dari 0.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnumMember {
    pub name: String,
    pub value: Option<Expr>,
    pub span: Span,
}

/// Satu deklarasi `typedef` di body modul.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TypeDefDecl {
    /// Nama tipe baru yang dihasilkan.
    pub name: String,
    pub body: TypeDefBody,
    pub span: Span,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typedef_skalar_tidak_punya_anggota() {
        let decl = TypeDefDecl {
            name: "byte_t".to_string(),
            body: TypeDefBody::Scalar {
                width: WidthExpr::Literal(8),
                signed: false,
            },
            span: Span::dummy(),
        };
        assert!(matches!(decl.body, TypeDefBody::Scalar { .. }));
    }

    #[test]
    fn typedef_enum_menyimpan_anggota_dan_nilai_kosong() {
        let decl = TypeDefDecl {
            name: "state_t".to_string(),
            body: TypeDefBody::Enum {
                base_width: WidthExpr::Literal(2),
                members: vec![
                    EnumMember {
                        name: "IDLE".to_string(),
                        value: None,
                        span: Span::dummy(),
                    },
                    EnumMember {
                        name: "RUN".to_string(),
                        value: Some(Expr::Number(3)),
                        span: Span::dummy(),
                    },
                ],
            },
            span: Span::dummy(),
        };
        match decl.body {
            TypeDefBody::Enum { members, .. } => {
                assert_eq!(members.len(), 2);
                assert!(members[0].value.is_none());
                assert_eq!(members[1].value, Some(Expr::Number(3)));
            }
            other => panic!("harus enum, dapat {other:?}"),
        }
    }
}
