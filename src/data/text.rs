//! Normalising names, values and Arabic text for comparison.

/// Lowercase snake_case with transliteration marks removed:
/// `Hafs-an-Asim`, `ḥafṣ ʿan ʿāṣim` and `hafs_an_asim` all become `hafs_an_asim`.
pub fn norm(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_lower = false;
    for c in s.chars() {
        let mapped = match c {
            'ā' | 'á' | 'â' | 'à' | 'Ā' => Some('a'),
            'ī' | 'í' | 'î' | 'Ī' => Some('i'),
            'ū' | 'ú' | 'û' | 'Ū' => Some('u'),
            'ḥ' | 'Ḥ' => Some('h'),
            'ṣ' | 'Ṣ' => Some('s'),
            'ḍ' | 'Ḍ' => Some('d'),
            'ṭ' | 'Ṭ' => Some('t'),
            'ẓ' | 'Ẓ' => Some('z'),
            'ʿ' | 'ʾ' | '’' | '‘' | '\'' | '`' => None,
            _ => Some(c),
        };
        let Some(c) = mapped else { continue };
        if c.is_ascii_alphanumeric() {
            if c.is_ascii_uppercase() && prev_lower {
                out.push('_');
            }
            prev_lower = c.is_ascii_lowercase() || c.is_ascii_digit();
            out.push(c.to_ascii_lowercase());
        } else if c.is_alphanumeric() {
            prev_lower = false;
            out.extend(c.to_lowercase());
        } else {
            prev_lower = false;
            if !out.ends_with('_') && !out.is_empty() {
                out.push('_');
            }
        }
    }
    while out.ends_with('_') {
        out.pop();
    }
    out
}

/// The tokens of a key name: `verseKey`, `verse_key` and `verse-key` all give [verse, key].
pub fn tokens(key: &str) -> Vec<String> {
    norm(key).split('_').filter(|t| !t.is_empty()).map(String::from).collect()
}

pub fn has_arabic(s: &str) -> bool {
    s.chars().filter(|c| ('\u{0621}'..='\u{064A}').contains(c)).take(2).count() >= 2
}

/// Arabic letters only, with marks removed and the forms of alif unified,
/// so that Uthmani and simple spellings of the same words compare equal.
pub fn ar_norm(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        let c = match c {
            // Alif with hamzah or madd, alif wasla, and the alif-with-mark letters
            // of Arabic Extended-B that some Quran fonts encode.
            '\u{0622}' | '\u{0623}' | '\u{0625}' | '\u{0671}' | '\u{0870}'..='\u{0882}' => 'ا',
            '\u{0649}' => 'ي',
            _ => c,
        };
        let is_mark = matches!(c,
            '\u{0610}'..='\u{061A}' | '\u{064B}'..='\u{065F}' | '\u{0670}' | '\u{06D6}'..='\u{06ED}'
            | '\u{0640}' | '\u{08D3}'..='\u{08FF}' | '\u{0883}'..='\u{089F}');
        if is_mark {
            continue;
        }
        if ('\u{0621}'..='\u{064A}').contains(&c) {
            out.push(c);
        } else if c.is_whitespace() && !out.ends_with(' ') && !out.is_empty() {
            out.push(' ');
        }
    }
    out.trim_end().to_string()
}

/// Whether the text opens with «بسم الله الرحمن الرحيم», and whether more follows it.
pub fn basmalah(text: &str) -> Option<bool> {
    let words: Vec<String> = ar_norm(text).split(' ').map(String::from).collect();
    let opening = ["بسم", "الله", "الرحمن", "الرحيم"];
    if words.len() >= 4 && words[..4].iter().zip(opening).all(|(a, b)| a == b) {
        Some(words.len() > 4)
    } else {
        None
    }
}

pub fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0; b.len() + 1];
    for i in 1..=a.len() {
        cur[0] = i;
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalises_names() {
        assert_eq!(norm("Hafs-an-Asim"), "hafs_an_asim");
        assert_eq!(norm("ḥafṣ ʿan ʿĀṣim"), "hafs_an_asim");
        assert_eq!(norm("ayahCount"), "ayah_count");
        assert_eq!(norm("madani-last"), "madani_last");
        assert_eq!(tokens("verseKey"), vec!["verse", "key"]);
    }

    #[test]
    fn finds_the_basmalah() {
        assert_eq!(basmalah("بِسْمِ ٱللَّهِ ٱلرَّحْمَـٰنِ ٱلرَّحِيمِ"), Some(false));
        assert_eq!(basmalah("بِسْمِ ٱللَّهِ ٱلرَّحْمَـٰنِ ٱلرَّحِيمِ ٱلْحَمْدُ لِلَّهِ"), Some(true));
        assert_eq!(basmalah("بسم الله الرحمن الرحيم"), Some(false));
        assert_eq!(basmalah("ٱلْحَمْدُ لِلَّهِ رَبِّ ٱلْعَـٰلَمِينَ"), None);
    }
}
