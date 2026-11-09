//! The `quranic-terminology` command line.

use super::config::Config;
use super::files::{select, shown, MAX_BYTES};
use super::report::{self, plural, Summary};
use super::scan::{scan, write};
use super::terms::Terms;
use crate::args::{Arg, Args};
use std::path::PathBuf;
use std::process::ExitCode;

const NAME: &str = "quranic-terminology";
const VERSION: &str = env!("CARGO_PKG_VERSION");

const HELP: &str = "quranic-terminology — checks names in code against the Quranic vocabulary naming standard

Usage:
  quranic-terminology [options] [paths...]   check files and folders (default: the configured paths, or .)

It reads code, data keys and documents. Comments, labels and documents are read
as prose, where the names readers see are fine.

Options:
  --format text|table|json   one line per finding, one row per name, or everything for tools (default: text)
  --json                     the same as --format json
  --limit N                  findings or rows to show (default: 20, or 10 in the table)
  --unsafe-fixes             apply the supported renames; they can break imports, APIs and bindings
  --config PATH              the configuration (default: the nearest .terminology.json)
  -h, --help                 show this help
  -V, --version              show the version
";

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Format {
    #[default]
    Text,
    Table,
    Json,
}

#[derive(Default)]
struct Options {
    paths: Vec<PathBuf>,
    config: Option<PathBuf>,
    format: Format,
    limit: Option<usize>,
    unsafe_fixes: bool,
}

enum Command {
    Check(Options),
    Help,
    Version,
}

fn parse(args: impl IntoIterator<Item = String>) -> Result<Command, String> {
    let mut args = Args::new(args);
    let mut o = Options::default();
    while let Some(arg) = args.next()? {
        match arg {
            Arg::Path(p) => o.paths.push(p),
            Arg::Option(option) => match option.as_str() {
                "-h" | "--help" => return Ok(Command::Help),
                "-V" | "--version" => return Ok(Command::Version),
                "--format" => {
                    o.format = match args.value()?.as_str() {
                        "text" => Format::Text,
                        "table" => Format::Table,
                        "json" => Format::Json,
                        other => return Err(format!("--format is text, table or json, not `{other}`")),
                    }
                }
                "--json" => o.format = Format::Json,
                "--limit" => {
                    let value = args.value()?;
                    o.limit = Some(value.parse().ok().filter(|n| *n > 0).ok_or_else(|| format!("--limit is a positive number, not `{value}`"))?);
                }
                "--unsafe-fixes" => o.unsafe_fixes = true,
                "--config" => o.config = Some(args.value()?.into()),
                _ => return Err(format!("unknown option {option}")),
            },
        }
    }
    Ok(Command::Check(o))
}

/// Runs the check with the arguments after the program name.
pub fn run(args: impl IntoIterator<Item = String>) -> ExitCode {
    let result = match parse(args) {
        Ok(Command::Check(o)) => check(o),
        Ok(Command::Help) => {
            print!("{HELP}");
            Ok(ExitCode::SUCCESS)
        }
        Ok(Command::Version) => {
            println!("{NAME} {VERSION}");
            Ok(ExitCode::SUCCESS)
        }
        Err(e) => Err(format!("{e}\nRun `{NAME} --help` for the options.")),
    };
    result.unwrap_or_else(|e| {
        eprintln!("{NAME}: {e}");
        ExitCode::from(2)
    })
}

fn check(o: Options) -> Result<ExitCode, String> {
    let cfg = Config::load(Config::find(o.config.as_deref()).as_deref())?;
    let terms = Terms::load(&cfg.ignore_words)?;
    let paths = if !o.paths.is_empty() {
        o.paths
    } else if !cfg.paths.is_empty() {
        cfg.paths.iter().map(|p| cfg.root.join(p)).collect()
    } else {
        vec![std::env::current_dir().map_err(|e| e.to_string())?]
    };
    let selection = select(&paths, &cfg)?;
    if selection.files.is_empty() && o.format != Format::Json {
        eprintln!("note: no files to check (excluded, unsupported, or symlinked)");
    }
    let result = scan(&selection.files, &terms, &cfg, o.unsafe_fixes)?;
    let fixed_files = if result.renames.is_empty() { 0 } else { write(&result.renames)? };
    let mut summary = Summary::new(&result.findings, result.counts);
    summary.skipped_large_files = selection.skipped.iter().map(|p| shown(p)).collect();
    summary.fixed_names = result.renames.len();
    summary.fixed_files = fixed_files;
    summary.concepts = terms.concepts;
    summary.config = cfg.file.as_ref().map(|f| f.display().to_string());

    match o.format {
        Format::Json => println!("{}", report::json(&result.findings, &summary)),
        Format::Table => print!("{}", report::table(&result.findings, &summary, o.limit.unwrap_or(10))),
        Format::Text => print!("{}", report::by_file(&result.findings, &summary, o.limit.unwrap_or(20))),
    }
    if o.format != Format::Json {
        if summary.fixed_names > 0 {
            println!(
                "\nFixed {} in {}. Review the changes, then stage them.",
                plural(summary.fixed_names, "name", "names"),
                plural(summary.fixed_files, "file", "files")
            );
        }
        if !summary.skipped_large_files.is_empty() {
            let skipped = &summary.skipped_large_files;
            let more = if skipped.len() > 3 { format!(" and {} more", skipped.len() - 3) } else { String::new() };
            eprintln!(
                "skipped {} larger than {} MB: {}{more}",
                plural(skipped.len(), "file", "files"),
                MAX_BYTES / 1_000_000,
                skipped[..skipped.len().min(3)].join(", ")
            );
        }
    }
    Ok(if summary.errors > 0 { ExitCode::FAILURE } else { ExitCode::SUCCESS })
}
