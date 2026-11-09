//! Path patterns in `.terminology.json`, the same for both checks.

/// `*` and `?` match within one folder name, `**` any number of folders,
/// `[a-z]` and `[!a-z]` one character of a set.
pub(crate) fn glob(pattern: &str, path: &str) -> bool {
    fn parts(p: &[&str], s: &[&str]) -> bool {
        match (p.first(), s.first()) {
            (None, None) => true,
            (Some(&"**"), _) => parts(&p[1..], s) || (!s.is_empty() && parts(p, &s[1..])),
            (Some(a), Some(b)) => segment(&a.chars().collect::<Vec<_>>(), &b.chars().collect::<Vec<_>>()) && parts(&p[1..], &s[1..]),
            _ => false,
        }
    }
    let p: Vec<&str> = pattern.trim_start_matches("./").split('/').collect();
    let s: Vec<&str> = path.trim_start_matches("./").split('/').collect();
    parts(&p, &s)
}

/// A pattern covers a path when it matches the path or a folder above it:
/// `legacy`, `legacy/` and `legacy/*` all cover `legacy/db/schema.sql`.
pub(crate) fn covers(pattern: &str, path: &str) -> bool {
    let pattern = pattern.trim_end_matches('/');
    let path = path.trim_end_matches('/');
    glob(pattern, path) || path.match_indices('/').any(|(i, _)| glob(pattern, &path[..i]))
}

fn segment(p: &[char], s: &[char]) -> bool {
    match p.first() {
        None => s.is_empty(),
        Some('*') => segment(&p[1..], s) || (!s.is_empty() && segment(p, &s[1..])),
        Some('?') => !s.is_empty() && segment(&p[1..], &s[1..]),
        Some('[') => match set_end(p) {
            Some(end) => !s.is_empty() && in_set(&p[1..end], s[0]) && segment(&p[end + 1..], &s[1..]),
            None => s.first() == Some(&'[') && segment(&p[1..], &s[1..]),
        },
        Some(c) => s.first() == Some(c) && segment(&p[1..], &s[1..]),
    }
}

/// The `]` closing a set; a `]` first in the set is a member.
fn set_end(p: &[char]) -> Option<usize> {
    let mut j = 1 + usize::from(p.get(1) == Some(&'!'));
    if p.get(j) == Some(&']') {
        j += 1;
    }
    (j..p.len()).find(|&k| p[k] == ']')
}

fn in_set(set: &[char], c: char) -> bool {
    let (negated, set) = match set.split_first() {
        Some(('!', rest)) => (true, rest),
        _ => (false, set),
    };
    let mut found = false;
    let mut i = 0;
    while i < set.len() {
        if i + 2 < set.len() && set[i + 1] == '-' {
            found |= (set[i]..=set[i + 2]).contains(&c);
            i += 3;
        } else {
            found |= set[i] == c;
            i += 1;
        }
    }
    found != negated
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stars_stay_within_a_folder() {
        assert!(glob("data/**/*.json", "data/a/b.json") && glob("data/**/*.json", "data/b.json"));
        assert!(glob("**/fixtures/*", "x/fixtures/y.json"));
        assert!(!glob("data/*.json", "data/a/b.json"));
        assert!(glob("model[0-9].py", "model1.py") && !glob("model[!0-9].py", "model1.py") && glob("a[]].py", "a].py"));
    }

    #[test]
    fn a_folder_pattern_covers_what_is_in_it() {
        for pattern in ["legacy", "legacy/", "legacy/*", "legacy/**"] {
            assert!(covers(pattern, "legacy/db/schema.sql"), "{pattern}");
        }
        assert!(!covers("legacy/**", "db/schema.sql"));
        assert!(covers("model.py", "model.py") && !covers("model.py", "src/model.py"));
        assert!(covers("**/model.py", "src/model.py"));
    }
}
