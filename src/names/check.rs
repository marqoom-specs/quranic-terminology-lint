//! Checking one identifier against the index, and the name to use instead.

use super::rules::{Rule, Rules};
use super::terms::{key, Hit, Terms};
use super::words::{is_upper, part_spans, plain, render, split};
use std::collections::BTreeSet;
use std::fmt;
use std::ops::Range;

const NUMBER_ABBREVIATIONS: [&str; 4] = ["no", "num", "nr", "nbr"];
const SEQUENCE_WORDS: [&str; 4] = ["idx", "index", "seq", "ord"];
const ABBREVIATIONS: [(&str, &str); 2] = [("srh", "surah"), ("wrd", "word")];
const TIMINGS: [&str; 4] = ["ayah_timing", "word_timing", "letter_timing", "phoneme_timing"];

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    Error,
    /// Reported, but does not block.
    Warning,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    pub rules: Rules,
    pub severity: Severity,
    /// The words as written, in snake_case: `aya_no`.
    pub found: String,
    /// The name to use: `ayah_number`.
    pub preferred: String,
    /// An English gloss (`verse`), which a project may allow.
    pub gloss: bool,
    pub arabic: String,
}

impl Finding {
    fn new(rules: impl Into<Rules>, severity: Severity, found: String, preferred: String) -> Finding {
        Finding { rules: rules.into(), severity, found, preferred, gloss: false, arabic: String::new() }
    }

    /// An error whose preferred name is a code name can be applied as a fix.
    pub fn fixable(&self) -> bool {
        self.severity == Severity::Error && self.preferred.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
    }
}

/// The findings for one identifier. In `prose` (a comment, a document, a
/// label) display forms are fine: "al-Fatihah", "Qur'an", "verse".
pub fn check(identifier: &str, terms: &Terms, prose: bool) -> Vec<Finding> {
    if prose && is_upper(identifier) && identifier.chars().count() > 1 {
        return vec![]; // A Unicode character name, `ARABIC FATHA`, is kept as given (rule 043).
    }
    let raw = split(identifier);
    let words: Vec<String> = raw.iter().map(|w| plain(w)).collect();

    // Words the project asked to leave alone.
    let mut ignored = BTreeSet::new();
    for phrase in &terms.ignored {
        for start in 0..(words.len() + 1).saturating_sub(phrase.len()) {
            if words[start..start + phrase.len()] == phrase[..] {
                ignored.extend(start..start + phrase.len());
            }
        }
    }

    // The longest known names first, each word in at most one.
    let mut taken = ignored.clone();
    let mut matched: Vec<(Range<usize>, &Hit)> = Vec::new();
    let mut out = Vec::new();
    for size in (1..=terms.longest.min(words.len())).rev() {
        for start in 0..=words.len() - size {
            let span = start..start + size;
            if span.clone().any(|i| taken.contains(&i)) {
                continue;
            }
            let gram = words[span.clone()].join("_");
            let Some(hit) = terms.get(&gram) else { continue };
            taken.extend(span.clone());
            matched.push((span.clone(), hit));
            if let Some(f) = finding_for(hit, &gram, &raw, &words, span, terms, prose) {
                out.push(f);
            }
        }
    }
    if prose {
        return out;
    }

    // What follows a Quranic concept: `surah_no`, `ayah_idx`, `waqf_type`.
    for (span, hit) in &matched {
        let term = terms.term(hit);
        let end = span.end;
        if !term.is_quranic_concept() || end >= words.len() || ignored.contains(&end) {
            continue;
        }
        let (code, after, written) = (&term.code, words[end].as_str(), raw[span.start..=end].join("_"));
        let f = if after == "type" && !code.ends_with("_mark") {
            Finding::new(Rule::TypeSuffix, Severity::Warning, written, "a name for what it classifies".into())
        } else if NUMBER_ABBREVIATIONS.contains(&after) {
            Finding::new(Rule::ReferenceNumbers, Severity::Error, written, format!("{code}_number"))
        } else if SEQUENCE_WORDS.contains(&after) {
            Finding::new(Rule::Position, Severity::Warning, written, format!("{code}_number or {code}_position"))
        } else {
            continue;
        };
        out.push(Finding { arabic: term.arabic.clone(), ..f });
    }
    for (i, word) in words.iter().enumerate() {
        if let Some((_, full)) = ABBREVIATIONS.iter().find(|(short, _)| short == word).filter(|_| !taken.contains(&i)) {
            out.push(Finding::new(Rule::ClearNames, Severity::Warning, word.clone(), full.to_string()));
        }
    }
    // A timing named after a recording: `hafs_word_timing`.
    let timing = matched.iter().map(|(_, h)| &terms.term(h).code).find(|c| TIMINGS.contains(&c.as_str()));
    if let Some(timing) = timing {
        if matched.iter().any(|(_, h)| terms.term(h).is_recording()) {
            out.push(Finding::new(Rule::AudioTimings, Severity::Warning, identifier.to_string(), timing.clone()));
        }
    }
    out
}

/// The finding for one known name inside an identifier, if it breaks a rule.
fn finding_for(hit: &Hit, gram: &str, raw: &[String], words: &[String], span: Range<usize>, terms: &Terms, prose: bool) -> Option<Finding> {
    let term = terms.term(hit);
    let marked = span.clone().any(|i| raw[i] != words[i]);
    let rules = if marked { hit.rules.with(Rule::Characters) } else { hit.rules };
    if rules.is_empty() {
        return None;
    }
    let following = words.get(span.end).map_or("", String::as_str);
    if hit.rules.is(Rule::TaMarbutah) && gram.ends_with("at") && following == "al" {
        return None; // `surat_al_...` is idafah, written with t (rule 020).
    }
    if hit.rules.is(Rule::LeadingArticle) && span.start > 0 {
        return None; // Inside a compound, `al` is idafah (rule 033), not a leading article.
    }
    let display = key(&term.display);
    if prose && (marked || hit.warning || hit.gloss || rules.code_only() || gram == display || gram == format!("{display}s")) {
        return None;
    }
    // Marks and apostrophes only occur in strings, often quoted text.
    let warning = hit.warning || rules.is(Rule::Characters);
    Some(Finding {
        rules,
        severity: if warning { Severity::Warning } else { Severity::Error },
        found: raw[span].join("_"),
        preferred: if hit.plural { term.plural.clone() } else { term.code.clone() },
        gloss: hit.gloss,
        arabic: term.arabic.clone(),
    })
}

/// The identifier with every fixable finding replaced, in its own case style:
/// `getSuraNo` → `getSurahNumber`. Where two fixes overlap (`sura`,
/// `sura_no`), the longer one wins.
pub fn suggest<'a>(identifier: &str, findings: impl IntoIterator<Item = &'a Finding>) -> Option<String> {
    let spans = part_spans(identifier);
    let words: Vec<String> = spans.iter().map(|r| identifier[r.clone()].to_lowercase()).collect();
    let mut edits: Vec<(Range<usize>, &str)> = Vec::new();
    for f in findings {
        let size = f.found.split('_').count();
        for i in 0..(words.len() + 1).saturating_sub(size) {
            if words[i..i + size].join("_") == f.found {
                edits.push((spans[i].start..spans[i + size - 1].end, &f.preferred));
            }
        }
    }
    if edits.is_empty() {
        return None;
    }
    edits.sort_by_key(|(r, _)| std::cmp::Reverse(r.len()));
    let mut kept: Vec<(Range<usize>, &str)> = Vec::new();
    for (r, preferred) in edits {
        if kept.iter().all(|(k, _)| r.end <= k.start || r.start >= k.end) {
            kept.push((r, preferred));
        }
    }
    kept.sort_by(|a, b| (b.0.start, b.0.end, b.1).cmp(&(a.0.start, a.0.end, a.1)));
    let mut out = identifier.to_string();
    for (r, preferred) in kept {
        let written = out[r.clone()].to_string();
        out.replace_range(r, &render(&written, preferred));
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::OnceLock;

    fn terms() -> &'static Terms {
        static TERMS: OnceLock<Terms> = OnceLock::new();
        TERMS.get_or_init(|| Terms::load(&[]).unwrap())
    }

    fn found(identifier: &str, prose: bool) -> Vec<(String, String, Vec<&'static str>, Severity)> {
        check(identifier, terms(), prose)
            .into_iter()
            .map(|f| (f.found, f.preferred, f.rules.iter().map(Rule::code).collect(), f.severity))
            .collect()
    }

    #[test]
    fn errors_name_the_rule() {
        for (identifier, found_as, preferred, rule) in [
            ("chapter_title", "chapter", "surah", "001"),
            ("qaloun", "qaloun", "qalun", "056"),
            ("surat_id", "surat", "surah", "019"),
            ("aya_number", "aya", "ayah", "019"),
            ("hamzah_al_wasl", "hamzah_al_wasl", "hamzat_al_wasl", "020"),
            ("tajweed", "tajweed", "tajwid", "021"),
            ("tafseerText", "tafseer", "tafsir", "021"),
            ("makkiyy", "makkiyy", "makki", "022"),
            ("rub_al_hizb", "rub_al_hizb", "rubu_al_hizb", "026"),
            ("asbab_an_nuzul", "asbab_an_nuzul", "asbab_al_nuzul", "030"),
            ("al_fatihah", "al_fatihah", "fatihah", "031"),
            ("AlBaqarah", "al_baqarah", "baqarah", "031"),
            ("rasm_al_uthmani", "rasm_al_uthmani", "rasm_uthmani", "032"),
            ("rubu_hizb", "rubu_hizb", "rubu_al_hizb", "033"),
            ("waqf_jaiz", "waqf_jaiz", "waqf_jaiz_mustawi_al_tarafayn", "046"),
            ("omitted_alif", "omitted_alif", "dagger_alif", "046"),
            ("small_alif", "small_alif", "dagger_alif", "001"),
            ("ayat", "ayat", "ayahs", "049"),
            ("suwar", "suwar", "surahs", "049"),
            ("surah_no", "surah_no", "surah_number", "068"),
            ("sawtam", "sawtam", "phoneme", "001"),
            ("sifa", "sifa", "sifah", "019"),
            ("sifat", "sifat", "sifahs", "049"),
            ("silat_mim_al_jam", "silat_mim_al_jam", "silat_meem_al_jama", "001"),
            ("silat_meem_al_jam", "silat_meem_al_jam", "silat_meem_al_jama", "026"),
        ] {
            let expected = (found_as.to_string(), preferred.to_string(), vec![rule], Severity::Error);
            assert!(found(identifier, false).contains(&expected), "{identifier}: {:?}", found(identifier, false));
        }
    }

    #[test]
    fn warnings_do_not_block() {
        for (identifier, rule) in [
            ("waqf_type", "050"),
            ("ayah_idx", "069"),
            ("hafs_word_timing", "073"),
            ("murattal_letter_timings", "073"),
            ("warsh_an_nafi_phoneme_timing", "073"),
            ("srh", "clear-names"),
            ("qirāʾah", "009"),
            ("qira'ah", "009"),
        ] {
            assert!(found(identifier, false).iter().any(|(_, _, r, s)| r == &[rule] && *s == Severity::Warning), "{identifier}");
        }
    }

    #[test]
    fn the_plural_keeps_its_number() {
        assert_eq!(found("suras", false), [("suras".into(), "surahs".into(), vec!["019"], Severity::Error)]);
    }

    #[test]
    fn canonical_and_ordinary_names_are_left_alone() {
        for identifier in [
            "ayah", "surahs", "ayahKey", "SurahNumber", "WORD_TIMING", "word_timings", "has_word_timings",
            "word_timestamp", "word_timestamps", "WordTimestamp", "tajwid", "hamzat_al_wasl",
            "waqf_jaiz_mustawi_al_tarafayn", "waqfJaizMustawiAlTarafayn", "noon_sakinah", "small_meem",
            "aal_imran", "waqf_mark_type", "rubu_al_hizb", "asbab_al_nuzul", "makki", "kufi", "thumn_al_hizb",
            "ayah_timings", "letter_timings", "LetterTimings", "phoneme-timings", "getPhonemeTimings",
            "phonemes", "sifah", "sifahs", "silat_meem_al_jama", "madd_al_silah",
            // Two concepts carry `silah`, so alone it names neither; `qasr_silah` is a wajh.
            "silah", "qasr_silah",
            "part", "pause", "timing", "reading", "stop", "line_no", "token_type", "font_index",
            "feel", "room", "sad", "elephant", "data", "format", "root", "teen", "ay", "sabs", "noor",
            // English concepts don't take Arabic spelling rules.
            "at_token", "atToken", "el_page", "alLine", "liine", "al_character",
        ] {
            assert_eq!(found(identifier, false), [], "{identifier}");
        }
    }

    #[test]
    fn prose_allows_display_forms_but_not_misspellings() {
        for identifier in ["al-Fatihah", "Qur'an", "Tajweed", "verse", "chapter", "ARABIC"] {
            assert_eq!(found(identifier, true), [], "{identifier}");
        }
        assert_eq!(found("Sura", true), [("sura".into(), "surah".into(), vec!["019"], Severity::Error)]);
    }

    #[test]
    fn arabic_words_in_mixed_names_are_still_checked() {
        for identifier in ["ayaEnding", "suraObjectives", "ayaKey"] {
            assert!(found(identifier, false).iter().any(|f| f.3 == Severity::Error), "{identifier}");
        }
    }

    #[test]
    fn ignored_words() {
        let ignore = |words: &[&str]| Terms::load(&words.iter().map(|w| w.to_string()).collect::<Vec<_>>()).unwrap();
        let sura = ignore(&["sura"]);
        assert!(check("sura", &sura, false).is_empty() && check("suras", &sura, false).is_empty() && check("surah", &sura, false).is_empty());
        let aya = ignore(&["aya"]);
        for identifier in ["aya", "ayas", "ayaKey", "aya_key", "AYA_KEY", "getAyaNo"] {
            assert!(check(identifier, &aya, false).is_empty(), "{identifier}");
        }
        // They don't hide other mistakes.
        assert_eq!(check("ayaKeySura", &aya, false).iter().map(|f| f.found.as_str()).collect::<Vec<_>>(), ["sura"]);
        assert!(!check("ayat", &aya, false).is_empty());
        // A compound respects word boundaries and case.
        let compound = ignore(&["ayaKey"]);
        assert!(check("get_aya_key", &compound, false).is_empty() && check("ayaKeys", &compound, false).is_empty());
        assert!(!check("ayaNumber", &compound, false).is_empty());
    }

    #[test]
    fn english_apostrophes_are_not_marks() {
        for text in ["the line's end", "don't", "it's", "we'll", "they're"] {
            for (_, identifier) in super::super::words::identifiers(text) {
                assert_eq!(found(identifier, false), [], "{text}");
            }
        }
    }

    #[test]
    fn suggestions_keep_the_case_style() {
        let suggest_for = |identifier: &str| {
            let findings = check(identifier, terms(), false);
            suggest(identifier, findings.iter().filter(|f| f.fixable()))
        };
        assert_eq!(suggest_for("getSuraNo").as_deref(), Some("getSurahNumber"));
        assert_eq!(suggest_for("AYA_AYA").as_deref(), Some("AYAH_AYAH"));
        assert_eq!(suggest_for("aya_no").as_deref(), Some("ayah_number"));
    }
}
