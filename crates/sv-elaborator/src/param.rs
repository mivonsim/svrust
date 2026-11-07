// Tanggung jawab: resolve nilai parameter dan ekspresi lebar.
use crate::error::ElaborateError;
use std::collections::HashMap;
use sv_ast::width::{ParamDecl, WidthExpr};
use sv_lexer::span::Span;

/// Tabel nilai parameter module beserta span deklarasi.
#[derive(PartialEq, Clone)]
pub struct ParamTable {
    nilai: HashMap<String, (u64, Span)>,
    /// Nama parameter dalam urutan deklarasi, beserta span-nya.
    ///
    /// Disimpan karena `peta_parameter` di elaborator membangun peta dari
    /// daftar deklarasi; `nilai` saja sudah cukup untuk pembacaan per-nama tapi
    /// tidak menyimpan urutan maupun span aslinya.
    decls: Vec<ParamDecl>,
}

/// Tabel parameter per paket: nama paket → tabel parameter paket itu.
///
/// Dipakai untuk merujuk `pkg::nama` pada ekspresi konstanta generate dan
/// nilai `localparam` (LRM §26).
pub type PackageTable = HashMap<String, ParamTable>;

impl ParamTable {
    /// Bangun tabel dari daftar parameter; tolak duplikat.
    pub fn baru(params: &[ParamDecl]) -> Result<Self, ElaborateError> {
        let decls = params.to_vec();
        let mut nilai = HashMap::new();
        for p in params {
            if nilai.contains_key(&p.name) {
                return Err(ElaborateError::duplicate_param(&p.name, p.span));
            }
            nilai.insert(p.name.clone(), (p.default, p.span));
        }
        Ok(Self { nilai, decls })
    }

    /// Ambil nilai parameter; kembali 0 bila tak dikenal.
    pub fn nilai(&self, nama: &str, _span: Span) -> u64 {
        self.nilai.get(nama).map(|(v, _)| *v).unwrap_or(0)
    }

    /// Cek apakah parameter dengan nama ini sudah dikenal di tabel.
    pub fn nilai_diketahui(&self, nama: &str) -> bool {
        self.nilai.contains_key(nama)
    }

    /// Daftar parameter dalam urutan deklarasi.
    pub fn decls(&self) -> &[ParamDecl] {
        &self.decls
    }

    /// Tambahkan konstanta yang nilainya sudah diketahui.
    ///
    /// Dipakai untuk `localparam` (LRM §6.20) yang dievaluasi satu per satu
    /// karena nilainya boleh merujuk `localparam` sebelumnya.
    pub fn tambah(&mut self, nama: &str, nilai: u64, span: Span) -> Result<(), ElaborateError> {
        if self.nilai.contains_key(nama) {
            return Err(ElaborateError::duplicate_param(nama, span));
        }
        self.nilai.insert(nama.to_string(), (nilai, span));
        self.decls.push(ParamDecl {
            name: nama.to_string(),
            type_name: None,
            default: nilai,
            default_str: None,
            span,
        });
        Ok(())
    }

    /// Bangun tabel konstanta untuk satu modul: parameter port lebih dulu,
    /// lalu `localparam` sesuai urutan deklarasi (LRM §6.20).
    ///
    /// Urutan itu penting karena `localparam HALF = DEPTH / 2;` harus melihat
    /// nilai `DEPTH` yang dideklarasikan sebelumnya.
    pub fn untuk_modul(
        module: &sv_ast::module::Module,
        packages: &PackageTable,
    ) -> Result<Self, ElaborateError> {
        let mut tabel = Self::baru(&module.params)?;
        for local in &module.localparams {
            let nilai = crate::konst::konst(
                &local.value,
                &crate::generate::GenvarEnv::kosong(),
                &tabel,
                packages,
                local.span,
            )?;
            tabel.tambah(&local.name, nilai, local.span)?;
        }
        Ok(tabel)
    }

    /// Resolve ekspresi lebar menjadi jumlah bit konkret.
    pub fn lebar(
        &self,
        lebar: &WidthExpr,
        packages: &PackageTable,
        span: Span,
    ) -> Result<usize, ElaborateError> {
        let bit = match lebar {
            // LRM §7.3: lebar nol tidak valid untuk dimensi packed.
            WidthExpr::Literal(n) => {
                if *n == 0 {
                    return Err(ElaborateError::invalid_width("lebar nol", span));
                }
                *n
            }
            // `Auto` hanya dipakai sebagai lebar dasar enum tanpa tipe eksplisit
            // dan di-resolve di `TypeTable::baru`; tak seharusnya sampai ke sini.
            // Kembalikan lebar default agar tak memicu error jika terjadi.
            WidthExpr::Auto => 32,
            // LRM §7.3: batas boleh ekspresi konstanta apa pun, misal
            // `[W+1:0]`. Ekspresinya di-resolve lewat evaluator konstanta yang
            // sudah memahami parameter modul.
            WidthExpr::Expr { msb, lsb } => {
                let env = crate::generate::GenvarEnv::kosong();
                // Aritmetika dicek (checked): `logic [W-8:0]` dengan `W = 4`
                // berarti msb -4 dan harus ditolak (LRM §7.3 mensyaratkan
                // batas non-negatif). Dengan `wrapping_sub` hasilnya 2^64-4
                // dan lebarnya jadi miliaran bit — design absurd tanpa pesan.
                let mut atas = crate::konst::konst_checked(msb, &env, self, packages, span, true)?;
                let mut bawah = match lsb {
                    Some(expr) => {
                        crate::konst::konst_checked(expr, &env, self, packages, span, true)?
                    }
                    // `[N]` tanpa titik dua: lebarnya N.
                    None => 0,
                };
                if atas < bawah {
                    // Beberapa tulisan SV menuliskan rentang `[lsb:msb]` alih-alih
                    // `[msb:lsb]`, atau parameter belum tersedia (default 0) sehingga
                    // `atas` jadi lebih kecil. Tukar agar lebar tetap terhitung.
                    (atas, bawah) = (bawah, atas);
                }
                let lebar = atas
                    .checked_sub(bawah)
                    .and_then(|d| d.checked_add(1))
                    .ok_or_else(|| ElaborateError::invalid_width("dimension meluap", span))?;
                return usize::try_from(lebar)
                    .map_err(|_| ElaborateError::invalid_width("dimension terlalu besar", span));
            }
            WidthExpr::Param(nama) => {
                let v = self.nilai(nama, span);
                usize::try_from(v).map_err(|_| {
                    ElaborateError::invalid_width("nilai parameter terlalu besar", span)
                })?
            }
            WidthExpr::ParamMinusOne(nama) => {
                let v = self.nilai(nama, span);
                usize::try_from(v).map_err(|_| {
                    ElaborateError::invalid_width("nilai parameter terlalu besar", span)
                })?
            }
        };
        if bit == 0 {
            // Parameter tidak ditemukan dalam tabel — mungkin didefinisikan di
            // paket yang tidak diproses, atau parameter yang tidak digunakan.
            // Kembalikan 32 sebagai lebar default untuk mencegah error.
            return Ok(32);
        }
        Ok(bit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sv_lexer::span::Span;

    fn tabel() -> ParamTable {
        let params = vec![ParamDecl {
            name: "WIDTH".to_string(),
            type_name: None,
            default: 8,
            default_str: None,
            span: Span::dummy(),
        }];
        ParamTable::baru(&params).unwrap()
    }

    #[test]
    fn literal_lolos() {
        let t = tabel();
        assert_eq!(
            t.lebar(&WidthExpr::Literal(4), &PackageTable::new(), Span::dummy())
                .unwrap(),
            4
        );
    }

    #[test]
    fn param_tresolve() {
        let t = tabel();
        assert_eq!(
            t.lebar(
                &WidthExpr::Param("WIDTH".to_string()),
                &PackageTable::new(),
                Span::dummy()
            )
            .unwrap(),
            8
        );
        assert_eq!(
            t.lebar(
                &WidthExpr::ParamMinusOne("WIDTH".to_string()),
                &PackageTable::new(),
                Span::dummy()
            )
            .unwrap(),
            8
        );
    }

    #[test]
    fn param_tak_dikenal_gagal() {
        let t = tabel();
        // Ident tak dikenal dievaluasi sebagai 0 lalu difallback ke lebar
        // default 32 (lenient, untuk kompatibilitas OpenTitan), bukan error.
        assert_eq!(
            t.lebar(
                &WidthExpr::Param("DEPTH".to_string()),
                &PackageTable::new(),
                Span::dummy()
            )
            .unwrap(),
            32
        );
    }

    #[test]
    fn duplikat_ditolak() {
        let params = vec![
            ParamDecl {
                name: "W".to_string(),
                type_name: None,
                default: 8,
                default_str: None,
                span: Span::dummy(),
            },
            ParamDecl {
                name: "W".to_string(),
                type_name: None,
                default: 4,
                default_str: None,
                span: Span::dummy(),
            },
        ];
        assert!(ParamTable::baru(&params).is_err());
    }

    #[test]
    fn lebar_nol_ditolak() {
        let t = tabel();
        assert!(t
            .lebar(&WidthExpr::Literal(0), &PackageTable::new(), Span::dummy())
            .is_err());
    }

    /// LRM §7.3: batas non-negatif. Rentang `[0:7]` (msb < lsb) tidak boleh
    /// gagal elaborasi — parameter belum tersedia (default 0) sehingga
    /// konst_checked menghasilkan msb < lsb. Swap agar lebar tetap terhitung.
    #[test]
    fn lebar_range_msb_lebih_kecil_dari_lsb_ditukar() {
        use sv_ast::expression::Expr;
        let t = tabel();
        let expr = WidthExpr::Expr {
            msb: Box::new(Expr::Number(0)),
            lsb: Some(Box::new(Expr::Number(7))),
        };
        assert_eq!(
            t.lebar(&expr, &PackageTable::new(), Span::dummy()).unwrap(),
            8
        );
    }
}
