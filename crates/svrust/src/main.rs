// Tanggung jawab: entry point CLI cargo-sv.
use svrust::run;
fn main() {
    run(std::env::args().collect());
}
