//! Printing results for people (text, Markdown) and for tools (JSON).
//!
//! A problem found in several places is shown once, with every place it
//! occurs. Identical files are reported once, under the first path. The
//! summary of what to fix comes last in the terminal, where it stays on
//! screen, and first in Markdown, which is read from the top.

use crate::data::findings::*;
use serde_json::json;
use std::collections::{BTreeMap, HashMap};
use std::fmt::Write;

pub struct Summary<'a> {
    pub title: &'a str,
    pub files: usize,
    pub bytes: u64,
    pub millis: u128,
    pub findings: &'a [Finding],
    pub inventory: &'a [Inventory],
    pub evidence: &'a [Evidence],
    /// First path → the other paths holding the same file.
    pub copies: &'a BTreeMap<String, Vec<String>>,
    pub config: Option<String>,
}

const WIDTH: usize = 100;
const PLACES: usize = 5;
const FIX_FIRST: usize = 5;
/// With this many kinds of problem or fewer, the details are short enough to need no summary.
const SUMMARY_FROM: usize = 3;
/// Bodies of Quran data are shown as one group when more than this many look alike.
const GROUP_FROM: usize = 3;

fn plural(n: impl Into<u64>, one: &str, many: &str) -> String {
    let n = n.into();
    format!("{n} {}", if n == 1 { one } else { many })
}

/// Findings of a severity, names apart: they have their own count.
fn count(findings: &[Finding], s: Severity) -> usize {
    findings.iter().filter(|f| f.severity == s && f.group != Group::Names).count()
}

fn names_count(findings: &[Finding]) -> usize {
    findings.iter().filter(|f| f.group == Group::Names).count()
}

fn totals(findings: &[Finding]) -> String {
    let mut t = format!(
        "{}, {}, {}",
        plural(count(findings, Severity::Error) as u64, "error", "errors"),
        plural(count(findings, Severity::Warning) as u64, "warning", "warnings"),
        plural(count(findings, Severity::Note) as u64, "note", "notes")
    );
    match names_count(findings) {
        0 => {}
        n => t += &format!("; {} the dictionary gives differently", plural(n as u64, "field name", "field names")),
    }
    t
}

fn copied(s: &Summary) -> usize {
    s.copies.values().map(Vec::len).sum()
}

/// Wraps `text` at word boundaries; lines after the first start with `indent`.
fn wrap(text: &str, first: usize, indent: &str) -> String {
    let mut out = String::new();
    let mut col = first;
    for word in text.split(' ') {
        let w = word.chars().count();
        if col > indent.len() && col + 1 + w > WIDTH {
            out.push('\n');
            out.push_str(indent);
            col = indent.len();
        } else if !out.is_empty() {
            out.push(' ');
            col += 1;
        }
        out.push_str(word);
        col += w;
    }
    out
}

/// One problem and every place it occurs: findings with the same code and wording.
struct Issue<'a> {
    first: &'a Finding,
    places: Vec<&'a Finding>,
}

fn issues(findings: &[Finding]) -> Vec<Issue<'_>> {
    let mut out: Vec<Issue> = Vec::new();
    let mut index: HashMap<(Severity, Group, &str, &str, Option<&str>), usize> = HashMap::new();
    for f in findings {
        let key = (f.severity, f.group, f.code, f.message.as_str(), f.fix.as_deref());
        match index.get(&key) {
            Some(&i) => out[i].places.push(f),
            None => {
                index.insert(key, out.len());
                out.push(Issue { first: f, places: vec![f] });
            }
        }
    }
    out.sort_by_key(|i| (i.first.severity, rank(i.first.code)));
    out
}

/// The first clause of a message: what is wrong, without the why.
fn headline(message: &str) -> &str {
    let cut = [": ", "; ", ", so "].iter().filter_map(|p| message.find(p)).min().unwrap_or(message.len());
    &message[..cut]
}

/// One line of the summary: a kind of problem, the files it is in, the fix.
struct Headline {
    severity: Severity,
    title: String,
    files: Vec<String>,
    fix: Option<String>,
}

/// Errors and warnings, one line per kind of problem however many files have it.
fn fix_first(issues: &[Issue]) -> Vec<Headline> {
    struct Acc {
        h: Headline,
        code: &'static str,
        fixes: Vec<String>,
        values: Vec<(String, Option<String>)>,
    }
    let mut out: Vec<Acc> = Vec::new();
    let mut index: HashMap<(Severity, String), usize> = HashMap::new();
    for i in issues.iter().filter(|i| i.first.severity != Severity::Note && i.first.group != Group::Names) {
        let f = i.first;
        let t = title(f.code).map(String::from).unwrap_or_else(|| headline(&f.message).to_string());
        let at = *index.entry((f.severity, t.clone())).or_insert_with(|| {
            out.push(Acc { h: Headline { severity: f.severity, title: t, files: vec![], fix: None }, code: f.code, fixes: vec![], values: vec![] });
            out.len() - 1
        });
        let a = &mut out[at];
        for p in &i.places {
            if !a.h.files.contains(&p.file) {
                a.h.files.push(p.file.clone());
            }
        }
        if let Some(fx) = &f.fix {
            if !a.fixes.contains(fx) {
                a.fixes.push(fx.clone());
            }
        }
        if let Some(v) = &f.value {
            if !a.values.contains(v) {
                a.values.push(v.clone());
            }
        }
    }
    out.into_iter()
        .map(|mut a| {
            a.h.fix = if !a.values.is_empty() {
                Some(value_fix(a.code, &a.values))
            } else if a.fixes.len() == 1 {
                a.fixes.pop()
            } else {
                general_fix(a.code).map(String::from)
            };
            a.h
        })
        .collect()
}

/// `write `hafs_an_asim` for `hafsh``, for every value of a kind.
fn value_fix(code: &str, values: &[(String, Option<String>)]) -> String {
    let shown = 4;
    let more = |n: usize| if n > shown { format!(" and {} more", n - shown) } else { String::new() };
    let with: Vec<String> = values.iter().filter_map(|(raw, c)| c.as_ref().map(|c| format!("`{c}` for `{raw}`"))).collect();
    let without: Vec<String> = values.iter().filter(|(_, c)| c.is_none()).map(|(raw, _)| format!("`{raw}`")).collect();
    let mut parts = Vec::new();
    if !with.is_empty() {
        let maybe = if code == "value-unknown" { " (if that is what they mean)" } else { "" };
        parts.push(format!("write {}{}{maybe}", with.iter().take(shown).cloned().collect::<Vec<_>>().join(", "), more(with.len())));
    }
    if !without.is_empty() {
        let what = if matches!(code, "value-ambiguous" | "value-qiraah-as-riwayah") { "name the riwayah meant by" } else { "use registry codes for" };
        parts.push(format!("{what} {}{}", without.iter().take(shown).cloned().collect::<Vec<_>>().join(", "), more(without.len())));
    }
    parts.join("; ")
}

fn files_list(files: &[String]) -> String {
    match files.len() {
        0..=3 => files.join(", "),
        n => format!("{}, {} and {} more files", files[0], files[1], n - 2),
    }
}

fn mark(s: Severity) -> &'static str {
    match s {
        Severity::Error => "✗",
        Severity::Warning => "!",
        Severity::Note => "·",
    }
}

fn place(f: &Finding, copies: &BTreeMap<String, Vec<String>>) -> String {
    let mut s = match f.line {
        Some(l) => format!("{}:{l}", f.file),
        None => f.file.clone(),
    };
    if !f.at.is_empty() && f.at != "/" {
        let _ = write!(s, " [{}]", f.at);
    }
    if f.count > 1 {
        let _ = write!(s, " ×{}", f.count);
    }
    if let Some(c) = copies.get(&f.file) {
        let _ = write!(s, " (+ {})", plural(c.len() as u64, "copy", "copies"));
    }
    s
}

// ------------------------------------------------------------- inventory

/// Bodies of Quran data that look alike (one surah per file, say) shown as one.
struct Body<'a> {
    items: Vec<&'a Inventory>,
}

fn bodies(inventory: &[Inventory]) -> Vec<Body<'_>> {
    let mut out: Vec<Body> = Vec::new();
    let mut index: HashMap<(String, String, String, usize), usize> = HashMap::new();
    for i in inventory {
        let key = (i.shape.clone(), i.at.clone(), stated(i), i.surahs);
        match index.get(&key) {
            Some(&at) => out[at].items.push(i),
            None => {
                index.insert(key, out.len());
                out.push(Body { items: vec![i] });
            }
        }
    }
    // Two or three alike read better one by one: each keeps its own numbering.
    out.into_iter()
        .flat_map(|b| if b.items.len() > GROUP_FROM { vec![b] } else { b.items.into_iter().map(|i| Body { items: vec![i] }).collect() })
        .collect()
}

/// Files in a group, counting copies.
fn file_count(b: &Body, copies: &BTreeMap<String, Vec<String>>) -> usize {
    b.items.len() + b.items.iter().filter_map(|i| copies.get(&i.file)).map(Vec::len).sum::<usize>()
}

fn numbering(b: &Body, copies: &BTreeMap<String, Vec<String>>) -> String {
    let compared: usize = b.items.iter().map(|i| i.compared).sum();
    let fits: Vec<usize> = (0..6).map(|s| b.items.iter().map(|i| i.fits.get(s).copied().unwrap_or(0)).sum()).collect();
    let max = fits.iter().copied().max().unwrap_or(0);
    let best: Vec<&str> = (0..6).filter(|s| fits[*s] == max).map(|s| crate::data::registry::SYSTEMS[s]).collect();
    let fit = if best.len() == 6 { "any system".to_string() } else { best.join(" or ") };
    if b.items.len() == 1 {
        return format!("fits {fit} ({max}/{})", plural(compared as u64, "surah", "surahs"));
    }
    // For a group, how many of its files fit fully.
    let s = (0..6).find(|s| fits[*s] == max).unwrap_or(0);
    let full: usize = b.items.iter().filter(|i| i.fits.get(s).copied() == Some(i.compared)).map(|i| 1 + copies.get(&i.file).map_or(0, Vec::len)).sum();
    let files = file_count(b, copies);
    if full == files {
        format!("fits {fit} (all {files} files)")
    } else {
        format!("fits {fit} ({full} of {files} files fully)")
    }
}

fn sajdah(b: &Body) -> Option<String> {
    let mut t = SajdahMarks::default();
    for m in b.items.iter().filter_map(|i| i.sajdah.as_ref()) {
        t.marked += m.marked;
        t.at_places += m.at_places;
        t.other_numbering.extend(&m.other_numbering);
        t.other_marks += m.other_marks;
        t.differ.extend(&m.differ);
    }
    if t.marked == 0 {
        return None;
    }
    let with = b.items.iter().filter(|i| i.sajdah.is_some()).count();
    let mut s = if b.items.len() > 1 && with < b.items.len() {
        format!("in {with} of {} files, {} marked", b.items.len(), t.marked)
    } else {
        format!("{} marked", t.marked)
    };
    let differ = if t.differ.is_empty() { String::new() } else { format!("; they differ in {} (see below)", plural(t.differ.len() as u64, "surah", "surahs")) };
    if t.other_numbering.is_empty() {
        if t.differ.is_empty() {
            s += ", all at the registry's places";
        } else {
            let _ = write!(s, ": {} at the registry's places{differ}", t.at_places);
        }
        return Some(s);
    }
    // Not Kufi: compared surah by surah, by number only.
    let _ = write!(s, "; the registry's places are Kufi ayah numbers, so only the number in each surah is compared: it matches in {}{differ}",
        plural(t.other_numbering.len() as u64, "surah", "surahs"));
    Some(s)
}

fn stated(i: &Inventory) -> String {
    match (&i.stated_riwayah, &i.stated_numbering) {
        (Some(r), Some(n)) => format!("states {r}, {n}"),
        (Some(r), None) => format!("states {r}"),
        (None, Some(n)) => format!("states {n}"),
        (None, None) => match &i.hint {
            Some(h) if h.starts_with("names") => h.clone(),
            Some(h) => format!("states nothing; {h}"),
            None => "states nothing".into(),
        },
    }
}

/// Where a group of bodies is: one path, or the folder they share.
fn body_place(b: &Body, copies: &BTreeMap<String, Vec<String>>) -> String {
    let i = b.items[0];
    let at = if i.at == "/" { String::new() } else { format!(" {}", i.at) };
    if b.items.len() == 1 {
        return format!("{}{at}", i.file);
    }
    let dirs: Vec<&str> = b.items.iter().map(|i| i.file.rsplit_once('/').map_or("", |(d, _)| d)).collect();
    let mut common: Vec<&str> = dirs[0].split('/').collect();
    for d in &dirs[1..] {
        let parts: Vec<&str> = d.split('/').collect();
        let same = common.iter().zip(&parts).take_while(|(a, b)| a == b).count();
        common.truncate(same);
    }
    let files = file_count(b, copies);
    let dir = common.join("/");
    if dir.is_empty() {
        format!("{files} files like {}{at}", i.file)
    } else {
        format!("{dir}/ ({files} files like {}){at}", i.file.rsplit('/').next().unwrap_or(""))
    }
}

fn body_size(b: &Body) -> String {
    let i = b.items[0];
    if b.items.len() == 1 {
        return format!("{} · {} · {}", i.shape, plural(i.surahs as u64, "surah", "surahs"), plural(i.ayahs, "ayah", "ayahs"));
    }
    if b.items.iter().all(|x| x.surahs == i.surahs && x.ayahs == i.ayahs) {
        return format!("{} · {} · {} each", i.shape, plural(i.surahs as u64, "surah", "surahs"), plural(i.ayahs, "ayah", "ayahs"));
    }
    let surahs: usize = b.items.iter().map(|i| i.surahs).sum();
    let ayahs: u32 = b.items.iter().map(|i| i.ayahs).sum();
    format!("{} · {} · {} in all", i.shape, plural(surahs as u64, "surah", "surahs"), plural(ayahs, "ayah", "ayahs"))
}

fn same_file_line(b: &Body, copies: &BTreeMap<String, Vec<String>>) -> Option<String> {
    if b.items.len() != 1 {
        return None;
    }
    copies.get(&b.items[0].file).map(|c| match c.len() {
        1 => format!("same file at {}", c[0]),
        n => format!("same file at {} and {} other paths", c[0], n - 1),
    })
}

// ------------------------------------------------------------------ text

const GROUPS: [(Group, &str); 3] = [
    (Group::Meaning, "Meaning — what a consumer would read wrongly"),
    (Group::Values, "Values — codes no other project can resolve"),
    (Group::Names, "Names — fields and columns the dictionary names differently"),
];

pub fn text(s: &Summary) -> String {
    let mut o = String::new();
    let issues = issues(s.findings);
    let _ = writeln!(o, "{}", s.title);
    let copies = match copied(s) {
        0 => String::new(),
        n => format!("; {} of other files, reported once", plural(n as u64, "is a copy", "are copies")),
    };
    let _ = writeln!(o, "Read {} ({:.1} MB) in {} ms{copies}{}", plural(s.files as u64, "data file", "data files"), s.bytes as f64 / 1e6, s.millis,
        s.config.as_ref().map(|c| format!(" · declaration: {c}")).unwrap_or_default());

    if !s.inventory.is_empty() {
        let _ = writeln!(o, "\nQuran data");
        for b in bodies(s.inventory) {
            let _ = writeln!(o, "  {}", body_place(&b, s.copies));
            let _ = writeln!(o, "      {} · {}", body_size(&b), numbering(&b, s.copies));
            let _ = writeln!(o, "      {}", stated(b.items[0]));
            if let Some(sj) = sajdah(&b) {
                let _ = writeln!(o, "      sajdah: {}", wrap(&sj, 14, "              "));
            }
            if let Some(c) = same_file_line(&b, s.copies) {
                let _ = writeln!(o, "      {c}");
            }
        }
    }

    for (group, title) in GROUPS {
        let items: Vec<&Issue> = issues.iter().filter(|i| i.first.group == group).collect();
        if items.is_empty() {
            continue;
        }
        let _ = writeln!(o, "\n{title}");
        if group == Group::Names {
            let all: Vec<&Finding> = s.findings.iter().filter(|f| f.group == group).collect();
            for line in names(&all) {
                let _ = writeln!(o, "  {line}");
            }
            continue;
        }
        let pad = "             ";
        for i in items {
            let f = i.first;
            let label = format!("  {} {:<8} ", mark(f.severity), f.severity.to_string());
            let _ = writeln!(o, "{label}{}", wrap(&f.message, label.chars().count(), pad));
            for (n, p) in i.places.iter().take(PLACES).enumerate() {
                let _ = writeln!(o, "{pad}{} {}", if n == 0 { "at" } else { "  " }, place(p, s.copies));
            }
            if i.places.len() > PLACES {
                let _ = writeln!(o, "{pad}   … and {} more places (--format json lists them all)", i.places.len() - PLACES);
            }
            if let Some(fix) = &f.fix {
                let _ = writeln!(o, "{pad}→  {}", wrap(fix, pad.len() + 3, &format!("{pad}   ")));
            }
            let _ = writeln!(o, "{pad}basis: {}", f.basis);
        }
    }
    evidence_text(&mut o, s.evidence, s.copies);

    let headlines = fix_first(&issues);
    if headlines.len() > SUMMARY_FROM {
        let _ = writeln!(o, "\nFix first");
        for (n, h) in headlines.iter().take(FIX_FIRST).enumerate() {
            let _ = writeln!(o, "  {}. {} {}", n + 1, mark(h.severity), h.title);
            let _ = writeln!(o, "       in {}", wrap(&files_list(&h.files), 10, "          "));
            if let Some(fix) = &h.fix {
                let _ = writeln!(o, "       → {}", wrap(fix, 9, "         "));
            }
        }
        if headlines.len() > FIX_FIRST {
            let _ = writeln!(o, "  … and {} above", plural((headlines.len() - FIX_FIRST) as u64, "more kind of problem", "more kinds of problem"));
        }
    }
    let _ = writeln!(o, "\n{}", totals(s.findings));
    o
}

/// Names folded to one line each: the field, the dictionary's name, and where it occurs.
fn names(items: &[&Finding]) -> Vec<String> {
    let mut by: BTreeMap<(String, String), (std::collections::BTreeSet<String>, usize)> = BTreeMap::new();
    for f in items {
        let canonical = f.fix.as_deref().unwrap_or("").split('`').nth(1).unwrap_or("").to_string();
        let e = by.entry((f.at.clone(), canonical)).or_default();
        e.0.insert(f.file.clone());
        e.1 += f.count;
    }
    let mut rows: Vec<_> = by.into_iter().collect();
    rows.sort_by(|a, b| b.1 .1.cmp(&a.1 .1).then(a.0.cmp(&b.0)));
    let total = rows.len();
    let mut out: Vec<String> = rows
        .iter()
        .take(20)
        .map(|((field, canonical), (files, n))| {
            let place = if files.len() == 1 { files.iter().next().unwrap().clone() } else { format!("{} files", files.len()) };
            format!("`{field}` → `{canonical}`  ({place}, {n}×)")
        })
        .collect();
    if total > 20 {
        out.push(format!("… and {} more names (--format json lists them all)", total - 20));
    }
    out
}

fn evidence_rows(evidence: &[Evidence], copies: &BTreeMap<String, Vec<String>>) -> Vec<String> {
    let mut by: BTreeMap<(String, u32, u32), Vec<(u32, String)>> = BTreeMap::new();
    for e in evidence {
        by.entry((e.system.clone(), e.surah, e.registry)).or_default().push((e.data, e.file.clone()));
    }
    by.into_iter()
        .map(|((system, surah, registry), seen)| {
            let files: Vec<String> = seen
                .iter()
                .map(|(d, f)| match copies.get(f) {
                    Some(c) => format!("{f} ({d}; + {})", plural(c.len() as u64, "copy", "copies")),
                    None => format!("{f} ({d})"),
                })
                .collect();
            format!("{system}, surah {surah}: registry {registry}; data says {}", files.join(", "))
        })
        .collect()
}

fn evidence_text(o: &mut String, evidence: &[Evidence], copies: &BTreeMap<String, Vec<String>>) {
    if evidence.is_empty() {
        return;
    }
    let _ = writeln!(o, "\nRegistry evidence — where data disagrees with Marqoom's ayah_counts");
    for row in evidence_rows(evidence, copies) {
        let _ = writeln!(o, "  {}", wrap(&row, 2, "    "));
    }
}

// -------------------------------------------------------------- Markdown

pub fn markdown(s: &Summary) -> String {
    let mut o = String::new();
    let issues = issues(s.findings);
    let _ = writeln!(o, "# {}\n", s.title);
    let copies = match copied(s) {
        0 => String::new(),
        n => format!(" {} of other files, reported once.", plural(n as u64, "is a copy", "are copies")),
    };
    let _ = writeln!(o, "Read {} ({:.1} MB) in {} ms.{copies} **{}.**\n", plural(s.files as u64, "data file", "data files"), s.bytes as f64 / 1e6, s.millis, totals(s.findings));

    let headlines = fix_first(&issues);
    if headlines.len() > SUMMARY_FROM {
        let _ = writeln!(o, "## Fix first\n");
        for (n, h) in headlines.iter().take(FIX_FIRST).enumerate() {
            let files: Vec<String> = h.files.iter().map(|f| format!("`{f}`")).collect();
            let _ = writeln!(o, "{}. **{}** {} — {}", n + 1, h.severity, h.title, files_list(&files));
            if let Some(fix) = &h.fix {
                let _ = writeln!(o, "   Fix: {fix}");
            }
        }
        if headlines.len() > FIX_FIRST {
            let _ = writeln!(o, "\n… and {} below.", plural((headlines.len() - FIX_FIRST) as u64, "more kind of problem", "more kinds of problem"));
        }
        o.push('\n');
    }

    if !s.inventory.is_empty() {
        let _ = writeln!(o, "## Quran data\n\n| Where | Shape and size | Numbering | Says about itself | Sajdah marks |\n|---|---|---|---|---|");
        for b in bodies(s.inventory) {
            let mut place = format!("`{}`", body_place(&b, s.copies));
            if let Some(c) = same_file_line(&b, s.copies) {
                let _ = write!(place, " ({c})");
            }
            let _ = writeln!(o, "| {place} | {} | {} | {} | {} |", body_size(&b), numbering(&b, s.copies), stated(b.items[0]), sajdah(&b).unwrap_or_default());
        }
        o.push('\n');
    }
    for (group, title) in GROUPS {
        let items: Vec<&Issue> = issues.iter().filter(|i| i.first.group == group).collect();
        if items.is_empty() {
            continue;
        }
        let _ = writeln!(o, "## {title}\n");
        if group == Group::Names {
            let all: Vec<&Finding> = s.findings.iter().filter(|f| f.group == group).collect();
            for line in names(&all) {
                let _ = writeln!(o, "- {line}");
            }
            o.push('\n');
            continue;
        }
        for i in items {
            let f = i.first;
            let _ = writeln!(o, "- **{}** — {}", f.severity, f.message);
            let mut places: Vec<String> = i.places.iter().take(PLACES).map(|p| format!("`{}`", place(p, s.copies))).collect();
            if i.places.len() > PLACES {
                places.push(format!("and {} more", i.places.len() - PLACES));
            }
            let _ = writeln!(o, "  - At: {}", places.join(", "));
            if let Some(fix) = &f.fix {
                let _ = writeln!(o, "  - Fix: {fix}");
            }
            let _ = writeln!(o, "  - Basis: {}", f.basis);
        }
        o.push('\n');
    }
    if !s.evidence.is_empty() {
        let _ = writeln!(o, "## Registry evidence\n\nWhere data disagrees with Marqoom's `ayah_counts`:\n");
        for row in evidence_rows(s.evidence, s.copies) {
            let _ = writeln!(o, "- {row}");
        }
    }
    o
}

// ------------------------------------------------------------------ JSON

pub fn json(s: &Summary) -> String {
    let mut ordered: Vec<&Finding> = s.findings.iter().collect();
    ordered.sort_by_key(|f| (f.severity, rank(f.code)));
    let findings: Vec<_> = ordered.iter().map(|f| json!({
        "severity": f.severity.to_string(), "group": format!("{:?}", f.group).to_lowercase(), "code": f.code,
        "file": f.file, "line": f.line, "at": f.at, "message": f.message, "fix": f.fix, "basis": f.basis, "count": f.count,
    })).collect();
    let inventory: Vec<_> = s.inventory.iter().map(|i| json!({
        "file": i.file, "at": i.at, "shape": i.shape, "surahs": i.surahs, "ayahs": i.ayahs,
        "numbering_fits": i.numbering, "matched": i.matched, "compared": i.compared,
        "riwayah": i.stated_riwayah, "ayah_numbering_system": i.stated_numbering, "hint": i.hint,
        "sajdah": i.sajdah.as_ref().map(|m| json!({
            "marked": m.marked, "at_registry_places": m.at_places,
            "surahs_counted_differently": m.other_numbering, "marks_in_those_surahs": m.other_marks, "surahs_that_differ": m.differ,
        })),
    })).collect();
    let evidence: Vec<_> = s.evidence.iter().map(|e| json!({
        "ayah_numbering_system": e.system, "surah": e.surah, "registry": e.registry, "data": e.data, "file": e.file,
    })).collect();
    let fix: Vec<_> = fix_first(&issues(s.findings)).iter().map(|h| json!({
        "severity": h.severity.to_string(), "problem": h.title, "files": h.files, "fix": h.fix,
    })).collect();
    serde_json::to_string_pretty(&json!({
        "tool": s.title, "files": s.files, "bytes": s.bytes, "millis": s.millis as u64,
        "summary": { "errors": count(s.findings, Severity::Error), "warnings": count(s.findings, Severity::Warning), "notes": count(s.findings, Severity::Note), "names": names_count(s.findings) },
        "fix_first": fix, "quran_data": inventory, "findings": findings, "registry_evidence": evidence, "copies": s.copies,
    })).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn finding(file: &str, code: &'static str, message: &str, value: Option<(&str, Option<&str>)>) -> Finding {
        Finding {
            severity: Severity::Warning,
            group: if code.starts_with("value-") { Group::Values } else { Group::Meaning },
            code,
            file: file.into(),
            line: None,
            at: String::new(),
            message: message.into(),
            fix: Some("fix it".into()),
            basis: "rule 072",
            count: 1,
            value: value.map(|(r, c)| (r.to_string(), c.map(String::from))),
        }
    }

    #[test]
    fn one_problem_in_many_files_is_one_issue() {
        let fs = vec![
            finding("a.json", "word-order-only", "words by order", None),
            finding("b.json", "word-order-only", "words by order", None),
            finding("b.json", "basmalah-attached", "1:1 begins with the basmalah", None),
        ];
        let i = issues(&fs);
        assert_eq!(i.len(), 2);
        assert_eq!(i[0].first.code, "basmalah-attached"); // ranked above word order
        assert_eq!(i[1].places.len(), 2);
    }

    #[test]
    fn values_of_a_kind_share_one_summary_line() {
        let fs = vec![
            finding("a.json", "value-alternative", "`makkah` is a recorded spelling", Some(("makkah", Some("makki")))),
            finding("a.json", "value-alternative", "`madinah` is a recorded spelling", Some(("madinah", Some("madani")))),
        ];
        let h = fix_first(&issues(&fs));
        assert_eq!(h.len(), 1);
        assert_eq!(h[0].fix.as_deref(), Some("write `makki` for `makkah`, `madani` for `madinah`"));
    }

    #[test]
    fn headline_is_the_first_clause() {
        assert_eq!(headline("`hafsh` is not a riwayah in the registry, so no one can resolve it; the nearest"), "`hafsh` is not a riwayah in the registry");
        assert_eq!(headline("1:1 begins with the basmalah; word positions"), "1:1 begins with the basmalah");
    }

    #[test]
    fn wraps_at_the_width() {
        let w = wrap(&"word ".repeat(40), 4, "    ");
        assert!(w.lines().all(|l| l.chars().count() <= WIDTH));
        assert!(w.lines().skip(1).all(|l| l.starts_with("    ")));
    }
}
