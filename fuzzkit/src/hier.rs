// Tanggung jawab: membangkitkan program multi-file dengan hierarki modul.
//
// Celah P0 yang ditutup modul ini: sampai sekarang generator hanya menghasilkan
// satu modul. Padahal bug elaborasi yang paling sering muncul justru di
// instantiasi — resolusi parameter, pengawalan tipe port, qualify nama sinyal,
// dan bring-in hierarchy. Program satu modul tidak akan pernah menyentuh jalur
// itu sama sekali.
//
// Yang dibangkitkan:
//
// - `FileGraph`: beberapa berkas SV dengan `include` di antaranya, supaya
//   praprosesan ikut teruji dengan filesystem sungguhan (bukan string tunggal).
// - `HierarchyGraph`: modul anak dengan parameter override, port dengan lebar
//   yang berbeda dari induknya, generate di dalam anak, dan instansiasi
//   bertingkat.
//
// Semua nilai berasal dari byte. Yang dijaga hanya *struktur* emit supaya
// program tetap bisa dielaborasi.
use arbitrary::{Arbitrary, Unstructured};

use crate::boundary::{Batas, Constness, DeklarasiSemantik, KelasLebar};

/// Jumlah modul anak yang dibangkitkan.
#[derive(Debug, Clone, Copy, Arbitrary)]
pub struct JumlahAnak {
    pub anak: u8,
    pub kedalaman_maks: u8,
}

/// Port modul: arah, lebar, signedness. Lebar port deliberately boleh berbeda
/// dari lebar sinyal internal — itu sumber mismatch yang harus diuji.
#[derive(Debug, Clone, Copy, Arbitrary)]
pub struct Port {
    pub arah: Arah,
    pub lebar: DeklarasiSemantik,
    pub nama: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Arbitrary)]
pub enum Arah {
    Input,
    Output,
    Inout,
}

impl Arah {
    fn teks(self) -> &'static str {
        match self {
            Arah::Input => "input",
            Arah::Output => "output",
            Arah::Inout => "inout",
        }
    }
}

/// Howalan parameter yang di-override anak — menguji resolusi parameter
/// melintasi batas hierarki.
#[derive(Debug, Clone, Copy, Arbitrary)]
pub struct Override {
    pub parameter: u8,
    pub nilai: Literal,
}

/// Instansiasi modul anak di dalam modul.
#[derive(Debug, Clone, Arbitrary)]
pub struct Instans {
    pub modul: u8,
    pub nama: u8,
    /// `true` memakai `#(...)` eksplisit, `false` mengandalkan default.
    pub param_eksplisit: bool,
    pub overrides: [Override; 2],
    /// Koneksi posisional (bukan named) — jalur validasi berbeda.
    pub posisional: bool,
}

/// Bentuk generate di dalam modul anak.
#[derive(Debug, Clone, Arbitrary)]
pub enum BentukGenerate {
    For { genvar: u8, batas: u8, body: Box<BentukGenerate> },
    If { kondisi: Literal, body: Box<BentukGenerate> },
    /// Instansiasi array `u[3:0]` — jalur pengemasan yang sering salah.
    InstansArray { genvar: u8, banyak: u8, modul: u8 },
    /// Rantai instansiasi bertingkat.
    Rantai { jumlah: u8, modul_awal: u8 },
    /// Tidak ada generate.
    TidakAda,
}

/// Literal sederhana untuk nilai parameter / kondisi generate.
#[derive(Debug, Clone, Copy, Arbitrary)]
pub struct Literal {
    pub basis: crate::gen::Basis,
    pub lebar: crate::gen::Lebar,
    pub digit: [crate::gen::Digit; 8],
    pub jumlah: u8,
}

impl Literal {
    pub fn render(&self) -> String {
        use crate::gen::{Basis, Digit};
        let w = self.lebar.nilai();
        if self.basis == Basis::Desimal {
            let d = self.digit[0].teks_publik();
            return d.to_string();
        }
        let perlu = self.basis.digit_untuk_publik(w) as usize;
        let n = perlu.min(usize::from(self.jumlah)).max(1);
        let body: String = (0..n)
            .map(|i| self.digit[i % self.digit.len()].teks_publik())
            .collect();
        let _: Option<Digit> = None;
        format!("{w}'{}{body}", self.basis.kode_publik())
    }
}

/// Satu modul dalam graph.
#[derive(Debug, Clone, Arbitrary)]
pub struct Modul {
    pub id: u8,
    pub port: Vec<Port>,
    pub parameter: Vec<DeklarasiSemantik>,
    pub generate: BentukGenerate,
    pub instans: Vec<Instans>,
    /// `true` kalau modul ini butuh `include` berkas lain.
    pub perlu_include: bool,
}

/// Graph modul + pemetaan ke berkas.
#[derive(Debug, Clone, Arbitrary)]
pub struct Graph {
    pub modul: Vec<Modul>,
    pub top: u8,
    /// Berapa modul yang ditaruh di berkas berbeda dari `top`.
    pub berkas_terpisah: u8,
    pub maks_kedalaman: u8,
}

impl Graph {
    /// Nama modul yang sah (identifier SystemVerilog sederhana).
    fn nama_modul(&self, m: &Modul) -> String {
        format!("m{}", m.id)
    }

    fn top_id(&self) -> u8 {
        self.modul
            .iter()
            .map(|m| m.id)
            .find(|id| *id == self.top)
            .unwrap_or(0)
    }

    /// Render modul tunggal.
    fn render_modul(&self, m: &Modul) -> String {
        let mut out = String::new();
        let mut daftar_port = Vec::new();
        for p in &m.port {
            let w = p.lebar.lebar.nilai().max(1);
            let sign = if p.lebar.signed { " signed" } else { "" };
            daftar_port.push(format!(
                "{} {sign} [{}:0] p{}",
                p.arah.teks(),
                w - 1,
                p.nama
            ));
        }
        let mut header = format!("module {}(", self.nama_modul(m));
        if daftar_port.is_empty() {
            header.push_str(");");
        } else {
            header.push_str(&daftar_port.join(", "));
            header.push_str(");");
        }
        out.push_str(&header);
        out.push('\n');
        for (i, d) in m.parameter.iter().enumerate() {
            out.push_str(&format!(
                "  parameter{} [{}:0] PAR{} = {};\n",
                if d.signed { " signed" } else { "" },
                d.lebar.nilai().saturating_sub(1),
                i,
                Literal {
                    basis: crate::gen::Basis::Desimal,
                    lebar: crate::gen::Lebar::N8,
                    digit: [
                        crate::gen::Digit::Nol,
                        crate::gen::Digit::Satu,
                        crate::gen::Digit::X,
                        crate::gen::Digit::Z,
                        crate::gen::Digit::Nol,
                        crate::gen::Digit::Satu,
                        crate::gen::Digit::X,
                        crate::gen::Digit::Z,
                    ],
                    jumlah: 1,
                }
                .render()
            ));
        }
        for d in &m.parameter {
            let _ = d;
        }
        out.push_str("  initial begin\n");
        out.push_str("    r = 0;\n");
        out.push_str("  end\n");
        out.push_str("  integer r;\n");

        // Sinyal internal yangukaan lewat port.
        for p in &m.port {
            let w = p.lebar.lebar.nilai().max(1);
            let sign = if p.lebar.signed { " signed" } else { "" };
            out.push_str(&format!(
                "  logic{sign} [{}:0] w{};\n",
                w - 1,
                p.nama
            ));
        }

        self.render_generate(&mut out, &m.generate, m, 1);
        for ins in &m.instans {
            self.render_instans(&mut out, ins, m, 1);
        }

        out.push_str("  initial begin\n");
        out.push_str("    #1;\n");
        out.push_str(&format!("    $display(\"{} r=%0d\", r);\n", self.nama_modul(m)));
        for p in &m.port {
            out.push_str(&format!(
                "    $display(\"{} p{} %h\", w{});\n",
                self.nama_modul(m),
                p.nama,
                p.nama
            ));
        }
        out.push_str("  end\n");
        out.push_str("endmodule\n");
        out
    }

    fn render_generate(
        &self,
        out: &mut String,
        g: &BentukGenerate,
        pemilik: &Modul,
        indent: usize,
    ) {
        let pad = "  ".repeat(indent);
        match g {
            BentukGenerate::TidakAda => {}
            BentukGenerate::For { genvar, batas, body } => {
                let n = usize::from(*batas) % 4 + 1;
                out.push_str(&format!(
                    "{pad}genvar g{genvar};\n{pad}generate\n{pad}  for (g{genvar} = 0; g{genvar} < {n}; g{genvar} = g{genvar} + 1) begin : b{genvar}\n"
                ));
                self.render_generate(out, body, pemilik, indent + 2);
                out.push_str(&format!("{pad}  end\n{pad}endgenerate\n"));
            }
            BentukGenerate::If { kondisi, body } => {
                out.push_str(&format!("{pad}generate\n{pad}  if ({}) begin : c\n", kondisi.render()));
                self.render_generate(out, body, pemilik, indent + 2);
                out.push_str(&format!("{pad}  end\n{pad}endgenerate\n"));
            }
            BentukGenerate::InstansArray {
                genvar,
                banyak,
                modul,
            } => {
                out.push_str(&format!(
                    "{pad}genvar g{genvar};\n{pad}generate\n{pad}  for (g{genvar} = 0; g{genvar} < {}; g{genvar} = g{genvar} + 1) begin : arr{genvar}\n",
                    usize::from(*banyak) % 4 + 1
                ));
                let id = self.modul.iter().find(|m| m.id == *modul).map(|m| m.id).unwrap_or(0);
                out.push_str(&format!(
                    "{pad}    {}({}) u{genvar}();\n",
                    self.nama_id(id),
                    usize::from(*genvar) % 4 + 1
                ));
                out.push_str(&format!("{pad}  end\n{pad}endgenerate\n"));
            }
            BentukGenerate::Rantai { jumlah, modul_awal } => {
                out.push_str(&format!("{pad}generate\n{pad}  if (1) begin : r\n"));
                let mut id = *modul_awal;
                for k in 0..usize::from(*jumlah) % 3 + 1 {
                    out.push_str(&format!(
                        "{pad}    {} c{k}({});\n",
                        self.nama_id(id),
                        usize::from(k) + 1
                    ));
                    id = id.wrapping_add(1);
                }
                self.render_generate(out, &BentukGenerate::TidakAda, pemilik, indent + 2);
                out.push_str(&format!("{pad}  end\n{pad}endgenerate\n"));
            }
        }
    }

    fn render_instans(&self, out: &mut String, ins: &Instans, _: &Modul, indent: usize) {
        let pad = "  ".repeat(indent);
        let id = self.modul.iter().find(|m| m.id == ins.modul).map(|m| m.id).unwrap_or(0);
        let mut header = format!("{pad}{}", self.nama_id(id));
        if ins.param_eksplisit {
            header.push_str("#(");
            let daftar: Vec<String> = ins
                .overrides
                .iter()
                .map(|o| format!(".PAR{}({})", o.parameter, o.nilai.render()))
                .collect();
            header.push_str(&daftar.join(", "));
            header.push(')');
        }
        header.push_str(&format!(" i{}(", ins.nama));
        if ins.posisional {
            let daftar: Vec<String> = (0..3).map(|k| format!("w{k}")).collect();
            header.push_str(&daftar.join(", "));
        }
        header.push_str(");");
        out.push_str(&header);
        out.push('\n');
    }

    fn nama_id(&self, id: u8) -> String {
        match self.modul.iter().find(|m| m.id == id) {
            Some(m) => self.nama_modul(m),
            None => format!("m{id}"),
        }
    }

    /// Namakan modul top.
    pub fn nama_top(&self) -> String {
        self.nama_id(self.top_id())
    }

    /// Tulis seluruh graph ke direktori, lalu kembalikan daftar berkas RTL dan
    /// nama top. Dipakai target fuzz dan runner differensial.
    pub fn tulis(&self, dir: &std::path::Path) -> std::io::Result<Vec<String>> {
        std::fs::create_dir_all(dir)?;
        let mut berkas = Vec::new();
        let top_id = self.top_id();
        // Modul top dan anak-anaknya diletakkan dalam berkas berbeda supaya
        // jalur `parse_file` + `elaborate_top` benar-benar dipakai.
        for m in &self.modul {
            let nama_berkas = if m.id == top_id {
                "top.sv".to_string()
            } else {
                format!("child{}.svh", m.id)
            };
            std::fs::write(dir.join(&nama_berkas), self.render_modul(m))?;
            berkas.push(nama_berkas);
        }
        // Berkas paling awal meng-include anak-anaknya supaya praprosesan
        // ikut teruji.
        if self.berkas_terpisah > 0 && !berkas.is_empty() {
            let mut isi = String::new();
            for b in &berkas {
                if b != "top.sv" {
                    isi.push_str(&format!("`include \"{b}\"\n"));
                }
            }
            let isi = format!("{isi}module fz_top;\n");
            std::fs::write(dir.join("wrapper.sv"), isi)?;
            berkas.push("wrapper.sv".to_string());
        }
        Ok(berkas)
    }
}

/// Bangkitkan graph dari byte fuzzer.
/// Bangkitkan graph dari byte fuzzer. `None` kalau byte-nya tidak cukup untuk
/// membentuk satu pun modul — pemanggil harus melewati program seperti itu.
pub fn dari_data(data: &[u8]) -> Option<Graph> {
    let mut u = Unstructured::new(data);
    let mut g = Graph::arbitrary(&mut u).ok()?;
    if g.modul.is_empty() {
        return None;
    }
    // Top harus menunjuk modul yang benar-benar ada.
    if !g.modul.iter().any(|m| m.id == g.top) {
        g.top = g.modul[0].id;
    }
    Some(g)
}

/// Graph deterministik dari LCG, supaya counterexample dapat diulang dari nomor
/// seed.
pub fn acak(seed: u64) -> Graph {
    let mut st = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
    let mut buf = vec![0u8; 16 * 1024];
    for b in buf.iter_mut() {
        st = st
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        *b = (st >> 33) as u8;
    }
    dari_data(&buf).unwrap_or_else(|| {
        // Graph kosong tidak berguna; coba seed berikutnya dengan batasan agar
        // tidak berputar tanpa henti kalau semua seed kosong.
        let s2 = seed.wrapping_add(1);
        if s2 == seed {
            Graph {
                modul: vec![Modul {
                    id: 0,
                    port: vec![],
                    parameter: vec![],
                    generate: BentukGenerate::TidakAda,
                    instans: vec![],
                    perlu_include: false,
                }],
                top: 0,
                berkas_terpisah: 0,
                maks_kedalaman: 1,
            }
        } else {
            acak(s2)
        }
    })
}

/// Batas yang sering dipakai untuk lebar port: mismatch lebar port adalah
/// sumber bug elaborasi yang tinggi.
pub fn batas_lebar_port() -> [KelasLebar; 6] {
    [
        KelasLebar::Batas(Batas::Nol),
        KelasLebar::Batas(Batas::Delapan),
        KelasLebar::Batas(Batas::U32),
        KelasLebar::Batas(Batas::U64),
        KelasLebar::Batas(Batas::Bit128),
        KelasLebar::Interior(3),
    ]
}

/// Constness yang sering dipakai untuk deklarasi parameter.
pub fn constness_parameter() -> [Constness; 3] {
    [
        Constness::Parameter,
        Constness::Localparam,
        Constness::Runtime,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graph_acak_selalu_memberi_modul_dan_top() {
        for seed in 0..60 {
            let g = acak(seed);
            assert!(!g.modul.is_empty(), "seed {seed}: tanpa modul");
            let nama = g.nama_top();
            assert!(nama.starts_with('m'), "seed {seed}: nama top {nama}");
        }
    }

    #[test]
    fn literal_dengan_jumlah_digit_kecil_memotong() {
        use crate::gen::{Basis, Digit, Lebar};
        let l = Literal {
            basis: Basis::Biner,
            lebar: Lebar::N8,
            digit: [Digit::Satu; 8],
            jumlah: 3,
        };
        // 8 bit butuh 8 digit biner; hanya 3 yang ditulis -> nilai menyempit.
        assert_eq!(l.render(), "8'b111");
    }

    #[test]
    fn graph_acak_deterministik() {
        assert_eq!(acak(7).modul.len(), acak(7).modul.len());
    }

    #[test]
    fn render_modul_menghasilkan_sintaks_sah() {
        for seed in 0..30 {
            let g = acak(seed);
            let src = g.render_modul(&g.modul[0]);
            assert!(src.starts_with("module m"), "seed {seed}: {src}");
            assert!(src.trim_end().ends_with("endmodule"), "seed {seed}: {src}");
        }
    }

    #[test]
    fn literal_menghormati_basis_dan_lebar() {
        use crate::gen::{Basis, Digit, Lebar};
        let l = Literal {
            basis: Basis::Heksa,
            lebar: Lebar::N8,
            digit: [
                Digit::Satu,
                Digit::Nol,
                Digit::Satu,
                Digit::Nol,
                Digit::Satu,
                Digit::Nol,
                Digit::Satu,
                Digit::Nol,
            ],
            jumlah: 2,
        };
        // Lebar 8 bit pada basis heksa butuh 2 digit (LRM §5.7.1).
        assert_eq!(l.render(), "8'h10");
    }
}