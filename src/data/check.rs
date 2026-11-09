//! The checks, file by file.

use crate::data::config::Config;
use crate::data::detect::{self, QuranData};
use crate::data::findings::*;
use crate::data::registry::{Registry, SYSTEMS};
use crate::data::text::{basmalah, norm, tokens};
use crate::data::values::{codeish, concept_of_key, Concept, Resolver, Verdict};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

pub struct Ctx<'a> {
    pub reg: &'a Registry,
    pub res: &'a Resolver<'a>,
    pub cfg: &'a Config,
}

const SAJDAH_MARK: char = '\u{06E9}';

pub fn check_file(ctx: &Ctx, path: &Path) -> FileResult {
    let mut out = FileResult::default();
    let rel = ctx.cfg.rel(path);
    out.file = rel.clone();
    let Ok(text) = std::fs::read_to_string(path) else { return out };
    out.bytes = text.len() as u64;
    out.digest = {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        text.hash(&mut h);
        h.finish()
    };
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    let mut f = FileCheck { ctx, rel: &rel, text: &text, out: &mut out, seen: HashMap::new() };
    match ext.as_str() {
        "json" | "geojson" => {
            let Ok(doc) = serde_json::from_str::<Value>(&text) else { return out };
            if doc.get("openapi").is_some() || doc.get("swagger").is_some() {
                f.openapi(&doc);
            } else {
                f.json(&doc);
            }
        }
        "csv" | "tsv" => f.table(if ext == "tsv" { '\t' } else { ',' }),
        "sql" => f.sql(),
        _ => {}
    }
    f.flush();
    out
}

struct FileCheck<'a, 'b> {
    ctx: &'a Ctx<'a>,
    rel: &'a str,
    text: &'a str,
    out: &'b mut FileResult,
    /// Deduplication: one finding per (code, subject), counting repeats.
    seen: HashMap<(String, String), Finding>,
}

/// What the data says about itself.
#[derive(Default)]
struct Stated {
    riwayah: Option<String>,
    numbering: Option<String>,
    /// A riwayah the data names that does not resolve to a code.
    unresolved: Option<String>,
    /// Fields holding ayah or word keys: field → (word keys, ayah keys, an example).
    refs: BTreeMap<String, (usize, usize, String)>,
}

/// `2:255` is an ayah reference, `2:255:3` a word reference.
fn reference_kind(s: &str) -> Option<u8> {
    let parts: Vec<&str> = s.split(':').collect();
    if !(2..=3).contains(&parts.len()) || parts.iter().any(|p| p.is_empty() || p.len() > 3 || !p.bytes().all(|b| b.is_ascii_digit())) {
        return None;
    }
    let surah: u32 = parts[0].parse().ok()?;
    (1..=114).contains(&surah).then_some(parts.len() as u8)
}

impl<'a, 'b> FileCheck<'a, 'b> {
    fn published(&self) -> bool {
        self.ctx.cfg.published(self.rel)
    }

    fn add(&mut self, subject: &str, finding: Finding) {
        let key = (finding.code.to_string(), subject.to_string());
        if let Some(existing) = self.seen.get_mut(&key) {
            existing.count += finding.count;
            return;
        }
        self.seen.insert(key, finding);
    }

    #[allow(clippy::too_many_arguments)] // Every field of a finding, at each of its call sites.
    fn finding(&self, severity: Severity, group: Group, code: &'static str, at: &str, line: Option<usize>, message: String, fix: Option<String>, basis: &'static str) -> Finding {
        Finding { severity, group, code, file: self.rel.to_string(), line, at: at.to_string(), message, fix, basis, count: 1, value: None }
    }

    fn flush(&mut self) {
        let mut all: Vec<Finding> = self.seen.drain().map(|(_, f)| f).collect();
        all.sort_by(|a, b| (a.severity, a.group, a.line, &a.message, &a.at).cmp(&(b.severity, b.group, b.line, &b.message, &b.at)));
        self.out.findings.extend(all);
    }

    // ---------------------------------------------------------------- JSON

    fn json(&mut self, doc: &Value) {
        let mut keys: BTreeMap<String, usize> = BTreeMap::new();
        let mut stated = Stated::default();
        self.walk(doc, None, "", 0, &mut keys, &mut stated);
        self.names(&keys);
        let refs = std::mem::take(&mut stated.refs);
        for (key, (words, ayahs, example)) in refs {
            let line = line_of(self.text, &format!("\"{example}\""));
            let decl = self.ctx.cfg.declaration(self.rel).cloned().unwrap_or_default();
            if words >= 3 && decl.segmentation.is_none() {
                let numbering = match (&stated.numbering, &stated.riwayah) {
                    (Some(n), _) => format!("; the numbering is stated (`{n}`)"),
                    (None, Some(r)) => format!("; the riwayah is stated (`{r}`)"),
                    (None, None) => ", or the numbering, so the ayah part is read as Kufi".into(),
                };
                let mut f = self.finding(Severity::Warning, Group::Meaning, "word-keys-unstated", &key, line,
                    format!("holds word keys (`surah:ayah:word`) but doesn't state the text or word segmentation they count by{numbering}"),
                    Some("state the text and segmentation (data.files.<file>.word_segmentation), and the riwayah".into()), "rules 071, 072");
                f.count = words;
                self.add(&key, f);
            } else if ayahs >= 3 && stated.numbering.is_none() && stated.riwayah.is_none() {
                let mut f = self.finding(Severity::Note, Group::Meaning, "ayah-keys-unstated", &key, line,
                    "holds ayah keys (`surah:ayah`) but doesn't state the riwayah or numbering; they are read as Kufi".into(),
                    Some("state `riwayah` or `ayah_numbering_system`".into()), "rule 072");
                f.count = ayahs;
                self.add(&key, f);
            }
        }
        for item in detect::find(doc) {
            self.quran(item, &stated);
        }
    }

    fn walk(&mut self, v: &Value, parent: Option<&str>, at: &str, depth: usize, keys: &mut BTreeMap<String, usize>, stated: &mut Stated) {
        match v {
            Value::Object(o) => {
                for (k, child) in o {
                    if k.parse::<f64>().is_err() {
                        *keys.entry(k.clone()).or_default() += 1;
                    }
                    if let Value::String(s) = child {
                        if let Some(kind) = reference_kind(s) {
                            let e = stated.refs.entry(k.clone()).or_insert((0, 0, s.clone()));
                            if kind == 3 { e.0 += 1 } else { e.1 += 1 }
                        }
                    }
                    if let Some(concept) = concept_of_key(k, parent) {
                        // A single value states something; a list only names options.
                        if let (Value::String(s), true) = (child, depth <= 3) {
                            self.remember(concept, s, stated);
                        }
                        let values: Vec<&str> = match child {
                            Value::String(s) => vec![s.as_str()],
                            Value::Array(a) => a.iter().filter_map(|x| x.as_str()).collect(),
                            _ => vec![],
                        };
                        for s in values {
                            self.value(concept, k, s, &format!("{at}/{k}"));
                        }
                    }
                }
                for (k, child) in o {
                    if child.is_object() || child.is_array() {
                        self.walk(child, Some(k), &format!("{at}/{k}"), depth + 1, keys, stated);
                    }
                }
            }
            Value::Array(a) => {
                self.recitations(a, parent, at);
                for (i, child) in a.iter().enumerate() {
                    if child.is_object() || child.is_array() {
                        // Elements share a shape: the path names the array, not each index.
                        let _ = i;
                        self.walk(child, parent, &format!("{at}/*"), depth + 1, keys, stated);
                    }
                }
            }
            _ => {}
        }
    }

    fn remember(&self, concept: Concept, raw: &str, stated: &mut Stated) {
        let code = match self.ctx.res.resolve_with(concept, raw, !self.published()) {
            Verdict::Canonical(c) | Verdict::Format(c) | Verdict::Declared(c) | Verdict::Alternative(c) | Verdict::Arabic(c) => Some(c),
            Verdict::Rawi(_, r) if r.len() == 1 => Some(r[0].clone()),
            _ => None,
        };
        if concept == Concept::Riwayah && code.is_none() && stated.unresolved.is_none() {
            stated.unresolved = Some(raw.to_string());
        }
        match concept {
            Concept::Riwayah if stated.riwayah.is_none() => stated.riwayah = code,
            Concept::Numbering if stated.numbering.is_none() => stated.numbering = code,
            _ => {}
        }
    }

    /// A value of a field that carries a registry concept.
    fn value(&mut self, concept: Concept, key: &str, raw: &str, at: &str) {
        // `bazzi|duri`: a list in one cell.
        if raw.contains('|') {
            for part in raw.split('|').map(str::trim).filter(|p| !p.is_empty()) {
                self.value(concept, key, part, at);
            }
            return;
        }
        if !codeish(raw) {
            return;
        }
        let line = crate::data::findings::line_of(self.text, &format!("\"{raw}\"")).or_else(|| line_of(self.text, key));
        let what = concept.code();
        let subject = format!("{what}={raw}");
        let unknown_severity = if concept == Concept::Revelation { Severity::Warning } else { Severity::Error };
        let published = self.published();
        let verdict = self.ctx.res.resolve_with(concept, raw, !published);
        let likely = match &verdict {
            Verdict::Format(c) | Verdict::Alternative(c) | Verdict::Arabic(c) | Verdict::Unknown(Some(c)) => Some(c.clone()),
            Verdict::Rawi(_, r) if r.len() == 1 => Some(r[0].clone()),
            _ => None,
        };
        let (sev, code, msg, fix): (Severity, &'static str, String, Option<String>) = match verdict {
            Verdict::Canonical(_) | Verdict::Declared(_) => return,
            Verdict::Format(c) => (Severity::Note, "value-format",
                format!("`{raw}` is the {what} `{c}` written another way; the standard writes enum values in snake_case"),
                Some(format!("write `{c}`"))),
            Verdict::Alternative(c) => (Severity::Warning, "value-alternative",
                format!("`{raw}` is a recorded spelling of the {what} `{c}`; other projects will expect the code"),
                Some(if published { format!("write `{c}`: this file is published, so a mapping in your declaration doesn't reach its readers") } else { format!("write `{c}`, or declare the mapping `\"{raw}\": \"{c}\"`") })),
            Verdict::Arabic(c) => (Severity::Note, "value-arabic",
                format!("`{raw}` is the Arabic name of the {what} `{c}`; keep it for display and exchange the code"),
                Some(format!("add `{c}` beside it"))),
            Verdict::Rawi(r, riwayahs) if riwayahs.len() == 1 => (Severity::Warning, "value-rawi",
                format!("`{raw}` names the rawi `{r}`, not a riwayah; the riwayah is `{}`", riwayahs[0]),
                Some(format!("write `{}`", riwayahs[0]))),
            Verdict::Rawi(r, riwayahs) => (Severity::Error, "value-ambiguous",
                format!("`{raw}` names the rawi `{r}`, who transmits more than one riwayah: {}; the value doesn't say which", list(&riwayahs)),
                Some("write the riwayah's code".into())),
            Verdict::Qiraah(q, riwayahs) => (Severity::Error, "value-qiraah-as-riwayah",
                format!("`{raw}` is the qiraah `{q}`, not a riwayah; its riwayahs are {}", list(&riwayahs)),
                Some("write the riwayah's code".into())),
            Verdict::Unknown(Some(c)) => (unknown_severity, "value-unknown",
                format!("`{raw}` is not a {what} in the registry, so no other project can resolve it; the nearest code is `{c}`"),
                Some(if published { format!("write `{c}` in the published data") } else { format!("write `{c}`, or declare the mapping `\"{raw}\": \"{c}\"` if that is what it means") })),
            Verdict::Unknown(None) => (unknown_severity, "value-unknown",
                format!("`{raw}` is not a {what} in the registry, so no other project can resolve it"),
                Some("use a registry code, or declare a mapping".into())),
        };
        let basis = match concept {
            Concept::Numbering => "registries: ayah_numbering; rule 055",
            Concept::Revelation => "dictionary: makki, madani",
            _ => "registries: qiraat",
        };
        let mut f = self.finding(sev, Group::Values, code, at, line, msg, fix, basis);
        f.value = Some((raw.to_string(), likely));
        self.add(&subject, f);
    }

    /// Non-canonical names of the data's own fields, once per name.
    fn names(&mut self, keys: &BTreeMap<String, usize>) {
        for (k, n) in keys {
            if let Some(canonical) = self.ctx.res.canonical_key(k) {
                if norm(k) == canonical {
                    continue;
                }
                let line = line_of(self.text, &format!("\"{k}\""));
                let sev = if self.published() { Severity::Warning } else { Severity::Note };
                let mut f = self.finding(sev, Group::Names, "name-noncanonical", k, line,
                    format!("field `{k}` names a concept the dictionary calls `{canonical}`"),
                    Some(format!("`{canonical}` (adapt the case to the layer)")), "rule 001; section 97");
                f.count = *n;
                self.seen.insert(("name-noncanonical".into(), k.clone()), f);
            }
        }
    }

    /// Recitations that name a reciter but not the riwayah the audio follows.
    fn recitations(&mut self, a: &[Value], parent: Option<&str>, at: &str) {
        let objects: Vec<_> = a.iter().filter_map(|x| x.as_object()).collect();
        if objects.len() < 2 {
            return;
        }
        let reciter = |o: &serde_json::Map<String, Value>| o.keys().any(|k| {
            let t = tokens(k);
            t.first().is_some_and(|x| x == "reciter" || x == "qari" || x == "reciters") // terminology: ignore
        });
        let riwayah = |o: &serde_json::Map<String, Value>| {
            o.keys().any(|k| matches!(concept_of_key(k, None), Some(Concept::Riwayah | Concept::Qiraah | Concept::Rawi)))
        };
        let with_reciter = objects.iter().filter(|o| reciter(o)).count();
        if with_reciter * 2 < objects.len() || objects.iter().any(|o| riwayah(o)) {
            return;
        }
        let line = parent.and_then(|p| line_of(self.text, &format!("\"{p}\"")));
        let f = self.finding(Severity::Warning, Group::Meaning, "recitation-without-riwayah", at, line,
            format!("{with_reciter} recitations name a reciter but not the riwayah the audio follows; a consumer can't tell which text or numbering its timings match"),
            Some("add `riwayah` with a registry code to each recitation".into()), "registries: qiraat; rule 072");
        self.add(at, f);
    }

    // ---------------------------------------------------------- Quran data

    fn quran(&mut self, d: QuranData, stated_in_data: &Stated) {
        let reg = self.ctx.reg;
        let compared: Vec<(u32, u32)> = d.counts.iter().map(|(s, c)| (*s, *c)).filter(|(s, _)| (1..=114).contains(s)).collect();
        if compared.is_empty() {
            return;
        }
        let fits: Vec<usize> = (0..6).map(|i| compared.iter().filter(|(s, c)| reg.counts[*s as usize - 1][i] == *c).count()).collect();
        let max = *fits.iter().max().unwrap();
        let best: Vec<String> = (0..6).filter(|i| fits[*i] == max).map(|i| SYSTEMS[i].to_string()).collect();
        let n = compared.len();
        let diffs = |i: usize| -> Vec<(u32, u32, u32)> {
            compared.iter().filter(|(s, c)| reg.counts[*s as usize - 1][i] != *c).map(|(s, c)| (*s, *c, reg.counts[*s as usize - 1][i])).collect()
        };

        // What is stated: in the data, then in the project's declaration.
        // Published data must state these itself: its consumers never see the declaration.
        let decl = if self.published() { Default::default() } else { self.ctx.cfg.declaration(self.rel).cloned().unwrap_or_default() };
        let resolve = |c: Concept, v: &Option<String>| v.as_ref().and_then(|v| match self.ctx.res.resolve(c, v) {
            Verdict::Canonical(x) | Verdict::Format(x) | Verdict::Alternative(x) | Verdict::Declared(x) => Some(x),
            _ => None,
        });
        let riwayah = stated_in_data.riwayah.clone().or_else(|| resolve(Concept::Riwayah, &decl.riwayah));
        // A block named after a system states it: `system_profiles/basri`.
        let named = d.pointer.rsplit('/').next().map(String::from).filter(|seg| matches!(self.ctx.res.resolve(Concept::Numbering, seg), Verdict::Canonical(_) | Verdict::Format(_)));
        let numbering = stated_in_data.numbering.clone().or_else(|| resolve(Concept::Numbering, &decl.numbering)).or_else(|| resolve(Concept::Numbering, &named));
        let hint = self.riwayah_from_path();

        let ayahs: u32 = compared.iter().map(|(_, c)| c).sum();
        self.out.inventory.push(Inventory {
            file: self.rel.to_string(),
            at: d.pointer.clone(),
            shape: d.shape.to_string(),
            surahs: n,
            ayahs,
            numbering: best.clone(),
            fits: fits.clone(),
            matched: max,
            compared: n,
            stated_riwayah: riwayah.clone(),
            stated_numbering: numbering.clone(),
            sajdah: None,
            hint: match (&stated_in_data.unresolved, &riwayah) {
                (Some(raw), None) => Some(format!("names `{raw}`, which is not a registry code")),
                _ => hint.clone().map(|(raw, code)| match code {
                    Some(c) => format!("file name says `{raw}` → {c}"),
                    None => format!("file name says `{raw}`"),
                }),
            },
        });

        let at = d.pointer.clone();
        let line = None;
        let unique_best = (best.len() == 1).then(|| best[0].clone());
        // An edition may differ from the registry at a few disputed points; more than that is another system.
        let near = |i: usize| { let d = diffs(i); (n >= 100 && !d.is_empty() && d.len() <= 5 && fits[i] == max).then_some(d) };
        let show = |d: &[(u32, u32, u32)]| d.iter().take(6).map(|(s, c, r)| format!("surah {s}: {c} here, {r} in the registry")).collect::<Vec<_>>().join("; ");
        let kufi = 3usize;

        let rel: &str = self.rel;
        let evidence = |sys: usize, d: &[(u32, u32, u32)], out: &mut FileResult| {
            for (s, c, r) in d {
                out.evidence.push(Evidence { system: SYSTEMS[sys].into(), surah: *s, registry: *r, data: *c, file: rel.to_string() });
            }
        };

        if let Some(sys) = numbering.as_deref().and_then(Registry::system_index) {
            // The data states its numbering.
            if fits[sys] < n {
                if let Some(d) = near(sys) {
                    evidence(sys, &d, self.out);
                    let f = self.finding(Severity::Warning, Group::Meaning, "counts-differ-from-registry", &at, line,
                        format!("states `{}` and matches the registry in {} of {} surahs; {}", SYSTEMS[sys], fits[sys], n, show(&d)),
                        Some("check these surahs against the source edition; if the data is right, the registry needs review".into()), "registries: ayah_counts");
                    self.add(&format!("counts {at}"), f);
                } else {
                    let f = self.finding(Severity::Error, Group::Meaning, "numbering-contradicts-statement", &at, line,
                        format!("states `{}` but its ayah counts fit {} ({} of {} surahs); {}", SYSTEMS[sys], list(&best), max, n, show(&diffs(sys))),
                        Some("correct the stated numbering, or the data".into()), "rule 072; registries: ayah_counts");
                    self.add(&format!("numbering {at}"), f);
                }
            }
        } else if let Some(r) = &riwayah {
            // The riwayah is stated; its numbering is the conventional one.
            if let Some(sys) = reg.numbering_of_riwayah(r).and_then(Registry::system_index) {
                if fits[sys] < n {
                    if let Some(d) = near(sys) {
                        evidence(sys, &d, self.out);
                        let f = self.finding(Severity::Warning, Group::Meaning, "counts-differ-from-registry", &at, line,
                            format!("`{r}` is counted in `{}`, which matches the registry in {} of {} surahs; {}", SYSTEMS[sys], fits[sys], n, show(&d)),
                            Some("check these surahs against the source edition".into()), "registries: ayah_counts");
                        self.add(&format!("counts {at}"), f);
                    } else {
                        let f = self.finding(Severity::Warning, Group::Meaning, "numbering-not-conventional", &at, line,
                            format!("`{r}` is conventionally counted in `{}`, but these counts fit {} ({} of {} surahs)", SYSTEMS[sys], list(&best), max, n),
                            unique_best.as_ref().map(|b| format!("state `\"ayah_numbering_system\": \"{b}\"` beside the riwayah")), "rule 072; registries: qiraat");
                        self.add(&format!("numbering {at}"), f);
                    }
                }
            }
        } else if fits[kufi] == n {
            // Nothing stated, and the counts are Kufi: rule 072 reads them correctly.
            let renumbered = hint.as_ref().and_then(|(_, c)| c.as_ref()).and_then(|r| reg.numbering_of_riwayah(r).map(|s| (r.clone(), s.to_string())));
            match renumbered {
                Some((r, sys)) if sys != "kufi" && n >= 100 => {
                    let f = self.finding(Severity::Warning, Group::Meaning, "numbering-renumbered", &at, line,
                        format!("numbered in `kufi`, though the file name says `{r}`, which is counted in `{sys}`; nothing in the data says the numbering was changed"),
                        Some(format!("state `\"riwayah\": \"{r}\", \"ayah_numbering_system\": \"kufi\"` so consumers know the numbering was changed on purpose")), "rule 072");
                    self.add(&format!("numbering {at}"), f);
                }
                _ => {
                    let f = self.finding(if self.published() { Severity::Warning } else { Severity::Note }, Group::Meaning, "numbering-unstated", &at, line,
                        "doesn't say which riwayah or numbering it follows; its counts are Kufi, which rule 072 assumes, so it reads correctly by default".to_string(),
                        Some("state `riwayah` (or `ayah_numbering_system`) so the default isn't a guess".into()), "rule 072");
                    self.add(&format!("numbering {at}"), f);
                }
            }
        } else {
            // Nothing stated, and the counts are not Kufi: consumers will misread them.
            let example = diffs(kufi).first().map(|(s, c, r)| format!("surah {s} has {c} ayahs here and {r} in Kufi"));
            let fit_text = match &unique_best {
                Some(b) if fits[Registry::system_index(b).unwrap()] == n => format!("The counts fit `{b}`"),
                Some(b) => format!("The counts come closest to `{b}` ({max} of {n} surahs; {})", show(&diffs(Registry::system_index(b).unwrap()))),
                None if n == 1 => format!("The count fits {}", list(&best)),
                None => format!("The counts fit {} ({max} of {n} surahs)", list(&best)),
            };
            let what = if d.shape == "surah table with ayah counts" {
                "ayah counts that are not Kufi, and nothing says so: a consumer would take them for Kufi counts"
            } else {
                "ayah numbers that are not Kufi, and nothing says so: rule 072 reads unstated numbers as Kufi, so a consumer would pair them with the wrong ayahs"
            };
            // A per-ayah mapping to Kufi numbering is the bridge consumers need; only the statement is missing.
            let sev = if d.kufi_mapping.is_some() { Severity::Note } else if self.published() { Severity::Error } else { Severity::Warning };
            let mapped = d.kufi_mapping.as_ref().map(|k| format!(" Each ayah carries `{k}`, which maps it to Kufi numbering, so a careful consumer can convert; the file still doesn't name its riwayah.")).unwrap_or_default();
            let hint_text = hint.as_ref().map(|(raw, c)| match c {
                Some(c) => format!(" The file name says `{raw}` → `{c}`."),
                None => format!(" The file name says `{raw}`."),
            }).unwrap_or_default();
            let f = self.finding(sev, Group::Meaning, "numbering-unstated-not-kufi", &at, line,
                format!("{what} ({}). {fit_text}.{hint_text}{mapped}", example.unwrap_or_default()),
                Some(match (&unique_best, hint.as_ref().and_then(|(_, c)| c.clone())) {
                    (_, Some(r)) => format!("state `\"riwayah\": \"{r}\"` in the data, or in `.terminology.json` under data.files"),
                    (Some(b), None) => format!("state `\"ayah_numbering_system\": \"{b}\"`"),
                    _ => "state the riwayah or the numbering".into(),
                }), "rule 072");
            self.add(&format!("numbering {at}"), f);
            if let Some(b) = &unique_best {
                let i = Registry::system_index(b).unwrap();
                if let Some(d) = near(i) {
                    evidence(i, &d, self.out);
                }
            }
        }

        // The basmalah inside an ayah's text moves every word position after it.
        if let Some(first) = d.texts.get(&1).and_then(|t| t.first()) {
            if basmalah(first) == Some(true) {
                let sev = if decl.segmentation.is_some() { Severity::Note } else { Severity::Warning };
                let f = self.finding(sev, Group::Meaning, "basmalah-attached", &at, line_of(self.text, first.split(' ').next().unwrap_or("")),
                    "1:1 begins with the basmalah and continues into the ayah; word positions in 1:1 are 4 higher than in data where the basmalah stands apart or is not counted".into(),
                    Some("state the word segmentation (data.files.<file>.word_segmentation), or keep the basmalah separate".into()), "rules 071, 072; dictionary: basmalah");
                self.add(&format!("basmalah-1 {at}"), f);
            }
        }
        let with_basmalah: Vec<u32> = d.texts.iter().filter(|(s, t)| **s != 1 && **s != 9 && t.first().is_some_and(|x| basmalah(x) == Some(true))).map(|(s, _)| *s).collect();
        if !with_basmalah.is_empty() {
            let f = self.finding(Severity::Warning, Group::Meaning, "basmalah-in-first-ayah", &at, None,
                format!("the first ayah's text includes the basmalah in {} surahs (e.g. {}:1); word positions there are 4 higher than in data that keeps it apart", with_basmalah.len(), with_basmalah[0]),
                Some("keep the basmalah apart from the ayah, or state the word segmentation".into()), "rules 071, 072; dictionary: basmalah");
            self.add(&format!("basmalah-n {at}"), f);
        }

        // Places of prostration marked in the text. The registry gives them in
        // Kufi numbering, so Kufi text is compared ayah by ayah and any other
        // only by the number of marks in each surah: an equal ayah count doesn't
        // mean equal boundaries (Maryam: 98 in both, yet Kufi 19:58 is 19:57 in
        // madani_first).
        let marks: BTreeSet<(u32, u32)> = d.texts.iter().flat_map(|(s, t)| t.iter().enumerate().filter(|(_, x)| x.contains(SAJDAH_MARK)).map(move |(i, _)| (*s, i as u32 + 1))).collect();
        if !marks.is_empty() {
            let is_kufi = numbering.as_deref() == Some("kufi") || (numbering.is_none() && fits[kufi] == n);
            let registry: BTreeSet<(u32, u32)> = reg.sajdah.iter().copied().filter(|(s, _)| d.texts.contains_key(s)).collect();
            let fmt = |s: &BTreeSet<(u32, u32)>| {
                let all: Vec<String> = s.iter().map(|(a, b)| format!("{a}:{b}")).collect();
                match all.len() {
                    0 => "nothing".to_string(),
                    1 => all[0].clone(),
                    n => format!("{} and {}", all[..n - 1].join(", "), all[n - 1]),
                }
            };
            let in_surah = |set: &BTreeSet<(u32, u32)>, s: u32| -> BTreeSet<(u32, u32)> { set.iter().copied().filter(|(x, _)| *x == s).collect() };
            let surahs: BTreeSet<u32> = marks.iter().chain(registry.iter()).map(|(s, _)| *s).collect();
            let mut differs = Vec::new();
            let mut summary = SajdahMarks { marked: marks.len(), ..Default::default() };
            for s in surahs {
                let (m, r) = (in_surah(&marks, s), in_surah(&registry, s));
                if is_kufi && m == r {
                    summary.at_places += m.len();
                } else if is_kufi {
                    summary.differ.push(s);
                    differs.push(format!("surah {s} marks {}, the registry {}", fmt(&m), fmt(&r)));
                } else if m.len() == r.len() {
                    summary.other_numbering.push(s);
                    summary.other_marks += m.len();
                } else {
                    summary.differ.push(s);
                    let has = match m.len() { 0 => "no mark".to_string(), 1 => "1 mark".to_string(), k => format!("{k} marks") };
                    differs.push(format!("surah {s} has {has}, the registry {} ({})", r.len(), fmt(&r)));
                }
            }
            if !differs.is_empty() {
                let f = self.finding(Severity::Warning, Group::Meaning, "sajdah-differs", &at, None,
                    format!("the sajdah marks ۩ differ from the registry in {}: {}", if differs.len() == 1 { "1 surah".to_string() } else { format!("{} surahs", differs.len()) }, differs.join("; ")),
                    Some("check these places against the mushaf the text comes from; if the text is right, report it as registry evidence".into()), "registries: sajdah");
                self.add(&format!("sajdah {at}"), f);
            }
            if let Some(i) = self.out.inventory.last_mut() {
                i.sajdah = Some(summary);
            }
        }

        if d.words == Some(false) {
            let f = self.finding(Severity::Warning, Group::Meaning, "word-order-only", &at, line_of(self.text, "\"words\""),
                "words are identified only by their order in each ayah; another project can't match them without knowing the text and segmentation".into(),
                Some("give each word `word_position` (or `word_key`) and state the text and segmentation".into()), "rules 069, 071, 072");
            self.add(&format!("words {at}"), f);
        }
        let _ = line;
    }

    /// A riwayah named in the file's path: `quran_warsh.json`, `ara-quranqaloon.json`, `hafs/…`.
    fn riwayah_from_path(&self) -> Option<(String, Option<String>)> {
        let path = self.rel.to_ascii_lowercase();
        let stem = path.rsplit('/').next().unwrap_or("").split('.').next().unwrap_or("").to_string();
        let parts: Vec<&str> = path.split('/').collect();
        let candidates: Vec<String> = std::iter::once(stem.clone())
            .chain(parts.iter().rev().skip(1).take(2).map(|s| s.to_string()))
            .flat_map(|s| s.split(['_', '-', '.', ' ']).map(String::from).collect::<Vec<_>>())
            .filter(|s| s.len() >= 4)
            .collect();
        // Exact tokens first.
        for t in &candidates {
            match self.ctx.res.resolve_close(Concept::Riwayah, t) {
                Verdict::Canonical(c) | Verdict::Format(c) | Verdict::Alternative(c) | Verdict::Declared(c) => return Some((t.clone(), Some(c))),
                Verdict::Rawi(_, r) if r.len() == 1 => return Some((t.clone(), Some(r[0].clone()))),
                Verdict::Rawi(_, _) => return Some((t.clone(), None)),
                Verdict::Unknown(Some(c)) if t.len() >= 5 => return Some((t.clone(), Some(c))),
                _ => {}
            }
        }
        // Then names inside a token: `quranwarsh`.
        for t in &candidates {
            for m in &self.ctx.reg.members {
                if m.kind != "rawi" && m.kind != "riwayah" {
                    continue;
                }
                for name in std::iter::once(m.code.clone()).chain(m.alts.iter().map(|a| norm(a))) {
                    if name.len() >= 5 && !name.contains('_') && t.contains(&name) && t != &name {
                        let code = if m.kind == "riwayah" { Some(m.code.clone()) } else {
                            let r = self.ctx.reg.riwayahs_of(|x| &x.rawi, &m.code);
                            (r.len() == 1).then(|| r[0].clone())
                        };
                        return Some((name.clone(), code));
                    }
                }
            }
        }
        None
    }

    // ------------------------------------------------------------- OpenAPI

    fn openapi(&mut self, doc: &Value) {
        let mut keys: BTreeMap<String, usize> = BTreeMap::new();
        if let Some(paths) = doc.get("paths").and_then(|p| p.as_object()) {
            for p in paths.keys() {
                for seg in p.split('/') {
                    if let Some(name) = seg.strip_prefix('{').and_then(|s| s.strip_suffix('}')) {
                        *keys.entry(name.to_string()).or_default() += 1;
                    }
                }
            }
        }
        self.schema_walk(doc, "", &mut keys);
        self.names(&keys);
    }

    fn schema_walk(&mut self, v: &Value, at: &str, keys: &mut BTreeMap<String, usize>) {
        match v {
            Value::Object(o) => {
                if let Some(props) = o.get("properties").and_then(|p| p.as_object()) {
                    for (name, schema) in props {
                        *keys.entry(name.clone()).or_default() += 1;
                        self.schema_values(name, schema, &format!("{at}/properties/{name}"));
                    }
                }
                if let (Some(name), Some(_)) = (o.get("name").and_then(|n| n.as_str()), o.get("in")) {
                    *keys.entry(name.to_string()).or_default() += 1;
                    if let Some(schema) = o.get("schema") {
                        self.schema_values(name, schema, &format!("{at}/parameters/{name}"));
                    }
                }
                for (k, child) in o {
                    self.schema_walk(child, &format!("{at}/{k}"), keys);
                }
            }
            Value::Array(a) => {
                for (i, child) in a.iter().enumerate() {
                    self.schema_walk(child, &format!("{at}/{i}"), keys);
                }
            }
            _ => {}
        }
    }

    fn schema_values(&mut self, name: &str, schema: &Value, at: &str) {
        let Some(concept) = concept_of_key(name, None) else { return };
        let mut values: Vec<String> = Vec::new();
        for k in ["enum", "examples"] {
            if let Some(a) = schema.get(k).and_then(|x| x.as_array()) {
                values.extend(a.iter().filter_map(|x| x.as_str().map(String::from)));
            }
        }
        for k in ["default", "example", "const"] {
            if let Some(s) = schema.get(k).and_then(|x| x.as_str()) {
                values.push(s.to_string());
            }
        }
        for v in values {
            self.value(concept, name, &v, at);
        }
    }

    // ---------------------------------------------------------- CSV / TSV

    fn table(&mut self, sep: char) {
        let mut rows = csv_rows(self.text, sep);
        if rows.len() < 2 {
            return;
        }
        let header = rows.remove(0);
        let mut keys = BTreeMap::new();
        for h in &header {
            if !h.is_empty() {
                keys.insert(h.clone(), rows.len());
            }
        }
        self.names(&keys);
        let mut stated = Stated::default();
        let mut mixed = false;
        for (i, h) in header.iter().enumerate() {
            if let Some(concept) = concept_of_key(h, None) {
                let distinct: BTreeSet<&str> = rows.iter().filter_map(|r| r.get(i).map(|s| s.as_str())).filter(|s| !s.is_empty()).collect();
                if distinct.len() > 40 {
                    continue; // free text, not a coded column
                }
                let single = distinct.len() == 1;
                mixed |= !single && matches!(concept, Concept::Riwayah | Concept::Qiraah | Concept::Numbering);
                for v in distinct {
                    if single {
                        self.remember(concept, v, &mut stated);
                    }
                    self.value(concept, h, v, &format!("column {h}"));
                }
            }
        }
        // Surah and ayah columns give per-surah counts.
        let find = |pred: fn(&str) -> bool| header.iter().position(|h| pred(h));
        let s_col = find(|h| { let t = tokens(h); !t.is_empty() && ["surah", "sura", "chapter"].contains(&t[0].as_str()) && t.len() <= 2 && t.get(1).is_none_or(|x| ["number", "id", "no", "num"].contains(&x.as_str())) }); // terminology: ignore
        let a_col = find(|h| { let t = tokens(h); !t.is_empty() && ["ayah", "aya", "verse"].contains(&t[0].as_str()) && t.len() <= 2 && t.get(1).is_none_or(|x| ["number", "no", "num"].contains(&x.as_str())) }); // terminology: ignore
        if let (Some(sc), Some(ac), false) = (s_col, a_col, mixed) {
            let mut d = QuranData { pointer: format!("columns {}, {}", header[sc], header[ac]), shape: "rows", ..Default::default() };
            let mut seen: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();
            for r in &rows {
                if let (Some(s), Some(a)) = (r.get(sc).and_then(|x| x.parse::<u32>().ok()), r.get(ac).and_then(|x| x.parse::<u32>().ok())) {
                    if (1..=114).contains(&s) {
                        seen.entry(s).or_default().insert(a);
                    }
                }
            }
            // Only complete surahs count: rows 1..n with none missing.
            for (s, ayahs) in seen {
                let max = *ayahs.iter().max().unwrap_or(&0);
                if max as usize == ayahs.len() && ayahs.contains(&1) {
                    d.counts.insert(s, max);
                }
            }
            if !d.counts.is_empty() {
                self.quran(d, &stated);
            }
        }
    }

    // ------------------------------------------------------------------ SQL

    fn sql(&mut self) {
        let text = self.text.to_string();
        let lower = text.to_ascii_lowercase();
        let mut keys: BTreeMap<String, usize> = BTreeMap::new();
        let mut pos = 0;
        while let Some(i) = lower[pos..].find("create table") {
            let start = pos + i;
            let Some(open) = lower[start..].find('(').map(|o| start + o) else { break };
            let name = unquote(text[start + 12..open].split_whitespace().last().unwrap_or(""));
            let name = name.rsplit('.').next().unwrap_or(&name).to_string();
            *keys.entry(name.clone()).or_default() += 1;
            let close = matching_paren(&text, open).unwrap_or(text.len());
            for def in split_top(&text[open + 1..close.min(text.len())]) {
                let first = def.split_whitespace().next().unwrap_or("");
                let upper = first.to_ascii_uppercase();
                if ["PRIMARY", "FOREIGN", "UNIQUE", "CHECK", "CONSTRAINT", "INDEX", "KEY", ""].contains(&upper.as_str()) {
                    self.sql_check(&def, &name);
                    continue;
                }
                let col = unquote(first);
                *keys.entry(col.clone()).or_default() += 1;
                if let Some(concept) = concept_of_key(&col, None) {
                    for v in quoted_values(&def) {
                        self.value(concept, &col, &v, &format!("{name}.{col}"));
                    }
                }
            }
            pos = close.min(text.len());
            if pos <= start { break; }
        }
        // CREATE TYPE riwayah AS ENUM ('hafs', 'warsh')
        let mut pos = 0;
        while let Some(i) = lower[pos..].find("create type") {
            let start = pos + i;
            let end = lower[start..].find(';').map(|e| start + e).unwrap_or(text.len());
            let stmt = &text[start..end];
            let name = unquote(stmt[11..].split_whitespace().next().unwrap_or(""));
            if lower[start..end].contains("enum") {
                if let Some(concept) = concept_of_key(&name, None) {
                    for v in quoted_values(stmt) {
                        self.value(concept, &name, &v, &format!("type {name}"));
                    }
                }
            }
            pos = end;
        }
        self.names(&keys);
    }

    fn sql_check(&mut self, def: &str, table: &str) {
        // CHECK (riwayah IN ('hafs', 'warsh'))
        let lower = def.to_ascii_lowercase();
        let Some(i) = lower.find("check") else { return };
        let after = def[i + 5..].trim_start().trim_start_matches('(').trim_start();
        let col = unquote(after.split(|c: char| !(c.is_alphanumeric() || "_\"`".contains(c))).next().unwrap_or(""));
        if let Some(concept) = concept_of_key(&col, None) {
            for v in quoted_values(&def[i..]) {
                self.value(concept, &col, &v, &format!("{table}.{col}"));
            }
        }
    }
}

fn line_of(text: &str, needle: &str) -> Option<usize> {
    crate::data::findings::line_of(text, needle)
}

fn list(items: &[String]) -> String {
    items.iter().map(|s| format!("`{s}`")).collect::<Vec<_>>().join(", ")
}

fn unquote(s: &str) -> String {
    s.trim_matches(|c| c == '"' || c == '`' || c == '[' || c == ']' || c == '\'').to_string()
}

fn matching_paren(text: &str, open: usize) -> Option<usize> {
    let mut depth = 0;
    let mut in_str = false;
    for (i, c) in text[open..].char_indices() {
        match c {
            '\'' => in_str = !in_str,
            '(' if !in_str => depth += 1,
            ')' if !in_str => {
                depth -= 1;
                if depth == 0 {
                    return Some(open + i);
                }
            }
            _ => {}
        }
    }
    None
}

fn split_top(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0;
    let mut in_str = false;
    let mut cur = String::new();
    for c in s.chars() {
        match c {
            '\'' => { in_str = !in_str; cur.push(c) }
            '(' if !in_str => { depth += 1; cur.push(c) }
            ')' if !in_str => { depth -= 1; cur.push(c) }
            ',' if !in_str && depth == 0 => out.push(std::mem::take(&mut cur).trim().to_string()),
            _ => cur.push(c),
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur.trim().to_string());
    }
    out
}

fn quoted_values(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut parts = s.split('\'');
    parts.next();
    while let (Some(v), _) = (parts.next(), parts.next()) {
        out.push(v.to_string());
    }
    out
}

fn csv_rows(text: &str, sep: char) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if quoted {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    cur.push('"');
                    chars.next();
                } else {
                    quoted = false;
                }
            } else {
                cur.push(c);
            }
        } else if c == '"' && cur.is_empty() {
            quoted = true;
        } else if c == sep {
            row.push(std::mem::take(&mut cur));
        } else if c == '\n' {
            row.push(std::mem::take(&mut cur).trim_end_matches('\r').to_string());
            if !(row.len() == 1 && row[0].is_empty()) && !row[0].starts_with('#') {
                rows.push(std::mem::take(&mut row));
            } else {
                row.clear();
            }
        } else {
            cur.push(c);
        }
    }
    if !cur.is_empty() || !row.is_empty() {
        row.push(cur);
        rows.push(row);
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_quoted_csv() {
        let rows = csv_rows("surah,ayah,text\n1,1,\"a, b\"\n", ',');
        assert_eq!(rows[1][2], "a, b");
    }

    #[test]
    fn reads_sql_values() {
        assert_eq!(quoted_values("CHECK (riwayah IN ('hafs', 'warsh'))"), vec!["hafs", "warsh"]);
    }
}
