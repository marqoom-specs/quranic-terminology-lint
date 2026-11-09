//! The project's `.terminology.json`: what to check, and what to leave alone.

use crate::glob::covers;
use regex::Regex;
use serde_json::Value;
use std::path::{Path, PathBuf};

pub const CONFIG_NAME: &str = ".terminology.json";
const KEYS: [&str; 6] = ["paths", "exclude", "ignore_words", "allow_gloss_in", "compatibility", "external_names"];
/// Read by the data check, from the same file.
const DATA: &str = "data";

#[derive(Debug, Default)]
pub struct Config {
    /// The folder holding the configuration, or the current directory.
    pub root: PathBuf,
    pub file: Option<PathBuf>,
    /// What to check when no paths are given.
    pub paths: Vec<String>,
    pub exclude: Vec<String>,
    /// Words to accept wherever they occur, as written in code.
    pub ignore_words: Vec<String>,
    /// Paths where English glosses (`verse`) are allowed.
    pub allow_gloss_in: Vec<String>,
    /// Paths whose names stay for compatibility: reported, never blocking.
    pub compatibility: Vec<String>,
    /// Names that belong to someone else, as regular expressions.
    pub external: Option<Regex>,
}

impl Config {
    /// The configuration file: the one given, or the nearest one above the
    /// current directory.
    pub fn find(explicit: Option<&Path>) -> Option<PathBuf> {
        if let Some(p) = explicit {
            return Some(p.to_path_buf());
        }
        let cwd = std::env::current_dir().ok()?;
        cwd.ancestors().map(|d| d.join(CONFIG_NAME)).find(|p| p.is_file())
    }

    pub fn load(path: Option<&Path>) -> Result<Config, String> {
        let Some(path) = path else {
            return Ok(Config { root: std::env::current_dir().map_err(|e| e.to_string())?, ..Config::default() });
        };
        let shown = path.display();
        let text = std::fs::read_to_string(path).map_err(|e| format!("{shown}: {e}"))?;
        let raw: Value = serde_json::from_str(&text).map_err(|e| format!("{shown}: {e}"))?;
        let Value::Object(raw) = raw else {
            return Err(format!("{shown}: configuration must be an object"));
        };
        let mut unknown: Vec<&String> =
            raw.keys().filter(|k| !KEYS.contains(&k.as_str()) && k.as_str() != DATA && !k.starts_with(['_', '$'])).collect();
        unknown.sort();
        if !unknown.is_empty() {
            let mut known: Vec<&str> = KEYS.iter().copied().chain([DATA]).collect();
            known.sort();
            return Err(format!("{shown}: unknown keys {unknown:?}; the keys are {known:?}"));
        }
        if raw.get(DATA).is_some_and(|d| !d.is_object()) {
            return Err(format!("{shown}: {DATA} must be an object"));
        }
        let strings = |name: &str| -> Result<Vec<String>, String> {
            match raw.get(name) {
                None => Ok(vec![]),
                Some(Value::Array(a)) => a
                    .iter()
                    .map(|v| v.as_str().map(String::from).ok_or_else(|| format!("{shown}: {name} must be an array of strings")))
                    .collect(),
                Some(_) => Err(format!("{shown}: {name} must be an array of strings")),
            }
        };
        let external_names = strings("external_names")?;
        let external = if external_names.is_empty() {
            None
        } else {
            let joined = external_names.iter().map(|p| format!("(?:{p})")).collect::<Vec<_>>().join("|");
            Some(Regex::new(&joined).map_err(|e| format!("{shown}: external_names is not a regular expression: {e}"))?)
        };
        let root = path.canonicalize().map_err(|e| format!("{shown}: {e}"))?.parent().map(Path::to_path_buf).unwrap_or_default();
        Ok(Config {
            root,
            file: Some(path.to_path_buf()),
            paths: strings("paths")?,
            exclude: strings("exclude")?,
            ignore_words: strings("ignore_words")?,
            allow_gloss_in: strings("allow_gloss_in")?,
            compatibility: strings("compatibility")?,
            external,
        })
    }
}

/// Whether any of the patterns covers the path.
pub fn matches(rel: &str, patterns: &[String]) -> bool {
    patterns.iter().any(|p| covers(p, rel))
}
