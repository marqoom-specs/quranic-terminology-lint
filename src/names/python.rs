//! The parts of Python source the names check needs: comments and string
//! literals. Lexing fails where Python's own tokenizer fails (an unterminated
//! string, an unclosed bracket, a bad dedent), so a file that isn't valid
//! Python gets no automatic exemptions.

use std::ops::Range;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Comment,
    Str,
    /// An f-string or t-string: its replacement fields are code.
    Formatted,
}

#[derive(Clone, Debug)]
pub struct Token {
    pub kind: Kind,
    /// Byte range in the source.
    pub span: Range<usize>,
    /// Inside an f-string's replacement field.
    pub nested: bool,
}

pub fn tokenize(text: &str) -> Result<Vec<Token>, String> {
    let mut lexer = Lexer { b: text.as_bytes(), pos: 0, brackets: 0, indents: vec![(0, 0)], tokens: Vec::new() };
    lexer.run()?;
    Ok(lexer.tokens)
}

const PREFIXES: [&str; 11] = ["r", "u", "b", "br", "rb", "f", "fr", "rf", "t", "tr", "rt"];

struct Lexer<'a> {
    b: &'a [u8],
    pos: usize,
    brackets: usize,
    /// Indentation levels, measured with tabs of 8 columns and of 1.
    indents: Vec<(usize, usize)>,
    tokens: Vec<Token>,
}

impl Lexer<'_> {
    fn at(&self, i: usize) -> Option<u8> {
        self.b.get(i).copied()
    }

    fn run(&mut self) -> Result<(), String> {
        let mut line_start = true;
        let mut continued = false;
        while let Some(c) = self.at(self.pos) {
            if line_start {
                line_start = false;
                if self.brackets == 0 && !continued {
                    self.indentation()?;
                }
            }
            if !matches!(c, b' ' | b'\t' | b'\x0c' | b'\n') {
                continued = false;
            }
            match c {
                b'\n' => {
                    self.pos += 1;
                    line_start = true;
                }
                b'#' => self.comment(false),
                b'\\' => match self.at(self.pos + 1) {
                    Some(b'\n') => {
                        self.pos += 2;
                        line_start = true;
                        continued = true;
                    }
                    None => return Err("unexpected EOF in multi-line statement".into()),
                    Some(_) => return Err("unexpected character after line continuation character".into()),
                },
                b'"' | b'\'' => self.string(self.pos, false, false)?,
                b'(' | b'[' | b'{' => {
                    self.brackets += 1;
                    self.pos += 1;
                }
                b')' | b']' | b'}' => {
                    self.brackets = self.brackets.saturating_sub(1);
                    self.pos += 1;
                }
                b'\0' => return Err("source code cannot contain null bytes".into()),
                c if c < 0x20 && !matches!(c, b'\t' | b'\x0c' | b'\r') || c == 0x7f => {
                    return Err(format!("invalid non-printable character U+{c:04X}"));
                }
                c if c.is_ascii_alphabetic() || c == b'_' || c >= 0x80 => self.name(false)?,
                c if c.is_ascii_digit() => {
                    while self.at(self.pos).is_some_and(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'.') {
                        self.pos += 1;
                    }
                }
                _ => self.pos += 1,
            }
        }
        if self.brackets > 0 || continued {
            return Err("unexpected EOF in multi-line statement".into());
        }
        Ok(())
    }

    /// Checks a line's indentation against the enclosing blocks.
    fn indentation(&mut self) -> Result<(), String> {
        let (mut col, mut alt, mut p) = (0, 0, self.pos);
        loop {
            match self.at(p) {
                Some(b' ') => (col, alt) = (col + 1, alt + 1),
                Some(b'\t') => (col, alt) = ((col / 8 + 1) * 8, alt + 1),
                Some(b'\x0c') => (col, alt) = (0, 0),
                _ => break,
            }
            p += 1;
        }
        if matches!(self.at(p), None | Some(b'\n' | b'#')) {
            return Ok(()); // A blank line or a comment doesn't indent.
        }
        let tab_error = || Err("inconsistent use of tabs and spaces in indentation".to_string());
        let (top, alt_top) = *self.indents.last().unwrap();
        if col == top {
            if alt != alt_top {
                return tab_error();
            }
        } else if col > top {
            if alt <= alt_top {
                return tab_error();
            }
            self.indents.push((col, alt));
        } else {
            while self.indents.len() > 1 && col < self.indents.last().unwrap().0 {
                self.indents.pop();
            }
            let (top, alt_top) = *self.indents.last().unwrap();
            if col != top {
                return Err("unindent does not match any outer indentation level".into());
            }
            if alt != alt_top {
                return tab_error();
            }
        }
        Ok(())
    }

    fn comment(&mut self, nested: bool) {
        let start = self.pos;
        while self.at(self.pos).is_some_and(|c| c != b'\n') {
            self.pos += 1;
        }
        self.tokens.push(Token { kind: Kind::Comment, span: start..self.pos, nested });
    }

    /// A name, or the prefix of a string literal (`rb"…"`, `f'…'`).
    fn name(&mut self, nested: bool) -> Result<(), String> {
        let start = self.pos;
        while self.at(self.pos).is_some_and(|c| c.is_ascii_alphanumeric() || c == b'_' || c >= 0x80) {
            self.pos += 1;
        }
        if matches!(self.at(self.pos), Some(b'"' | b'\'')) {
            let prefix = std::str::from_utf8(&self.b[start..self.pos]).unwrap_or("").to_ascii_lowercase();
            if PREFIXES.contains(&prefix.as_str()) {
                let formatted = prefix.contains(['f', 't']);
                return self.string(start, formatted, nested);
            }
        }
        Ok(())
    }

    /// A string literal whose prefix starts at `start`; `self.pos` is at its quote.
    fn string(&mut self, start: usize, formatted: bool, nested: bool) -> Result<(), String> {
        let quote = self.b[self.pos];
        let triple = self.at(self.pos + 1) == Some(quote) && self.at(self.pos + 2) == Some(quote);
        self.pos += if triple { 3 } else { 1 };
        loop {
            match self.at(self.pos) {
                None if triple => return Err("EOF in multi-line string".into()),
                None => return Err("unterminated string literal".into()),
                Some(b'\\') => {
                    if self.at(self.pos + 1).is_none() {
                        return Err("unterminated string literal".into());
                    }
                    self.pos += 2;
                }
                Some(b'\n') if !triple => return Err("unterminated string literal".into()),
                Some(c) if c == quote => {
                    if !triple {
                        self.pos += 1;
                        break;
                    }
                    if self.at(self.pos + 1) == Some(quote) && self.at(self.pos + 2) == Some(quote) {
                        self.pos += 3;
                        break;
                    }
                    self.pos += 1;
                }
                Some(b'{') if formatted => {
                    if self.at(self.pos + 1) == Some(b'{') {
                        self.pos += 2;
                    } else {
                        self.pos += 1;
                        self.replacement_field()?;
                    }
                }
                Some(b'}') if formatted && self.at(self.pos + 1) == Some(b'}') => self.pos += 2,
                Some(_) => self.pos += 1,
            }
        }
        let kind = if formatted { Kind::Formatted } else { Kind::Str };
        self.tokens.push(Token { kind, span: start..self.pos, nested });
        Ok(())
    }

    /// The expression in `{…}` of an f-string, up to its closing brace.
    fn replacement_field(&mut self) -> Result<(), String> {
        let mut depth = 0usize;
        loop {
            match self.at(self.pos) {
                None => return Err("unterminated f-string".into()),
                Some(b'#') => self.comment(true),
                Some(b'"' | b'\'') => self.string(self.pos, false, true)?,
                Some(c) if c.is_ascii_alphabetic() || c == b'_' || c >= 0x80 => self.name(true)?,
                Some(b'(' | b'[' | b'{') => {
                    depth += 1;
                    self.pos += 1;
                }
                Some(b')' | b']') => {
                    depth = depth.saturating_sub(1);
                    self.pos += 1;
                }
                Some(b'}') => {
                    self.pos += 1;
                    if depth == 0 {
                        return Ok(());
                    }
                    depth -= 1;
                }
                Some(b':') if depth == 0 => {
                    self.pos += 1;
                    return self.format_spec();
                }
                Some(_) => self.pos += 1,
            }
        }
    }

    /// The format spec after `:`, which may hold replacement fields of its own.
    fn format_spec(&mut self) -> Result<(), String> {
        loop {
            match self.at(self.pos) {
                None | Some(b'"' | b'\'') => return Err("f-string: expecting '}'".into()),
                Some(b'{') => {
                    self.pos += 1;
                    self.replacement_field()?;
                }
                Some(b'}') => {
                    self.pos += 1;
                    return Ok(());
                }
                Some(_) => self.pos += 1,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(text: &str) -> Result<Vec<(Kind, &str, bool)>, String> {
        Ok(tokenize(text)?.into_iter().map(|t| (t.kind, &text[t.span], t.nested)).collect())
    }

    #[test]
    fn finds_comments_and_strings() {
        assert_eq!(kinds("x = 'a b'  # note\n").unwrap(), [(Kind::Str, "'a b'", false), (Kind::Comment, "# note", false)]);
        assert_eq!(kinds("s = rb'\\''\nt = '''a\nb'''\n").unwrap(), [(Kind::Str, "rb'\\''", false), (Kind::Str, "'''a\nb'''", false)]);
        assert_eq!(kinds("x = xf\"abc\"\n").unwrap(), [(Kind::Str, "\"abc\"", false)]);
    }

    #[test]
    fn f_strings_hold_code() {
        assert_eq!(
            kinds("x = f\"{d['a b']}\"\n").unwrap(),
            [(Kind::Str, "'a b'", true), (Kind::Formatted, "f\"{d['a b']}\"", false)]
        );
        assert_eq!(kinds("f\"{a:{b}}\"\n").unwrap(), [(Kind::Formatted, "f\"{a:{b}}\"", false)]);
        assert_eq!(kinds("f\"{\"a\"}\"\n").unwrap(), [(Kind::Str, "\"a\"", true), (Kind::Formatted, "f\"{\"a\"}\"", false)]);
    }

    #[test]
    fn fails_where_python_fails() {
        for bad in [
            "x = 'abc\ny = 1\n",
            "x = \"\"\"abc\n",
            "x = (1,\n",
            "x = 1 \\\n",
            "x = 1 \\ y\n",
            "if x:\n        a = 1\n    b = 2\n",
            "if x:\n\ta = 1\n        b = 2\n",
            "x = 1\x00\n",
            "x = 1\x0b\n",
        ] {
            assert!(tokenize(bad).is_err(), "{bad:?}");
        }
        for good in ["x = $\n", "x = a ? b\n", "x = )\n", "x = (1]\n", "", "x = 'a b'", "x = 1\n\x0cy = 2\n"] {
            assert!(tokenize(good).is_ok(), "{good:?}");
        }
    }
}
