//! Printing findings: one per line, as a table of names, or as JSON.

use super::check::Severity;
use super::rules::Rules;
use super::scan::{Counts, Located};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fmt::Write;

/// `1 file`, `2 files`.
pub fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// Totals over the findings outside compatibility paths.
#[derive(Debug, Default)]
pub struct Summary {
    pub counts: Counts,
    pub errors: usize,
    pub warnings: usize,
    pub known: usize,
    pub skipped_large_files: Vec<String>,
    pub fixed_names: usize,
    pub fixed_files: usize,
    pub concepts: usize,
    pub config: Option<String>,
}

impl Summary {
    pub fn new(findings: &[Located], counts: Counts) -> Summary {
        let live = || findings.iter().filter(|f| !f.known);
        Summary {
            errors: live().filter(|f| f.finding.severity == Severity::Error).count(),
            warnings: live().filter(|f| f.finding.severity == Severity::Warning).count(),
            known: findings.iter().filter(|f| f.known).count(),
            counts,
            ..Summary::default()
        }
    }
}

/// One line per name: the place, the corrected name, and every rule it breaks.
pub fn by_file(findings: &[Located], summary: &Summary, limit: usize) -> String {
    // A name that breaks two rules is one line, with both.
    let mut lines: Vec<(&Located, String, Rules)> = Vec::new();
    for f in findings {
        let change = match &f.suggestion {
            Some(s) => format!("{} → {s}", f.identifier),
            None => format!("{} → {} in {}", f.finding.found, f.finding.preferred, f.identifier),
        };
        match lines.last_mut() {
            Some((g, c, rules)) if (&g.file, g.line, &g.identifier, g.finding.severity, &*c) == (&f.file, f.line, &f.identifier, f.finding.severity, &change) => {
                *rules = rules.union(f.finding.rules);
            }
            _ => lines.push((f, change, f.finding.rules)),
        }
    }
    let mut o = String::new();
    for (f, change, rules) in lines.iter().take(limit) {
        let place = if f.known { " (compatibility)" } else { "" };
        let times = if f.count > 1 { format!(" ×{}", f.count) } else { String::new() };
        let _ = writeln!(o, "{}:{}: {}{place} {change}{times} ({})", f.file, f.line, f.finding.severity, rules.message());
    }
    if lines.len() > limit {
        let _ = writeln!(o, "… and {} more (use --limit or --format json)", lines.len() - limit);
    }
    let _ = writeln!(o, "{}, {} in {}", plural(summary.errors, "error", "errors"), plural(summary.warnings, "warning", "warnings"), plural(summary.counts.files_scanned, "file", "files"));
    o
}

/// Each name once, with how often it occurs: the shape of a migration.
pub fn table(findings: &[Located], summary: &Summary, limit: usize) -> String {
    type Row = (Severity, String, String, String);
    let mut groups: HashMap<Row, usize> = HashMap::new();
    for f in findings.iter().filter(|f| !f.known) {
        let rules = f.finding.rules.iter().map(|r| r.code()).collect::<Vec<_>>().join(",");
        *groups.entry((f.finding.severity, f.finding.found.clone(), f.finding.preferred.clone(), rules)).or_default() += f.count;
    }
    let mut rows: Vec<(&Row, usize)> = groups.iter().map(|(k, n)| (k, *n)).collect();
    rows.sort_by(|a, b| (a.0 .0, std::cmp::Reverse(a.1), a.0).cmp(&(b.0 .0, std::cmp::Reverse(b.1), b.0)));
    let total = rows.len();
    rows.truncate(limit);

    let mut o = String::new();
    let _ = writeln!(
        o,
        "Quranic terminology · {} · {} · {}",
        plural(summary.counts.files_scanned, "file", "files"),
        plural(summary.errors, "error", "errors"),
        plural(summary.warnings, "warning", "warnings")
    );
    let width = |title: &str, cell: fn(&Row) -> &str| rows.iter().map(|(r, _)| cell(r).chars().count()).chain([title.len()]).max().unwrap_or(0);
    let widths = [width("Found", |r| &r.1), width("Preferred", |r| &r.2), width("Rule", |r| &r.3)];
    for (severity, title) in [(Severity::Error, "Errors"), (Severity::Warning, "Warnings (they don't block)")] {
        let mine: Vec<_> = rows.iter().filter(|(r, _)| r.0 == severity).collect();
        if mine.is_empty() {
            continue;
        }
        let _ = writeln!(o, "\n{title}");
        let _ = writeln!(o, "{:<w0$}  {:<w1$}  {:<w2$}  Count", "Found", "Preferred", "Rule", w0 = widths[0], w1 = widths[1], w2 = widths[2]);
        for ((_, found, preferred, rule), n) in mine {
            let _ = writeln!(o, "{found:<w0$}  {preferred:<w1$}  {rule:<w2$}  {n:>5}", w0 = widths[0], w1 = widths[1], w2 = widths[2]);
        }
    }
    if total > rows.len() {
        let _ = writeln!(o, "\nShowing {} of {total}. Use --limit to show more.", rows.len());
    }
    if summary.known > 0 {
        let _ = writeln!(o, "{} in compatibility paths (they don't block).", plural(summary.known, "finding", "findings"));
    }
    if total > 0 {
        let _ = writeln!(o, "Use --format text for locations and rule names.");
    }
    o
}

pub fn json(findings: &[Located], summary: &Summary) -> String {
    let findings: Vec<Value> = findings
        .iter()
        .map(|f| {
            json!({
                "file": f.file, "line": f.line, "identifier": f.identifier,
                "rules": f.finding.rules.iter().map(|r| r.code()).collect::<Vec<_>>(),
                "severity": f.finding.severity.to_string(), "found": f.finding.found, "preferred": f.finding.preferred,
                "arabic": f.finding.arabic, "message": f.finding.rules.message(),
                "known": f.known, "count": f.count, "suggested_identifier": f.suggestion,
            })
        })
        .collect();
    let c = &summary.counts;
    json!({
        "summary": {
            "files_scanned": c.files_scanned, "errors": summary.errors, "warnings": summary.warnings, "known": summary.known,
            "ignored_lines": c.ignored_lines, "external_names": c.external_names, "allowed_glosses": c.allowed_glosses,
            "reference_literals": c.reference_literals, "skipped_large_files": summary.skipped_large_files,
            "fixed_names": summary.fixed_names, "fixed_files": summary.fixed_files, "concepts": summary.concepts,
            "config": summary.config,
        },
        "findings": findings,
    })
    .to_string()
}
