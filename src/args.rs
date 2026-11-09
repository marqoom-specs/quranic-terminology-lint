//! Command-line arguments, read the same way by both commands.

use std::path::PathBuf;

pub(crate) enum Arg {
    /// `--format`, `-h`. A value follows with [`Args::value`].
    Option(String),
    Path(PathBuf),
}

/// Arguments in order: options and paths, `--format json` or `--format=json`,
/// and only paths after `--`.
pub(crate) struct Args<I> {
    args: I,
    /// The value given as `--option=value`, until it is read.
    inline: Option<String>,
    option: String,
    paths_only: bool,
}

impl<I: Iterator<Item = String>> Args<I> {
    pub fn new(args: impl IntoIterator<IntoIter = I>) -> Self {
        Args { args: args.into_iter(), inline: None, option: String::new(), paths_only: false }
    }

    pub fn next(&mut self) -> Result<Option<Arg>, String> {
        if self.inline.take().is_some() {
            return Err(format!("{} takes no value", self.option));
        }
        let Some(arg) = self.args.next() else { return Ok(None) };
        if self.paths_only || arg == "-" || !arg.starts_with('-') {
            return Ok(Some(Arg::Path(arg.into())));
        }
        if arg == "--" {
            self.paths_only = true;
            return self.next();
        }
        self.option = match arg.split_once('=') {
            Some((option, value)) if option.starts_with("--") => {
                self.inline = Some(value.to_string());
                option.to_string()
            }
            _ => arg,
        };
        Ok(Some(Arg::Option(self.option.clone())))
    }

    /// The value of the option just read.
    pub fn value(&mut self) -> Result<String, String> {
        self.inline.take().or_else(|| self.args.next()).ok_or_else(|| format!("{} needs a value", self.option))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(args: &[&str]) -> Result<Vec<String>, String> {
        let mut a = Args::new(args.iter().map(|s| s.to_string()));
        let mut out = Vec::new();
        while let Some(arg) = a.next()? {
            out.push(match arg {
                Arg::Option(o) if o == "--format" => format!("{o}={}", a.value()?),
                Arg::Option(o) => o,
                Arg::Path(p) => p.display().to_string(),
            });
        }
        Ok(out)
    }

    #[test]
    fn options_values_and_paths() {
        assert_eq!(read(&["src", "--format", "json", "--strict"]).unwrap(), ["src", "--format=json", "--strict"]);
        assert_eq!(read(&["--format=table", "--", "--odd"]).unwrap(), ["--format=table", "--odd"]);
        assert_eq!(read(&["--strict=yes"]).unwrap_err(), "--strict takes no value");
        assert_eq!(read(&["--format"]).unwrap_err(), "--format needs a value");
    }
}
