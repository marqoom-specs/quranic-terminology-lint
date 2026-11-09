//! Reading files line by line, and writing the unsafe fixes.

use super::check::{check, suggest, Finding};
use super::config::{matches, Config};
use super::files::{relative, shown, slashed};
use super::layout::{Format, Layout, Source};
use super::terms::Terms;
use super::words::{identifiers, is_alpha, is_lower, is_title, is_upper};
use std::borrow::Cow;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

/// A finding at its place, counted once per line.
#[derive(Debug)]
pub struct Located {
    pub file: String,
    pub line: usize,
    pub identifier: String,
    pub finding: Finding,
    /// In a compatibility path: reported, never blocking.
    pub known: bool,
    pub count: usize,
    /// The whole identifier corrected, when the finding can be fixed.
    pub suggestion: Option<String>,
}

#[derive(Debug, Default)]
pub struct Counts {
    pub files_scanned: usize,
    pub ignored_lines: usize,
    pub external_names: usize,
    pub allowed_glosses: usize,
    pub reference_literals: usize,
}

impl Counts {
    fn add(&mut self, o: &Counts) {
        self.files_scanned += o.files_scanned;
        self.ignored_lines += o.ignored_lines;
        self.external_names += o.external_names;
        self.allowed_glosses += o.allowed_glosses;
        self.reference_literals += o.reference_literals;
    }
}

/// A name to replace, at its byte column on a line (0-based).
#[derive(Debug)]
pub struct Rename {
    pub path: PathBuf,
    pub line: usize,
    pub column: usize,
    pub old: String,
    pub new: String,
}

#[derive(Debug, Default)]
pub struct Scan {
    pub findings: Vec<Located>,
    pub counts: Counts,
    pub renames: Vec<Rename>,
}

/// Checks every file, in parallel; the results keep the files' order.
pub fn scan(files: &[PathBuf], terms: &Terms, cfg: &Config, fixing: bool) -> Result<Scan, String> {
    let next = AtomicUsize::new(0);
    let workers = std::thread::available_parallelism().map_or(4, |n| n.get()).min(16).min(files.len().max(1));
    let mut results: Vec<(usize, Result<Scan, String>)> = std::thread::scope(|s| {
        let handles: Vec<_> = (0..workers)
            .map(|_| {
                s.spawn(|| {
                    let mut mine = Vec::new();
                    loop {
                        let i = next.fetch_add(1, Ordering::Relaxed);
                        let Some(path) = files.get(i) else { break };
                        mine.push((i, scan_file(path, terms, cfg, fixing)));
                    }
                    mine
                })
            })
            .collect();
        handles.into_iter().flat_map(|h| h.join().expect("a scanning thread panicked")).collect()
    });
    results.sort_by_key(|(i, _)| *i);
    let mut all = Scan::default();
    for (_, result) in results {
        let one = result?;
        all.counts.add(&one.counts);
        all.findings.extend(one.findings);
        all.renames.extend(one.renames);
    }
    all.findings.sort_by(|a, b| (&a.file, a.line, &a.identifier).cmp(&(&b.file, b.line, &b.identifier)));
    Ok(all)
}

fn scan_file(path: &Path, terms: &Terms, cfg: &Config, fixing: bool) -> Result<Scan, String> {
    let file = shown(path);
    let bytes = std::fs::read(path).map_err(|e| format!("{file}: {e}"))?;
    // Read strictly: a file that can't be decoded is an error, not a pass.
    let text = String::from_utf8(bytes).map_err(|e| {
        format!("{file}: not UTF-8 (invalid byte at byte {}); add it to exclude if it is not source", e.utf8_error().valid_up_to())
    })?;
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let format = Format::of(name).ok_or_else(|| format!("{file}: not a supported file"))?;
    let src = Source::new(&text);
    let layout = Layout::new(&src, format, fixing, cfg.external.as_ref());
    let rel = slashed(&relative(path, &cfg.root));
    let gloss_allowed = matches(&rel, &cfg.allow_gloss_in);
    let known = matches(&rel, &cfg.compatibility);

    let mut out = Scan::default();
    out.counts.files_scanned = 1;
    out.counts.reference_literals = layout.references;
    let mut slots: HashMap<(usize, String, String, super::rules::Rules), usize> = HashMap::new();
    for (n, range) in src.ranges().iter().enumerate() {
        let masked = &layout.masked[range.clone()];
        if ignored(masked) {
            out.counts.ignored_lines += 1;
            continue;
        }
        let line: Cow<str> = match &cfg.external {
            Some(re) => {
                let mut hidden = 0;
                let line = re.replace_all(masked, |c: &regex::Captures| {
                    hidden += 1;
                    " ".repeat(c[0].len())
                });
                out.counts.external_names += hidden;
                line
            }
            None => Cow::Borrowed(masked),
        };
        for (column, identifier) in identifiers(&line) {
            if layout.names.as_ref().is_some_and(|names| !names[n].contains(column)) {
                continue;
            }
            let in_note = format.is_prose() || layout.prose[n].contains(column);
            let mut reported = Vec::new();
            for f in check(identifier, terms, in_note) {
                if gloss_allowed && f.gloss {
                    out.counts.allowed_glosses += 1;
                } else {
                    reported.push(f);
                }
            }
            let suggestion = suggest(identifier, reported.iter().filter(|f| f.fixable()));
            for f in reported {
                let slot = (n, identifier.to_string(), f.found.clone(), f.rules);
                if let Some(&i) = slots.get(&slot) {
                    out.findings[i].count += 1;
                    continue;
                }
                slots.insert(slot, out.findings.len());
                out.findings.push(Located {
                    file: file.clone(),
                    line: n + 1,
                    identifier: identifier.to_string(),
                    suggestion: suggestion.clone().filter(|_| f.fixable()),
                    finding: f,
                    known,
                    count: 1,
                });
            }
            if !fixing || known {
                continue;
            }
            if let Some(new) = fix_for(&layout, n, column, identifier, suggestion, in_note, format.is_prose()) {
                out.renames.push(Rename { path: path.to_path_buf(), line: n, column, old: identifier.to_string(), new });
            }
        }
    }
    Ok(out)
}

/// The replacement an unsafe fix may write here, if any. Strings hold data,
/// keys and paths, and are never edited; nor is code quoted in prose, or prose
/// whose place in the file is only a guess.
fn fix_for(layout: &Layout, n: usize, column: usize, identifier: &str, suggestion: Option<String>, in_note: bool, document: bool) -> Option<String> {
    let safe_prose = layout.editable[n].contains(column);
    let strings = layout.strings.as_ref().map(|s| &s[n]);
    if strings.is_none() && !safe_prose || strings.is_some_and(|s| s.contains(column)) {
        return None;
    }
    let new = suggestion?;
    // A word for people. In `backticks` it names code: leave it.
    if in_note && (layout.code[n].contains(column) || !plain_word(identifier, &new)) {
        return None;
    }
    if document && !safe_prose {
        return None;
    }
    Some(new)
}

/// A plain word in a sentence, which a fix may correct; not an identifier,
/// which names code.
fn plain_word(old: &str, new: &str) -> bool {
    is_alpha(old) && is_alpha(new) && (is_lower(old) || is_title(old) || is_upper(old))
}

/// `# terminology: ignore`
fn ignored(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    lower.match_indices("terminology:").any(|(i, m)| lower[i + m.len()..].trim_start().starts_with("ignore"))
}

/// Writes the renames, keeping each file's line endings. Returns the number
/// of files changed.
pub fn write(renames: &[Rename]) -> Result<usize, String> {
    let mut by_file: Vec<(&Path, Vec<&Rename>)> = Vec::new();
    for r in renames {
        match by_file.iter_mut().find(|(p, _)| *p == r.path.as_path()) {
            Some((_, edits)) => edits.push(r),
            None => by_file.push((&r.path, vec![r])),
        }
    }
    for (path, mut edits) in by_file.iter().cloned() {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let mut lines = lines_with_endings(&text);
        edits.sort_by_key(|r| std::cmp::Reverse((r.line, r.column)));
        for r in edits {
            let line = &mut lines[r.line];
            line.replace_range(r.column..r.column + r.old.len(), &r.new);
        }
        std::fs::write(path, lines.concat()).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    Ok(by_file.len())
}

/// Lines with their endings, `\n`, `\r\n` or `\r`.
fn lines_with_endings(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut start = 0;
    let b = text.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let end = match b[i] {
            b'\n' => i + 1,
            b'\r' if b.get(i + 1) == Some(&b'\n') => i + 2,
            b'\r' => i + 1,
            _ => {
                i += 1;
                continue;
            }
        };
        out.push(text[start..end].to_string());
        start = end;
        i = end;
    }
    if start < text.len() {
        out.push(text[start..].to_string());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_line_endings() {
        assert_eq!(lines_with_endings("a\r\nb\rc\nd"), ["a\r\n", "b\r", "c\n", "d"]);
        assert!(ignored("sura = 1 # Terminology:  IGNORE") && !ignored("terminology"));
        assert!(plain_word("Sura", "Surah") && !plain_word("aya_id", "ayah_id")); // terminology: ignore
    }
}
