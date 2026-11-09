//! `quranic-data`: checks Quranic data against the Marqoom registries.

fn main() -> std::process::ExitCode {
    quranic_terminology_lint::data::cli::run(std::env::args().skip(1))
}
