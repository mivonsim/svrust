// Tanggung jawab: implementasi runtime system task testbench ($display, $finish).
use crate::bits::Bits;
use crate::logic::Logic;
use crate::time::SimTime;
use std::fmt::Write as _;

/// Satu argumen nilai yang menunggu diformat.
///
/// Lebar logis dan tanda disimpan terpisah karena `Bits` selalu berukuran
/// `MAX_WIDTH`, sedangkan `%h`/`%b` harus mengikuti lebar deklarasi aslinya.
#[derive(Debug, Clone)]
pub struct FormatArg<const M: usize> {
    /// Nilai argumen pada lebar penyimpanan penuh.
    pub value: Bits<M>,
    /// Lebar logis hasil deklarasi SystemVerilog.
    pub width: u32,
    /// `true` bila tipe bertanda; menentukan `%d` dan bit `%b` teratas.
    pub signed: bool,
}

impl<const M: usize> FormatArg<M> {
    /// Nilai sebagai integer bertanda sesuai lebar logis.
    ///
    /// Lebar logis 64 diperlakukan penuh: `1i64 << 64` adalah shift overflow
    /// (panic di build debug, wrap diam-diam di release), padahal nilai
    /// 64-bit bertanda negatif sangat wajar — `sword_t'(-8'sd1)` berlebar 64
    /// dan sering dicetak dengan `%0d`. Karena itu bit ke-64 dihindari lewat
    /// `wrapping_sub` dari representasi dua's complement yang sudah di-cast.
    fn to_i64(&self) -> i64 {
        if !self.signed || self.width == 0 {
            return self.value.to_u64() as i64;
        }
        let lebar = (self.width as usize).min(64);
        let mentah = self.value.to_u64();
        let tanda = 1u64 << (lebar - 1);
        if mentah & tanda == 0 {
            return mentah as i64;
        }
        // Nilai negatif = nilai mentah dikurangi 2^lebar. Untuk `lebar == 64`
        // kita tidak bisa membentuk 2^64 di i64, tapi representasi dua's
        // complement dari `mentah` sudah persis nilai yang dicari.
        if lebar == 64 {
            return mentah as i64;
        }
        (mentah as i64) - (1i64 << lebar)
    }

    /// Teks desimal kalau semua digit-nya 0/1; `None` kalau ada `x`/`z`.
    fn teks_known_desimal(&self) -> Option<String> {
        let lebar = self.width as usize;
        for i in 0..lebar {
            match self.value.get(i) {
                Logic::Zero | Logic::One => {}
                Logic::X | Logic::Z => return None,
            }
        }
        Some(match self.signed {
            true => self.to_i64().to_string(),
            false => self.value.to_u64().to_string(),
        })
    }

    /// Teks desimal untuk nilai yang mengandung `x`/`z`.
    ///
    /// LRM §20.4: seluruh digit dicetak `x` kalau ada `x` di antaranya, `z`
    /// kalau ada `z`. Untuk `%d`(desimal) iverilog mencetak `x` per digit
    /// desimal yang tidak diketahui, jadi bentuk sederhana dipakai: satu `x`
    /// untuk nilai sepenuhnya tak diketahui.
    fn teks_unknown_desimal(&self) -> String {
        let lebar = self.width as usize;
        let mut semua_x = true;
        let mut ada_z = false;
        for i in 0..lebar {
            match self.value.get(i) {
                Logic::Z => {
                    ada_z = true;
                    semua_x = false;
                }
                Logic::X => semua_x = false,
                _ => {}
            }
        }
        if semua_x {
            "x".to_string()
        } else if ada_z {
            "z".to_string()
        } else {
            // Campuran angka dan `x` — cetak `x` karena angkanya tidak boleh
            // dianggap pasti.
            "x".to_string()
        }
    }

    /// Digit biner MSB-dulu sepanjang lebar logis.
    fn to_bits_str(&self) -> String {
        let lebar = self.width as usize;
        let mut out = String::with_capacity(lebar);
        for i in (0..lebar).rev() {
            out.push(match self.value.get(i) {
                Logic::One => '1',
                Logic::Zero => '0',
                Logic::X => 'x',
                Logic::Z => 'z',
            });
        }
        out
    }

    /// Digit heksadesimal sepanjang `ceil(width/4)`.
    ///
    /// LRM §20.4: digit yang mengandung `z` dicetak `z`, yang mengandung `x`
    /// (tanpa `z`) dicetak `x`, dan nibble yang 0/1 semua dicetak sebagai
    /// heksadesimal. Mengabaikan `x`/`z` di sini membuat `8'hxA` tercetak `0a`
    /// — hilangnya informasi yang justru harus terlihat.
    fn to_hex_str(&self) -> String {
        // `x`/`z` dan digit heksa selalu dicetak huruf kecil, apa pun
        // specifier-nya. LRM §20.4 tidak menetapkan huruf untuk kedua bentuk
        // `x`/`z`, dan iverilog pun tidak konsisten: `8'hxA` dicetak `xa`
        // sementara `2'b1x` dicetak `0X` — mengikuti huruf yang ditulis di
        // source. Menyalin perilaku itu berarti mencatat huruf asli tiap digit
        // di lexer, jadi bentuk lowercase yang seragam dipilih di sini.
        let digits = self.width.div_ceil(4) as usize;
        let mut out = String::with_capacity(digits);
        for d in (0..digits).rev() {
            let mut nibble = 0u8;
            let mut ada_x = false;
            let mut ada_z = false;
            for bit in 0..4usize {
                let idx = d * 4 + bit;
                // Bit di luar lebar logis bukan bagian nilai, jadi bukan
                // `x`/`z` — kalau ikut dihitung, setiap nilai akan tercetak
                // penuh `x` pada lebar yang bukan kelipatan empat.
                if idx >= self.width as usize {
                    continue;
                }
                match self.value.get(idx) {
                    Logic::One => nibble |= 1 << bit,
                    Logic::X => ada_x = true,
                    Logic::Z => ada_z = true,
                    Logic::Zero => {}
                }
            }
            out.push(if ada_z {
                'z'
            } else if ada_x {
                'x'
            } else {
                std::char::from_digit(nibble as u32, 16).unwrap_or('0')
            });
        }
        out
    }
}

/// Render `$display` dengan gaya printf SystemVerilog (LRM §20.2).
///
/// Baris baru selalu ditambahkan di akhir karena `$display` berbeda dengan
/// `$write` yang tidak menambahkannya.
pub fn format_args<const M: usize>(fmt: &str, args: &[FormatArg<M>]) -> String {
    let mut out = String::new();
    let mut sisa: Vec<&FormatArg<M>> = args.iter().collect();
    let chars: Vec<char> = fmt.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] != '%' {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        i += 1;
        if i >= chars.len() {
            out.push('%');
            break;
        }
        // `%0Nd` — nol di depan berarti rata kanan dengan lebar N.
        let mut nol = false;
        let mut lebar: Option<usize> = None;
        while i < chars.len() && (chars[i] == '0' || chars[i].is_ascii_digit()) {
            if chars[i] == '0' {
                nol = true;
            } else if lebar.is_none() {
                lebar = chars[i].to_digit(10).map(|d| d as usize);
            }
            i += 1;
        }
        if i >= chars.len() {
            break;
        }
        let spec = chars[i];
        i += 1;

        if spec == '%' {
            out.push('%');
            continue;
        }
        let Some(arg) = sisa.first().copied() else {
            // Tidak ada argumen tersisa: spesifikasi dicetak apa adanya.
            let _ = write!(out, "%{}{}", if nol { "0" } else { "" }, spec);
            continue;
        };
        sisa.remove(0);

        let teks = match spec {
            'd' | 'D' => {
                // LRM §20.4: `%d` mengikuti signedness tipe. Nilai unsigned
                // 64-bit harus dicetak penuh — `u64::MAX as i64` justru
                // menjadi -1, membuat `time'(-1)` tampil seolah negatif.
                //
                // Digit `x`/`z` harus ikut tercetak. `to_u64()` menghitungnya
                // sebagai 0, jadi variabel yang belum diinisialisasi tampil
                // sebagai angka pasti `0` — persis kebalikan dari `%h`/`%b`
                // yang sudah menjaganya. iverilog mencetak `x`.
                if let Some(teks) = arg.teks_known_desimal() {
                    pad(&teks, nol, lebar)
                } else {
                    arg.teks_unknown_desimal()
                }
            }
            'h' | 'H' | 'x' | 'X' => arg.to_hex_str(),
            'b' | 'B' => arg.to_bits_str(),
            'c' | 'C' => char::from_u32(arg.value.to_u64() as u32)
                .map(|c| c.to_string())
                .unwrap_or_default(),
            // LRM §20.4: `%t` merender nilai waktu dengan satuan otomatis.
            // Nilai dibaca sebagai nanosecond, satuan bawaan `timeunit`.
            't' | 'T' => SimTime::from_nanos(arg.value.to_u64()).format(),
            // `%s` dan `%m` tidak punya sumber nilai di engine ini.
            's' | 'S' | 'm' | 'M' => String::new(),
            _ => {
                let _ = write!(out, "%{}", spec);
                continue;
            }
        };
        out.push_str(&teks);
    }

    // Argumen berlebih dicetak sebagai desimal dengan pemisah spasi (LRM §20.2).
    for (n, arg) in sisa.iter().enumerate() {
        if n > 0 || !out.is_empty() {
            out.push(' ');
        }
        out.push_str(&arg.to_i64().to_string());
    }

    out.push('\n');
    out
}

/// Rata-kan teks ke lebar tertentu; `nol` berarti pad dengan karakter nol.
fn pad(teks: &str, nol: bool, lebar: Option<usize>) -> String {
    let Some(target) = lebar else {
        return teks.to_string();
    };
    let panjang = teks.chars().count();
    if panjang >= target {
        return teks.to_string();
    }
    let filler = if nol { '0' } else { ' ' };
    let mut out = String::new();
    for _ in panjang..target {
        out.push(filler);
    }
    // Rata kanan: sign untuk negatif harus tetap di depan digit.
    if let Some(stripped) = teks.strip_prefix('-') {
        out.insert(0, '-');
        out.push_str(stripped);
    } else {
        out.push_str(teks);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arg(value: u64, width: u32, signed: bool) -> FormatArg<8> {
        FormatArg {
            value: Bits::from_u64(value),
            width,
            signed,
        }
    }

    #[test]
    fn format_desimal_dengan_baris_baru() {
        let hasil = format_args("n=%d", &[arg(42, 8, false)]);
        assert_eq!(hasil, "n=42\n");
    }

    // BUG: `to_hex_str` dulu mengabaikan `X`/`Z` sehingga `8'hxA` tercetak
    // `0a`; `to_i64` dulu melakukan `1i64 << 64` yang shift overflow (panic di
    // debug, wrap diam-diam di release) untuk nilai 64-bit bertanda negatif.

    fn arg_dari_bits(value: Bits<8>, width: u32, signed: bool) -> FormatArg<8> {
        FormatArg {
            value,
            width,
            signed,
        }
    }

    #[test]
    fn hex_menampilkan_x_dan_z() {
        // LRM §20.4: digit `x`/`z` tampil apa adanya pada `%h`, bukan jadi `0`.
        // 8'bxx00_1010: nibble atas `x` karena bit 6 dan 7 `x`, nibble bawah
        // 0b1010 = `a` yang tidak terpengaruh.
        let mut v = Bits::<8>::from_u64(0x0A);
        v.set(6, Logic::X);
        v.set(7, Logic::X);
        let a = arg_dari_bits(v, 8, false);
        assert_eq!(a.to_hex_str(), "xa");
        assert_eq!(a.to_bits_str(), "xx001010");
    }

    #[test]
    fn hex_menampilkan_z_bukan_x() {
        // LRM §5.7.1: `z` (impedansi tinggi) lebih spesifik dari `x`, jadi
        // nibble yang mengandung `z` tercetak `z` walau digitnya juga punya
        // bit 0/1.
        // 8'b0100_1111 dengan bit 4 diganti `z` -> nibble atas menjadi `z`
        // walau digitnya juga punya bit 0 dan 1 yang bernilai.
        let mut v = Bits::<8>::from_u64(0x4F);
        v.set(4, Logic::Z);
        let a = arg_dari_bits(v, 8, false);
        assert_eq!(a.to_hex_str(), "zf");
        assert_eq!(a.to_bits_str(), "010z1111");
    }

    #[test]
    fn hex_mengabaikan_bit_di_luar_lebar_logis() {
        // Bit di luar lebar logis bukan bagian nilai. Kalau ikut dihitung,
        // setiap nilai pada lebar bukan kelipatan empat akan tercetak `x`.
        let mut v = Bits::<8>::zero();
        v.set(5, Logic::One);
        v.set(6, Logic::X);
        v.set(7, Logic::X);
        let a = arg_dari_bits(v, 6, false);
        // Lebar 6 -> dua digit heksa; digit atas hanya membaca bit 4..5 = 0b10.
        assert_eq!(a.to_hex_str(), "20");
    }

    #[test]
    fn i64_lebar_64_negatif_tidak_panic() {
        // `1i64 << 64` adalah shift overflow: panic di build debug, wrap
        // diam-diam di release. Nilai negatif 64-bit harus tetap terbaca -1.
        let a = FormatArg::<64> {
            value: Bits::<64>::from_u64(u64::MAX),
            width: 64,
            signed: true,
        };
        assert_eq!(a.to_i64(), -1);
    }

    #[test]
    fn i64_lebar_64_tidak_menyalin_bit_ke_atas() {
        // Bit 63 = 0 berarti positif penuh, bukan negatif akibat wrap.
        let a = FormatArg::<64> {
            value: Bits::<64>::from_u64(u64::MAX >> 1),
            width: 64,
            signed: true,
        };
        assert_eq!(a.to_i64(), i64::MAX);
    }

    #[test]
    fn to_signed_lebar_32_negatif() {
        // Helper yang sama dipakai perbandingan bertanda di codegen.
        let v = Bits::<64>::from_u64(0xFFFF_FFFF);
        assert_eq!(crate::to_signed::<32, 64>(v), -1);
    }

    #[test]
    fn to_signed_lebar_di_atas_64_dibatasi() {
        // Lebar di atas 64 tidak bisa diwakili i64; harus membaca yang ada
        // tanpa menggeser berlebihan.
        let v = Bits::<64>::from_u64(5);
        assert_eq!(crate::to_signed::<128, 64>(v), 5);
    }

    #[test]
    fn format_tanpa_spesifikasi_menampilkan_desimal() {
        let hasil = format_args("", &[arg(7, 8, false), arg(9, 8, false)]);
        assert_eq!(hasil, "7 9\n");
    }

    #[test]
    fn hex_mengikuti_lebar_logis() {
        let hasil = format_args("%h", &[arg(0xAB, 8, false)]);
        assert_eq!(hasil, "ab\n");
    }

    #[test]
    fn biner_mengikuti_lebar_logis() {
        let hasil = format_args("%b", &[arg(0b1010, 8, false)]);
        assert_eq!(hasil, "00001010\n");
    }

    #[test]
    fn desimal_bertanda_menampilkan_minus() {
        let hasil = format_args("%d", &[arg(0xFF, 8, true)]);
        assert_eq!(hasil, "-1\n");
    }

    #[test]
    fn nol_leading_menampilkan_nol_depan() {
        let hasil = format_args("%04d", &[arg(7, 8, false)]);
        assert_eq!(hasil, "0007\n");
    }

    #[test]
    fn persen_ganda_menghasilkan_literal_persen() {
        let hasil = format_args::<8>("100%%", &[]);
        assert_eq!(hasil, "100%\n");
    }

    #[test]
    fn spesifikasi_tanpa_argumen_ditambah_otomatis() {
        // LRM §20.2: spesifikasi tanpa argumen ditampilkan apa adanya.
        let hasil = format_args("a=%d b=%d", &[arg(1, 8, false)]);
        assert_eq!(hasil, "a=1 b=%d\n");
    }

    #[test]
    fn argumen_berlebihan_dicetak_sebagai_desimal() {
        let hasil = format_args("x=%d", &[arg(1, 8, false), arg(2, 8, false)]);
        assert_eq!(hasil, "x=1 2\n");
    }

    #[test]
    fn teks_biasa_keloloskan_apa_adanya() {
        let hasil = format_args::<8>("halo dunia", &[]);
        assert_eq!(hasil, "halo dunia\n");
    }

    #[test]
    fn format_waktu_menggunakan_satuan_otomatis() {
        // LRM §20.4: `%t` menulis waktu lengkap dengan satuannya.
        let hasil = format_args::<64>("t=%t", &[waktu(100)]);
        assert_eq!(hasil, "t=100ns\n");
    }

    /// Waktu simulasi lebarnya 64 bit, jadi argumennya perlu `Bits<64>`.
    fn waktu(nanos: u64) -> FormatArg<64> {
        FormatArg {
            value: Bits::from_u64(nanos),
            width: 64,
            signed: false,
        }
    }

    #[test]
    fn format_waktu_memilih_satuan_terbesar_yang_habis_dibagi() {
        // 2000 ns habis dibagi microsecond => ditulis "2us"; 1500 ns tidak
        // habis dibagi, jadi tetap "1500ns".
        let hasil = format_args::<64>("%t %t", &[waktu(2000), waktu(1500)]);
        assert_eq!(hasil, "2us 1500ns\n");
    }

    #[test]
    fn format_waktu_nol_menghasilkan_nol_tanpa_satuan() {
        let hasil = format_args::<64>("t=%t", &[waktu(0)]);
        assert_eq!(hasil, "t=0\n");
    }
}

#[cfg(test)]
mod test_empat_state_desimal {
    use super::*;

    /// BUG (ditemukan fuzzing diferensial): `%d` memakai `to_u64()` yang
    /// menghitung digit `x`/`z` sebagai 0, jadi variabel yang belum
    /// diinisialisasi tercetak `0` — persis kebalikan dari `%h`/`%b` yang
    /// sudah menjaganya. LRM §4.3.1 memberi nilai awal `x`, dan iverilog
    /// mencetak `x`.
    #[test]
    fn persen_d_dari_nilai_x_mencetak_x_bukan_nol() {
        let arg = FormatArg {
            value: Bits::<8>::all(Logic::X),
            width: 8,
            signed: false,
        };
        assert_eq!(arg.teks_known_desimal(), None);
        assert_eq!(arg.teks_unknown_desimal(), "x");
    }

    #[test]
    fn persen_d_dari_nilai_z_mencetak_z() {
        let arg = FormatArg {
            value: Bits::<8>::all(Logic::Z),
            width: 8,
            signed: false,
        };
        assert_eq!(arg.teks_unknown_desimal(), "z");
    }

    #[test]
    fn persen_d_dari_nilai_known_tetap_angka() {
        let arg = FormatArg {
            value: Bits::<8>::from_u64(42),
            width: 8,
            signed: false,
        };
        assert_eq!(arg.teks_known_desimal().as_deref(), Some("42"));
    }

    #[test]
    fn persen_d_campuran_angka_dan_x_mencetak_x() {
        // Bit 0 yang bernilai 1 tidak boleh membuat nilainya terbaca pasti.
        let mut v = Bits::<8>::all(Logic::X);
        v.set(0, Logic::One);
        let arg = FormatArg {
            value: v,
            width: 8,
            signed: false,
        };
        assert_eq!(arg.teks_unknown_desimal(), "x");
    }

    #[test]
    fn persen_d_signed_dari_x_tetap_x() {
        let arg = FormatArg {
            value: Bits::<8>::all(Logic::X),
            width: 8,
            signed: true,
        };
        assert_eq!(arg.teks_unknown_desimal(), "x");
    }
}
