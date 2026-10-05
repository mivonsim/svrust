// Tanggung jawab: konversi lebar antar Bits<N> (resize/widen/truncate).
use crate::bits::Bits;
use crate::logic::Logic;

impl<const N: usize> Bits<N> {
    /// Perlebar ke lebar lebih besar, zero-extend.
    pub fn widen<const M: usize>(&self) -> Bits<M> {
        let mut out = Bits::<M>::zero();
        let copy = N.min(M);
        for i in 0..copy {
            out.set(i, self.get(i));
        }
        out
    }

    /// Perlebar ke lebar lebih besar, sign-extend bila MSB adalah 1.
    pub fn sign_extend<const M: usize>(&self) -> Bits<M> {
        let mut out = self.widen::<M>();
        if N == 0 {
            return out;
        }
        let top = self.get(N - 1);
        if top == Logic::One {
            for i in N..M {
                out.set(i, Logic::One);
            }
        }
        out
    }

    /// Perkecil ke lebar lebih kecil (truncate bit atas).
    pub fn resize<const M: usize>(&self) -> Bits<M> {
        let mut out = Bits::<M>::zero();
        let copy = N.min(M);
        for i in 0..copy {
            out.set(i, self.get(i));
        }
        out
    }

    /// Konversi signed: tanda disebar ke seluruh lebar.
    pub fn to_signed_wide<const M: usize>(&self) -> Bits<M> {
        self.sign_extend::<M>()
    }
}

/// Rakit beberapa nilai menjadi satu vektor (LRM §11.8.1).
///
/// `parts` dan `lebars` berpasangan: item paling kiri menjadi MSB. Digit
/// `X`/`Z` ikut terbawa ke posisi yang sama — memakai `to_u64` di sini akan
/// membuangnya, sehingga `{8'hxA, 8'h5Z}` jadi `0a50` bukan `xa5z`.
pub fn concat_kbits<const M: usize>(parts: &[Bits<M>], lebars: &[usize]) -> Bits<M> {
    let mut out = Bits::<M>::zero();
    let mut geser = lebars.iter().sum::<usize>();
    for (part, lebar) in parts.iter().zip(lebars.iter()) {
        geser -= lebar;
        for i in 0..*lebar {
            let digit = part.get(i);
            if !matches!(digit, Logic::Zero) {
                out.set(geser + i, digit);
            }
        }
    }
    out
}

/// Ambil `width` bit dari `lsb` ke atas (LRM §11.8.1 part-select).
///
/// Digit `X`/`Z` ikut terbawa — `to_u64` akan membuangnya sehingga `x`/`z`
/// pada hasil part-select hilang.
///
/// `W` adalah lebar **logis** `v`, bukan `M`. Penjaga di luar jangkauan harus
/// membandingkan dengan `W`: `M` adalah kapasitas array Rust, yang selalu lebih
/// besar dari lebar sinyal mana pun, sehingga penjaga berbasis `M` tidak pernah
/// kena dan bit di luar sinyal terbaca sebagai angka, bukan `X` (LRM §7.8).
pub fn select_kbits<const W: usize, const M: usize>(v: Bits<M>, lsb: u32, width: u32) -> Bits<M> {
    let mut out = Bits::<M>::zero();
    for i in 0..width {
        let sumber = match lsb.checked_add(i) {
            Some(s) => s as usize,
            None => continue,
        };
        // `W` (lebar logis) bisa lebih besar dari `M` (kapasitas vektor) kalau
        // pemanggil salah menghitung lebar design; `v.get` mengembalikan `X`
        // untuk posisi di luar kapasitas, jadi kedua batas tetap diperiksa.
        let digit = if sumber < W && sumber < M {
            v.get(sumber)
        } else {
            Logic::X
        };
        if !matches!(digit, Logic::Zero) {
            out.set(i as usize, digit);
        }
    }
    out
}

/// Baca elemen ke-`index` dari vektor rata (LRM §7.8).
///
/// Sinyal unpacked disimpan rata selebar `ELEM * N` dengan elemen ke-`k` pada
/// bit `[k*ELEM +: ELEM]`. Indeks di luar jangkauan menghasilkan `X` — nilai
/// tak diketahui, sesuai LRM §7.8 indeks out-of-bounds.
///
/// `W` adalah lebar **logis** vektor rata, bukan `N`. Penjaga OOB harus
/// membandingkan dengan `W`: `N` adalah kapasitas array Rust (`MAX_WIDTH`),
/// yang selalu lebih besar dari lebar sinyal mana pun, sehingga penjaga
/// berbasis `N` tidak pernah kena dan indeks di atas lebar logis membaca nol
/// alih-alih `X`.
pub fn index_read<const ELEM: usize, const W: usize, const N: usize>(
    base: &Bits<N>,
    index: &Bits<N>,
) -> Bits<N> {
    let mut out = Bits::<N>::zero();
    let geser = index
        .to_u64()
        .checked_mul(ELEM as u64)
        .and_then(|g| usize::try_from(g).ok())
        .filter(|g| g.checked_add(ELEM).is_some_and(|akhir| akhir <= W.min(N)));
    let Some(geser) = geser.filter(|_| index.is_fully_known()) else {
        // Indeks di luar jangkauan atau indeksnya sendiri tak diketahui:
        // seluruh elemen `X` (LRM §7.8).
        for bit in 0..ELEM.min(N) {
            out.set(bit, Logic::X);
        }
        return out;
    };
    for bit in 0..ELEM.min(N) {
        out.set(bit, base.get(geser + bit));
    }
    out
}

/// Bit-select dengan indeks runtime: `a[i]` pada sinyal **packed** (LRM §7.8).
///
/// Berbeda dengan `index_read`, elemennya satu bit pada posisi `i` — bukan
/// `i * ELEM`. Indeks di luar jangkauan menghasilkan `X` (LRM §7.8).
///
/// `W` adalah lebar **logis** base, bukan `N` (kapasitas array Rust). Tanpa
/// itu penjaga OOB tidak pernah kena: `a[8]` pada `logic [7:0] a` akan membaca
/// bit yang ada di luar sinyal dan menghasilkan angka berbeda tergantung
/// `MAX_WIDTH` design — hasil simulasi yang bergantung pada konfigurasi build.
pub fn select_bit<const W: usize, const N: usize>(base: &Bits<N>, index: &Bits<N>) -> Bits<N> {
    let mut out = Bits::<N>::zero();
    // Indeks tak diketahui menghasilkan `X`, bukan angka — `to_u64()` menghitung
    // digit `x`/`z` sebagai 0 sehingga `a[3'bxxx]` diam-diam membaca bit 0.
    let geser = usize::try_from(index.to_u64())
        .ok()
        .filter(|i| *i < W && index.is_fully_known());
    match geser {
        Some(i) => out.set(0, base.get(i)),
        None => out.set(0, Logic::X),
    }
    out
}

/// Masker `width` digit `1` mulai dari posisi `lsb`.
///
/// Dipakai untuk part-select pada LHS (LRM §10.10.1). Masker HARUS bertipe
/// `Bits`, bukan `u64`: sinyal dapat lebih lebar dari 64 bit (array unpacked
/// disimpan rata, `logic [127:0]`), dan `u64` membuat irisan di atas bit 63
/// salah geser — di build debug `1u64 << 64` panic, di release `<<` di-wrap
/// diam-diam sehingga tulisan mendarat di bit yang salah.
pub fn range_mask<const N: usize>(lsb: u32, width: u32) -> Bits<N> {
    let mut out = Bits::<N>::zero();
    for i in 0..width {
        if let Some(pos) = lsb.checked_add(i) {
            if (pos as usize) < N {
                out.set(pos as usize, Logic::One);
            }
        }
    }
    out
}

/// Perbandingan `case` pada lebar logis `W` (LRM §12.5).
///
/// `label`, `x_wildcard`, dan `z_wildcard` bertipe `u64` jadi lebar selector
/// dibatasi 64 bit — elaborator menolak selektor lebih lebar dengan pesan jelas.
///
/// Bandingkan per digit, bukan lewat `to_u64()`: `to_u64()` membuang digit
/// `x`/`z` sehingga pada selektor lebar semua bit di atas 63 hilang dari
/// perbandingan dan cabang yang salah ikut diambil.
///
/// `x_wildcard` sudah disesuaikan jenis statementnya oleh pemanggil:
/// `casex` menyertakan posisi `x`, `casez` tidak.
pub fn case_eq<const W: usize, const N: usize>(
    sel: &Bits<N>,
    label: u64,
    x_wildcard: u64,
    z_wildcard: u64,
) -> bool {
    let semua = if W >= 64 { u64::MAX } else { (1u64 << W) - 1 };
    let x_wildcard = x_wildcard & semua;
    let z_wildcard = z_wildcard & semua;
    for i in 0..W.min(64) {
        let s = sel.get(i);
        let l = matches!(label >> i & 1, 1);
        let cocok = if x_wildcard >> i & 1 == 1 {
            // LRM §12.5: pada `casex`, `x`/`z`/`?` di label adalah
            // don't-care — cocok dengan digit apa pun pada selektor.
            true
        } else if z_wildcard >> i & 1 == 1 {
            // Pada `casez`, `z`/`?` di label juga don't-care. Digit `x` di
            // label tetap dibandingkan (hanya `casex` yang melonggarkannya).
            true
        } else {
            // Digit `x`/`z` pada SELEKTOR hanya cocok dengan `x`/`z` pada
            // label; kalau label sudah 0/1 (di atas), tidak cocok.
            matches!(s, Logic::Zero | Logic::One) && (if l { Logic::One } else { Logic::Zero }) == s
        };
        if !cocok {
            return false;
        }
    }
    true
}

/// Baca nilai sebagai integer bertanda pada lebar logis `W`.
///
/// LRM §11.4.5: perbandingan bertanda memakai nilai bertanda sepanjang lebar
/// operand. Tanpa ini `-1` pada lebar 32 terbaca sebagai 4294967295 karena
/// `to_u64` hanya melihat bit mentah. X/Z dihitung 0, seperti `to_u64`.
///
/// `W` di-cap ke 64 karena nilai tidak bisa diwakili di luar itu; untuk lebar
/// lebih besar, bit di atas 64 tidak terbaca — sama seperti `to_u64`.
pub fn to_signed<const W: usize, const M: usize>(v: Bits<M>) -> i64 {
    let lebar = W.min(64);
    if lebar == 0 {
        return 0;
    }
    let mentah = v.to_u64();
    let tanda = 1u64 << (lebar - 1);
    if mentah & tanda == 0 {
        return mentah as i64;
    }
    // Untuk lebar 64, representasi dua's complement sudah persis hasilnya.
    if lebar == 64 {
        return mentah as i64;
    }
    (mentah as i64) - (1i64 << lebar)
}

/// Digit ke-`i` dari nilai yang lebar logisnya `SRC`; di atas `SRC` pakai `fill`.
///
/// `SRC` adalah lebar logis nilai, bukan lebar penyimpanan `M`. Bit di atas
/// `SRC` tidak Carry nilai apa pun, jadi isinya harus diganti `fill` — bukan
/// dibaca apa adanya dari penyimpanan.
fn digit_dalam<const SRC: usize, const M: usize>(v: &Bits<M>, i: usize, fill: Logic) -> Logic {
    if i < SRC {
        v.get(i)
    } else {
        fill
    }
}

/// Ubah nilai yang lebar logisnya `SRC` menjadi lebar logis `DST` tanpa tanda.
///
/// LRM §6.14: cast ke tipe unsigned memakai ekspresi sized, sehingga saat
/// melebar bit atas diisi nol dan saat menyempit bit atas dipotong. Lebar
/// penyimpanan `M` tidak berubah — hanya makna bit di atas `DST` yang
/// berubah, sehingga kode yang membaca nilai dengan lebar logis lain tetap
/// benar.
pub fn resize<const SRC: usize, const DST: usize, const M: usize>(v: Bits<M>) -> Bits<M> {
    let mut out = Bits::<M>::zero();
    for i in 0..DST {
        out.set(i, digit_dalam::<SRC, M>(&v, i, Logic::Zero));
    }
    out
}

/// Ubah nilai yang lebar logisnya `SRC` (signed) menjadi lebar logis `DST`.
///
/// LRM §6.14: cast ke tipe signed mempertahankan tanda, jadi saat melebar bit
/// MSB asli disalin ke seluruh bit baru. Saat menyempit tidak ada yang perlu
/// disalin karena bit atas dipotong.
pub fn sign_extend<const SRC: usize, const DST: usize, const M: usize>(v: Bits<M>) -> Bits<M> {
    // `SRC == 0` tidak punya bit tanda; LRM menganggapnya nol.
    let sign = if SRC == 0 {
        Logic::Zero
    } else {
        v.get(SRC - 1)
    };
    let mut out = Bits::<M>::zero();
    for i in 0..DST {
        out.set(i, digit_dalam::<SRC, M>(&v, i, sign));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logic::Logic;
    use crate::SignalCell;

    // BUG-1: `resize`/`sign_extend` untuk cast LRM §6.14 sebelumnya tidak ada
    // sama sekali, padahal codegen sudah memanggilnya. Lebar yang dipakai
    // adalah lebar *logis* sumber dan tujuan, bukan lebar penyimpanan, jadi
    // test wajib memakai `M` yang lebih besar dari `SRC`.

    /// Masker seluruh bit pada lebar logis tertentu.
    fn mask(n: usize) -> u64 {
        if n == 0 {
            0
        } else if n >= 64 {
            u64::MAX
        } else {
            (1u64 << n) - 1
        }
    }

    /// Baca `DST` bit hasil sebagai integer; X/Z dihitung 0.
    fn nilai<const M: usize>(v: &Bits<M>, dst: usize) -> u64 {
        let mut out = 0u64;
        for i in 0..dst {
            if matches!(v.get(i), Logic::One) {
                out |= 1u64 << i;
            }
        }
        out
    }

    #[test]
    fn resize_melebar_mengisi_nol() {
        // 8'hA5 -> 16'h00A5: cast unsigned tidak menyalin MSB sumber.
        let hasil = resize::<8, 16, 16>(Bits::<16>::from_u64(0xA5));
        assert_eq!(nilai(&hasil, 16), 0x00A5);
    }

    #[test]
    fn resize_menyempit_memotong() {
        // 16'hBEEF -> 8'hEF.
        let hasil = resize::<16, 8, 16>(Bits::<16>::from_u64(0xBEEF));
        assert_eq!(nilai(&hasil, 8), 0xEF);
    }

    #[test]
    fn resize_lebar_sama_tidak_mengubah_nilai() {
        let hasil = resize::<8, 8, 16>(Bits::<16>::from_u64(0xA5));
        assert_eq!(nilai(&hasil, 8), 0xA5);
    }

    #[test]
    fn resize_mengabaikan_bit_di_atas_lebar_sumber() {
        // Sumber 8 bit: bit 8..15 bukan bagian nilai, jadi saat melebar ke 16
        // bit harus diisi nol, bukan ikut terbawa.
        let hasil = resize::<8, 16, 16>(Bits::<16>::from_u64(0xFFA5));
        assert_eq!(nilai(&hasil, 16), 0x00A5);
    }

    #[test]
    fn sign_extend_melebar_menyalin_msb() {
        // -8'sd3 = 8'hFD -> 16'hFFFD: MSB sumber disalin ke semua bit baru.
        let hasil = sign_extend::<8, 16, 16>(Bits::<16>::from_u64(0xFD));
        assert_eq!(nilai(&hasil, 16), 0xFFFD);
    }

    #[test]
    fn sign_extend_msb_nol_tidak_menyalin() {
        // 8'h7F positif -> 16'h007F, bukan 16'h7FFF.
        let hasil = sign_extend::<8, 16, 16>(Bits::<16>::from_u64(0x7F));
        assert_eq!(nilai(&hasil, 16), 0x007F);
    }

    #[test]
    fn sign_extend_menyempit_tidak_mengubah() {
        // 16'hFFFD -> 8'hFD: saat menyempit tidak ada yang perlu disalin.
        let hasil = sign_extend::<16, 8, 16>(Bits::<16>::from_u64(0xFFFD));
        assert_eq!(nilai(&hasil, 8), 0xFD);
    }

    #[test]
    fn sign_extend_lebar_sama_tidak_mengubah() {
        // Sign cast `signed'(x)` tidak mengubah lebar maupun nilainya.
        let hasil = sign_extend::<8, 8, 16>(Bits::<16>::from_u64(0xFD));
        assert_eq!(nilai(&hasil, 8), 0xFD);
    }

    #[test]
    fn resize_lebar_sumber_nol_hasil_nol() {
        // Lebar 0 tidak punya bit; hasil harus kosong, bukan panic.
        let hasil = resize::<0, 8, 16>(Bits::<16>::from_u64(0xFF));
        assert_eq!(nilai(&hasil, 8), 0);
        assert_eq!(mask(8), 0xFF);
    }

    #[test]
    fn sign_extend_sumber_nol_tidak_panic() {
        // `SRC == 0` tidak punya bit tanda; LRM menganggapnya nol.
        let hasil = sign_extend::<0, 8, 16>(Bits::<16>::from_u64(0xFF));
        assert_eq!(nilai(&hasil, 8), 0);
    }

    #[test]
    fn cast_mempertahankan_digit_x_dalam_lebar_tujuan() {
        // LRM §6.14: X/Z di dalam nilai ikut terbawa ke bit yang sama selama
        // bit itu masih berada di dalam lebar tujuan.
        let mut sumber = Bits::<16>::zero();
        sumber.set(2, Logic::X);
        sumber.set(0, Logic::One);
        let hasil = resize::<8, 4, 16>(sumber);
        assert_eq!(hasil.get(2), Logic::X);
        assert_eq!(hasil.get(0), Logic::One);
    }

    #[test]
    fn cast_membuang_digit_di_luar_lebar_tujuan() {
        // Bit di luar lebar tujuan dipotong, termasuk yang bernilai X: nilai
        // 4-bit tidak boleh menyimpan informasi tak diketahui di bit 4.
        let mut sumber = Bits::<16>::zero();
        sumber.set(4, Logic::X);
        let hasil = resize::<8, 4, 16>(sumber);
        assert_eq!(hasil.get(4), Logic::Zero);
    }

    #[test]
    fn cast_mengisi_nol_pada_bit_baru_bukan_x() {
        // Bit yang baru dibuat saat melebar diisi nol, bukan X — kalau diisi X
        // maka setiap cast melebar akan membuat nilai tak diketahui.
        let hasil = resize::<8, 16, 16>(Bits::<16>::from_u64(0xA5));
        assert!(hasil.is_fully_known());
        assert_eq!(hasil.get(8), Logic::Zero);
    }

    #[test]
    fn sign_extend_menyalin_digit_x_sebagai_tanda() {
        // MSB sumber yang X ikut disalin sebagai tanda, supaya hasilnya tetap
        // tidak diketahui — bukan diam-diam jadi 0.
        let mut sumber = Bits::<16>::zero();
        sumber.set(7, Logic::X);
        let hasil = sign_extend::<8, 16, 16>(sumber);
        assert_eq!(hasil.get(7), Logic::X);
        assert_eq!(hasil.get(8), Logic::X);
        assert_eq!(hasil.get(6), Logic::Zero);
    }

    // BUG: `a[i]` pada sinyal packed dan `mem[i]` pada array unpacked punya
    // sintaks sama, tapi geser posisinya berbeda: `i` versus `i * lebar`.
    // Semuanya dilayani satu helper yang menggeser `i * lebar`, sehingga
    // bit-select packed selalu jatuh di luar jangkauan dan hasilnya nol.

    #[test]
    fn select_bit_membaca_satu_bit_pada_posisi_indeks() {
        // 0xA5 = 1010_0101; bit 2 = 1.
        let base = Bits::<64>::from_u64(0xA5);
        let hasil = select_bit::<8, 64>(&base, &Bits::<64>::from_u64(2));
        assert_eq!(nilai(&hasil, 1), 1, "bit 2 dari 0xA5 adalah 1");
        // Bit 0 = 1, bit 1 = 0.
        assert_eq!(
            nilai(&select_bit::<8, 64>(&base, &Bits::<64>::from_u64(0)), 1),
            1
        );
        assert_eq!(
            nilai(&select_bit::<8, 64>(&base, &Bits::<64>::from_u64(1)), 1),
            0
        );
    }

    #[test]
    fn select_bit_menghasilkan_x_bukan_nol_saat_di_luar_jangkauan() {
        // LRM §7.8: bit di luar jangkauan menghasilkan `X`. Penjaga OOB harus
        // membandingkan dengan lebar LOGIS `W`, bukan kapasitas `N`: kalau
        // `N` yang dipakai, `a[8]` pada `logic [7:0] a` membaca bit yang ada
        // di luar sinyal dan hasilnya bergantung pada konfigurasi build.
        let base = Bits::<64>::from_u64(u64::MAX);
        let hasil = select_bit::<8, 64>(&base, &Bits::<64>::from_u64(8));
        assert_eq!(hasil.get(0), Logic::X, "indeks 8 pada sinyal 8 bit harus X");
        assert!(
            !hasil.is_fully_known(),
            "hasil di luar jangkauan tidak boleh diketahui"
        );
    }

    #[test]
    fn select_bit_membandingkan_dengan_lebar_logis_bukan_kapasitas() {
        // Penjaga utama: `N` = 64 adalah kapasitas, `W` = 8 lebar logis.
        // Bit 8..63 sengaja berisi 1 supaya bug terlihat kalau `W` diabaikan.
        let mut base = Bits::<64>::zero();
        for bit in 8..64 {
            base.set(bit, Logic::One);
        }
        let hasil = select_bit::<8, 64>(&base, &Bits::<64>::from_u64(9));
        assert_eq!(
            hasil.get(0),
            Logic::X,
            "indeks 9 di luar lebar logis 8 harus X, bukan membaca bit di luar sinyal"
        );
    }

    #[test]
    fn select_bit_bukan_elemen_rata_seperti_index_read() {
        // Penjaga utama: keduanya harus berbeda. `index_read::<8, 64>` menggeser
        // `idx * 8`; `select_bit` menggeser `idx`.
        let base = Bits::<64>::from_u64(0xA5);
        let idx = Bits::<64>::from_u64(2);
        let bit = select_bit::<8, 64>(&base, &idx);
        let elem = index_read::<8, 32, 64>(&base, &idx);
        assert_eq!(nilai(&bit, 1), 1, "bit-select: bit 2 dari 0xA5 = 1");
        assert_eq!(
            nilai(&elem, 8),
            0,
            "elemen array pada offset 16 bit di luar jangkauan 4 elemen -> X, bukan angka"
        );
    }

    #[test]
    fn index_read_membaca_elemen_rata_utuh() {
        // Empat elemen 8 bit: elemen 2 pada bit [16..24).
        let mut base = Bits::<64>::zero();
        for bit in 16..24 {
            base.set(bit, Logic::One);
        }
        let hasil = index_read::<8, 32, 64>(&base, &Bits::<64>::from_u64(2));
        assert_eq!(nilai(&hasil, 8), 0xFF, "elemen 2 harus terbaca utuh");
    }

    // BUG: indeks dengan digit `x`/`z` dihitung `to_u64()` sebagai 0, sehingga
    // `a[3'bxxx]` diam-diam membaca bit 0. LRM §7.8: hasilnya `X`.
    #[test]
    fn select_bit_dengan_indeks_x_menghasilkan_x() {
        let base = Bits::<64>::all(Logic::One);
        let mut idx = Bits::<64>::zero();
        for bit in 0..3 {
            idx.set(bit, Logic::X);
        }
        let hasil = select_bit::<8, 64>(&base, &idx);
        assert_eq!(hasil.get(0), Logic::X, "indeks x harus menghasilkan X");
        let mut idx_z = Bits::<64>::zero();
        idx_z.set(0, Logic::Z);
        assert_eq!(
            select_bit::<8, 64>(&base, &idx_z).get(0),
            Logic::X,
            "indeks z juga tak diketahui"
        );
    }

    #[test]
    fn index_read_dengan_indeks_x_menghasilkan_x() {
        let base = Bits::<64>::all(Logic::One);
        let mut idx = Bits::<64>::zero();
        idx.set(1, Logic::X);
        let hasil = index_read::<8, 32, 64>(&base, &idx);
        for bit in 0..8 {
            assert_eq!(hasil.get(bit), Logic::X, "bit {bit} harus X");
        }
    }

    // BUG: penjaga OOB `index_read` membandingkan dengan `N` (kapasitas vektor
    // Rust), bukan `W` (lebar logis). Test ini membedakan keduanya: bit
    // 32..63 sengaja berisi 1, jadi kalau `W` diabaikan hasilnya terbaca
    // sebagai angka, bukan `X`.
    #[test]
    fn index_read_membandingkan_dengan_lebar_logis_bukan_kapasitas() {
        let mut base = Bits::<64>::zero();
        for bit in 32..64 {
            base.set(bit, Logic::One);
        }
        // Empat elemen 8 bit = 32 bit. Elemen ke-4 sudah di luar jangkauan.
        let hasil = index_read::<8, 32, 64>(&base, &Bits::<64>::from_u64(4));
        assert_eq!(hasil.get(0), Logic::X, "elemen 4 dari 4 elemen harus X");
        assert!(!hasil.is_fully_known());
        // Elemen ke-3 masih sah: posisi 24..32, semua nol.
        let sah = index_read::<8, 32, 64>(&base, &Bits::<64>::from_u64(3));
        assert!(sah.is_fully_known(), "elemen 3 masih dalam jangkauan");
    }

    // BUG: penjaga OOB `select_kbits` juga membandingkan dengan `M`.
    #[test]
    fn select_kbits_membandingkan_dengan_lebar_logis_bukan_kapasitas() {
        let mut base = Bits::<64>::zero();
        for bit in 8..64 {
            base.set(bit, Logic::One);
        }
        // Sinyal logis 8 bit dengan semua bit di atas 8 berisi 1. Ambil 4 bit
        // dari posisi 6: posisi 6 dan 7 sah (bernilai 0), posisi 8 dan 9 di
        // luar jangkauan jadi harus `X`.
        let hasil = select_kbits::<8, 64>(base, 6, 4);
        assert_eq!(hasil.get(0), Logic::Zero, "posisi 6 sah dan bernilai 0");
        assert_eq!(hasil.get(1), Logic::Zero, "posisi 7 sah dan bernilai 0");
        assert_eq!(
            hasil.get(2),
            Logic::X,
            "posisi 8 di atas lebar logis harus X"
        );
        assert_eq!(
            hasil.get(3),
            Logic::X,
            "posisi 9 di atas lebar logis harus X"
        );
    }

    #[test]
    fn select_kbits_dalam_jangkauan_tetap_terbaca() {
        let hasil = select_kbits::<8, 64>(Bits::<64>::from_u64(0xA5), 0, 4);
        assert_eq!(nilai(&hasil, 4), 0x5);
    }

    // BUG: `write_masked` memakai `to_u64()`, yang membuang digit `X`/`Z` dan
    // memotong bit di atas 63. Part-select pada LHS adalah satu-satunya jalur
    // tulis yang memakainya, jadi setiap `a[hi:lo] = x` kehilangan 4-state-nya.
    #[test]
    fn write_masked_mempertahankan_digit_x_dan_z() {
        let mut cell = SignalCell::<8>::new(Bits::from_u64(0xF0));
        // Nilai sudah digeser ke posisi irisan (bit 4..7), seperti yang
        // dilakukan codegen sebelum memanggil `write_masked`.
        let mut nilai = Bits::<8>::zero();
        nilai.set(6, Logic::One);
        nilai.set(5, Logic::X);
        nilai.set(4, Logic::Z);
        cell.write_masked(nilai, Bits::from_u64(0xF0));
        let hasil = cell.read();
        assert_eq!(hasil.get(6), Logic::One);
        assert_eq!(hasil.get(5), Logic::X, "X harus tersimpan");
        assert_eq!(hasil.get(4), Logic::Z, "Z harus tersimpan");
        assert!(!hasil.is_fully_known());
    }

    #[test]
    fn write_masked_menulis_bit_di_atas_63() {
        // BUG: `mask` bertipe `u64`, jadi irisan di atas bit 63 tidak bisa
        // diwakili — `1u64 << 120` panic di debug dan wrap di release.
        let mut cell = SignalCell::<128>::new(Bits::<128>::zero());
        let nilai = Bits::<128>::from_u64(0xBB) << 120;
        cell.write_masked(nilai, range_mask::<128>(120, 8));
        let hasil = cell.read();
        // 0xBB = 1011_1011; tiap bit harus mendarat di posisinya.
        let pola = 0xBBu64;
        for bit in 120..128 {
            let expect = matches!(pola >> (bit - 120) & 1, 1);
            assert_eq!(
                matches!(hasil.get(bit), Logic::One),
                expect,
                "bit {bit} harus sesuai pola 0xBB"
            );
        }
        // Bit di luar irisan tetap nol.
        for bit in 112..120 {
            assert_eq!(hasil.get(bit), Logic::Zero, "bit {bit} harus nol");
        }
    }

    #[test]
    fn range_mask_mencakup_rentang_yang_benar() {
        let m = range_mask::<128>(4, 4);
        for bit in 0..128 {
            let expect = (4..8).contains(&bit);
            assert_eq!(matches!(m.get(bit), Logic::One), expect, "bit {bit} salah");
        }
    }

    #[test]
    fn range_mask_di_atas_bit_63_tidak_wrap() {
        let m = range_mask::<128>(64, 4);
        assert_eq!(m.get(64), Logic::One);
        assert_eq!(m.get(67), Logic::One);
        assert_eq!(m.get(63), Logic::Zero, "tidak boleh meluber ke bawah");
        assert_eq!(m.get(68), Logic::Zero, "tidak boleh meluber ke atas");
    }

    // BUG: `W` (lebar logis base) bisa lebih besar dari `N` (kapasitas vektor)
    // kalau pemanggil salah menghitung lebar design. `Bits::get` di luar
    // kapasitas mengembalikan `Logic::X`, jadi hasilnya `X` — bukan nilai yang
    // sebenarnya ada di base.
    #[test]
    fn lebar_logis_di_atas_kapasitas_menghasilkan_x_bukan_pembacaan_salah() {
        let base = Bits::<8>::all(Logic::One);
        let idx = Bits::<8>::from_u64(20);
        // `W` = 32 tapi kapasitas hanya 8 bit.
        let hasil = select_bit::<32, 8>(&base, &idx);
        assert_eq!(hasil.get(0), Logic::X, "harus X, bukan di luar kapasitas");
        // `select_kbits` juga wajib memeriksa kedua batas.
        let k = select_kbits::<32, 8>(Bits::<8>::all(Logic::One), 20, 4);
        for bit in 0..4 {
            assert_eq!(k.get(bit), Logic::X, "bit {bit} harus X");
        }
        // `index_read` dengan `W` lebih besar dari `N` tidak boleh panic.
        let e = index_read::<8, 1024, 8>(&Bits::<8>::all(Logic::One), &Bits::<8>::from_u64(20));
        assert!(!e.is_fully_known());
    }

    #[test]
    fn concat_kbits_tersedia_di_akar_runtime() {
        // Codegen memanggil `sv_runtime::concat_kbits::<MAX_WIDTH>`; kalau tidak
        // di-re-export, kode simulasi gagal dikompilasi dengan
        // "cannot find function `concat_kbits` in crate `sv_runtime`".
        let hasil = crate::convert::concat_kbits::<64>(
            &[Bits::<64>::from_u64(0xA), Bits::<64>::from_u64(0x5)],
            &[8, 8],
        );
        // LRM §11.8.1: item paling kiri mengisi MSB, jadi `{0xA, 0x5}` = 0x0A05.
        assert_eq!(nilai(&hasil, 16), 0x0A05);
    }
}
