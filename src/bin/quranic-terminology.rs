//! `quranic-terminology`: checks names in code against the naming standard.

fn main() -> std::process::ExitCode {
    quranic_terminology_lint::names::cli::run(std::env::args().skip(1))
}
