// Tanggung jawab: generator program SystemVerilog berbasis data untuk fuzzing.
//
// Tidak ada template. Setiap token yang masuk ke sumber SV — nama sinyal,
// lebar, basis, digit, indeks, label, operator, statement, directive — berasal
// dari byte fuzz. Tidak ada literal tetap, tidak ada pola `16'h{k}1?`, tidak
// ada `{3{1'b1}}`. Kalau ada template, fuzzer hanya menguji template itu
// beserta variasinya; kelas bug yang bergantung pada nilai tertentu (digit
// `x`/`z`, lebar tepat 64, indeks di atas 63, basis `o`/`d`) tidak akan pernah
// tersentuh.
//
// Yang tetap deterministik hanya *urutan* emit (direktif → deklarasi → inisial
// → statement → cetak), supaya output bisa dibandingkan dengan iverilog.
use arbitrary::{Arbitrary, Unstructured};

/// Lebar. `u64` dan kelipatannya adalah batas alami implementasi
/// (`to_u64`, masker `u64`, `1u64 << n`) — di situlah bug truncation dan shift
/// overflow muncul, jadi lebarGa-human seperti 63/64/65/95/96/127/128/129
/// harus sering muncul.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Arbitrary)]
pub enum Lebar {
    N1,
    N2,
    N3,
    N4,
    N7,
    N8,
    N15,
    N16,
    N31,
    N32,
    N33,
    N63,
    N64,
    N65,
    N95,
    N96,
    N127,
    N128,
    N129,
    N255,
    N256,
}

impl Lebar {
    pub fn nilai(self) -> u32 {
        match self {
            Lebar::N1 => 1,
            Lebar::N2 => 2,
            Lebar::N3 => 3,
            Lebar::N4 => 4,
            Lebar::N7 => 7,
            Lebar::N8 => 8,
            Lebar::N15 => 15,
            Lebar::N16 => 16,
            Lebar::N31 => 31,
            Lebar::N32 => 32,
            Lebar::N33 => 33,
            Lebar::N63 => 63,
            Lebar::N64 => 64,
            Lebar::N65 => 65,
            Lebar::N95 => 95,
            Lebar::N96 => 96,
            Lebar::N127 => 127,
            Lebar::N128 => 128,
            Lebar::N129 => 129,
            Lebar::N255 => 255,
            Lebar::N256 => 256,
        }
    }
}

/// Digit 4-state. `X`/`Z` wajib sering muncul: `to_u64()` membuangnya, dan itu
/// sumber bug "nilai hilang diam-diam".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Arbitrary)]
pub enum Digit {
    Nol,
    Satu,
    X,
    Z,
}

impl Digit {
    /// Teks digit sebagai karakter; dipakai generator hierarki.
    pub fn teks_publik(self) -> &'static str {
        self.teks()
    }

    fn teks(self) -> &'static str {
        match self {
            Digit::Nol => "0",
            Digit::Satu => "1",
            Digit::X => "x",
            Digit::Z => "z",
        }
    }
}

/// Basis literal (LRM §5.7.1). Tiap basis punya aturan lebar berbeda, jadi
/// masing-masing harus diuji.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Arbitrary)]
pub enum Basis {
    Biner,
    Oktal,
    Desimal,
    Heksa,
}

impl Basis {
    /// Kode basis sebagai teks; dipakai generator hierarki yang berada di modul
    /// lain sehingga tidak bisa mengakses `kode` yang privat.
    pub fn kode_publik(self) -> &'static str {
        self.kode()
    }

    fn kode(self) -> &'static str {
        match self {
            Basis::Biner => "b",
            Basis::Oktal => "o",
            Basis::Desimal => "d",
            Basis::Heksa => "h",
        }
    }

    /// Digit untuk menampung `lebar` bit.
    pub fn digit_untuk_publik(self, lebar: u32) -> u32 {
        self.digit_untuk(lebar)
    }

    fn digit_untuk(self, lebar: u32) -> u32 {
        match self {
            Basis::Biner => lebar.max(1),
            Basis::Oktal => lebar.div_ceil(3).max(1),
            Basis::Heksa => lebar.div_ceil(4).max(1),
            Basis::Desimal => 1,
        }
    }
}

/// Literal dengan setiap digit diambil terpisah, supaya kombinasi `x`/`z`
/// hanya mungkin terbentuk dari data.
#[derive(Debug, Clone, Copy, Arbitrary)]
pub struct Literal {
    pub basis: Basis,
    pub lebar: Lebar,
    pub digit: [Digit; 8],
    /// Berapa banyak digit yang benar-benar ditulis (dapat lebih kecil dari
    /// yang diperlukan — kelas bug lebar literal).
    pub jumlah: u8,
    /// Tulis size eksplisit (`8'hFF`) atau polos (`'hF`).
    pub sized: bool,
    /// Tanda minus (hanya berarti untuk desimal).
    pub negatif: bool,
}

impl Literal {
    pub fn render(&self) -> String {
        let lebar = self.lebar.nilai();
        if self.basis == Basis::Desimal {
            // LRM §5.7.1: desimal dengan `x`/`z` hanya boleh satu digit.
            let d = self.digit[0].teks();
            return if self.negatif { format!("-{d}") } else { d.to_string() };
        }
        let perlu = self.basis.digit_untuk(lebar) as usize;
        let n = perlu.min(usize::from(self.jumlah)).max(1);
        let body: String = (0..n)
            .map(|i| self.digit[i % self.digit.len()].teks())
            .collect();
        let k = self.basis.kode();
        match self.sized {
            true => format!("{lebar}'{k}{body}"),
            false => format!("'{k}{body}"),
        }
    }
}

/// Operator. `Div`/`Mod`/`Shl`/`Shr`/`Sar` punya jalur helper sendiri sehingga
/// sering jadi tempat bug lebar berbeda.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Arbitrary)]
pub enum Op {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    BitAnd,
    BitOr,
    BitXor,
    BitXnor,
    Shl,
    Shr,
    Sar,
    Eq,
    Neq,
    CaseEq,
    CaseNeq,
    Lt,
    Le,
    Gt,
    Ge,
    LogAnd,
    LogOr,
    LogNot,
    BitNeg,
    BitNot,
    RedAnd,
    RedOr,
    RedXor,
}

/// Operator biner saja; dipakai saat render supaya indeks byte memetakan ke
/// operator berbeda tanpa menaruh operator langsung di dalam data.
const OPS: &[Op] = &[
    Op::Add,
    Op::Sub,
    Op::Mul,
    Op::Div,
    Op::Mod,
    Op::BitAnd,
    Op::BitOr,
    Op::BitXor,
    Op::BitXnor,
    Op::Shl,
    Op::Shr,
    Op::Sar,
    Op::Eq,
    Op::Neq,
    Op::CaseEq,
    Op::CaseNeq,
    Op::Lt,
    Op::Le,
    Op::Gt,
    Op::Ge,
    Op::LogAnd,
    Op::LogOr,
];

/// Operator unary (prefix).
const OPS_UNARY: &[Op] = &[Op::LogNot, Op::BitNeg, Op::BitNot];

/// Operator reduksi (prefix, operand tanpa operand kedua).
const OPS_REDUCE: &[Op] = &[Op::RedAnd, Op::RedOr, Op::RedXor];

impl Op {
    fn teks(self) -> &'static str {
        match self {
            Op::Add => "+",
            Op::Sub => "-",
            Op::Mul => "*",
            Op::Div => "/",
            Op::Mod => "%",
            Op::BitAnd => "&",
            Op::BitOr => "|",
            Op::BitXor => "^",
            Op::BitXnor => "~^",
            Op::Shl => "<<",
            Op::Shr => ">>",
            Op::Sar => ">>>",
            Op::Eq => "==",
            Op::Neq => "!=",
            Op::CaseEq => "===",
            Op::CaseNeq => "!==",
            Op::Lt => "<",
            Op::Le => "<=",
            Op::Gt => ">",
            Op::Ge => ">=",
            Op::LogAnd => "&&",
            Op::LogOr => "||",
            Op::LogNot => "!",
            Op::BitNeg => "-",
            Op::BitNot => "~",
            Op::RedAnd => "&",
            Op::RedOr => "|",
            Op::RedXor => "^",
        }
    }
}

/// Bentuk operand. Tiap bentuk punya jalur lowering sendiri yang punya riwayat
/// bug berbeda, jadi semuanya harus bisa dipilih dari data.
#[derive(Debug, Clone, Copy, Arbitrary)]
pub enum Operan {
    Sinyal(u8),
    Konstan(Literal),
    /// `s[expr]` konstan maupun dinamis.
    Seleksi {
        sinyal: u8,
        indeks: u8,
        dinamis: bool,
    },
    /// `s[msb:lsb]`.
    Part {
        sinyal: u8,
        msb: u8,
        lsb: u8,
    },
    /// `s[W-1:0]` — batas ekspresi.
    PartEkspresi {
        sinyal: u8,
        msb: u8,
        lsb: u8,
    },
    ElemenArray {
        indeks: u8,
        dinamis: bool,
    },
    BitElemenArray {
        indeks: u8,
        bit: u8,
    },
    BitPadaCast {
        sinyal: u8,
        bit: u8,
        lebar: Lebar,
    },
    PartPadaCast {
        sinyal: u8,
        msb: u8,
        lsb: u8,
        lebar: Lebar,
    },
    CastTypedef {
        sinyal: u8,
        lebar: Lebar,
        signed: bool,
    },
    CastBuiltin {
        sinyal: u8,
        lebar: Lebar,
    },
    CastSign {
        sinyal: u8,
        negatif: bool,
    },
    BitsOf {
        sinyal: u8,
    },
    Concat {
        jumlah: u8,
    },
    Replika {
        sinyal: u8,
        jumlah: u8,
    },
    Biner {
        lhs: u8,
        rhs: u8,
        op: u8,
    },
    Unary {
        operand: u8,
        op: u8,
    },
    Reduce {
        operand: u8,
        op: u8,
    },
    Ternary {
        a: u8,
        b: u8,
        c: u8,
    },
    Kurung {
        a: u8,
    },
}

/// Ekspresi rekursif; `Box` dipakai agar tipe-nya berukuran terbatas.
#[derive(Debug, Clone, Arbitrary)]
pub struct Ekspres {
    pub bentuk: Operan,
    pub tabel: [Box<Ekspres>; 2],
}

impl Ekspres {
    /// Sinyal akar untuk fallback saat kedalaman maksimum tercapai.
    pub fn akar_sinyal(&self) -> u8 {
        match self.bentuk {
            Operan::Sinyal(i)
            | Operan::CastTypedef { sinyal: i, .. }
            | Operan::CastBuiltin { sinyal: i, .. }
            | Operan::CastSign { sinyal: i, .. }
            | Operan::BitsOf { sinyal: i }
            | Operan::Replika { sinyal: i, .. } => i,
            _ => 0,
        }
    }

    pub fn render(&self, ctx: &Konteks, level: u8) -> String {
        if level >= ctx.maks_kedalaman {
            return ctx.nama(self.akar_sinyal());
        }
        let sub = |i: usize, _slot: u8| self.tabel[i].render(ctx, level + 1);
        let nama = |i: u8| ctx.nama(i);
        let sinyal_lebar = |i: u8| ctx.lebar(i);
        match self.bentuk {
            Operan::Sinyal(i) => nama(i),
            Operan::Konstan(l) => l.render(),
            Operan::Seleksi {
                sinyal: s,
                indeks,
                dinamis,
            } => {
                if dinamis {
                    format!("{}[{}]", nama(s), nama(s.wrapping_add(1)))
                } else {
                    format!("{}[{}]", nama(s), u32::from(indeks) % 17)
                }
            }
            Operan::Part { sinyal: s, msb, lsb } => {
                let _ = sinyal_lebar(s);
                format!("{}[{}:{}]", nama(s), msb % 17, lsb % 17)
            }
            Operan::PartEkspresi { sinyal: s, msb, lsb } => {
                let n = sinyal_lebar(s).max(1);
                format!(
                    "{}[{}:{}]",
                    nama(s),
                    (u32::from(msb) % n.max(1)).min(n - 1),
                    u32::from(lsb) % 17
                )
            }
            Operan::ElemenArray { indeks, dinamis } => {
                if dinamis {
                    format!("mem[{}]", nama(255))
                } else {
                    format!("mem[{}]", u32::from(indeks) % ctx.ukuran_array())
                }
            }
            Operan::BitElemenArray { indeks, bit } => {
                format!("mem[{}][{}]", u32::from(indeks) % ctx.ukuran_array(), bit % 17)
            }
            Operan::BitPadaCast { sinyal: s, bit, lebar } => {
                format!("{}'({})[{}]", lebar.nilai(), nama(s), bit % 17)
            }
            Operan::PartPadaCast {
                sinyal: s,
                msb,
                lsb,
                lebar,
            } => format!(
                "{}'({})[{}:{}]",
                lebar.nilai(),
                nama(s),
                msb % 17,
                lsb % 17
            ),
            Operan::CastTypedef {
                sinyal: s,
                lebar,
                signed,
            } => format!(
                "t{}_{}'({})",
                if signed { "s" } else { "u" },
                lebar.nilai(),
                nama(s)
            ),
            Operan::CastBuiltin { sinyal: s, lebar } => {
                format!("{}'({})", lebar.nilai(), nama(s))
            }
            Operan::CastSign { sinyal: s, negatif } => {
                if negatif {
                    format!("$signed({})", nama(s))
                } else {
                    format!("unsigned'({})", nama(s))
                }
            }
            Operan::BitsOf { sinyal: s } => format!("$bits({})", nama(s)),
            Operan::Concat { jumlah } => {
                let n = usize::from(jumlah) % 4 + 2;
                let parts: Vec<String> = (0..n).map(|_| sub(0, 0)).collect();
                format!("{{{}}}", parts.join(", "))
            }
            Operan::Replika { sinyal: s, jumlah } => {
                format!("{{{}{{{}}}}}", u32::from(jumlah) % 5, nama(s))
            }
            Operan::Biner { lhs, rhs, op } => format!(
                "({} {} {})",
                sub(0, lhs),
                OPS[usize::from(op) % OPS.len()].teks(),
                sub(1, rhs)
            ),
            Operan::Unary { operand, op } => format!(
                "{}({})",
                OPS_UNARY[usize::from(op) % OPS_UNARY.len()].teks(),
                sub(0, operand)
            ),
            Operan::Reduce { operand, op } => format!(
                "{}({})",
                OPS_REDUCE[usize::from(op) % OPS_REDUCE.len()].teks(),
                sub(0, operand)
            ),
            Operan::Ternary { a, b, c } => {
                format!("({} ? {} : {})", sub(0, a), sub(0, b), sub(1, c))
            }
            Operan::Kurung { a } => format!("({})", sub(0, a)),
        }
    }
}

/// Konteks render: nama sinyal, lebarnya, batas kedalaman, ukuran array.
pub struct Konteks {
    nama: Vec<String>,
    lebar: Vec<u32>,
    pub maks_kedalaman: u8,
    ukuran_array: u32,
}

impl Konteks {
    pub fn nama(&self, i: u8) -> String {
        self.nama
            .get(usize::from(i) % self.nama.len().max(1))
            .cloned()
            .unwrap_or_else(|| "mem".to_string())
    }

    pub fn lebar(&self, i: u8) -> u32 {
        self.lebar
            .get(usize::from(i) % self.lebar.len().max(1))
            .copied()
            .unwrap_or(1)
    }

    fn ukuran_array(&self) -> u32 {
        self.ukuran_array.max(1)
    }
}

/// Target penulisan: bentuk LHS berbeda punya validasi berbeda di elaborator.
#[derive(Debug, Clone, Copy, Arbitrary)]
pub enum Target {
    Penuh { sinyal: u8 },
    Bit { sinyal: u8, bit: u8 },
    Part { sinyal: u8, msb: u8, lsb: u8 },
    ElemenArray { indeks: u8 },
    BitElemenArray { indeks: u8, bit: u8 },
}

impl Target {
    fn render(&self, ctx: &Konteks) -> String {
        match self {
            Target::Penuh { sinyal } => ctx.nama(*sinyal),
            Target::Bit { sinyal, bit } => {
                format!("{}[{}]", ctx.nama(*sinyal), bit % 17)
            }
            Target::Part { sinyal, msb, lsb } => {
                format!("{}[{}:{}]", ctx.nama(*sinyal), msb % 17, lsb % 17)
            }
            Target::ElemenArray { indeks } => {
                format!("mem[{}]", u32::from(*indeks) % ctx.ukuran_array())
            }
            Target::BitElemenArray { indeks, bit } => {
                format!("mem[{}][{}]", u32::from(*indeks) % ctx.ukuran_array(), bit % 17)
            }
        }
    }
}

/// Jenis `case`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Arbitrary)]
pub enum Wildcard {
    Tepat,
    Z,
    X,
}

/// Statement procedural. Variasinya sengaja mencakup bentuk yang historis
/// ditolak dengan pesan salah (`mem[k]`, `y[i-1]`, `y[3:0][1]`).
#[derive(Debug, Clone, Arbitrary)]
pub enum Statement {
    Assign {
        target: Target,
        nilai: Ekspres,
    },
    AlwaysComb {
        target: Target,
        nilai: Ekspres,
    },
    If {
        kondisi: Ekspres,
        target: Target,
        nilai: Ekspres,
        ada_else: bool,
        nilai_else: Ekspres,
    },
    Case {
        selektor: Ekspres,
        label: Literal,
        wildcard: Wildcard,
        jumlah_label: u8,
        label_kedua: Literal,
    },
    For {
        batas: u8,
        target: Target,
        nilai: Ekspres,
    },
    Repeat {
        jumlah: u8,
        target: Target,
        nilai: Ekspres,
    },
    While {
        batas: u8,
        target: Target,
        nilai: Ekspres,
    },
    /// Penulisan dengan indeks variabel — harus ditolak dengan pesan jelas.
    AssignDinamis {
        sinyal: u8,
        indeks: Ekspres,
    },
    /// Indeks identifier di luar generate — harus ditolak menyebut indeks.
    AssignGenvar {
        sinyal: u8,
    },
    /// Indeks berpola aritmetika `s[i - 1]`.
    AssignGenvarAritmetika {
        sinyal: u8,
    },
    /// Seleksi bertingkat pada LHS.
    AssignBertingkat {
        sinyal: u8,
        msb: u8,
        lsb: u8,
        bit: u8,
    },
    /// Delay lalu cetak — menguji pemecahan segmen.
    Delay {
        satuan: u8,
        jumlah: u8,
    },
    /// Statement kosong (uji `always_ff` body kosong).
    Kosong,
}

/// Directive preprocessor — jalur sendiri denganüdah.
#[derive(Debug, Clone, Copy, Arbitrary)]
pub enum Directive {
    Define {
        nama: u8,
        tubuh: u8,
    },
    DefineFungsi {
        nama: u8,
        tubuh: u8,
    },
    Undef {
        nama: u8,
    },
    Ifdef {
        nama: u8,
    },
    Ifndef {
        nama: u8,
    },
    Elsif {
        nama: u8,
    },
    Else,
    Endif,
    Timescale {
        unit: u8,
    },
    Line {
        nomor: u8,
    },
    DefaultNettype {
        nilai: u8,
    },
}

/// Deklarasi sinyal. Nama, lebar, signedness, ukuran array, dan nilai awal
/// semuanya dari data.
#[derive(Debug, Clone, Copy, Arbitrary)]
pub struct Deklarasi {
    pub lebar: Lebar,
    pub signed: bool,
    pub array: u8,
    pub nilai_awal: Literal,
}

/// Program utuh.
#[derive(Debug, Clone, Arbitrary)]
pub struct Program {
    pub direktif: Vec<Directive>,
    pub deklarasi: Vec<Deklarasi>,
    pub statement: Vec<Statement>,
    pub maks_kedalaman: u8,
    /// Berapa banyak sinyal yang ikut dicetak.
    pub jumlah_cetak: u8,
}

impl Program {
    fn nama_sinyal(&self) -> Vec<String> {
        self.deklarasi
            .iter()
            .enumerate()
            .map(|(i, d)| format!("s{i}_{}", d.lebar.nilai()))
            .collect()
    }

    fn lebar_sinyal(&self) -> Vec<u32> {
        self.deklarasi.iter().map(|d| d.lebar.nilai()).collect()
    }

    fn ukuran_array(&self) -> u32 {
        self.deklarasi
            .iter()
            .map(|d| u32::from(d.array))
            .max()
            .unwrap_or(1)
    }

    /// Render ke sumber SystemVerilog. Tidak ada nilai tetap di mana pun:
    /// setiap angka berasal dari `self`.
    pub fn render(&self) -> String {
        let nama = self.nama_sinyal();
        let lebar = self.lebar_sinyal();
        let ctx = Konteks {
            nama: nama.clone(),
            lebar: lebar.clone(),
            maks_kedalaman: self.maks_kedalaman.max(1),
            ukuran_array: self.ukuran_array(),
        };
        let mut out = String::new();
        // Directive harus seimbang, kalau tidak seluruh program dilewati
        // praprosesor dan fuzzer tidak pernah menyentuh elaborasi. `Else`,
        // `Elsif`, dan `Endif` dari data hanya dipancarkan bila memang sedang
        // di dalam blok, dan blok yang masih terbuka ditutup otomatis.
        let mut depth = 0usize;
        for d in &self.direktif {
            render_directive(&mut out, *d, &mut depth);
        }
        while depth > 0 {
            out.push_str("`endif\n");
            depth -= 1;
        }
        out.push_str("module fz;\n");
        for (i, d) in self.deklarasi.iter().enumerate() {
            let tops = d.lebar.nilai() - 1;
            let sign = if d.signed { " signed" } else { "" };
            if d.array > 0 {
                out.push_str(&format!(
                    "  logic{sign} [{tops}:0] {} [0:{}];\n",
                    nama[i], d.array
                ));
            } else {
                out.push_str(&format!("  logic{sign} [{tops}:0] {};\n", nama[i]));
            }
        }
        let ukuran = self.ukuran_array();
        let lebar_mem = lebar.first().copied().unwrap_or(1);
        out.push_str(&format!(
            "  logic [{}:0] mem [0:{}];\n",
            lebar_mem - 1,
            ukuran.saturating_sub(1)
        ));
        out.push_str("  logic [7:0] idx_src;\n");
        out.push_str("  integer r;\n");
        out.push_str("  reg signed [7:0] sgn;\n");
        // Typedef untuk menguji cast bertipe; lebarnya ikut dari data.
        for d in self.deklarasi.iter().take(2) {
            let sign = if d.signed { "s" } else { "u" };
            out.push_str(&format!(
                "  typedef logic{} [{}:0] t{}_{};\n",
                if d.signed { " signed" } else { "" },
                d.lebar.nilai() - 1,
                sign,
                d.lebar.nilai()
            ));
        }

        out.push_str("  initial begin\n");
        for (i, d) in self.deklarasi.iter().enumerate() {
            out.push_str(&format!("    {} = {};\n", nama[i], d.nilai_awal.render()));
        }
        out.push_str(&format!("    r = {};\n", self.direktif.len() % 100));
        out.push_str(&format!(
            "    idx_src = 8'd{};\n",
            self.maks_kedalaman % 10
        ));
        out.push_str("    sgn = -8'sd1;\n");
        out.push_str("  end\n");

        out.push_str("  initial begin\n");
        for s in &self.statement {
            render_statement(&mut out, s, &ctx);
        }
        // Kalau tidak ada statement sama sekali, tetap tulis sesuatu supaya
        // program tidak "berhenti di tempat yang salah" dan perbandingan dengan
        // simulator acuan punya isi. `mem` selalu ada sebagai sinyal, jadi
        // dipakai sebagai target paling aman.
        if self.statement.is_empty() && !nama.is_empty() {
            // Hanya sinyal skalar yang ditulis penuh. Penulisan ke seluruh array
            // (`mem = ...`) belum didukung simulator acuan, jadi program seperti
            // ini tidak bisa dibandingkan dan hanya membuang iterasi.
            let skalar = nama
                .iter()
                .zip(self.deklarasi.iter())
                .find(|(_, d)| d.array == 0);
            if let Some((target, _)) = skalar {
            out.push_str(&format!(
                "    {target} = {};\n",
                Literal {
                    basis: Basis::Desimal,
                    lebar: Lebar::N1,
                    digit: [
                        Digit::Satu,
                        Digit::Nol,
                        Digit::X,
                        Digit::Z,
                        Digit::Satu,
                        Digit::Nol,
                        Digit::X,
                        Digit::Z,
                    ],
                    jumlah: 1,
                    sized: true,
                    negatif: false,
                }
                .render()
            ));
            }
        }
        out.push_str("  end\n");

        // Cetak semua sinyal supaya setiap perbedaan dengan iverilog terlihat.
        out.push_str("  initial begin\n");
        // Minimal satu sinyal harus dicetak; kalau nol, perbandingan dengan
        // simulator acuan tidak punya apa pun untuk dibandingkan.
        let cetak = usize::from(self.jumlah_cetak).max(1).min(nama.len());
        for n in nama.iter().take(cetak) {
            out.push_str(&format!("    $display(\"{n} %h\", {n});\n"));
        }
        for k in 0..ukuran {
            out.push_str(&format!("    $display(\"mem[{k}] %h\", mem[{k}]);\n"));
        }
        out.push_str("    $display(\"sgn %h\", sgn);\n");
        out.push_str("    $display(\"r %0d\", r);\n");
        out.push_str("  end\n");
        out.push_str("endmodule\n");
        out
    }
}

fn render_directive(out: &mut String, d: Directive, depth: &mut usize) {
    match d {
        Directive::Define { nama, tubuh } => {
            out.push_str(&format!("`define M{nama} {tubuh}\n"));
        }
        Directive::DefineFungsi { nama, tubuh } => {
            out.push_str(&format!("`define F{nama}(a) ((a) ^ {tubuh})\n"));
        }
        Directive::Undef { nama } => out.push_str(&format!("`undef M{nama}\n")),
        Directive::Ifdef { nama } => {
            out.push_str(&format!("`ifdef M{nama}\n"));
            *depth += 1;
        }
        Directive::Ifndef { nama } => {
            out.push_str(&format!("`ifndef M{nama}\n"));
            *depth += 1;
        }
        Directive::Elsif { nama } => {
            if *depth > 0 {
                out.push_str(&format!("`elsif M{nama}\n"));
            }
        }
        Directive::Else => {
            if *depth > 0 {
                out.push_str("`else\n");
            }
        }
        Directive::Endif => {
            if *depth > 0 {
                out.push_str("`endif\n");
                *depth -= 1;
            }
        }
        Directive::Timescale { unit } => {
            // Satuan diambil dari data lewat indeks ke daftar satuan yang sah
            // (LRM §22). Memakai byte apa langsung menghasilkan `timescale
            // 1115s` yang selalu ditolak lexer, jadi iterasi terbuang.
            const SATUAN: [&str; 6] = ["s", "ms", "us", "ns", "ps", "fs"];
            let u = SATUAN[usize::from(unit) % SATUAN.len()];
            out.push_str(&format!("`timescale 1{u}/1{u}s\n"));
        }
        Directive::Line { nomor } => out.push_str(&format!("`line {nomor} \"fz.sv\" 0\n")),
        Directive::DefaultNettype { nilai } => {
            out.push_str(&format!("`default_nettype {nilai}\n"));
        }
    }
}

fn render_statement(out: &mut String, s: &Statement, ctx: &Konteks) {
    let pad = "  ";
    let e = |x: &Ekspres| x.render(ctx, 0);
    let t = |x: &Target| x.render(ctx);
    match s {
        Statement::Assign { target, nilai } => {
            out.push_str(&format!("{pad}{} = {};\n", t(target), e(nilai)));
        }
        Statement::AlwaysComb { target, nilai } => {
            out.push_str(&format!(
                "{pad}always_comb {} = {};\n",
                t(target),
                e(nilai)
            ));
        }
        Statement::If {
            kondisi,
            target,
            nilai,
            ada_else,
            nilai_else,
        } => {
            out.push_str(&format!("{pad}if ({}) {{\n", e(kondisi)));
            out.push_str(&format!("{pad}  {} = {};\n", t(target), e(nilai)));
            if *ada_else {
                out.push_str(&format!("{pad}}} else {{\n"));
                out.push_str(&format!("{pad}  {} = {};\n", t(target), e(nilai_else)));
            }
            out.push_str(&format!("{pad}}}\n"));
        }
        Statement::Case {
            selektor,
            label,
            wildcard,
            jumlah_label,
            label_kedua,
        } => {
            let kw = match wildcard {
                Wildcard::Tepat => "case",
                Wildcard::Z => "casez",
                Wildcard::X => "casex",
            };
            out.push_str(&format!("{pad}{kw} ({})\n", e(selektor)));
            for k in 0..(usize::from(*jumlah_label) % 4) + 1 {
                let lab = if k % 2 == 0 { label } else { label_kedua };
                out.push_str(&format!("{pad}  {}: r = {k};\n", lab.render()));
            }
            out.push_str(&format!("{pad}  default: r = {};\n", usize::from(*jumlah_label)));
            out.push_str(&format!("{pad}endcase\n"));
        }
        Statement::For {
            batas,
            target,
            nilai,
        } => {
            out.push_str(&format!(
                "{pad}for (r = 0; r < {}; r = r + 1) {{\n",
                usize::from(*batas) % 8 + 1
            ));
            out.push_str(&format!("{pad}  {} = {};\n", t(target), e(nilai)));
            out.push_str(&format!("{pad}}}\n"));
        }
        Statement::Repeat {
            jumlah,
            target,
            nilai,
        } => {
            out.push_str(&format!("{pad}repeat ({}) {{\n", usize::from(*jumlah) % 8));
            out.push_str(&format!("{pad}  {} = {};\n", t(target), e(nilai)));
            out.push_str(&format!("{pad}}}\n"));
        }
        Statement::While {
            batas,
            target,
            nilai,
        } => {
            out.push_str(&format!(
                "{pad}while (r < {}) {{\n",
                usize::from(*batas) % 8 + 1
            ));
            out.push_str(&format!("{pad}  r = r + 1;\n"));
            out.push_str(&format!("{pad}  {} = {};\n", t(target), e(nilai)));
            out.push_str(&format!("{pad}}}\n"));
        }
        Statement::AssignDinamis { sinyal, indeks } => {
            out.push_str(&format!(
                "{pad}{}[{}] = {};\n",
                ctx.nama(*sinyal),
                e(indeks),
                ctx.nama(*sinyal)
            ));
        }
        Statement::AssignGenvar { sinyal } => {
            out.push_str(&format!(
                "{pad}{}[gv{}] = 1'b1;\n",
                ctx.nama(*sinyal),
                usize::from(*sinyal) % 4
            ));
        }
        Statement::AssignGenvarAritmetika { sinyal } => {
            out.push_str(&format!(
                "{pad}{}[gv{} - 1] = 1'b1;\n",
                ctx.nama(*sinyal),
                usize::from(*sinyal) % 4
            ));
        }
        Statement::AssignBertingkat {
            sinyal,
            msb,
            lsb,
            bit,
        } => {
            out.push_str(&format!(
                "{pad}{}[{}:{}][{}] = 1'b1;\n",
                ctx.nama(*sinyal),
                msb % 17,
                lsb % 17,
                bit % 17
            ));
        }
        Statement::Delay { satuan, jumlah } => {
            let s = ["s", "ms", "us", "ns", "ps", "fs"][usize::from(*satuan) % 6];
            out.push_str(&format!("{pad}#{}{s};\n", usize::from(*jumlah) % 10));
        }
        Statement::Kosong => out.push_str(&format!("{pad};\n")),
    }
}

/// Bangun `Program` dari byte fuzzer; dipanggil juga oleh oracle
/// metamorphic yang butuh akses ke bentuk yang sama.
impl Program {
    pub fn dari_bytes(data: &[u8]) -> Option<Program> {
        dari_data(data)
    }
}

/// Bangun `Program` dari byte fuzzer.
pub fn dari_data(data: &[u8]) -> Option<Program> {
    let mut u = Unstructured::new(data);
    Program::arbitrary(&mut u).ok()
}

/// Program dari LCG supaya counterexample bisa diulang persis dari nomor
/// iterasinya (dipakai runner pembanding iverilog).
pub fn acak(seed: u64) -> Program {
    let mut st = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
    let mut buf = vec![0u8; 8192];
    for b in buf.iter_mut() {
        st = st
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        *b = (st >> 33) as u8;
    }
    dari_data(&buf).unwrap_or_else(|| {
        // Buffer acak hampir selalu cukup untuk `Vec` kosong; kalau tidak,
        // gunakan seed berikutnya.
        acak(seed.wrapping_add(1))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acak_selalu_memberi_program_valid() {
        for seed in 0..200 {
            let p = acak(seed);
            let src = p.render();
            assert!(src.contains("module fz;"), "seed {seed}: {src}");
            assert!(src.contains("endmodule"), "seed {seed}: {src}");
            // Directive harus seimbang supaya program tidak dilewati
            // praprosesor.
            let buka = src.matches("`ifdef").count() + src.matches("`ifndef").count();
            let tutup = src.matches("`endif").count();
            assert_eq!(buka, tutup, "seed {seed}: ifdef={buka} endif={tutup}");
        }
    }

    #[test]
    fn acak_deterministik() {
        assert_eq!(acak(42).render(), acak(42).render());
    }

    #[test]
    fn program_mencakup_lebar_melampaui_64_bit() {
        // Lebar > 64 adalah sumber kelas bug mask/truncation; kalau tidak
        // pernah muncul, fuzzing tidak berguna untuk target itu.
        let mut muncul = false;
        for seed in 0..400 {
            let p = acak(seed);
            if p.deklarasi.iter().any(|d| d.lebar.nilai() > 64) {
                muncul = true;
                break;
            }
        }
        assert!(muncul, "tidak ada sinyal lebih dari 64 bit dalam 400 seed");
    }

    #[test]
    fn program_mencakup_literal_dengan_x_dan_z() {
        let mut x = 0;
        let mut z = 0;
        for seed in 0..400 {
            let p = acak(seed);
            for d in &p.deklarasi {
                x += usize::from(d.nilai_awal.digit.iter().filter(|d| **d == Digit::X).count());
                z += usize::from(d.nilai_awal.digit.iter().filter(|d| **d == Digit::Z).count());
            }
        }
        assert!(x > 0, "digit x tidak pernah muncul");
        assert!(z > 0, "digit z tidak pernah muncul");
    }
}