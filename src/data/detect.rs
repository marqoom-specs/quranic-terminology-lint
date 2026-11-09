//! Finding Quran data inside a JSON document: per-surah ayah lists, surah
//! tables with counts, flat ayah records, and single-surah files.

use crate::data::text::{has_arabic, tokens};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

#[derive(Debug, Default, Clone)]
pub struct QuranData {
    /// JSON path of the node that holds it, `/` for the root.
    pub pointer: String,
    /// What shape it was read from, for the report.
    pub shape: &'static str,
    /// Ayah count per surah number.
    pub counts: BTreeMap<u32, u32>,
    /// Ayah texts per surah, when the data carries text.
    pub texts: BTreeMap<u32, Vec<String>>,
    /// Whether ayah objects carry word lists, and whether those words carry a position or key.
    pub words: Option<bool>,
    /// A field mapping each ayah to Kufi numbering, such as `number_in_hafs`.
    pub kufi_mapping: Option<String>,
}

/// A key that maps an ayah to its Kufi (Hafs) number: `number_in_hafs`, `hafs_ayah_key`, `kufi_number`.
pub fn kufi_mapping_key(o: &Map<String, Value>) -> Option<String> {
    o.keys()
        .find(|k| {
            let t = tokens(k);
            t.iter().any(|x| x == "hafs" || x == "kufi")
                && t.iter().any(|x| ["number", "numbers", "key", "ayah", "verse", "in"].contains(&x.as_str()))
        })
        .cloned()
}

const AYAH_WORDS: &[&str] = &["ayah", "ayahs", "aya", "ayat", "ayas", "verse", "verses"];
const SURAH_WORDS: &[&str] = &["surah", "sura", "chapter"];

fn has_token(key: &str, words: &[&str]) -> bool {
    tokens(key).iter().any(|t| words.contains(&t.as_str()))
}

/// A field that counts a surah's ayahs: `ayahCount`, `verses_count`, `numberOfAyahs`, `total_ayahs`.
fn is_count_key(key: &str) -> bool {
    let t = tokens(key);
    let ayah = t.iter().any(|x| AYAH_WORDS.contains(&x.as_str()));
    let count = t.iter().any(|x| ["count", "number", "total", "num", "length", "len"].contains(&x.as_str()));
    let of = t.iter().any(|x| x == "of") || t.iter().any(|x| x == "count" || x == "total" || x == "num");
    ayah && count && of && !t.iter().any(|x| x == "in" || x == "start" || x == "first" || x == "id")
}

/// A field holding a surah number: `surah`, `surah_number`, `chapter_id`, `sura_no`.
fn is_surah_number_key(key: &str) -> bool {
    let t = tokens(key);
    if t.is_empty() || !SURAH_WORDS.contains(&t[0].as_str()) {
        return false;
    }
    t.len() == 1 || (t.len() == 2 && ["number", "id", "no", "num", "index"].contains(&t[1].as_str()))
}

/// A field holding an ayah's number within its surah.
fn is_ayah_number_key(key: &str) -> bool {
    let t = tokens(key);
    let n = norm_join(&t);
    if ["number_in_surah", "ayah_in_surah", "verse_in_surah", "number_in_sura"].contains(&n.as_str()) {
        return true;
    }
    if t.is_empty() || !["ayah", "aya", "verse"].contains(&t[0].as_str()) {
        return false;
    }
    t.len() == 1 || (t.len() == 2 && ["number", "no", "num", "index"].contains(&t[1].as_str()))
}

fn is_ayah_key_key(key: &str) -> bool {
    let n = norm_join(&tokens(key));
    ["ayah_key", "verse_key", "aya_key", "ayah_ref", "verse_ref"].contains(&n.as_str())
}

fn norm_join(t: &[String]) -> String {
    t.join("_")
}

fn as_u32(v: &Value) -> Option<u32> {
    match v {
        Value::Number(n) => n.as_u64().map(|x| x as u32),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

/// Mostly Arabic letters: Quran text, as opposed to a translation that quotes a word or two.
pub fn mostly_arabic(s: &str) -> bool {
    let letters = s.chars().filter(|c| c.is_alphabetic()).count();
    let arabic = s.chars().filter(|c| ('\u{0600}'..='\u{06FF}').contains(c) || ('\u{0870}'..='\u{08FF}').contains(c)).count();
    letters > 0 && arabic * 10 >= letters * 8 && has_arabic(s)
}

/// The text of an ayah given as a string or an object, in any language.
fn ayah_text(v: &Value) -> Option<String> {
    match v {
        Value::String(s) if s.chars().any(|c| c.is_alphabetic()) => Some(s.clone()),
        Value::Object(o) => {
            for k in ["text", "text_uthmani", "uthmani", "aya_text", "ayah_text", "verse", "content", "arabic", "ar", "text_ar", "translation"] {
                if let Some(Value::String(s)) = o.get(k) {
                    if s.chars().any(|c| c.is_alphabetic()) {
                        return Some(s.clone());
                    }
                }
            }
            None
        }
        _ => None,
    }
}

/// An array that reads as the ayahs of one surah. Texts are returned only when they are Arabic.
fn ayah_list(v: &Value) -> Option<(u32, Vec<String>, Option<bool>)> {
    let arr = v.as_array()?;
    if arr.is_empty() || arr.len() > 300 {
        return None;
    }
    let strings: Vec<String> = arr.iter().filter_map(ayah_text).collect();
    let numbered = arr.iter().filter(|a| a.as_object().is_some_and(|o| o.keys().any(|k| is_ayah_number_key(k)))).count();
    if strings.len() * 10 >= arr.len() * 9 || numbered * 10 >= arr.len() * 9 {
        // When ayahs carry numbers, the count is the highest number, not the length.
        let count = if numbered == arr.len() {
            arr.iter()
                .filter_map(|a| a.as_object()?.iter().find(|(k, _)| is_ayah_number_key(k)).and_then(|(_, v)| as_u32(v)))
                .max()
                .unwrap_or(arr.len() as u32)
        } else {
            arr.len() as u32
        };
        let arabic = strings.len() == arr.len() && strings.iter().all(|t| mostly_arabic(t));
        Some((count, if arabic { strings } else { Vec::new() }, word_positions(arr)))
    } else {
        None
    }
}

/// For ayah objects with word lists: whether each word carries a position or a key.
fn word_positions(ayahs: &[Value]) -> Option<bool> {
    let mut seen = false;
    for a in ayahs.iter().take(20) {
        let Some(o) = a.as_object() else { continue };
        let Some(words) = o.iter().find(|(k, _)| tokens(k).first().is_some_and(|t| t == "words")).and_then(|(_, v)| v.as_array()) else { continue };
        for w in words.iter().take(5) {
            let Some(wo) = w.as_object() else { continue };
            seen = true;
            let positioned = wo.keys().any(|k| {
                let t = tokens(k);
                t.iter().any(|x| ["position", "key", "index", "idx", "location", "wid"].contains(&x.as_str()))
                    || t == ["id"]
                    || t == ["word", "number"]
            });
            if !positioned {
                return Some(false);
            }
        }
    }
    seen.then_some(true)
}

fn count_in_object(o: &Map<String, Value>) -> Option<(u32, Vec<String>, Option<bool>)> {
    // An array field of ayahs wins over a stated count.
    for (k, v) in o {
        if has_token(k, AYAH_WORDS) {
            if let Some(found) = ayah_list(v) {
                return Some(found);
            }
        }
    }
    for (k, v) in o {
        if is_count_key(k) {
            if let Some(n) = as_u32(v) {
                return Some((n, Vec::new(), None));
            }
        }
    }
    None
}

fn surah_number(o: &Map<String, Value>) -> Option<u32> {
    o.iter()
        .find(|(k, _)| is_surah_number_key(k) || *k == "number" || *k == "id" || *k == "index")
        .and_then(|(_, v)| as_u32(v))
        .filter(|n| (1..=114).contains(n))
}

fn detect_node(v: &Value, pointer: &str) -> Option<QuranData> {
    let mut d = QuranData { pointer: if pointer.is_empty() { "/".into() } else { pointer.into() }, ..Default::default() };
    match v {
        Value::Object(o) => {
            // {"1": [...], "2": [...]} — keyed by surah number.
            let numeric: Vec<(u32, &Value)> = o.iter().filter_map(|(k, v)| Some((k.parse::<u32>().ok()?, v))).collect();
            if !numeric.is_empty() && numeric.len() == o.len() && numeric.iter().all(|(n, _)| (1..=114).contains(n)) {
                for (n, val) in &numeric {
                    let found = match val {
                        Value::Array(_) => ayah_list(val),
                        Value::Object(inner) => {
                            // {"1": {"1": "...", "2": "..."}} — ayahs keyed by number.
                            let keys: Vec<u32> = inner.keys().filter_map(|k| k.parse().ok()).collect();
                            let contiguous = keys.iter().max().is_some_and(|m| *m as usize == keys.len());
                            if !keys.is_empty() && keys.len() == inner.len() && !contiguous {
                                return None; // a selection of ayahs, not a surah's ayahs
                            }
                            if !keys.is_empty() && keys.len() == inner.len() {
                                let texts: Vec<String> = keys.iter().filter_map(|k| inner.get(&k.to_string()).and_then(ayah_text)).collect();
                                let texts = if texts.iter().all(|t| mostly_arabic(t)) { texts } else { Vec::new() };
                                Some((*keys.iter().max().unwrap(), texts, None))
                            } else {
                                count_in_object(inner)
                            }
                        }
                        _ => None,
                    };
                    let (count, texts, words) = found?;
                    if d.kufi_mapping.is_none() {
                        d.kufi_mapping = val.as_array().and_then(|a| a.first()).and_then(|x| x.as_object()).and_then(kufi_mapping_key);
                    }
                    d.counts.insert(*n, count);
                    if !texts.is_empty() {
                        d.texts.insert(*n, texts);
                    }
                    d.words = d.words.or(words);
                }
                d.shape = "ayahs keyed by surah number";
                return Some(d);
            }
            // {"surah_id": 2, "ayahs": [...]} — one surah.
            if let Some(s) = o.iter().find(|(k, _)| is_surah_number_key(k)).and_then(|(_, v)| as_u32(v)) {
                if (1..=114).contains(&s) {
                    if let Some((count, texts, words)) = o
                        .iter()
                        .filter(|(k, _)| has_token(k, AYAH_WORDS))
                        .find_map(|(_, v)| ayah_list(v))
                    {
                        d.counts.insert(s, count);
                        if !texts.is_empty() {
                            d.texts.insert(s, texts);
                        }
                        d.words = words;
                        d.shape = "one surah's ayahs";
                        return Some(d);
                    }
                }
            }
            None
        }
        Value::Array(arr) => {
            if arr.is_empty() {
                return None;
            }
            // [[...], [...]] — 114 ayah lists in surah order.
            if arr.len() == 114 && arr.iter().all(|x| x.is_array()) {
                for (i, x) in arr.iter().enumerate() {
                    let (count, texts, words) = ayah_list(x)?;
                    d.counts.insert(i as u32 + 1, count);
                    if !texts.is_empty() {
                        d.texts.insert(i as u32 + 1, texts);
                    }
                    d.words = d.words.or(words);
                }
                d.shape = "114 ayah lists";
                return Some(d);
            }
            let objects: Vec<&Map<String, Value>> = arr.iter().filter_map(|x| x.as_object()).collect();
            if objects.len() != arr.len() {
                return None;
            }
            // [{"surah": 2, "ayah": 255, ...}] or [{"verse_key": "2:255"}] — flat ayah records.
            let first = objects[0];
            d.kufi_mapping = kufi_mapping_key(first);
            let surah_key = first.keys().find(|k| is_surah_number_key(k)).cloned();
            let ayah_key = first.keys().find(|k| is_ayah_number_key(k)).cloned();
            let key_key = first.keys().find(|k| is_ayah_key_key(k)).cloned();
            if arr.len() >= 20 && ((surah_key.is_some() && ayah_key.is_some()) || key_key.is_some()) {
                let mut texts: BTreeMap<u32, Vec<(u32, String)>> = BTreeMap::new();
                for o in &objects {
                    let (s, a) = if let (Some(sk), Some(ak)) = (&surah_key, &ayah_key) {
                        (o.get(sk).and_then(as_u32), o.get(ak).and_then(as_u32))
                    } else {
                        let key = o.get(key_key.as_ref().unwrap()).and_then(|v| v.as_str()).unwrap_or("");
                        let mut parts = key.split(':');
                        (parts.next().and_then(|x| x.parse().ok()), parts.next().and_then(|x| x.parse().ok()))
                    };
                    let (Some(s), Some(a)) = (s, a) else { continue };
                    if !(1..=114).contains(&s) {
                        continue;
                    }
                    let e = d.counts.entry(s).or_insert(0);
                    *e = (*e).max(a);
                    if let Some(t) = ayah_text(&Value::Object((*o).clone())) {
                        texts.entry(s).or_default().push((a, t));
                    }
                }
                // Only complete surahs: every ayah from 1 to the highest present.
                let mut distinct: BTreeMap<u32, std::collections::BTreeSet<u32>> = BTreeMap::new();
                for (s, list) in &texts {
                    distinct.entry(*s).or_default().extend(list.iter().map(|(a, _)| *a));
                }
                if !texts.is_empty() {
                    d.counts.retain(|s, max| distinct.get(s).is_some_and(|set| set.len() as u32 == *max));
                }
                if d.counts.is_empty() || (texts.is_empty() && arr.len() < d.counts.values().sum::<u32>() as usize) {
                    return None;
                }
                for (s, mut list) in texts {
                    list.sort_by_key(|(a, _)| *a);
                    if list.iter().all(|(_, t)| mostly_arabic(t)) {
                        d.texts.insert(s, list.into_iter().map(|(_, t)| t).collect());
                    }
                }
                d.shape = "ayah records";
                return Some(d);
            }
            // [{"number": 1, "ayahCount": 7}, ...] — a surah table.
            if arr.len() >= 2 && arr.len() <= 114 {
                let mut all = true;
                for (i, o) in objects.iter().enumerate() {
                    let n = surah_number(o).unwrap_or(i as u32 + 1);
                    match count_in_object(o) {
                        Some((count, texts, words)) => {
                            d.counts.insert(n, count);
                            if !texts.is_empty() {
                                d.texts.insert(n, texts);
                            }
                            d.words = d.words.or(words);
                        }
                        None => {
                            all = false;
                            break;
                        }
                    }
                }
                if all && (arr.len() == 114 || d.texts.len() == arr.len()) {
                    d.shape = if d.texts.is_empty() { "surah table with ayah counts" } else { "surahs with their ayahs" };
                    return Some(d);
                }
            }
            None
        }
        _ => None,
    }
}

/// Every Quran data node in the document, outermost first; a matched node is not searched inside.
pub fn find(v: &Value) -> Vec<QuranData> {
    let mut out = Vec::new();
    walk(v, "", 0, &mut out);
    out
}

fn walk(v: &Value, pointer: &str, depth: usize, out: &mut Vec<QuranData>) {
    if let Some(d) = detect_node(v, pointer) {
        out.push(d);
        return;
    }
    if depth >= 5 {
        return;
    }
    match v {
        Value::Object(o) => {
            for (k, child) in o {
                if child.is_object() || child.is_array() {
                    walk(child, &format!("{pointer}/{k}"), depth + 1, out);
                }
            }
        }
        Value::Array(a) => {
            // Only the first elements of long arrays: their siblings share the shape.
            for (i, child) in a.iter().enumerate().take(3) {
                if child.is_object() || child.is_array() {
                    walk(child, &format!("{pointer}/{i}"), depth + 1, out);
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reads_surah_keyed_text() {
        let v = json!({"1": ["بسم الله الرحمن الرحيم", "الحمد لله رب العالمين"], "112": ["قل هو الله أحد", "الله الصمد", "لم يلد ولم يولد", "ولم يكن له كفوا أحد"]});
        let d = find(&v);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].counts[&112], 4);
    }

    #[test]
    fn reads_a_single_surah_export() {
        let v = json!({"surah_id": 2, "ayahs": [{"ayah_number": 1, "start": 0.0}, {"ayah_number": 285, "start": 1.0}]});
        let d = find(&v);
        assert_eq!(d[0].counts[&2], 285);
    }

    #[test]
    fn reads_flat_records() {
        let mut recs = Vec::new();
        for a in 1..=7 { recs.push(json!({"chapter": 1, "verse": a, "text": "بسم الله"})); }
        for a in 1..=286 { recs.push(json!({"chapter": 2, "verse": a, "text": "الم"})); }
        let d = find(&json!({"quran": recs}));
        assert_eq!(d[0].counts[&2], 286);
        assert_eq!(d[0].pointer, "/quran");
    }

    #[test]
    fn reads_a_surah_table() {
        let rows: Vec<Value> = (1..=114).map(|n| json!({"id": n, "verses_count": if n == 2 { 286 } else { 7 }})).collect();
        let d = find(&json!({"chapters": rows}));
        assert_eq!(d[0].counts.len(), 114);
        assert_eq!(d[0].counts[&2], 286);
    }
}
