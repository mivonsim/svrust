// Tanggung jawab: menghitung waktu bangun absolut tiap segmen proses `initial`
// dan langkah waktu minimum dari proses waktu `always #N`.
//
// Begitu ada proses waktu (`always #5 clk = ~clk;`), jam simulasi punya
// penggeraknya sendiri, sehingga blok `initial` tidak boleh lagi melompat ke
// `#52` dalam satu langkah driver: pada waktu itu clock baru berayun dua kali,
// bukan sepuluh.
//
// Penjadwalan harus memakai waktu, bukan nomor langkah: segmen `initial` hanya
// boleh dijalankan setelah jam simulasi benar-benar mencapai waktu bangunnya
// (LRM §4.4 stratified event queue + §11.2 procedural timing control). Modul ini
// menghitung waktu bangun tiap segmen SEBELUM runtime sehingga syaratnya bisa
// ditulis sebagai konstanta — tanpa itu proses `initial` selalu lolos karena
// `time_now` masih 0 pada langkah pertama.
use sv_ir::process::{Process, ProcessKind, Statement};
use sv_ir::Design;
#[cfg(test)]
use sv_ir::TimeUnit;

/// Waktu bangun absolut sebuah segmen proses `initial`, dalam femtosecond.
///
/// `Some(n)` bila seluruh `#delay` sampai segmen itu berupa konstanta. `None`
/// bila ada delay yang nilainya baru diketahui saat runtime — pemanggil lalu
// jatuh ke perilaku lama (segmen berjalan sesuai urutan langkah).
pub type Wake = Option<u64>;

/// Nilai `#delay` dalam femtosecond bila amount-nya konstanta.
///
/// Delay runtime (`#n` dengan `n` variabel) menghasilkan `None` — pemanggil
/// harus berhenti dan tidak memakai konstanta, karena salah hitung membuat
/// segmen berjalan sebelum waktunya.
pub fn delay_femtos(delay: &Statement) -> Option<u64> {
    let Statement::Delay { amount, unit, .. } = delay else {
        return Some(0);
    };
    let sv_ir::Expr::Const { value, .. } = amount else {
        return None;
    };
    // `TimeUnit::femtos()` sudah tabel LRM §3.3; menyalin tabelnya di sini
    // berisiko keduanya berbeda diam-diam bila satu diperbarui.
    Some((*value).saturating_mul(unit.femtos()))
}

/// Waktu bangun tiap segmen satu proses `initial`, urut dari segmen 0.
///
/// Indeks hasil sejajar dengan `initial_step::segmen_process`: `wake[k]` adalah
/// waktu simulasi saat statement SEGMEN `k` benar-benar berjalan — yaitu waktu
/// setelah seluruh delay segmen-sebelumnya plus delay segmen itu sendiri.
///
/// Contoh `initial begin #5 a; #5 b; end` → segmen `[[], [Delay5,a], [Delay5,b]]`
/// → `wake = [0, 5, 10]` dalam femtosecond-adjusted unit.
pub fn wake_segments(process: &Process) -> Vec<Wake> {
    let mut wake = 0u64;
    let mut out = Vec::new();
    for segment in crate::initial_step::segmen_process(process) {
        // Segmen kosong (body yang diawali `#delay` menghasilkan segmen awal
        // kosong) TIDAK berarti delay tidak diketahui — waktu bangunnya sama
        // dengan waktu bangun segmen sebelumnya. Perlakukan sebagai nol
        // penundaan, bukan sebagai pembatalan rencana.
        let Some(delay) = segmen_delay(&segment) else {
            // Salah satu segmen tidak bisa dihitung: seluruh rencana dibuang
            // supaya pemanggil tahu tidak boleh memakainya. Waktu bangun
            // segmen berikutnya bergantung pada akumulasi delay segmen ini,
            // jadi menebak sisanya berarti salah diam-diam.
            return vec![None; out.len() + 1];
        };
        wake = wake.saturating_add(delay);
        out.push(Some(wake));
    }
    out
}

/// Delay (femtosecond) yang menentukan segmen `initial`.
///
/// Segmen setelah yang pertama diawali statement `Delay` atau `EventControl`
/// (lihat `initial_step::segmen_process`), jadi statement pertama sudah
/// menentukan delay segmen itu.
fn segmen_delay(segment: &[Statement]) -> Option<u64> {
    let Some(pertama) = segment.first() else {
        return Some(0);
    };
    match pertama {
        // `EventControl` menunggu edge, bukan menunda waktu — delay-nya nol.
        Statement::EventControl { .. } => Some(0),
        Statement::Delay { .. } => delay_femtos(pertama),
        // Bentuk lain tidak mungkin di awal segmen, tapi delay nol aman: yang
        // penting bukan salah hitung, melainkan tidak membatalkan rencana.
        _ => Some(0),
    }
}

/// Wake plan seluruh proses `initial` pada design, urut per proses.
///
/// Mengembalikan `None` bila salah satu proses punya delay runtime — pemanggil
/// lalu memakai jalur penjadwalan lama berbasis nomor langkah.
pub fn wake_plan(design: &Design) -> Option<Vec<Vec<Wake>>> {
    design
        .processes
        .iter()
        .filter(|p| p.kind == ProcessKind::Initial)
        .map(|process| {
            let plan = wake_segments(process);
            if plan.iter().all(Option::is_some) {
                Some(plan)
            } else {
                None
            }
        })
        .collect()
}

/// Delay terkecil (femtosecond) di seluruh proses waktu `always #N`.
///
/// Dipakai driver untuk menghitung berapa banyak iterasi yang dibutuhkan supaya
/// jam simulasi mencapai waktu bangun terakhir — tanpa itu clock berhenti di t=5
/// sementara testbench sudah menunggu t=52.
pub fn langkah_waktu_minimum(design: &Design) -> Option<u64> {
    design
        .processes
        .iter()
        .filter(|p| p.kind == ProcessKind::Timed)
        .filter_map(|process| process.body.iter().find_map(delay_dalam).flatten())
        .min()
}

/// Periode tiap proses waktu, dalam femtosecond, urut sama dengan
/// `processes` yang dibingkai `ProcessKind::Timed`.
///
/// Periode dipakai driver untuk membatasi clock: proses waktu tidak boleh
/// melewati waktu bangun segmen `initial` yang sedang menunggu, karena pada
/// waktu itulah statement testbench dijalankan dan harus melihat counter pada
/// nilai yang benar.
pub fn periode_waktu(design: &Design) -> Option<Vec<u64>> {
    design
        .processes
        .iter()
        .filter(|p| p.kind == ProcessKind::Timed)
        .map(|process| {
            process
                .body
                .iter()
                .find_map(delay_dalam)
                .and_then(|delay| delay)
        })
        .collect()
}

/// Delay konstanta pertama yang ditemukan pada statement atau turunannya.
fn delay_dalam(stmt: &Statement) -> Option<Wake> {
    match stmt {
        Statement::Delay { .. } => Some(delay_femtos(stmt)),
        Statement::Block { body, .. } | Statement::EventControl { body, .. } => {
            body.iter().find_map(delay_dalam)
        }
        Statement::If {
            then_branch,
            else_branch,
            ..
        } => then_branch.iter().chain(else_branch).find_map(delay_dalam),
        Statement::Case { arms, .. } => arms
            .iter()
            .find_map(|arm| arm.body.iter().find_map(delay_dalam)),
        Statement::For { body, .. }
        | Statement::Repeat { body, .. }
        | Statement::While { body, .. } => body.iter().find_map(delay_dalam),
        Statement::Assign { .. } | Statement::SystemTask { .. } | Statement::Noop { .. } => None,
    }
}

/// Waktu bangun paling akhir dari seluruh segmen `initial`, dalam femtosecond.
///
/// Dipakai driver sebagai batas bawah iterasi: simulasi harus berjalan sampai
/// waktu ini tercapai, bukan sampai jumlah segmen habis.
pub fn wake_terakhir(design: &Design) -> Option<u64> {
    let plan = wake_plan(design)?;
    plan.iter()
        .flat_map(|segmen| segmen.iter().flatten().copied())
        .max()
}

#[cfg(test)]
mod tests {
    use super::*;
    use sv_ir::datatype::DataType;
    use sv_ir::process::Span;

    fn konstanta(n: u64) -> sv_ir::Expr {
        sv_ir::Expr::constant(n, DataType::logic(8))
    }

    fn variabel() -> sv_ir::Expr {
        sv_ir::Expr::SignalRef {
            signal: 0,
            data_type: DataType::logic(8),
        }
    }

    fn delay(amount: sv_ir::Expr, unit: TimeUnit) -> Statement {
        Statement::Delay {
            amount,
            unit,
            body: Box::new(Statement::Noop {
                span: Span::default(),
            }),
            span: Span::default(),
        }
    }

    fn kosong() -> Statement {
        Statement::Noop {
            span: Span::default(),
        }
    }

    fn initial(body: Vec<Statement>) -> Process {
        Process {
            name: "initial".to_string(),
            kind: ProcessKind::Initial,
            sensitivity: Vec::new(),
            body,
            span: Span::default(),
        }
    }

    fn timed(body: Vec<Statement>) -> Process {
        Process {
            name: "always_timed".to_string(),
            kind: ProcessKind::Timed,
            sensitivity: Vec::new(),
            body,
            span: Span::default(),
        }
    }

    /// BUG: `eval_initial` lama menjalankan tiap segmen once per langkah driver
    /// tanpa memandangi waktu bangunnya, sehingga `initial #52 $display` langsung
    /// melompat ke t=52 padahal clock `always #5` baru berayun dua kali — nilai
    /// yang dibaca testbench jadi salah tanpa pesan. Rencana waktu bangun wajib
    /// menumpuk delay segmen demi segmen.
    #[test]
    fn wake_menumpuk_delay_berturut_urutan() {
        let plan = wake_segments(&initial(vec![
            kosong(),
            delay(konstanta(5), TimeUnit::NanoSeconds),
            delay(konstanta(5), TimeUnit::NanoSeconds),
        ]));
        assert_eq!(
            plan,
            vec![
                Some(0),
                Some(5 * TimeUnit::NanoSeconds.femtos()),
                Some(10 * TimeUnit::NanoSeconds.femtos())
            ],
            "segmen tanpa delay bangun di t=0, lalu tiap segmen menambah delaynya"
        );
    }

    /// BUG: satuan `#n` diabaikan saat menghitung waktu bangun, jadi `#5us`
    /// dijadwalkan seolah 5 nanosecond — testbench `$display` keluar 1000x
    /// terlalu cepat dan tidak pernah ketemu clock yang benar.
    ///
    /// Segmen awal kosong tetap ikut dihitung: `initial begin #5us ... end`
    /// Pecah jadi dua segmen, dan segmen kosong itu bangun di t=0.
    #[test]
    fn wake_memakai_satuan_delay() {
        let plan = wake_segments(&initial(vec![delay(konstanta(5), TimeUnit::MicroSeconds)]));
        assert_eq!(
            plan,
            vec![Some(0), Some(5 * TimeUnit::MicroSeconds.femtos())]
        );
    }

    /// BUG: delay runtime (`#n` dengan `n` variabel) tidak boleh dipaksa jadi
    /// konstanta nol — segmen akan selalu lolos dan blok `initial` melompat
    /// ke waktu yang salah sebelum stimulus-nya siap.
    #[test]
    fn wake_delay_runtime_membatalkan_rencana() {
        assert_eq!(
            wake_segments(&initial(vec![delay(variabel(), TimeUnit::NanoSeconds)])),
            vec![None, None]
        );
    }

    /// Satu segmen yang gagal membatalkan SELURUH rencana proses, bukan hanya
    /// segmen itu: waktu bangun segmen berikutnya bergantung pada akumulasi
    /// delay segmen sebelumnya, jadi menebak sisanya berarti salah diam-diam.
    #[test]
    fn wake_satu_delay_runtime_membatalkan_seluruh_rencana() {
        let plan = wake_segments(&initial(vec![
            delay(konstanta(5), TimeUnit::NanoSeconds),
            delay(variabel(), TimeUnit::NanoSeconds),
        ]));
        assert_eq!(plan, vec![None, None, None]);
    }

    /// BUG: dua proses `always #5` dan `always #3` dijumlahkan jadi satu
    /// periode gabungan, sehingga clock melompat 8 nanosecond per langkah dan
    /// `$time` meleset — yang dipakai adalah delay TERSINGKIR.
    #[test]
    fn langkah_waktu_minimum_mengambil_delay_tersingkir() {
        let mut design = Design::new("tb");
        design
            .processes
            .push(timed(vec![delay(konstanta(5), TimeUnit::NanoSeconds)]));
        design
            .processes
            .push(timed(vec![delay(konstanta(3), TimeUnit::NanoSeconds)]));
        assert_eq!(
            langkah_waktu_minimum(&design),
            Some(3 * TimeUnit::NanoSeconds.femtos())
        );
    }

    #[test]
    fn langkah_waktu_minimum_tidak_ada_tanpa_proses_waktu() {
        let design = Design::new("tb");
        assert_eq!(langkah_waktu_minimum(&design), None);
    }

    /// Delay proses waktu bisa tersembunyi di dalam blok; tanpa penelusuran
    /// turunan, minimum terambil `None` dan driver kembali ke jumlah langkah
    /// tetap — clock pun membeku di t=0.
    #[test]
    fn langkah_waktu_minimum_menelusuri_blok_nested() {
        let mut design = Design::new("tb");
        design.processes.push(timed(vec![Statement::Block {
            body: vec![delay(konstanta(7), TimeUnit::NanoSeconds)],
            span: Span::default(),
        }]));
        assert_eq!(
            langkah_waktu_minimum(&design),
            Some(7 * TimeUnit::NanoSeconds.femtos())
        );
    }

    #[test]
    fn wake_terakhir_mengambil_waktu_bangun_maksimum() {
        let mut design = Design::new("tb");
        design
            .processes
            .push(initial(vec![delay(konstanta(52), TimeUnit::NanoSeconds)]));
        design
            .processes
            .push(initial(vec![delay(konstanta(13), TimeUnit::NanoSeconds)]));
        assert_eq!(
            wake_terakhir(&design),
            Some(52 * TimeUnit::NanoSeconds.femtos())
        );
    }

    #[test]
    fn wake_terakhir_membatalkan_rencana_bila_delay_runtime() {
        let mut design = Design::new("tb");
        design
            .processes
            .push(initial(vec![delay(variabel(), TimeUnit::NanoSeconds)]));
        assert_eq!(wake_terakhir(&design), None);
    }

    /// Periode tiap proses waktu dipakai driver untuk membatas clock. Salah
    /// urutannya (terbalik dengan urutan proses) membuat clock melompat jauh
    /// melewati waktu bangun testbench, dan statement-nya membaca counter yang
    /// belum naik ke nilai yang benar.
    #[test]
    fn periode_waktu_sejajar_dengan_urutan_proses() {
        let mut design = Design::new("tb");
        design
            .processes
            .push(timed(vec![delay(konstanta(5), TimeUnit::NanoSeconds)]));
        design
            .processes
            .push(timed(vec![delay(konstanta(3), TimeUnit::NanoSeconds)]));
        assert_eq!(
            periode_waktu(&design),
            Some(vec![
                5 * TimeUnit::NanoSeconds.femtos(),
                3 * TimeUnit::NanoSeconds.femtos()
            ])
        );
    }

    #[test]
    fn periode_waktu_membatalkan_rencana_bila_delay_runtime() {
        let mut design = Design::new("tb");
        design
            .processes
            .push(timed(vec![delay(variabel(), TimeUnit::NanoSeconds)]));
        assert_eq!(periode_waktu(&design), None);
    }
}
