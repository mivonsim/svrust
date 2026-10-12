// Tanggung jawab: memecah body blok `initial` menjadi segmen pada batas `#delay`.
use sv_ir::process::{Process, ProcessKind, Statement};
use sv_ir::Design;

/// Satu segmen dijalankan pada satu langkah waktu simulasi.
///
/// Segmen pertama tidak diawali delay; setiap segmen berikutnya diawali satu
/// statement `Delay`, sehingga waktu geser sebelum statement berikutnya jalan.
pub type Segments = Vec<Vec<Statement>>;

/// Pecah seluruh body proses `initial` pada batas `#delay` dan `@(...)`.
///
/// Batas `@(...)` penting supaya segmen yang diawali event control selalu
/// dimulai dengan statement itu: pemecahansegmen dipakai sebagai sinyal
/// "coba lagi langkah berikutnya" ketika edge belum terjadi.
///
/// Delay yang ada di dalam loop atau `if` tidak memecah segmen: proses
/// tidak dapat ditangguhkan di tengah loop pada model waktu-step ini, jadi
/// delay tersebut tetap berjalan sinkron di dalam segmennya.
pub fn segmen_process(process: &Process) -> Segments {
    let mut segments: Segments = vec![Vec::new()];
    for statement in &process.body {
        if matches!(
            statement,
            Statement::Delay { .. } | Statement::EventControl { .. }
        ) {
            segments.push(Vec::new());
        }
        segments
            .last_mut()
            .expect("selalu ada satu segmen terbuka")
            .push(statement.clone());
    }
    segments
}

/// Sinyal yang diawasi event control di seluruh segmen sebuah proses.
pub fn sinyal_event_control(segment: &[Statement]) -> Vec<u32> {
    let mut out = Vec::new();
    for statement in segment {
        if let Statement::EventControl { events, .. } = statement {
            for item in events {
                out.push(item.signal);
            }
        }
    }
    out
}

/// Jumlah event control pada seluruh segmen sebuah proses.
///
/// Dipakai driver untuk memberi langkah tambahan: setiap tunggu edge
/// memerlukan minimal satu langkah clock tambahan.
pub fn jumlah_tunggu(process: &Process) -> usize {
    segmen_process(process)
        .iter()
        .map(|segment| sinyal_event_control(segment).len())
        .sum()
}

/// Segmen dari seluruh proses `initial` pada design, diurut per waktu langkah.
pub fn segmen_design(design: &Design) -> Vec<Segments> {
    design
        .processes
        .iter()
        .filter(|p| p.kind == ProcessKind::Initial)
        .map(segmen_process)
        .collect()
}

/// Jumlah langkah waktu yang dibutuhkan seluruh proses `initial`.
pub fn jumlah_langkah(design: &Design) -> usize {
    segmen_design(design)
        .iter()
        .map(|segments| segments.len())
        .max()
        .unwrap_or(0)
}

/// Jumlah event waktu yang harus diproses sebelum jam simulasi mencapai waktu
/// bangun terakhir yang konstanta.
///
/// BUG: `langkah_driver` hanya menghitung jumlah SEGMENT. Begitu ada proses
/// waktu `always #5 clk = ~clk;`, segmen `#52` butuh sepuluh iterasi clock
/// sebelum waktunya benar-benar tiba — sedangkan `langkah_driver` mengembalikan
/// 2. Blok `initial` lalu melompat ke t=52 pada langkah kedua, membaca counter
/// yang baru berayun dua kali, dan mencetak angka yang salah tanpa pesan.
///
/// SATU pemanggilan `eval_timed` memproses SATU event — event terdekat di
/// antara seluruh proses. Maka jumlah langkah yang dibutuhkan adalah jumlah
/// EVENT, bukan kelipatan satu periode.
///
/// BUG: rumus `terakhir / periode` under-estimate karena event terjadi di
/// **irisan** seluruh periode, bukan kelipatan satu periode. Untuk `#3` + `#4`
/// event-nya t=3, 4, 6, 8, 9, 12 — butuh 7 panggilan untuk mencapai t=12,
/// sedangkan `12/4` hanya memberi 5 dan blok `initial` kehilangan seluruh
/// output-nya tanpa pesan.
///
/// Batas atas `BATAS_LANGKAH` mencegah waktu simulasi yang sangat besar
/// (`#100000000` dengan clock `#1`) menghasilkan loop yang tidak pernah selesai.
const BATAS_LANGKAH: usize = 1 << 20;

pub fn langkah_simulasi(design: &Design) -> usize {
    let dasar = langkah_driver(design);
    let (Some(terakhir), Some(periode)) = (
        crate::wake_plan::wake_terakhir(design),
        crate::wake_plan::periode_waktu(design),
    ) else {
        return dasar;
    };
    // Periode kosong atau nol berarti tidak ada proses waktu dengan delay
    // konstanta; jumlah event tidak bisa dihitung dan langkah dasar tetap dipakai.
    if periode.is_empty() || periode.iter().any(|p| *p == 0) {
        return dasar;
    }
    // Batas atas yang AMAN: setiap proses menyala `terakhir / periode[i]` kali,
    // dan satu panggilan memproses setidaknya satu proses — jadi jumlah
    // panggilan tidak bisa melebihi jumlah seluruh penembakan itu. Memakai
    // hanya periode terpanjang akan under-estimate dan testbench kehilangan
    // output-nya.
    let total: u128 = periode
        .iter()
        .map(|periode| (terakhir as u128).div_ceil(*periode as u128))
        .sum();
    let perlu = total.min(BATAS_LANGKAH as u128) as usize;
    dasar.max(perlu + 2)
}

/// Jumlah event control pada seluruh proses `initial` di design.
pub fn total_tunggu(design: &Design) -> usize {
    design
        .processes
        .iter()
        .filter(|p| p.kind == ProcessKind::Initial)
        .map(jumlah_tunggu)
        .sum()
}

/// Jumlah langkah yang harus dicadangkan driver.
///
/// Tiap event control menambah langkah karena segmen tidak boleh maju sampai
/// edge benar-benar terjadi; dua langkah cukup karena clock pada driver
/// ditoggle tiap langkah, jadi posedge muncul paling lambat pada langkah
/// kedua setelah menunggu dimulai.
pub fn langkah_driver(design: &Design) -> usize {
    jumlah_langkah(design) + 2 * total_tunggu(design)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sv_ir::datatype::DataType;
    use sv_ir::process::Span;

    fn kosong() -> Statement {
        Statement::Noop {
            span: Span::default(),
        }
    }

    fn delay() -> Statement {
        Statement::Delay {
            amount: sv_ir::Expr::constant(5, DataType::logic(8)),
            unit: sv_ir::TimeUnit::NanoSeconds,
            body: Box::new(kosong()),
            span: Span::default(),
        }
    }

    /// Satu item `@(posedge sig)` pada sinyal `sinyal`.
    fn event(sinyal: u32) -> sv_ir::process::EventItem {
        sv_ir::process::EventItem {
            edge: sv_ir::process::EdgeSensitivity::Posedge,
            signal: sinyal,
        }
    }

    fn proses(body: Vec<Statement>) -> Process {
        Process {
            name: "initial".to_string(),
            kind: ProcessKind::Initial,
            sensitivity: Vec::new(),
            body,
            span: Span::default(),
        }
    }

    #[test]
    fn body_tanpa_delay_menjadi_satu_segmen() {
        let segments = segmen_process(&proses(vec![kosong(), kosong()]));
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].len(), 2);
    }

    #[test]
    fn satu_delay_membelah_jadi_dua_segmen() {
        let segments = segmen_process(&proses(vec![kosong(), delay(), kosong()]));
        assert_eq!(segments.len(), 2);
        // Segmen kedua diawali delay supaya waktu bergeser lebih dulu.
        assert!(matches!(segments[1][0], Statement::Delay { .. }));
        assert_eq!(segments[1].len(), 2);
    }

    #[test]
    fn delay_pertama_membuat_segmen_kosong_di_awal() {
        let segments = segmen_process(&proses(vec![delay(), kosong()]));
        assert_eq!(segments.len(), 2);
        assert!(segments[0].is_empty(), "segmen awal kosong");
        assert!(matches!(segments[1][0], Statement::Delay { .. }));
    }

    #[test]
    fn jumlah_langkah_diambil_dari_segmen_terpanjang() {
        let design = Design::new("m");
        assert_eq!(jumlah_langkah(&design), 0);
    }

    #[test]
    fn event_control_juga_memecah_segmen() {
        let segments = segmen_process(&proses(vec![
            kosong(),
            Statement::EventControl {
                events: vec![event(3)],
                body: Vec::new(),
                span: Span::default(),
            },
            kosong(),
        ]));
        assert_eq!(segments.len(), 2);
        assert!(matches!(segments[1][0], Statement::EventControl { .. }));
    }

    #[test]
    fn segmen_event_control_menyimpan_sinyal_yang_diawasi() {
        let segments = segmen_process(&proses(vec![Statement::EventControl {
            events: vec![event(3)],
            body: Vec::new(),
            span: Span::default(),
        }]));
        // Event control memecah segmen, jadi segmen awal kosong.
        assert!(segments[0].is_empty());
        assert_eq!(sinyal_event_control(&segments[1]), vec![3]);
    }

    #[test]
    fn total_tunggu_menghitung_event_control_per_proses() {
        let design = Design::new("m");
        assert_eq!(total_tunggu(&design), 0);
        assert_eq!(langkah_driver(&design), 0);
    }

    #[test]
    fn proses_initial_kosong_tetap_menghasilkan_satu_segmen() {
        let segments = segmen_process(&proses(Vec::new()));
        assert_eq!(segments.len(), 1);
        assert!(segments[0].is_empty());
    }
}
