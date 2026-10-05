// Tanggung jawab: tabel simbol saat elaborasi.
use crate::error::ElaborateError;
use std::collections::HashMap;
use sv_ir::{DataType, SignalId, VarKind};
use sv_lexer::span::Span;

#[derive(Debug, Clone, PartialEq)]
pub struct Symbol {
    pub name: String,
    pub signal_id: SignalId,
    pub data_type: DataType,
    pub kind: VarKind,
    pub span: Span,
    /// `Some` bila sinyal ini array unpacked (LRM §7.8).
    pub unpacked: Option<UnpackedInfo>,
}

/// Simpul array unpacked (LRM §7.8) yang menempel pada simbol.
///
/// Dipisah dari `Symbol` supaya field baru tidak mengubah setiap call site:
/// mayoritas sinyal bukan array, dan constructor standarnya tetap berlaku.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnpackedInfo {
    pub dim: sv_ast::declaration::UnpackedDim,
    /// Lebar satu elemen. Sinyal disimpan rata selebar `elem_width * size`.
    pub elem_width: u32,
}

impl Symbol {
    /// Simpul dasar untuk sinyal skalar.
    pub fn baru(
        name: impl Into<String>,
        signal_id: SignalId,
        data_type: DataType,
        kind: VarKind,
        span: Span,
    ) -> Self {
        Self {
            name: name.into(),
            signal_id,
            data_type,
            kind,
            span,
            unpacked: None,
        }
    }

    /// Tandai simbol sebagai array unpacked (LRM §7.8).
    pub fn dengan_unpacked(
        mut self,
        dim: sv_ast::declaration::UnpackedDim,
        elem_width: u32,
    ) -> Self {
        self.unpacked = Some(UnpackedInfo { dim, elem_width });
        self
    }

    /// Lebar yang dilaporkan `$bits` (LRM §20): untuk array unpacked ini
    /// lebar ELEMENnya, bukan lebar total.
    pub fn bits_width(&self) -> u32 {
        match &self.unpacked {
            Some(info) => info.elem_width,
            None => self.data_type.width,
        }
    }

    /// Rentang bit elemen `index` pada sinyal rata, kalau sinyal ini array
    /// unpacked dan indeksnya dalam jangkauan.
    pub fn elemen(&self, index: u64) -> Option<(u32, u32)> {
        let info = self.unpacked.as_ref()?;
        let jarak = info.dim.offset(index)?;
        let w = info.elem_width;
        Some(((jarak as u32 + 1) * w - 1, (jarak as u32) * w))
    }
}

#[derive(Debug, Default)]
pub struct SymbolTable {
    symbols: Vec<Symbol>,
    index: HashMap<String, SignalId>,
    /// Lebar & signedness tiap nama tipe yang sedang terlihat (LRM §6.14).
    ///
    /// Disimpan di tabel simbol, bukan diteruskan sebagai parameter, karena
    /// `lower_expression` sudah menerima tabel ini di setiap call site; menambah
    /// satu argumen berarti menyentuh semua lowering statement.
    ///
    /// Kunci memakai nama yang sudah di-prefix instans, persis seperti
    /// `Symbol::name`: LRM §8.20 membuat typedef module-scoped, jadi dua
    /// instansi modul yang sama boleh punya `w_t` dengan lebar berbeda dan
    /// keduanya harus terpisah.
    types: HashMap<String, (u32, bool)>,
}

impl SymbolTable {
    pub fn new() -> Self {
        Self::default()
    }

    /// Deklarasi nama tipe agar bisa dipakai sebagai target cast `tipe'(x)`.
    ///
    /// `nama` harus sudah di-prefix instans pemanggil, sama seperti
    /// `Symbol::name`: typedef anak hanya boleh terlihat di badan anak, bukan
    /// di badan induk.
    ///
    /// LRM §8.20: nama yang sama tidak boleh dipakai untuk variabel dan
    /// typedef di satu scope. Namespaced secara terpisah di sini karena
    /// keduanya hidup berdampingan di `SymbolTable`, jadi pemeriksaan silangnya
    /// dilakukan eksplisit — tanpa itu `$bits(foo)` akan memilih tipe diam-diam
    /// dan kode yang salah lolos tanpa error.
    pub fn insert_type(
        &mut self,
        nama: &str,
        width: u32,
        signed: bool,
        span: Span,
    ) -> Result<(), ElaborateError> {
        // Tanpa prefix: nama polos bertabrakan dengan sinyal di scope yang sama.
        if !nama.contains("__") && self.index.contains_key(nama) {
            return Err(ElaborateError::new(
                format!("'{nama}' sudah dipakai sebagai nama sinyal; nama tipe harus berbeda"),
                span,
            ));
        }
        self.types.insert(nama.to_string(), (width, signed));
        Ok(())
    }

    /// Lebar & signedness nama tipe, bila ada.
    /// Lebar & signedness nama tipe, bila ada.
    ///
    /// `nama` boleh berupa nama ter-prefix instans (`u0__byte_t`) maupun nama
    /// polos yang ditulis user (`byte_t`). Nama polos dicocokkan dari yang
    /// tersempit agar pesan error bisa menyebut nama yang benar-benar ditulis
    /// di source (AGENTS.md aturan 3), bukan nama internal hasil prefix.
    pub fn type_info(&self, nama: &str) -> Option<(u32, bool)> {
        if let Some(hasil) = self.types.get(nama) {
            return Some(*hasil);
        }
        // Tanpa prefix eksplisit, cari nama yang berakhir dengan `nama` dan
        // prefiksnya pemisah instans yang sah (selalu berakhir `__`).
        let akhir = format!("__{nama}");
        self.types
            .iter()
            .find(|(kunci, _)| kunci.ends_with(&akhir))
            .map(|(_, nilai)| *nilai)
    }

    /// True bila ada nama tipe yang terdaftar.
    pub fn has_types(&self) -> bool {
        !self.types.is_empty()
    }

    pub fn insert(&mut self, symbol: Symbol) -> Result<SignalId, ElaborateError> {
        if self.index.contains_key(&symbol.name) {
            return Err(ElaborateError::duplicate_signal(&symbol.name, symbol.span));
        }
        let id = self.symbols.len() as SignalId;
        let mut symbol = symbol;
        symbol.signal_id = id;
        self.index.insert(symbol.name.clone(), id);
        self.symbols.push(symbol);
        Ok(id)
    }

    pub fn lookup(&self, name: &str) -> Option<&Symbol> {
        self.index
            .get(name)
            .and_then(|id| self.symbols.get(*id as usize))
    }

    pub fn get(&self, id: SignalId) -> Option<&Symbol> {
        self.symbols.get(id as usize)
    }

    pub fn len(&self) -> usize {
        self.symbols.len()
    }

    pub fn is_empty(&self) -> bool {
        self.symbols.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Symbol> {
        self.symbols.iter()
    }

    pub fn into_symbols(self) -> Vec<Symbol> {
        self.symbols
    }
}
