//! The `quranic-data` command line.

use super::check::{check_file, Ctx};
use super::config::Config;
use super::findings::*;
use super::registry::{Registry, SYSTEMS};
use super::report;
use super::values::Resolver;
use crate::args::{Arg, Args};
use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

const NAME: &str = "quranic-data";
const VERSION: &str = env!("CARGO_PKG_VERSION");

const HELP: &str = "quranic-data — checks Quranic data against the Marqoom registries

Usage:
  quranic-data [options] [paths...]   check data files, API schemas and SQL (default: .)
  quranic-data registry               check the registries themselves

It reads .json (data, API responses, OpenAPI), .csv, .tsv and .sql files.
Application code is not read.

Options:
  --format text|markdown|json   the report: for the terminal, for an issue, or for tools (default: text)
  --json                        the same as --format json
  --strict                      fail on warnings too
  --quiet                       leave out notes
  --config PATH                 the configuration (default: the nearest .terminology.json)
  -h, --help                    show this help
  -V, --version                 show the version
";

#[derive(Clone, Copy, Default)]
enum Format {
    #[default]
    Text,
    Markdown,
    Json,
}

#[derive(Default)]
struct Options {
    paths: Vec<PathBuf>,
    config: Option<PathBuf>,
    format: Format,
    strict: bool,
    quiet: bool,
}

enum Command {
    Check(Options),
    Registry,
    Help,
    Version,
}

fn parse(args: impl IntoIterator<Item = String>) -> Result<Command, String> {
    let mut args = Args::new(args);
    let mut o = Options::default();
    while let Some(arg) = args.next()? {
        match arg {
            Arg::Path(p) if p.as_os_str() == "registry" && o.paths.is_empty() => return Ok(Command::Registry),
            Arg::Path(p) => o.paths.push(p),
            Arg::Option(option) => match option.as_str() {
                "-h" | "--help" => return Ok(Command::Help),
                "-V" | "--version" => return Ok(Command::Version),
                "--format" => {
                    o.format = match args.value()?.as_str() {
                        "text" => Format::Text,
                        "markdown" => Format::Markdown,
                        "json" => Format::Json,
                        other => return Err(format!("--format is text, markdown or json, not `{other}`")),
                    }
                }
                "--json" => o.format = Format::Json,
                "--config" => o.config = Some(args.value()?.into()),
                "--strict" => o.strict = true,
                "--quiet" => o.quiet = true,
                _ => return Err(format!("unknown option {option}")),
            },
        }
    }
    if o.paths.is_empty() {
        o.paths.push(PathBuf::from("."));
    }
    Ok(Command::Check(o))
}

/// Runs the check with the arguments after the program name.
pub fn run(args: impl IntoIterator<Item = String>) -> ExitCode {
    let result = match parse(args) {
        Ok(Command::Check(o)) => check(o),
        Ok(Command::Registry) => Ok(registry_check(&Registry::load())),
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

const SKIP_DIRS: &[&str] = &[
    ".git", "node_modules", "target", ".venv", "venv", "__pycache__", ".dart_tool", ".next", ".gradle",
    ".idea", ".vscode", "Pods", ".pnpm-store", ".turbo", ".cache", "locales", "locale", "i18n", "l10n",
];
const SKIP_FILES: &[&str] = &[
    "package.json", "package-lock.json", "composer.json", "composer.lock", "jsconfig.json", "renovate.json",
    "firebase.json", "angular.json", "app.json", "tslint.json", "lerna.json", "turbo.json", "vercel.json",
    "manifest.webmanifest", ".terminology.json", "deno.json", "bun.lock", "pnpm-lock.yaml",
];
const MAX_BYTES: u64 = 64 * 1024 * 1024;

fn wanted(path: &Path) -> bool {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if SKIP_FILES.contains(&name) || name.starts_with("tsconfig") || name.starts_with(".eslintrc") || name.starts_with(".prettierrc") {
        return false;
    }
    matches!(path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref(), Some("json" | "geojson" | "csv" | "tsv" | "sql"))
}

fn collect(path: &Path, cfg: &Config, out: &mut Vec<PathBuf>) {
    if path.is_file() {
        if wanted(path) && !cfg.excluded(&cfg.rel(path))
            && std::fs::metadata(path).is_ok_and(|m| m.len() <= MAX_BYTES) {
                out.push(path.to_path_buf());
            }
        return;
    }
    let Ok(entries) = std::fs::read_dir(path) else { return };
    let mut entries: Vec<_> = entries.filter_map(|e| e.ok()).collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let p = e.path();
        let name = e.file_name();
        let name = name.to_string_lossy();
        if p.is_dir() {
            if SKIP_DIRS.contains(&name.as_ref()) || cfg.excluded(&format!("{}/", cfg.rel(&p))) {
                continue;
            }
            collect(&p, cfg, out);
        } else {
            collect(&p, cfg, out);
        }
    }
}

/// What a file's results say, without its path: equal for two copies of one file.
fn signature(r: &FileResult) -> String {
    let mut s = String::new();
    for f in &r.findings {
        let _ = writeln!(s, "{:?}|{}|{:?}|{}|{}|{:?}|{}", f.severity, f.code, f.line, f.at, f.message, f.fix, f.count);
    }
    for i in &r.inventory {
        let _ = writeln!(s, "{}|{}|{}|{}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}", i.at, i.shape, i.surahs, i.ayahs, i.numbering, i.fits, i.stated_riwayah, i.stated_numbering, i.hint, i.sajdah);
    }
    for e in &r.evidence {
        let _ = writeln!(s, "{}|{}|{}|{}", e.system, e.surah, e.registry, e.data);
    }
    s
}

fn registry_check(reg: &Registry) -> ExitCode {
    println!("{NAME} {VERSION} — the registries themselves\n");
    let mut problems = 0;
    for (i, sys) in SYSTEMS.iter().enumerate() {
        let sum: u32 = reg.counts.iter().map(|r| r[i]).sum();
        let total = reg.totals[i];
        if sum == total {
            println!("  ✓ {sys:<13} the 114 surah counts add up to {sum}, the stated total");
        } else {
            problems += 1;
            let also = if reg.also_reported[i].contains(&sum) { " (one of the totals also reported)" } else { "" };
            println!("  ! {sys:<13} the 114 surah counts add up to {sum}; the stated total is {total}{also}");
        }
    }
    let kufi = Registry::system_index("kufi").unwrap();
    for (s, a) in &reg.sajdah {
        let max = reg.counts.get(*s as usize - 1).map(|r| r[kufi]).unwrap_or(0);
        if *a > max {
            problems += 1;
            println!("  ! sajdah {s}:{a} is beyond the surah's Kufi count ({max})");
        }
    }
    println!("  ✓ {} sajdah places, all within their surah's Kufi count", reg.sajdah.len());
    for m in reg.members.iter().filter(|m| m.kind == "qiraah") {
        if Registry::system_index(&m.numbering).is_none() {
            problems += 1;
            println!("  ! qiraah {} names an unknown numbering `{}`", m.code, m.numbering);
        }
    }
    let riwayahs = reg.members.iter().filter(|m| m.kind == "riwayah").count();
    let qiraahs = reg.members.iter().filter(|m| m.kind == "qiraah").count();
    println!("  ✓ {qiraahs} qiraahs, {riwayahs} riwayahs; every qiraah names its numbering");
    println!("\n{problems} to review");
    ExitCode::SUCCESS
}

fn check(o: Options) -> Result<ExitCode, String> {
    let Options { paths, config, format, strict, quiet } = o;
    if let Some(missing) = paths.iter().find(|p| !p.exists()) {
        return Err(format!("no such path: {}", missing.display()));
    }
    let started = Instant::now();
    let reg = Registry::load();
    let cfg = Config::find(&paths[0], config.as_deref())?;
    let res = Resolver::new(&reg, cfg.mappings.clone());
    let ctx = Ctx { reg: &reg, res: &res, cfg: &cfg };

    let mut files = Vec::new();
    for p in &paths {
        collect(p, &cfg, &mut files);
    }

    // Read and check files in parallel; results keep the file order.
    let workers = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(16);
    let next = std::sync::atomic::AtomicUsize::new(0);
    let mut results: Vec<(usize, FileResult)> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..workers)
            .map(|_| {
                scope.spawn(|| {
                    let mut mine = Vec::new();
                    loop {
                        let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        if i >= files.len() {
                            break;
                        }
                        mine.push((i, check_file(&ctx, &files[i])));
                    }
                    mine
                })
            })
            .collect();
        handles.into_iter().flat_map(|h| h.join().unwrap()).collect()
    });
    results.sort_by_key(|(i, _)| *i);

    // Identical files that give identical results (a data folder bundled
    // twice) are reported once, under the first path.
    let mut copies: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut first: HashMap<(u64, String), String> = HashMap::new();
    let mut findings = Vec::new();
    let mut inventory = Vec::new();
    let mut evidence = Vec::new();
    let mut bytes = 0;
    for (_, r) in results {
        bytes += r.bytes;
        if !(r.findings.is_empty() && r.inventory.is_empty() && r.evidence.is_empty()) {
            let key = (r.digest, signature(&r));
            if let Some(primary) = first.get(&key) {
                copies.entry(primary.clone()).or_default().push(r.file.clone());
                continue;
            }
            first.insert(key, r.file.clone());
        }
        findings.extend(r.findings);
        inventory.extend(r.inventory);
        evidence.extend(r.evidence);
    }
    if quiet {
        findings.retain(|f| f.severity != Severity::Note);
    }
    let errors = findings.iter().filter(|f| f.severity == Severity::Error).count();
    let warnings = findings.iter().filter(|f| f.severity == Severity::Warning).count();

    let title = format!("{NAME} {VERSION} · Marqoom registries (terminology v0.1)");
    let summary = report::Summary {
        title: &title,
        files: files.len(),
        bytes,
        millis: started.elapsed().as_millis(),
        findings: &findings,
        inventory: &inventory,
        evidence: &evidence,
        copies: &copies,
        config: cfg.file.as_ref().map(|f| cfg.rel(f)),
    };
    let out = match format {
        Format::Json => report::json(&summary),
        Format::Markdown => report::markdown(&summary),
        Format::Text => report::text(&summary),
    };
    println!("{out}");
    Ok(if errors > 0 || (strict && warnings > 0) { ExitCode::FAILURE } else { ExitCode::SUCCESS })
}
