// Tanggung jawab: entry point CLI cargo-sv.
use cargo_sv::run;
fn main() {
    run(std::env::args().collect());
}
