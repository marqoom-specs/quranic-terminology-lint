//! Which concept a data field carries, and whether its values resolve to a
//! registry code.

use crate::data::registry::{Registry, SYSTEMS};
use crate::data::text::{ar_norm, levenshtein, norm, tokens};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Concept {
    Riwayah,
    Qiraah,
    Rawi,
    Numbering,
    Revelation,
}

impl Concept {
    pub fn code(self) -> &'static str {
        match self {
            Concept::Riwayah => "riwayah",
            Concept::Qiraah => "qiraah",
            Concept::Rawi => "rawi",
            Concept::Numbering => "ayah_numbering_system",
            Concept::Revelation => "revelation_type",
        }
    }
}

/// The concept a key names. The concept must be the key's last word, optionally
/// followed by `code`, `key` or a one-letter suffix: `riwayah`, `default_riwayah`,
/// `riwayah_code`, `riwayah_a`, `counting_system`. `parent` is the enclosing
/// key, so that `counting.system` reads as a numbering system.
pub fn concept_of_key(key: &str, parent: Option<&str>) -> Option<Concept> {
    // File names, paths and documentation fields are not data fields.
    if key.contains('.') || key.contains('/') || key.starts_with('_') || parent.is_some_and(|p| p.starts_with('_')) {
        return None;
    }
    let t = tokens(key);
    let mut end = t.len();
    if end == 0 {
        return None;
    }
    let qualifier = |w: &str| ["code", "key", "slug", "value"].contains(&w) || w.len() == 1 || w.chars().all(|c| c.is_ascii_digit());
    if end >= 2 && qualifier(&t[end - 1]) {
        end -= 1;
    }
    let last = t[end - 1].as_str();
    let before = if end >= 2 { t[end - 2].as_str() } else { "" };
    if last == "system" && (before == "numbering" || before == "counting") {
        return Some(Concept::Numbering);
    }
    if ["type", "place", "class"].contains(&last) && before == "revelation" {
        return Some(Concept::Revelation);
    }
    match last {
        "riwayah" | "riwaya" | "rewaya" | "riwayat" | "riwayahs" | "riwayas" => Some(Concept::Riwayah),
        "qiraah" | "qiraa" | "qiraat" | "qirat" | "qiraahs" => Some(Concept::Qiraah),
        "rawi" | "rawy" | "rawis" => Some(Concept::Rawi),
        "numbering" | "counting" => Some(Concept::Numbering),
        "revelation" => Some(Concept::Revelation),
        "system" if t.len() == 1 && parent.is_some_and(|p| {
            let p = tokens(p);
            p.iter().any(|x| x == "counting" || x == "numbering")
        }) => Some(Concept::Numbering),
        _ => None,
    }
}

/// Whether a string could be a code at all: not a number, hash, sentence or URL.
pub fn codeish(s: &str) -> bool {
    let s = s.trim();
    if s.is_empty() || s.len() > 64 || s.contains("://") {
        return false;
    }
    if s.chars().all(|c| c.is_ascii_digit() || c == '.' || c == '-') {
        return false;
    }
    if s.len() >= 16 && s.chars().all(|c| c.is_ascii_hexdigit()) {
        return false;
    }
    s.split_whitespace().count() <= 5
}

#[derive(Debug, Clone, PartialEq)]
pub enum Verdict {
    /// The registry code itself.
    Canonical(String),
    /// The code with other letter case or separators (`madani-last`).
    Format(String),
    /// Mapped to a code in the project's declaration.
    Declared(String),
    /// A spelling the registry records for one code.
    Alternative(String),
    /// The Arabic name of one member.
    Arabic(String),
    /// A rawi's name where a riwayah is meant; lists the riwayahs it could be.
    Rawi(String, Vec<String>),
    /// A qiraah where a riwayah is meant.
    Qiraah(String, Vec<String>),
    /// No member; with the closest code, if one is near.
    Unknown(Option<String>),
}

pub struct Resolver<'r> {
    reg: &'r Registry,
    arabic: HashMap<String, (String, String)>,
    pub mappings: HashMap<String, String>,
}

impl<'r> Resolver<'r> {
    pub fn new(reg: &'r Registry, mappings: HashMap<String, String>) -> Self {
        let mappings = mappings.into_iter().map(|(k, v)| (norm(&k), v)).collect();
        Resolver { reg, arabic: reg.arabic_index(), mappings }
    }

    fn member_code(&self, kind: &str, value: &str) -> Option<String> {
        self.reg
            .members
            .iter()
            .find(|m| m.kind == kind && (m.code == value || m.alts.iter().any(|a| norm(a) == value)))
            .map(|m| m.code.clone())
    }

    pub fn resolve(&self, concept: Concept, raw: &str) -> Verdict {
        self.resolve_with(concept, raw, true)
    }

    /// Like `resolve`, but a guess must be one edit from a recorded spelling:
    /// for names in file paths, where most words are not riwayahs at all.
    pub fn resolve_close(&self, concept: Concept, raw: &str) -> Verdict {
        match self.resolve(concept, raw) {
            Verdict::Unknown(Some(c)) => {
                let value = norm(raw);
                let close = self.reg.members.iter().filter(|m| m.kind == "riwayah" || m.kind == "rawi").any(|m| {
                    std::iter::once(m.code.clone()).chain(m.alts.iter().map(|a| norm(a))).any(|cand| levenshtein(&value, &cand) <= 1)
                });
                Verdict::Unknown(close.then_some(c))
            }
            v => v,
        }
    }

    /// `mappings` is false for published data: a consumer never sees the
    /// producer's declaration, so its mappings can't resolve published values.
    pub fn resolve_with(&self, concept: Concept, raw: &str, mappings: bool) -> Verdict {
        let value = norm(raw);
        if value.is_empty() {
            return Verdict::Unknown(None);
        }
        if let Some(code) = self.mappings.get(&value).filter(|_| mappings) {
            return Verdict::Declared(code.clone());
        }
        let exact = |code: &str| {
            if raw == code {
                Verdict::Canonical(code.to_string())
            } else {
                Verdict::Format(code.to_string())
            }
        };
        match concept {
            Concept::Numbering => {
                if let Some(code) = SYSTEMS.iter().find(|s| **s == value) {
                    return exact(code);
                }
                if let Some(code) = self.reg.numbering_alts.get(&value) {
                    return Verdict::Alternative(code.clone());
                }
                let near = SYSTEMS
                    .iter()
                    .map(|s| (levenshtein(&value, s), *s))
                    .min()
                    .filter(|(d, s)| *d <= 2 && *d * 3 <= s.len());
                Verdict::Unknown(near.map(|(_, s)| s.to_string()))
            }
            Concept::Revelation => match value.as_str() {
                "makki" | "madani" => exact(&value),
                "meccan" | "makkan" | "makkah" | "mecca" | "makka" | "makkiyah" | "makkiyya" => {
                    Verdict::Alternative("makki".into())
                }
                "medinan" | "madinan" | "madinah" | "medina" | "madina" | "madaniyah" | "madaniyya" => {
                    Verdict::Alternative("madani".into())
                }
                _ => match ar_norm(raw).as_str() {
                    "مكية" | "مكي" => Verdict::Arabic("makki".into()),
                    "مدنية" | "مدني" => Verdict::Arabic("madani".into()),
                    _ => Verdict::Unknown(None),
                },
            },
            Concept::Riwayah | Concept::Qiraah | Concept::Rawi => self.resolve_person(concept, raw, &value),
        }
    }

    fn resolve_person(&self, concept: Concept, raw: &str, value: &str) -> Verdict {
        let kind = match concept {
            Concept::Riwayah => "riwayah",
            Concept::Qiraah => "qiraah",
            _ => "rawi",
        };
        if let Some(m) = self.reg.members.iter().find(|m| m.kind == kind && m.code == value) {
            return if raw == m.code { Verdict::Canonical(m.code.clone()) } else { Verdict::Format(m.code.clone()) };
        }
        if let Some(code) = self.member_code(kind, value) {
            return Verdict::Alternative(code);
        }
        let ar = ar_norm(raw.trim_start_matches("رواية").trim_start_matches("رِوَايَة").trim());
        if !ar.is_empty() {
            if let Some((k, code)) = self.arabic.get(&ar) {
                if k == kind {
                    return Verdict::Arabic(code.clone());
                }
            }
        }
        if concept == Concept::Riwayah {
            if let Some(rawi) = self.member_code("rawi", value) {
                return Verdict::Rawi(rawi.clone(), self.reg.riwayahs_of(|m| &m.rawi, &rawi));
            }
            if let Some(q) = self.member_code("qiraah", value) {
                return Verdict::Qiraah(q.clone(), self.reg.riwayahs_of(|m| &m.parent, &q));
            }
        }
        // The nearest code or recorded spelling, of the riwayah or of a rawi.
        let mut best: Option<(usize, String)> = None;
        for m in &self.reg.members {
            if m.kind != kind && !(concept == Concept::Riwayah && m.kind == "rawi") {
                continue;
            }
            for cand in std::iter::once(m.code.clone()).chain(m.alts.iter().map(|a| norm(a))) {
                let d = levenshtein(value, &cand);
                if d <= 2 && d * 3 <= cand.len().max(3) && best.as_ref().is_none_or(|(bd, _)| d < *bd) {
                    let target = if m.kind == "rawi" && concept == Concept::Riwayah {
                        let r = self.reg.riwayahs_of(|x| &x.rawi, &m.code);
                        if r.len() == 1 { r[0].clone() } else { m.code.clone() }
                    } else {
                        m.code.clone()
                    };
                    best = Some((d, target));
                }
            }
        }
        Verdict::Unknown(best.map(|(_, c)| c))
    }

    /// The canonical form of a key, if one of its parts is a recorded
    /// non-canonical name (`verse_key` → `ayah_key`, `chapter_id` → `surah_id`).
    pub fn canonical_key(&self, key: &str) -> Option<String> {
        // Keys with dots, slashes, dashes or spaces are file names and labels, not field names.
        if key.contains(['.', '/', '-', ' ']) {
            return None;
        }
        let t = tokens(key);
        if t.is_empty() {
            return None;
        }
        let mut out: Vec<String> = Vec::new();
        let mut i = 0;
        let mut changed = false;
        'outer: while i < t.len() {
            for n in (1..=3.min(t.len() - i)).rev() {
                let gram = t[i..i + n].join("_");
                let singular = gram.strip_suffix('s').map(String::from);
                let first_singular = {
                    let mut g = t[i..i + n].to_vec();
                    if let Some(s) = g[0].strip_suffix('s') {
                        g[0] = s.to_string();
                    }
                    g.join("_")
                };
                // The gram as it is; without a final `s` (the plural is kept);
                // with its first word singular (`verses_count` → `ayah_count`).
                for (cand, plural) in [(Some(gram.clone()), false), (singular, true), (Some(first_singular), false)] {
                    let Some(cand) = cand else { continue };
                    if let Some(code) = self.reg.key_names.get(&cand) {
                        let mut code = code.clone();
                        if plural && !code.ends_with('s') {
                            code.push('s');
                        }
                        // `rasm_uthmani`: the replacement already carries the token before it.
                        if let Some(prev) = out.last() {
                            if code.starts_with(&format!("{prev}_")) {
                                out.pop();
                            }
                        }
                        out.push(code);
                        changed = true;
                        i += n;
                        continue 'outer;
                    }
                }
            }
            out.push(t[i].clone());
            i += 1;
        }
        let joined = out.join("_");
        (changed && joined != t.join("_")).then_some(joined)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::registry::Registry;

    fn resolver(reg: &Registry) -> Resolver<'_> {
        Resolver::new(reg, HashMap::new())
    }

    #[test]
    fn resolves_riwayah_values() {
        let reg = Registry::load();
        let r = resolver(&reg);
        assert_eq!(r.resolve(Concept::Riwayah, "hafs_an_asim"), Verdict::Canonical("hafs_an_asim".into()));
        assert_eq!(r.resolve(Concept::Riwayah, "Hafs-an-Asim"), Verdict::Format("hafs_an_asim".into()));
        assert_eq!(r.resolve(Concept::Riwayah, "warsh"), Verdict::Alternative("warsh_an_nafi".into()));
        assert_eq!(r.resolve(Concept::Riwayah, "hafsh"), Verdict::Unknown(Some("hafs_an_asim".into())));
        // A file name is a weaker signal: only one edit away counts.
        assert_eq!(r.resolve_close(Concept::Riwayah, "hafsh"), Verdict::Unknown(Some("hafs_an_asim".into())));
        assert_eq!(r.resolve_close(Concept::Riwayah, "surah"), Verdict::Unknown(None));
        assert_eq!(r.resolve(Concept::Riwayah, "bazzi"), Verdict::Rawi("bazzi".into(), vec!["bazzi_an_ibn_kathir".into()]));
        assert!(matches!(r.resolve(Concept::Riwayah, "duri"), Verdict::Rawi(_, v) if v.len() == 2));
        assert!(matches!(r.resolve(Concept::Riwayah, "asim"), Verdict::Qiraah(_, v) if v.len() == 2));
        assert_eq!(r.resolve(Concept::Riwayah, "حفص عن عاصم"), Verdict::Arabic("hafs_an_asim".into()));
    }

    #[test]
    fn resolves_numbering_and_revelation() {
        let reg = Registry::load();
        let r = resolver(&reg);
        assert_eq!(r.resolve(Concept::Numbering, "madani-last"), Verdict::Format("madani_last".into()));
        assert_eq!(r.resolve(Concept::Numbering, "kufan"), Verdict::Alternative("kufi".into()));
        assert_eq!(r.resolve(Concept::Revelation, "makkah"), Verdict::Alternative("makki".into()));
    }

    #[test]
    fn declared_mappings_do_not_reach_published_data() {
        let reg = Registry::load();
        let r = Resolver::new(&reg, HashMap::from([("hafsh".to_string(), "hafs_an_asim".to_string())]));
        assert_eq!(r.resolve(Concept::Riwayah, "hafsh"), Verdict::Declared("hafs_an_asim".into()));
        assert!(matches!(r.resolve_with(Concept::Riwayah, "hafsh", false), Verdict::Unknown(_)));
    }

    #[test]
    fn reads_concepts_from_keys() {
        assert_eq!(concept_of_key("riwaya", None), Some(Concept::Riwayah));
        assert_eq!(concept_of_key("riwayah_a", None), Some(Concept::Riwayah));
        assert_eq!(concept_of_key("system", Some("counting")), Some(Concept::Numbering));
        assert_eq!(concept_of_key("revelation_place", None), Some(Concept::Revelation));
        assert_eq!(concept_of_key("primary_riwaya_decision", None), None);
        assert_eq!(concept_of_key("numbering_effect", None), None);
        assert_eq!(concept_of_key("qiraat.json", None), None);
        assert_eq!(concept_of_key("riwayah_name", None), None);
    }

    #[test]
    fn names_keys_canonically() {
        let reg = Registry::load();
        let r = resolver(&reg);
        assert_eq!(r.canonical_key("verses_count").as_deref(), Some("ayah_count"));
        assert_eq!(r.canonical_key("chapter_id").as_deref(), Some("surah_id"));
        assert_eq!(r.canonical_key("verse_key").as_deref(), Some("ayah_key"));
        assert_eq!(r.canonical_key("riwaya").as_deref(), Some("riwayah"));
        assert_eq!(r.canonical_key("rasm_uthmani"), None);
        assert_eq!(r.canonical_key("surah_id"), None);
        assert_eq!(r.canonical_key("verseTimings").as_deref(), Some("ayah_timings"));
        assert_eq!(r.canonical_key("verses").as_deref(), Some("ayahs"));
    }
}
