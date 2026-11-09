//! The index of names: every canonical code name, and every way the rules say
//! it is commonly miswritten.
//!
//! The term files hold each concept's canonical code name and the irregular
//! names no rule can predict (`chapter` for `surah`). Everything a rule can
//! predict is generated here by running the spelling rules backwards: `tajwid`
//! yields `tajweed` (rule 021), `surah` yields `sura` (rule 019), `fatihah`
//! yields `al_fatihah` (rule 031). A finding therefore names the rule that was
//! broken, not just a list entry that matched.

use super::rules::{Rule, Rules};
use super::words::{plain, split};
use std::collections::{HashMap, HashSet};

const CONCEPTS: &str = include_str!("../../terms/concepts.tsv");
const MEMBERS: &str = include_str!("../../terms/members.tsv");

/// Ordinary English words that some entry records as a name (`part` for juz,
/// `pause` for waqf). Alone they are far more often something else, so they
/// are never reported; in a compound (`juz_part`) the compound is checked.
const ORDINARY_WORDS: &[&str] = &[
    "segment", "reading", "stop", "sign", "mark", "dot", "place", "count", "number", "line", "page",
    "word", "letter", "root", "text", "character", "glyph", "translation", "position", "index", "order",
    "part", "unit", "quarter", "half", "eighth", "chain", "path", "way", "style", "pace", "school", "pos",
    "simple", "timing", "section", "group", "station", "portion", "pause", "conversion", "commentary",
    "assimilation", "concealment", "prolongation", "orthography", "vocalization", "transmitter", "para",
    "mad", "lin", "wid", "rub",
];
/// Generated forms that are common English words.
const ENGLISH: &[&str] = &[
    "feel", "room", "hood", "teen", "jam", "rub", "cat", "rat", "mat", "sat", "hat", "fat", "pat", "bat",
    "vat", "ruk", "nun", "din", "fees", "seen", "keen",
];
const SUN_LETTERS: [&str; 10] = ["th", "dh", "sh", "t", "d", "r", "z", "s", "n", "l"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// A transliterated Arabic term (rule 002).
    Quranic,
    Borrowed,
    Standard,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    Concept,
    /// A member of a closed set: `surah`, `rawi`, `riwayah`, `tariq` …
    Member(String),
}

#[derive(Debug)]
pub struct Term {
    pub code: String,
    pub display: String,
    pub plural: String,
    pub arabic: String,
    pub kind: Kind,
    pub origin: Origin,
}

impl Term {
    fn member_of(&self, sets: &[&str]) -> bool {
        matches!(&self.kind, Kind::Member(k) if sets.contains(&k.as_str()))
    }

    /// Surah and tariq names are often ordinary words (`elephant`, `cow`).
    fn ordinary_names(&self) -> bool {
        self.member_of(&["surah", "tariq"])
    }

    pub fn is_recording(&self) -> bool {
        self.member_of(&["rawi", "riwayah", "qiraah"]) || ["murattal", "mujawwad", "muallim"].contains(&self.code.as_str())
    }

    pub fn is_quranic_concept(&self) -> bool {
        self.kind == Kind::Concept && self.origin == Origin::Quranic
    }

    /// The rule a member's other spellings break.
    fn member_rule(&self) -> Rule {
        match &self.kind {
            Kind::Member(k) if k == "surah" => Rule::SurahNames,
            Kind::Member(k) if matches!(k.as_str(), "rawi" | "qiraah" | "riwayah") => Rule::PersonalNames,
            Kind::Member(k) if k == "ayah_numbering_system" => Rule::NumberingNames,
            _ => Rule::OneName,
        }
    }
}

/// What a name in the index stands for, and the rules it breaks.
#[derive(Debug, Clone, Copy)]
pub struct Hit {
    pub term: usize,
    /// Empty for the canonical name itself.
    pub rules: Rules,
    pub warning: bool,
    pub plural: bool,
    /// An English gloss of the concept (`verse` for ayah).
    pub gloss: bool,
}

impl Hit {
    fn new(term: usize, rules: Rules) -> Hit {
        Hit { term, rules, warning: false, plural: false, gloss: false }
    }
}

pub struct Terms {
    pub terms: Vec<Term>,
    index: HashMap<String, Hit>,
    /// Words the project asked to leave alone, as word sequences.
    pub ignored: Vec<Vec<String>>,
    /// The most words in any indexed name.
    pub longest: usize,
    pub concepts: usize,
}

/// `Al-Fatihah` → `al_fatihah`
pub fn key(form: &str) -> String {
    form.trim().to_lowercase().replace(['-', ' '], "_")
}

fn items(cell: &str) -> impl Iterator<Item = String> + '_ {
    cell.split(',').filter(|v| !v.trim().is_empty()).map(key)
}

/// The rows of a tab-separated table, by column name.
pub fn read_table(text: &str, name: &str) -> Result<Vec<HashMap<String, String>>, String> {
    let mut header: Option<Vec<&str>> = None;
    let mut rows = Vec::new();
    for (number, line) in text.lines().enumerate() {
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let cells: Vec<&str> = line.split('\t').collect();
        match &header {
            None => header = Some(cells),
            Some(h) if h.len() != cells.len() => {
                return Err(format!("{name}:{}: {} columns, expected {}", number + 1, cells.len(), h.len()));
            }
            Some(h) => rows.push(h.iter().zip(cells).map(|(k, v)| (k.to_string(), v.to_string())).collect()),
        }
    }
    Ok(rows)
}

fn cell<'a>(row: &'a HashMap<String, String>, column: &str) -> &'a str {
    row.get(column).map_or("", String::as_str)
}

// --- the spelling rules, run backwards ------------------------------------------

/// Ways the rules say a word of a canonical name is commonly miswritten.
fn word_variants(word: &str, following: &str) -> Vec<(String, Rule)> {
    let chars: Vec<char> = word.chars().collect();
    let mut out = Vec::new();
    // Rule 021: tajwid → tajweed. Every combination of the first three i/u.
    let vowels: Vec<usize> = chars.iter().enumerate().filter(|(_, c)| matches!(c, 'i' | 'u')).map(|(i, _)| i).take(3).collect();
    for chosen in 1..1u32 << vowels.len() {
        let mut spelled = String::new();
        for (i, c) in chars.iter().enumerate() {
            match vowels.iter().position(|v| *v == i) {
                Some(bit) if chosen >> bit & 1 == 1 => spelled.push_str(if *c == 'i' { "ee" } else { "oo" }),
                _ => spelled.push(*c),
            }
        }
        out.push((spelled, Rule::LongVowels));
    }
    let stem = |n: usize| -> String { chars[..chars.len() - n].iter().collect() };
    if word.ends_with('i') && chars.len() > 3 {
        // Rule 022: makki → makkiyy
        out.push((format!("{word}yy"), Rule::Nisbah));
        out.push((format!("{word}y"), Rule::Nisbah));
        out.push((format!("{}ee", stem(1)), Rule::Nisbah));
    }
    if word.ends_with("ah") {
        // Rule 019: surah → sura, surat
        out.push((stem(1), Rule::TaMarbutah));
        out.push((format!("{}at", stem(2)), Rule::TaMarbutah));
    }
    if word.ends_with("at") && following == "al" {
        // Rule 020: hamzat_al → hamzah_al
        out.push((format!("{}ah", stem(2)), Rule::TaMarbutahInIdafah));
        out.push((stem(1), Rule::TaMarbutahInIdafah));
    }
    // Rule 026: rubu → rub. A final short vowel repeating the one before the
    // last consonants is dropped.
    if chars.len() >= 4 {
        let last = chars[chars.len() - 1];
        if matches!(last, 'a' | 'i' | 'u') {
            let repeats = (1..=2).any(|k| {
                chars.len() >= k + 2
                    && chars[chars.len() - 1 - k..chars.len() - 1].iter().all(|c| !matches!(c, 'a' | 'i' | 'u'))
                    && chars[chars.len() - 2 - k] == last
            });
            if repeats {
                out.push((stem(1), Rule::FinalHamzah));
            }
        }
    }
    out
}

/// Ways the article rules say a whole canonical name is miswritten.
fn article_variants(code: &str) -> Vec<(String, Rule)> {
    let parts: Vec<&str> = code.split('_').collect();
    let mut out = vec![
        (format!("al_{code}"), Rule::LeadingArticle),
        (format!("al{code}"), Rule::LeadingArticle),
        (format!("el_{code}"), Rule::LeadingArticle),
    ];
    for sun in SUN_LETTERS.iter().filter(|s| code.starts_with(*s)) {
        out.push((format!("a{sun}_{code}"), Rule::LeadingArticle));
    }
    for i in 1..parts.len().saturating_sub(1) {
        if parts[i] != "al" {
            continue;
        }
        let (head, rest) = (parts[..i].join("_"), parts[i + 1..].join("_"));
        for v in ["ul", "el", "il"] {
            out.push((format!("{head}_{v}_{rest}"), Rule::SunLetters));
        }
        for sun in SUN_LETTERS.iter().filter(|s| rest.starts_with(*s) && **s != "l") {
            out.push((format!("{head}_a{sun}_{rest}"), Rule::SunLetters));
        }
        out.push((format!("{head}_{rest}"), Rule::ArticleInIdafah)); // rubu_al_hizb → rubu_hizb
    }
    for i in 1..parts.len() {
        if parts[i - 1] != "al" && parts[i] != "al" {
            // rasm_uthmani → rasm_al_uthmani
            let mut with: Vec<&str> = parts[..i].to_vec();
            with.push("al");
            with.extend(&parts[i..]);
            out.push((with.join("_"), Rule::ArticleOnAdjective));
        }
    }
    out
}

/// Every miswriting the rules predict for a term, with the rules it breaks,
/// first prediction first.
///
/// A general concept keeps its English words (rule 003), but a Quranic word
/// inside it is still spelled by the rules: `aya_key` for `ayah_key`.
fn derived_forms(term: &Term, arabic_words: &HashSet<&str>) -> Vec<(String, Rules)> {
    let parts: Vec<&str> = term.code.split('_').collect();
    let general = term.origin != Origin::Quranic;
    let options: Vec<Vec<(String, Rules)>> = parts
        .iter()
        .enumerate()
        .map(|(i, part)| {
            let following = parts.get(i + 1).copied().unwrap_or("");
            let variants = if general && !arabic_words.contains(part) { vec![] } else { word_variants(part, following) };
            std::iter::once((part.to_string(), Rules::NONE)).chain(variants.into_iter().map(|(v, r)| (v, r.into()))).collect()
        })
        .collect();

    let mut forms: Vec<(String, Rules)> = Vec::new();
    let mut seen = HashSet::new();
    let mut add = |combo: &[&(String, Rules)]| {
        let rules = combo.iter().fold(Rules::NONE, |acc, (_, r)| acc.union(*r));
        if rules.is_empty() {
            return;
        }
        let form = combo.iter().map(|(p, _)| p.as_str()).collect::<Vec<_>>().join("_");
        if seen.insert(form.clone()) {
            forms.push((form, rules));
        }
    };
    let total: usize = options.iter().map(Vec::len).product();
    if total <= 64 {
        // Every combination, the last word changing fastest.
        let mut at = vec![0; options.len()];
        loop {
            add(&at.iter().zip(&options).map(|(i, o)| &o[*i]).collect::<Vec<_>>());
            let Some(k) = (0..options.len()).rev().find(|&k| at[k] + 1 < options[k].len()) else { break };
            at[k] += 1;
            at[k + 1..].iter_mut().for_each(|x| *x = 0);
        }
    } else {
        // A long name: one change at a time.
        for (i, choices) in options.iter().enumerate() {
            for variant in &choices[1..] {
                add(&options.iter().enumerate().map(|(j, o)| if j == i { variant } else { &o[0] }).collect::<Vec<_>>());
            }
        }
    }
    if !general {
        for (form, rule) in article_variants(&term.code) {
            if seen.insert(form.clone()) {
                forms.push((form, rule.into()));
            }
        }
    }
    forms
}

// --- the index ---------------------------------------------------------------

impl Terms {
    /// The index from the term files compiled into the binary.
    pub fn load(ignore_words: &[String]) -> Result<Terms, String> {
        Terms::from_tables(CONCEPTS, MEMBERS, ignore_words)
    }

    pub fn from_tables(concepts_tsv: &str, members_tsv: &str, ignore_words: &[String]) -> Result<Terms, String> {
        let concept_rows = read_table(concepts_tsv, "concepts.tsv")?;
        let member_rows = read_table(members_tsv, "members.tsv")?;
        if concept_rows.is_empty() {
            return Err("no concepts in the term files".into());
        }
        let mut terms = Vec::new();
        for row in &concept_rows {
            let code = cell(row, "code").to_string();
            let origin = match cell(row, "origin") {
                "quranic" => Origin::Quranic,
                "borrowed" => Origin::Borrowed,
                "standard" => Origin::Standard,
                other => return Err(format!("concepts.tsv: {code} has origin `{other}`")),
            };
            let plural = match cell(row, "plural") {
                "" => format!("{code}s"),
                p => p.to_string(),
            };
            terms.push(Term { plural, display: cell(row, "display").into(), arabic: cell(row, "arabic").into(), kind: Kind::Concept, origin, code });
        }
        for row in &member_rows {
            let code = cell(row, "code").to_string();
            terms.push(Term {
                plural: format!("{code}s"),
                display: cell(row, "display").into(),
                arabic: cell(row, "arabic").into(),
                kind: Kind::Member(cell(row, "kind").into()),
                origin: Origin::Quranic,
                code,
            });
        }
        let concepts = concept_rows.len();
        let mut t = Terms { terms, index: HashMap::new(), ignored: Vec::new(), longest: 0, concepts };

        // A bare name refers to the concept (rule 058); the first entry for a name wins.
        for (i, term) in t.terms.iter().enumerate() {
            t.index.entry(term.code.clone()).or_insert(Hit::new(i, Rules::NONE));
            t.index.entry(term.plural.clone()).or_insert(Hit { plural: true, ..Hit::new(i, Rules::NONE) });
        }
        let mut puts: Vec<(String, Hit)> = Vec::new();
        for (i, row) in concept_rows.iter().enumerate() {
            puts.extend(items(cell(row, "former_names")).map(|f| (f, Hit::new(i, Rule::Deprecated.into()))));
            puts.extend(items(cell(row, "arabic_plurals")).map(|f| (f, Hit { plural: true, ..Hit::new(i, Rule::Plural.into()) })));
        }
        let arabic_words: HashSet<&str> =
            t.terms.iter().filter(|t| t.origin == Origin::Quranic && !t.code.contains('_')).map(|t| t.code.as_str()).collect();
        for (i, term) in t.terms.iter().enumerate() {
            let single = !term.code.contains('_');
            for (form, rules) in derived_forms(term, &arabic_words) {
                // A short surah name respelled is usually an English word (`sab`, `feel`).
                let respelled = rules.contains(Rule::LongVowels) || rules.contains(Rule::FinalHamzah);
                if ENGLISH.contains(&form.as_str()) || single && term.ordinary_names() && respelled {
                    continue;
                }
                puts.push((form, Hit { warning: single && rules.is(Rule::FinalHamzah), ..Hit::new(i, rules) }));
            }
        }
        for (i, row) in concept_rows.iter().enumerate() {
            for form in items(cell(row, "other_spellings")) {
                let rule = if form.ends_with("_type") { Rule::TypeSuffix } else { Rule::OneName };
                puts.push((form, Hit::new(i, rule.into())));
            }
            puts.push((key(cell(row, "display")), Hit::new(i, Rule::Stability.into())));
            puts.extend(items(cell(row, "english")).map(|f| (f, Hit { gloss: true, ..Hit::new(i, Rule::OneName.into()) })));
        }
        for (j, row) in member_rows.iter().enumerate() {
            let i = concepts + j;
            let term = &t.terms[i];
            for form in items(cell(row, "other_spellings")) {
                if form.contains('_') || !term.ordinary_names() {
                    // not `sad`, `cow`
                    puts.push((form, Hit::new(i, term.member_rule().into())));
                }
            }
        }
        for (form, hit) in puts {
            t.put(form, hit);
        }
        // Rule 049: the plural adds `s`.
        let plurals: Vec<(String, Hit)> =
            t.index.iter().filter(|(_, h)| !h.rules.is_empty()).map(|(f, h)| (format!("{f}s"), Hit { plural: true, ..*h })).collect();
        for (form, hit) in plurals {
            t.put(form, hit);
        }

        for word in ignore_words.iter().filter(|w| !w.trim().is_empty()) {
            for form in [word.clone(), format!("{word}s")] {
                t.ignored.push(split(&form).iter().map(|p| plain(p)).collect());
            }
        }
        t.longest = t.index.keys().map(|f| f.split('_').count()).max().unwrap_or(1);
        Ok(t)
    }

    fn put(&mut self, form: String, hit: Hit) {
        let ordinary = |f: &str| ORDINARY_WORDS.contains(&f);
        if form.chars().count() > 2 && !ordinary(&form) && !ordinary(form.trim_end_matches('s')) {
            self.index.entry(form).or_insert(hit);
        }
    }

    pub fn get(&self, name: &str) -> Option<&Hit> {
        self.index.get(name)
    }

    pub fn term(&self, hit: &Hit) -> &Term {
        &self.terms[hit.term]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rules_predict_spellings_the_term_files_do_not_list() {
        let rows = read_table(CONCEPTS, "concepts.tsv").unwrap();
        let listed: HashSet<String> = rows.iter().flat_map(|r| items(cell(r, "other_spellings")).collect::<Vec<_>>()).collect();
        assert!(["tajweed", "sura", "aya", "al_fatihah"].iter().all(|f| !listed.contains(*f)));
    }

    #[test]
    fn the_term_files_are_consistent() {
        let rows = read_table(CONCEPTS, "concepts.tsv").unwrap();
        let codes: HashSet<&str> = rows.iter().map(|r| cell(r, "code")).collect();
        assert_eq!(codes.len(), rows.len());
        for code in codes {
            assert!(code == key(code) && code.is_ascii(), "{code}");
        }
        assert!(Terms::load(&[]).is_ok());
    }

    #[test]
    fn a_table_error_names_the_line() {
        let err = read_table("code\tdisplay\norphan\n", "concepts.tsv").unwrap_err();
        assert!(err.starts_with("concepts.tsv:2"), "{err}");
    }

    #[test]
    fn spelling_rules_run_backwards() {
        let has = |word: &str, form: &str, rule: Rule| word_variants(word, "").contains(&(form.to_string(), rule));
        assert!(has("tajwid", "tajweed", Rule::LongVowels));
        assert!(has("surah", "sura", Rule::TaMarbutah));
        assert!(has("makki", "makkiyy", Rule::Nisbah));
        assert!(has("rubu", "rub", Rule::FinalHamzah));
        assert!(word_variants("hamzat", "al").contains(&("hamzah".to_string(), Rule::TaMarbutahInIdafah)));
    }
}
