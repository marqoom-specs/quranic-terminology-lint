//! Which files are read, and what each part of a file is: code, prose written
//! for people, data, or a reference to a resource (a URL, a file name) whose
//! words aren't names at all.

use super::python::{self, Kind};
use regex::Regex;
use std::ops::Range;

const CODE: &[&str] = &[
    ".py", ".js", ".jsx", ".ts", ".tsx", ".mjs", ".cjs", ".go", ".rs", ".java", ".kt", ".swift", ".rb", ".php",
    ".cs", ".c", ".h", ".cpp", ".hpp", ".dart", ".sql", ".graphql", ".gql", ".proto", ".json", ".yml", ".yaml",
    ".toml", ".prisma", ".vue", ".svelte", ".scala", ".ex", ".exs", ".sh", ".html", ".xml", ".csv", ".tsv", ".ini",
    ".env", ".blade.php",
];
const PROSE: &[&str] = &[".md", ".mdx", ".txt", ".rst", ".adoc"];
const RESOURCES: &[&str] = &[".svg", ".png", ".jpg", ".jpeg", ".webp", ".pdf", ".zip", ".gz", ".mp3", ".wav", ".ogg", ".ttf", ".woff", ".woff2"];
const SLASH_COMMENTS: &[&str] = &[
    ".js", ".jsx", ".ts", ".tsx", ".mjs", ".cjs", ".go", ".rs", ".java", ".kt", ".swift", ".php", ".cs", ".c", ".h",
    ".cpp", ".hpp", ".dart", ".scala", ".vue", ".svelte", ".prisma", ".proto",
];
const HASH_COMMENTS: &[&str] = &[".py", ".rb", ".sh", ".yml", ".yaml", ".toml", ".ini", ".env", ".ex", ".exs", ".graphql", ".gql", ".php"];

/// A file's suffix, lowercased: `.py`, `.blade.php`; `.env.example` counts as `.env`.
pub fn suffix(name: &str) -> String {
    let lower = name.to_lowercase();
    if lower.ends_with(".blade.php") {
        return ".blade.php".into();
    }
    if lower == ".env.example" || lower == ".env.sample" {
        return ".env".into();
    }
    let base = lower.rsplit('/').next().unwrap_or("");
    let stem = base.trim_start_matches('.');
    stem.rfind('.').map(|i| stem[i..].to_string()).unwrap_or_default()
}

/// The kind of file, from its suffix.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Format(&'static str);

impl Format {
    /// The format of a file the names check reads, or `None`.
    pub fn of(name: &str) -> Option<Format> {
        let s = suffix(name);
        CODE.iter().chain(PROSE).find(|x| **x == s).map(|x| Format(x))
    }

    /// Markdown and other documents: read as prose throughout.
    pub fn is_prose(self) -> bool {
        PROSE.contains(&self.0)
    }

    fn is(self, suffixes: &[&str]) -> bool {
        suffixes.contains(&self.0)
    }
}

/// Byte ranges on one line, sorted and merged.
#[derive(Clone, Debug, Default)]
pub struct Spans(Vec<Range<usize>>);

impl Spans {
    pub fn new(mut ranges: Vec<Range<usize>>) -> Spans {
        ranges.sort_by_key(|r| (r.start, r.end));
        let mut out: Vec<Range<usize>> = Vec::with_capacity(ranges.len());
        for r in ranges {
            match out.last_mut() {
                Some(last) if r.start <= last.end => last.end = last.end.max(r.end),
                _ => out.push(r),
            }
        }
        Spans(out)
    }

    /// The whole of a line.
    pub fn whole(line: &str) -> Spans {
        Spans(vec![Range { start: 0, end: line.len() }])
    }

    pub fn contains(&self, at: usize) -> bool {
        let i = self.0.partition_point(|r| r.start <= at);
        i > 0 && at < self.0[i - 1].end
    }
}

/// Spans for every line of a file.
pub type PerLine = Vec<Spans>;

fn per_line(lines: usize, ranges: impl IntoIterator<Item = (usize, Range<usize>)>) -> PerLine {
    let mut out: Vec<Vec<Range<usize>>> = vec![Vec::new(); lines];
    for (line, r) in ranges {
        if let Some(v) = out.get_mut(line) {
            v.push(r);
        }
    }
    out.into_iter().map(Spans::new).collect()
}

/// A file's text and the byte range of each line. Lines end at `\n`; `\r\n`
/// and `\r` are read as `\n`.
pub struct Source {
    pub text: String,
    lines: Vec<Range<usize>>,
}

impl Source {
    pub fn new(text: &str) -> Source {
        let text = if text.contains('\r') { text.replace("\r\n", "\n").replace('\r', "\n") } else { text.to_string() };
        let mut lines = Vec::new();
        let mut start = 0;
        for piece in text.split_inclusive('\n') {
            let end = start + piece.len();
            lines.push(start..end - usize::from(piece.ends_with('\n')));
            start = end;
        }
        Source { text, lines }
    }

    pub fn len(&self) -> usize {
        self.lines.len()
    }

    pub fn lines(&self) -> impl Iterator<Item = &str> {
        self.lines.iter().map(|r| &self.text[r.clone()])
    }

    pub fn ranges(&self) -> &[Range<usize>] {
        &self.lines
    }

    /// A byte range of the text, cut into one range per line it touches.
    fn split(&self, span: Range<usize>) -> impl Iterator<Item = (usize, Range<usize>)> + '_ {
        let first = self.lines.partition_point(|r| r.end < span.start);
        self.lines[first..].iter().take_while(move |r| r.start < span.end).enumerate().map(move |(i, r)| {
            let start = span.start.max(r.start);
            (first + i, start - r.start..span.end.min(r.end).max(start) - r.start)
        })
    }
}

/// What each part of a file is.
#[derive(Default)]
pub struct Layout {
    /// Written for people: comments, and strings with spaces in code files.
    /// A display form there (`al-Fatihah`) is fine.
    pub prose: PerLine,
    /// In a data file, the spans that are names (keys, a header); `None` when
    /// every span is.
    pub names: Option<PerLine>,
    /// String literals, which a fix never edits; `None` when the file's syntax
    /// isn't understood well enough to fix anything outside comments.
    pub strings: Option<PerLine>,
    /// Prose that a fix may edit: comments, and plain documents.
    pub editable: PerLine,
    /// `Code` in a document: backticks and fenced blocks. Never edited.
    pub code: PerLine,
    /// File names, URLs and the project's external names, read as references.
    pub masked: String,
    pub references: usize,
}

impl Layout {
    pub fn new(src: &Source, format: Format, fixing: bool, external: Option<&Regex>) -> Layout {
        let tokens = format.is(&[".py"]).then(|| python::tokenize(&src.text));
        let mut layout = Layout {
            prose: if format.is_prose() { vec![Spans::default(); src.len()] } else { prose(src, format, tokens.as_ref()) },
            names: names(src, format),
            ..Layout::default()
        };
        if fixing {
            // A document has no string literals.
            layout.strings = if format.is_prose() { Some(vec![Spans::default(); src.len()]) } else { strings(src, format, tokens.as_ref()) };
            layout.editable = editable(src, format, tokens.as_ref());
            layout.code = code_spans(src, format.is_prose());
        }
        (layout.masked, layout.references) = mask_references(src, format, tokens.as_ref(), external);
        layout
    }
}

/// `\s` as a regular expression reads it.
fn is_space(c: char) -> bool {
    c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c)
}

fn is_word(c: char) -> bool {
    super::words::is_word(c)
}

type Tokens = Result<Vec<python::Token>, String>;

fn token_spans<'a>(src: &'a Source, tokens: &'a [python::Token], keep: impl Fn(&python::Token) -> bool + 'a) -> impl Iterator<Item = (usize, Range<usize>)> + 'a {
    tokens.iter().filter(move |t| keep(t)).flat_map(|t| src.split(t.span.clone()))
}

// --- prose -----------------------------------------------------------------------

/// Spans of a code file written for people: comments, and strings with spaces.
/// `"Surah al-Fatihah"` is a label and `// load al-Fatihah` a note, so they
/// are read like prose. `"ayah_number"` has no spaces: it is a name.
fn prose(src: &Source, format: Format, tokens: Option<&Tokens>) -> PerLine {
    if let Some(Ok(tokens)) = tokens {
        let spaced = |t: &python::Token| match t.kind {
            Kind::Comment => true,
            Kind::Str => src.text[t.span.clone()].chars().any(is_space),
            Kind::Formatted => false,
        };
        return per_line(src.len(), token_spans(src, tokens, spaced));
    }
    let mut markers = Vec::new();
    if format.is(SLASH_COMMENTS) {
        markers.push("//");
    }
    if format.is(HASH_COMMENTS) {
        markers.push("#");
    }
    if format.is(&[".sql"]) {
        markers.push("--");
    }
    let markup = format.is(&[".html", ".xml"]);
    src.lines()
        .map(|line| {
            let trimmed = line.trim_start_matches(is_space);
            if ["/*", "*", "<!--"].iter().any(|m| trimmed.starts_with(m)) {
                return Spans::whole(line);
            }
            let mut spans = Vec::new();
            if markup {
                // Text between tags is what the reader sees.
                let mut edge = 0;
                for tag in tags(line) {
                    spans.push(edge..tag.start);
                    edge = tag.end;
                }
                spans.push(edge..line.len());
            }
            spans.extend(quoted(line).into_iter().filter(|q| line[q.start + 1..q.end - 1].chars().any(is_space)));
            for marker in &markers {
                if let Some(i) = comment_start(line, marker) {
                    spans.push(i..line.len());
                }
            }
            Spans::new(spans)
        })
        .collect()
}

fn comment_start(line: &str, marker: &str) -> Option<usize> {
    if marker != "#" {
        return line.find(marker);
    }
    line.match_indices('#').map(|(i, _)| i).find(|&i| i == 0 || line[..i].chars().next_back().is_some_and(is_space))
}

/// Tags on a line, and the end of a tag begun on an earlier line.
fn tags(line: &str) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let end_of_run = |from: usize| line[from..].find(['<', '>']).map_or(line.len(), |i| from + i);
    if !line.starts_with('<') {
        let j = end_of_run(0);
        if line[j..].starts_with('>') {
            out.push(0..j + 1);
        }
    }
    let mut i = out.last().map_or(0, |r| r.end);
    while let Some(k) = line[i..].find('<') {
        let start = i + k;
        let j = end_of_run(start + 1);
        let end = if line[j..].starts_with('>') { j + 1 } else { j };
        out.push(start..end);
        i = end;
    }
    out
}

/// Quoted strings on a line. A quote only closes a string of its own kind,
/// so `"Al-Jumu'a"` is one string.
fn quoted(line: &str) -> Vec<Range<usize>> {
    let chars: Vec<(usize, char)> = line.char_indices().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let q = chars[i].1;
        if matches!(q, '"' | '\'' | '`') {
            let mut j = i + 1;
            while j < chars.len() && chars[j].1 != q {
                j += if chars[j].1 == '\\' { 2 } else { 1 };
            }
            if j < chars.len() {
                out.push(chars[i].0..chars[j].0 + 1);
                i = j + 1;
                continue;
            }
        }
        i += 1;
    }
    out
}

// --- names in data files -----------------------------------------------------------

/// In a data file, the spans that are names. A CSV's rows and a JSON file's
/// values are data, often copied from a source (`"Al-Baqarah"`). The names
/// chosen for them are the header and the keys.
fn names(src: &Source, format: Format) -> Option<PerLine> {
    if format.is(&[".csv", ".tsv"]) {
        let header = src.lines().next().map_or(0, str::len);
        return Some(per_line(src.len().max(1), [(0, 0..header)]));
    }
    if format.is(&[".json"]) {
        return Some(src.lines().map(|line| Spans::new(json_keys(line))).collect());
    }
    None
}

/// `"key"` followed by `:`.
fn json_keys(line: &str) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(k) = line[from..].find('"') {
        let start = from + k;
        from = start + 1;
        let mut chars = line[start + 1..].char_indices();
        let close = loop {
            match chars.next() {
                Some((i, '"')) => break Some(start + 1 + i),
                Some((_, '\\')) => {
                    if chars.next().is_none() {
                        break None;
                    }
                }
                Some(_) => {}
                None => break None,
            }
        };
        if let Some(close) = close {
            if line[close + 1..].trim_start_matches(is_space).starts_with(':') {
                out.push(start..close + 1);
                from = close + 1;
            }
        }
    }
    out
}

// --- for fixes ---------------------------------------------------------------------

/// String literals. A fix never edits these: they hold data, import paths and
/// keys that something outside the file depends on.
fn strings(src: &Source, format: Format, tokens: Option<&Tokens>) -> Option<PerLine> {
    if let Some(tokens) = tokens {
        let tokens = tokens.as_ref().ok()?;
        if tokens.iter().any(|t| t.kind == Kind::Formatted) {
            return None; // Interpolation needs semantic references, not a text rename.
        }
        return Some(per_line(src.len(), token_spans(src, tokens, |t| t.kind == Kind::Str)));
    }
    if format.is(&[".js", ".ts", ".mjs", ".cjs"]) {
        script_strings(src)
    } else {
        None // Data, markup and other languages are never fixed.
    }
}

/// Quoted strings in plain JavaScript or TypeScript. Template literals, a `/`
/// that could be division or a regular expression, and unfinished strings
/// make the file unfixable.
fn script_strings(src: &Source) -> Option<PerLine> {
    // Bytes, not characters: every character that matters here is ASCII, and
    // UTF-8 never uses an ASCII byte inside another character.
    let mut in_block = false;
    let mut out = Vec::with_capacity(src.len());
    for line in src.lines() {
        let b = line.as_bytes();
        let mut spans = Vec::new();
        let mut i = 0;
        while i < b.len() {
            let rest = &b[i..];
            if in_block {
                match rest.windows(2).position(|w| w == b"*/") {
                    Some(k) => (in_block, i) = (false, i + k + 2),
                    None => break,
                }
            } else if rest.starts_with(b"//") {
                break;
            } else if rest.starts_with(b"/*") {
                (in_block, i) = (true, i + 2);
            } else if matches!(b[i], b'`' | b'/') {
                return None;
            } else if matches!(b[i], b'"' | b'\'') {
                let (start, quote) = (i, b[i]);
                i += 1;
                while i < b.len() && b[i] != quote {
                    i += if b[i] == b'\\' { 2 } else { 1 };
                }
                if i >= b.len() {
                    return None;
                }
                i += 1;
                spans.push(start..i);
            } else {
                i += 1;
            }
        }
        out.push(Spans::new(spans));
    }
    (!in_block).then_some(out)
}

/// Prose whose place in the file is certain: Python comments, and plain
/// Markdown or text without HTML, templates or front matter.
fn editable(src: &Source, format: Format, tokens: Option<&Tokens>) -> PerLine {
    if let Some(tokens) = tokens {
        return match tokens {
            Ok(tokens) => per_line(src.len(), token_spans(src, tokens, |t| t.kind == Kind::Comment)),
            Err(_) => vec![Spans::default(); src.len()],
        };
    }
    let plain = format.is(&[".md", ".txt"]) && !src.text.contains(['<', '{']) && !src.text.starts_with("---") && !src.text.starts_with("+++");
    src.lines().map(|line| if plain { Spans::whole(line) } else { Spans::default() }).collect()
}

/// In a document, code: inline `code`, fenced and indented blocks, quotes.
/// Elsewhere, only `code` in backticks.
fn code_spans(src: &Source, document: bool) -> PerLine {
    let mut fence: Option<(char, usize)> = None;
    src.lines()
        .map(|line| {
            let whole = Spans::whole(line);
            let opening = if document { fence_of(line) } else { None };
            if let Some((ch, len)) = fence {
                if let Some((c, n, rest)) = opening {
                    if c == ch && n >= len && rest.trim().is_empty() {
                        fence = None;
                    }
                }
                return whole;
            }
            if let Some((c, n, _)) = opening {
                fence = Some((c, n));
                return whole;
            }
            if document && (line.starts_with("    ") || line.starts_with('\t') || line.starts_with('>') || line.contains('`')) {
                return whole; // Indented and quoted examples, and inline code, may hold literal data.
            }
            let mut ticks = Vec::new();
            let mut from = 0;
            while let Some(a) = line[from..].find('`') {
                let open = from + a;
                let Some(b) = line[open + 1..].find('`') else { break };
                ticks.push(open..open + 1 + b + 1);
                from = open + b + 2;
            }
            Spans::new(ticks)
        })
        .collect()
}

/// A fence that opens or closes a code block: three or more backticks or tildes.
fn fence_of(line: &str) -> Option<(char, usize, &str)> {
    let t = line.trim_start_matches(is_space);
    let c = t.chars().next().filter(|c| matches!(c, '`' | '~'))?;
    let n = t.len() - t.trim_start_matches(c).len();
    (n >= 3).then(|| (c, n, &t[n..]))
}

// --- references ----------------------------------------------------------------------

/// A comment or string in Python source, and whether it may cite files.
type Context = (Range<usize>, bool);

/// Masks URLs, quoted file names, and file names in prose or after `source:`,
/// keeping every line's length so positions stay the same. Other strings and
/// attribute names remain names. Returns the masked text and the number of
/// references masked.
fn mask_references(src: &Source, format: Format, tokens: Option<&Tokens>, external: Option<&Regex>) -> (String, usize) {
    // In Python, references are looked for in comments and strings, and file
    // names only in strings and `# source:` comments. Source that doesn't lex
    // gets no exemptions at all.
    let contexts: Option<Vec<Vec<Context>>> = tokens.map(|tokens| {
        let mut per: Vec<Vec<Context>> = vec![Vec::new(); src.len()];
        if let Ok(tokens) = tokens {
            for t in tokens.iter().filter(|t| !t.nested && t.kind != Kind::Formatted) {
                let files = t.kind == Kind::Str || source_line(&src.text[t.span.clone()], false);
                for (line, r) in src.split(t.span.clone()) {
                    per[line].push((r, files));
                }
            }
        }
        per
    });
    let mut masked = src.text.clone();
    let mut count = 0;
    for (n, line_range) in src.lines.iter().enumerate() {
        let line = &src.text[line_range.clone()];
        let (mut spans, ranges): (Vec<Range<usize>>, Vec<Range<usize>>) = match &contexts {
            Some(per) => (
                per[n].iter().flat_map(|(r, _)| urls(&line[r.clone()]).into_iter().map(move |u| u.start + r.start..u.end + r.start)).collect(),
                per[n].iter().filter(|(_, files)| *files).map(|(r, _)| r.clone()).collect(),
            ),
            None => (
                urls(line),
                if format.is_prose() || source_line(line, true) { vec![Range { start: 0, end: line.len() }] } else { quoted_bodies(line) },
            ),
        };
        for r in ranges {
            spans.extend(path_tokens(line, r).into_iter().filter(|t| is_file_name(&line[t.clone()])));
        }
        // A project's own external names are kept whole for the scanner.
        let protected: Vec<Range<usize>> = external.map(|re| re.find_iter(line).map(|m| m.range()).collect()).unwrap_or_default();
        spans.retain(|s| !protected.iter().any(|p| s.start < p.end && s.end > p.start));
        let merged = Spans::new(spans);
        count += merged.0.len();
        for r in &merged.0 {
            let at = line_range.start + r.start..line_range.start + r.end;
            masked.replace_range(at.clone(), &" ".repeat(at.len()));
        }
    }
    (masked, count)
}

fn is_file_name(token: &str) -> bool {
    let s = suffix(token);
    CODE.iter().chain(PROSE).chain(RESOURCES).any(|x| *x == s)
}

/// `https://…` up to a space, a quote, `<`, `>` or `)`.
fn urls(text: &str) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(k) = text[from..].find("http") {
        let start = from + k;
        let rest = &text[start + 4..];
        let scheme = if rest.starts_with("s://") { 8 } else if rest.starts_with("://") { 7 } else { 0 };
        if scheme == 0 {
            from = start + 4;
            continue;
        }
        let body = &text[start + scheme..];
        let len = body.find(|c: char| is_space(c) || "<>\"'`)".contains(c)).unwrap_or(body.len());
        if len == 0 {
            from = start + scheme;
            continue;
        }
        out.push(start..start + scheme + len);
        from = start + scheme + len;
    }
    out
}

/// The text of each quoted string on a line: `"uthmani-hafs.json"`.
fn quoted_bodies(line: &str) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(k) = line[from..].find(['"', '\'', '`']) {
        let open = from + k;
        let q = line[open..].chars().next().unwrap();
        let body = open + 1;
        match line[body..].find(['"', '\'', '`', '\n']) {
            Some(len) if len > 0 && line[body + len..].starts_with(q) => {
                out.push(body..body + len);
                from = body + len + 1;
            }
            Some(len) => from = body + len,
            None => break,
        }
    }
    out
}

/// A `source:` note, which cites files: `// source: uthmani-hafs.json`, or a
/// `"source":` key. `anywhere` lets the key appear anywhere on the line.
fn source_line(text: &str, anywhere: bool) -> bool {
    let after_word = |s: &str| -> bool {
        s.get(..6).is_some_and(|w| w.eq_ignore_ascii_case("source")) && s[6..].trim_start_matches(is_space).starts_with(':')
    };
    let t = text.trim_start_matches(is_space);
    let comment = ["//", "#", "*"].iter().find_map(|m| t.strip_prefix(m)).is_some_and(|rest| after_word(rest.trim_start_matches(is_space)));
    let key = |s: &str| s.starts_with('"') && s.get(1..7).is_some_and(|w| w.eq_ignore_ascii_case("source")) && s[7..].starts_with('"') && s[8..].trim_start_matches(is_space).starts_with(':');
    comment || if anywhere { text.match_indices('"').any(|(i, _)| key(&text[i..])) } else { key(text) }
}

/// File names inside `range` of a line: `uthmani-hafs.json`, `data/quran.json`.
fn path_tokens(line: &str, range: Range<usize>) -> Vec<Range<usize>> {
    let path_char = |c: char| c.is_ascii_alphanumeric() || "_@./-".contains(c);
    let boundary = |c: Option<char>| !c.is_some_and(|c| is_word(c) || c == '.');
    let mut out = Vec::new();
    let mut s = range.start;
    'scan: while s < range.end {
        let c = line[s..].chars().next().unwrap();
        if path_char(c) && boundary(line[..s].chars().next_back()) {
            let run = s + line[s..range.end].find(|c: char| !path_char(c)).unwrap_or(range.end - s);
            // The last `.` in the run that a word of letters and digits follows, ending the name.
            for (dot, _) in line[s + 1..run].match_indices('.').map(|(i, d)| (s + 1 + i, d)).collect::<Vec<_>>().into_iter().rev() {
                let ext = line[dot + 1..range.end].find(|c: char| !c.is_ascii_alphanumeric()).unwrap_or(range.end - dot - 1);
                if ext == 0 {
                    continue;
                }
                let end = dot + 1 + ext;
                if end >= range.end || boundary(line[end..].chars().next()) {
                    out.push(s..end);
                    s = end;
                    continue 'scan;
                }
            }
        }
        s += c.len_utf8();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spans_merge_and_find() {
        let s = Spans::new(vec![5..8, 0..2, 7..10]);
        assert!(s.contains(0) && s.contains(9) && !s.contains(2) && !s.contains(10));
    }

    #[test]
    fn finds_references() {
        let line = "see https://x.org/a.json) and \"uthmani-hafs.json\" or obj.aya"; // terminology: ignore
        assert_eq!(urls(line).iter().map(|r| &line[r.clone()]).collect::<Vec<_>>(), ["https://x.org/a.json"]);
        let bodies = quoted_bodies(line);
        assert_eq!(&line[bodies[0].clone()], "uthmani-hafs.json");
        assert_eq!(path_tokens(line, 0..line.len()).iter().map(|r| &line[r.clone()]).collect::<Vec<_>>(), ["//x.org/a.json", "uthmani-hafs.json", "obj.aya"]); // terminology: ignore
        assert!(source_line("// Source: a.json", false) && source_line(r#"{"source": "a.json"}"#, true) && !source_line(r#"{"source": "a.json"}"#, false) && !source_line("x = 1", true));
    }

    #[test]
    fn reads_tags_quotes_and_keys() {
        assert_eq!(tags("<h1>Chapter 1</h1>"), [0..4, 13..18]);
        assert_eq!(quoted(r#"["Al-Jumu'a", "Friday"]"#), [1..12, 14..22]); // terminology: ignore
        assert_eq!(json_keys(r#"{"a": "b c", "k": 1}"#), [1..4, 13..16]);
        assert_eq!(suffix("a/b.blade.php"), ".blade.php");
        assert_eq!(suffix(".bashrc"), "");
    }

    #[test]
    fn script_strings_survive_any_text() {
        let src = Source::new("\u{feff}const a = \"é\\\"\"; // ü\n");
        let spans = script_strings(&src).unwrap();
        assert!(spans[0].contains(13) && !spans[0].contains(4));
        assert!(script_strings(&Source::new("const r = /aya/;\n")).is_none()); // terminology: ignore
    }

    #[test]
    fn lines_split_on_newlines_only() {
        let src = Source::new("a\r\nb\x0cc\rd\n");
        assert_eq!(src.lines().collect::<Vec<_>>(), ["a", "b\x0cc", "d"]);
        assert_eq!(src.split(2..7).collect::<Vec<_>>(), [(1, 0..3), (2, 0..1)]);
    }
}
