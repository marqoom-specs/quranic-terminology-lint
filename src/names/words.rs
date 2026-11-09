//! Identifiers in a line of text, and the words they are made of.

use std::ops::Range;
use unicode_normalization::char::canonical_combining_class;
use unicode_normalization::UnicodeNormalization;

/// Apostrophes that may stand for hamzah or ayn in a transliteration (`qira'ah`).
const APOSTROPHES: [char; 5] = ['\'', '’', 'ʼ', 'ʾ', 'ʿ'];
/// English endings after an apostrophe: `line's`, `don't`, `we'll`.
const CONTRACTIONS: [&str; 7] = ["s", "t", "d", "m", "ll", "re", "ve"];

/// Latin letters, with the accented ones a transliteration uses.
pub fn is_letter(c: char) -> bool {
    c.is_ascii_alphabetic() || ('\u{C0}'..='\u{24F}').contains(&c) || ('\u{1E00}'..='\u{1EFF}').contains(&c)
}

/// A character of a word, as a regular expression's `\w` sees it.
pub fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn is_separator(c: char) -> bool {
    matches!(c, '_' | '.' | '-')
}

/// The identifiers in `line`, with their byte offsets: runs of letters,
/// digits, `_`, `.` and `-` that start with a letter. An apostrophe between
/// letters belongs to the identifier unless it begins an English contraction.
pub fn identifiers(line: &str) -> impl Iterator<Item = (usize, &str)> {
    let mut rest = 0;
    std::iter::from_fn(move || {
        let (start, _) = line[rest..].char_indices().find(|(_, c)| is_letter(*c)).map(|(i, c)| (rest + i, c))?;
        let mut end = start;
        let chars = line[start..].char_indices().map(|(i, c)| (start + i, c)).peekable();
        for (i, c) in chars {
            let keep = if is_letter(c) || c.is_ascii_digit() || is_separator(c) || i == start {
                true
            } else if APOSTROPHES.contains(&c) {
                let after = &line[i + c.len_utf8()..];
                let next_is_letter = after.chars().next().is_some_and(is_letter);
                let contraction = CONTRACTIONS.iter().any(|e| {
                    after.strip_prefix(e).is_some_and(|tail| !tail.chars().next().is_some_and(is_word))
                });
                next_is_letter && !contraction
            } else {
                false
            };
            if !keep {
                break;
            }
            end = i + c.len_utf8();
        }
        rest = end;
        Some((start, &line[start..end]))
    })
}

/// Byte offsets inside `chunk` where a camelCase word begins:
/// `getSuraNo` → `get|Sura|No`, `HTMLParser` → `HTML|Parser`.
fn camel_breaks(chunk: &str) -> Vec<usize> {
    let chars: Vec<(usize, char)> = chunk.char_indices().collect();
    (1..chars.len())
        .filter(|&i| {
            let (prev, cur) = (chars[i - 1].1, chars[i].1);
            let next = chars.get(i + 1).map(|x| x.1);
            (prev.is_ascii_lowercase() || prev.is_ascii_digit()) && cur.is_ascii_uppercase()
                || prev.is_ascii_uppercase() && cur.is_ascii_uppercase() && next.is_some_and(|n| n.is_ascii_lowercase())
        })
        .map(|i| chars[i].0)
        .collect()
}

/// Each word of an identifier with its byte range, however it is cased or separated.
pub fn part_spans(identifier: &str) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut offset = 0;
    for chunk in identifier.split(is_separator) {
        let mut edges = vec![0];
        edges.extend(camel_breaks(chunk));
        edges.push(chunk.len());
        out.extend(edges.windows(2).filter(|w| w[1] > w[0]).map(|w| offset + w[0]..offset + w[1]));
        offset += chunk.len() + 1;
    }
    out
}

/// The words of an identifier, lowercased: `getSuraNo` → `get`, `sura`, `no`.
pub fn split(identifier: &str) -> Vec<String> {
    part_spans(identifier).into_iter().map(|r| identifier[r].to_lowercase()).collect()
}

/// A word without apostrophes and accents: `qirāʾah` → `qiraah`.
pub fn plain(word: &str) -> String {
    word.chars()
        .filter(|c| !APOSTROPHES.contains(c))
        .nfkd()
        .filter(|c| canonical_combining_class(*c) == 0)
        .collect()
}

pub fn is_upper(s: &str) -> bool {
    s.chars().any(char::is_uppercase) && !s.chars().any(char::is_lowercase)
}

pub fn is_lower(s: &str) -> bool {
    s.chars().any(char::is_lowercase) && !s.chars().any(char::is_uppercase)
}

/// `Surah`: each cased run starts with a capital and continues in lowercase.
pub fn is_title(s: &str) -> bool {
    let (mut cased, mut after_cased) = (false, false);
    for c in s.chars() {
        if c.is_uppercase() {
            if after_cased {
                return false;
            }
            (cased, after_cased) = (true, true);
        } else if c.is_lowercase() {
            if !after_cased {
                return false;
            }
            (cased, after_cased) = (true, true);
        } else {
            after_cased = false;
        }
    }
    cased
}

pub fn is_alpha(s: &str) -> bool {
    !s.is_empty() && s.chars().all(char::is_alphabetic)
}

/// `preferred` (snake_case) written in the style of `written`: snake, kebab,
/// camel, Pascal or upper case.
pub fn render(written: &str, preferred: &str) -> String {
    let words: Vec<&str> = preferred.split('_').collect();
    let separator: Option<String> = written
        .char_indices()
        .find(|(_, c)| is_separator(*c))
        .map(|(i, _)| written[i..].chars().take_while(|c| is_separator(*c)).collect());
    if is_upper(written) && written.chars().count() > 1 {
        let upper: Vec<String> = words.iter().map(|w| w.to_uppercase()).collect();
        return upper.join(separator.as_deref().unwrap_or("_"));
    }
    let text = match &separator {
        Some(sep) => words.join(sep),
        None => {
            let mut text = words[0].to_string();
            for w in &words[1..] {
                text.push_str(&capitalize(w));
            }
            text
        }
    };
    if written.chars().next().is_some_and(char::is_uppercase) {
        capitalize_first(&text)
    } else {
        text
    }
}

fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    chars.next().map(|c| c.to_uppercase().chain(chars).collect()).unwrap_or_default()
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    chars.next().map(|c| c.to_uppercase().chain(chars.flat_map(char::to_lowercase)).collect()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(line: &str) -> Vec<&str> {
        identifiers(line).map(|(_, s)| s).collect()
    }

    #[test]
    fn finds_identifiers_and_transliterations() {
        assert_eq!(names("const getSuraNo = obj.aya_key;"), ["const", "getSuraNo", "obj.aya_key"]);
        assert_eq!(names("qira'ah and qirāʾah"), ["qira'ah", "and", "qirāʾah"]);
        assert_eq!(names("the line's end, don't"), ["the", "line", "s", "end", "don", "t"]);
        assert_eq!(names("1st _x"), ["st", "x"]);
    }

    #[test]
    fn splits_any_case_style() {
        assert_eq!(split("getSuraNo"), ["get", "sura", "no"]);
        assert_eq!(split("AYA_KEY"), ["aya", "key"]);
        assert_eq!(split("HTMLParser-v2"), ["html", "parser", "v2"]);
        assert_eq!(plain("qirāʾah"), "qiraah");
    }

    #[test]
    fn renders_in_the_written_style() {
        assert_eq!(render("Sura", "surah"), "Surah");
        assert_eq!(render("AYA_NO", "ayah_number"), "AYAH_NUMBER");
        assert_eq!(render("ayaNo", "ayah_number"), "ayahNumber");
        assert_eq!(render("aya-no", "ayah_number"), "ayah-number");
        assert!(is_title("Sura") && !is_title("SuRa") && is_upper("AYA") && is_lower("aya"));
    }
}
