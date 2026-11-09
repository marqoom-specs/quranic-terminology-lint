//! Which files are read.

use super::config::{matches, Config, CONFIG_NAME};
use super::layout::Format;
use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};

const SKIP_DIRS: [&str; 18] = [
    ".git", "node_modules", "vendor", "dist", "build", "out", "target", "__pycache__", ".venv", "venv", ".next", ".nuxt",
    ".cache", "coverage", ".mypy_cache", ".pytest_cache", "migrations_backup", "site-packages",
];
/// Larger files are generated or data, and are skipped.
pub const MAX_BYTES: u64 = 2_000_000;

#[derive(Debug, Default)]
pub struct Selection {
    pub files: Vec<PathBuf>,
    /// Files over `MAX_BYTES`.
    pub skipped: Vec<PathBuf>,
}

/// The files under `paths` to check. Symlinks, tool folders, virtualenvs and
/// excluded paths are left out; a missing path given explicitly is an error.
pub fn select(paths: &[PathBuf], cfg: &Config) -> Result<Selection, String> {
    let mut out = Selection::default();
    let mut seen = HashSet::new();
    for path in paths {
        if std::fs::symlink_metadata(path).is_err() {
            return Err(format!("no such path: {}", path.display()));
        }
        visit(&absolute(path), cfg, &mut seen, &mut out)?;
    }
    Ok(out)
}

fn visit(path: &Path, cfg: &Config, seen: &mut HashSet<PathBuf>, out: &mut Selection) -> Result<(), String> {
    let is_link = std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink());
    if is_link || !seen.insert(path.to_path_buf()) {
        return Ok(());
    }
    let rel = relative(path, &cfg.root);
    let parts = if rel.starts_with("..") { path } else { rel.as_path() };
    let skipped_dir = parts.components().any(|c| SKIP_DIRS.iter().any(|d| c.as_os_str() == *d));
    if skipped_dir || matches(&slashed(&rel), &cfg.exclude) {
        return Ok(());
    }
    if !path.exists() {
        return Err(format!("no such path: {}", path.display()));
    }
    if path.is_dir() {
        if path.join("pyvenv.cfg").exists() {
            return Ok(()); // A virtualenv, whatever it is called.
        }
        let mut children: Vec<PathBuf> =
            std::fs::read_dir(path).map_err(|e| format!("{}: {e}", path.display()))?.filter_map(|e| e.ok().map(|e| e.path())).collect();
        children.sort();
        for child in children {
            visit(&child, cfg, seen, out)?;
        }
        return Ok(());
    }
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if !path.is_file() || name == CONFIG_NAME || Format::of(name).is_none() {
        return Ok(());
    }
    let size = std::fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?.len();
    if size > MAX_BYTES { &mut out.skipped } else { &mut out.files }.push(path.to_path_buf());
    Ok(())
}

/// An absolute path with `.` and `..` resolved by name, symlinks kept.
pub fn absolute(path: &Path) -> PathBuf {
    let joined = if path.is_absolute() { path.to_path_buf() } else { std::env::current_dir().unwrap_or_default().join(path) };
    let mut out = PathBuf::new();
    for c in joined.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

/// `path` relative to `base`, by name: `../src/a.py`, or `.` for the same folder.
pub fn relative(path: &Path, base: &Path) -> PathBuf {
    let (path, base) = (absolute(path), absolute(base));
    let (p, b): (Vec<Component>, Vec<Component>) = (path.components().collect(), base.components().collect());
    let common = p.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let mut out: PathBuf = std::iter::repeat_n(Component::ParentDir, b.len() - common).collect();
    out.extend(&p[common..]);
    if out.as_os_str().is_empty() {
        out.push(".");
    }
    out
}

/// A path with `/` separators, for patterns.
pub fn slashed(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// A path as the report shows it: relative to the current directory, or
/// absolute when it is outside it.
pub fn shown(path: &Path) -> String {
    let rel = relative(path, &std::env::current_dir().unwrap_or_default());
    if rel.starts_with("..") { slashed(path) } else { slashed(&rel) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_paths_by_name() {
        assert_eq!(relative(Path::new("/a/b/c.py"), Path::new("/a")), PathBuf::from("b/c.py"));
        assert_eq!(relative(Path::new("/a/x"), Path::new("/a/b")), PathBuf::from("../x"));
        assert_eq!(relative(Path::new("/a"), Path::new("/a")), PathBuf::from("."));
        assert_eq!(absolute(Path::new("/a/./b/../c")), PathBuf::from("/a/c"));
    }
}
