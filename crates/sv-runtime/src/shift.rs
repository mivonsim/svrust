// Tanggung jawab: operator geser `<<`, `>>`, `>>>` pada Bits (LRM §11.4.10).
use crate::bits::Bits;
use crate::logic::Logic;

/// Geser kiri logis: bit yang vacated diisi nol.
///
/// `W` adalah lebar **logis** operand kiri, bukan kapasitas vektor `N`. Tanpa
/// itu, `logic [7:0] a` di design dengan `MAX_WIDTH = 32` akan menyimpan 32 bit,
/// dan `a << 1` pada helper berbasis `N` bisa menggeser bit di atas lebar sinyal
/// yang tidak pernah ditulis.
pub fn geser_kiri<const W: usize, const N: usize>(lhs: &Bits<N>, rhs: &Bits<N>) -> Bits<N> {
    geser::<W, N>(lhs, rhs, true, false)
}

/// Geser kanan logis (`>>`): posisi vacated diisi nol, apa pun signedness-nya.
pub fn geser_kanan_logis<const W: usize, const N: usize>(lhs: &Bits<N>, rhs: &Bits<N>) -> Bits<N> {
    geser::<W, N>(lhs, rhs, false, false)
}

/// Geser kanan aritmetik (`>>>`, LRM §11.4.10).
///
/// Posisi vacated diisi bit sign bila **tipe hasil** signed, dan nol kalau
/// unsigned. Penentuannya signedness hasil, bukan operand kiri — keduanya sama
/// untuk `>>>` karena lebar hasil mengikuti operand kiri, tapi penulisan
/// eksplisit mencegah helper ini salah dipakai bila nanti lebarnya dibedakan.
pub fn geser_kanan_aritmetik<const W: usize, const N: usize>(
    lhs: &Bits<N>,
    rhs: &Bits<N>,
    signed: bool,
) -> Bits<N> {
    geser::<W, N>(lhs, rhs, false, signed)
}

/// Badan bersama ketiga operator geser.
///
/// `kiri` memilih arah; `signed` hanya berpengaruh untuk geser kanan
/// aritmetik. Jumlah geser `x`/`z` membuat seluruh hasil `X` (LRM §11.4.10),
/// bukan angka — `to_u64()` menghitung digit tak diketahui sebagai 0 sehingga
/// tanpa penjaga ini `a >> 4'bzzzz` menjadi geser nol yang menyalin nilai `a`.
fn geser<const W: usize, const N: usize>(
    lhs: &Bits<N>,
    rhs: &Bits<N>,
    kiri: bool,
    signed: bool,
) -> Bits<N> {
    let lebar = W.min(N);
    let mut out = Bits::<N>::zero();
    if !rhs.is_fully_known() {
        // LRM §11.4.10: jumlah geser `x`/`z` membuat hasil unknown.
        for i in 0..lebar {
            out.set(i, Logic::X);
        }
        return out;
    }
    // Digit tak diketahui pada operand kiri ikut terbawa, jadi pergeseran
    // berbasis digit (bukan integer) seperti operasi aritmetika lain.
    let jumlah = usize::try_from(rhs.to_u64()).unwrap_or(usize::MAX);
    let sign = if signed && lebar > 0 {
        lhs.get(lebar - 1)
    } else {
        Logic::Zero
    };
    for i in 0..lebar {
        let src = if kiri {
            i.checked_sub(jumlah)
        } else {
            i.checked_add(jumlah)
        };
        let digit = match src {
            Some(s) if s < lebar => lhs.get(s),
            // Bit yang keluar dari jangkauan: nol untuk logis, sign untuk
            // aritmetik.
            _ => sign,
        };
        out.set(i, digit);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn geser_kiri_mengisi_nol() {
        let a = Bits::<8>::from_u64(0b0000_0011);
        let n = Bits::<8>::from_u64(2);
        assert_eq!(geser_kiri::<8, 8>(&a, &n).to_u64(), 0b0000_1100);
    }

    #[test]
    fn geser_kanan_logis_mengisi_nol() {
        let a = Bits::<8>::from_u64(0b1111_1100);
        let n = Bits::<8>::from_u64(2);
        assert_eq!(geser_kanan_logis::<8, 8>(&a, &n).to_u64(), 0b0011_1111);
    }

    #[test]
    fn geser_kanan_aritmetik_mengisi_tanda_bila_signed() {
        let a = Bits::<8>::from_u64(0b1000_0000);
        let n = Bits::<8>::from_u64(1);
        assert_eq!(geser_kanan_aritmetik::<8, 8>(&a, &n, true).to_u64(), 0b1100_0000);
    }

    #[test]
    fn geser_kanan_aritmetik_mengisi_nol_bila_unsigned() {
        // LRM §11.4.10: `>>>` pada tipe hasil unsigned sama dengan `>>`.
        let a = Bits::<8>::from_u64(0b1000_0000);
        let n = Bits::<8>::from_u64(1);
        assert_eq!(geser_kanan_aritmetik::<8, 8>(&a, &n, false).to_u64(), 0b0100_0000);
    }

    #[test]
    fn geser_di_luar_lebar_jadi_nol_atau_tanda() {
        let a = Bits::<8>::from_u64(0b1111_1111);
        let n = Bits::<8>::from_u64(20);
        assert_eq!(geser_kiri::<8, 8>(&a, &n).to_u64(), 0);
        assert_eq!(geser_kanan_logis::<8, 8>(&a, &n).to_u64(), 0);
        // Geser aritmetik melewati lebar penuh: seluruh bit hasil adalah sign.
        assert_eq!(geser_kanan_aritmetik::<8, 8>(&a, &n, true).to_u64(), 0xFF);
    }

    #[test]
    fn jumlah_geser_x_membuat_hasil_x() {
        // LRM §11.4.10: jumlah geser `x`/`z` menghasilkan unknown. Dulu
        // `to_u64()` menghitung `z` sebagai 0 sehingga hasilnya angka pasti.
        let a = Bits::<8>::from_u64(0b1010_0000);
        for n in [Bits::from_unknown(0, 0b0010, 0b0010, 4), Bits::from_unknown(0, 0, 0b0010, 4)]
        {
            assert_eq!(geser_kanan_logis::<8, 8>(&a, &n).to_hex_lebar(8), "xx");
            assert_eq!(geser_kiri::<8, 8>(&a, &n).to_hex_lebar(8), "xx");
        }
    }

    #[test]
    fn digit_x_pada_operand_kiri_ikut_terbawa() {
        let a = Bits::<8>::from_unknown(0b0000_1000, 0b0000_1000, 0, 8);
        let n = Bits::<8>::from_u64(2);
        // `x` pada bit 3 bergeser ke posisi 1.
        assert_eq!(geser_kanan_logis::<8, 8>(&a, &n).to_hex_lebar(8), "0x");
    }

    #[test]
    fn geser_menghormati_lebar_logis_bukan_kapasitas() {
        // Sinyal 4 bit di dalam vektor 16 bit: `1 << 4` harus keluar dari
        // jangkauan, bukan 저장 sebagai bit 4 yang tidak pernah ditulis.
        let a = Bits::<16>::from_u64(0b1000);
        let n = Bits::<16>::from_u64(4);
        assert_eq!(geser_kiri::<4, 16>(&a, &n).to_u64(), 0);
        assert_eq!(geser_kanan_logis::<4, 16>(&a, &n).to_u64(), 0);
    }
}