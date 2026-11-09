//! The project's declaration: the `data` section of `.terminology.json`.
//!
//! ```json
//! "data": {
//!   "publishes": ["exports/**/*.json", "openapi.json"],
//!   "files": { "data/quran_warsh.json": { "riwayah": "warsh_an_nafi" } },
//!   "mappings": { "hafsh": "hafs_an_asim" },
//!   "exclude": ["fixtures/legacy/**"]
//! }
//! ```

use crate::glob::covers;
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub const CONFIG_NAME: &str = ".terminology.json";

#[derive(Debug, Default, Clone)]
pub struct FileDeclaration {
    pub riwayah: Option<String>,
    pub numbering: Option<String>,
    pub segmentation: Option<String>,
}

#[derive(Debug, Default, Clone)]
pub struct Config {
    pub root: PathBuf,
    pub file: Option<PathBuf>,
    pub publishes: Vec<String>,
    pub files: Vec<(String, FileDeclaration)>,
    pub mappings: HashMap<String, String>,
    pub exclude: Vec<String>,
}

impl Config {
    pub fn find(start: &Path, explicit: Option<&Path>) -> Result<Config, String> {
        let path = match explicit {
            Some(p) => Some(p.to_path_buf()),
            None => {
                // The nearest configuration above the first path.
                let dir = match start.parent() {
                    _ if !start.is_file() => start,
                    Some(p) if !p.as_os_str().is_empty() => p,
                    _ => Path::new("."),
                };
                let dir = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
                dir.ancestors().map(|d| d.join(CONFIG_NAME)).find(|p| p.is_file())
            }
        };
        let Some(path) = path else {
            // Without one, paths are shown from where the check runs, as pre-commit runs it.
            return Ok(Config { root: std::env::current_dir().map_err(|e| e.to_string())?, ..Default::default() });
        };
        let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let raw: Value = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        let root = path.parent().map(Path::to_path_buf).unwrap_or_default();
        let mut cfg = Config { root, file: Some(path.clone()), ..Default::default() };
        let Some(data) = raw.get("data") else { return Ok(cfg) };
        let strings = |v: Option<&Value>| -> Vec<String> {
            v.and_then(|v| v.as_array())
                .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
                .unwrap_or_default()
        };
        cfg.publishes = strings(data.get("publishes"));
        cfg.exclude = strings(data.get("exclude"));
        if let Some(m) = data.get("mappings").and_then(|v| v.as_object()) {
            for (k, v) in m {
                if let Some(v) = v.as_str() {
                    cfg.mappings.insert(k.clone(), v.to_string());
                }
            }
        }
        if let Some(files) = data.get("files").and_then(|v| v.as_object()) {
            for (pattern, decl) in files {
                let get = |k: &str| decl.get(k).and_then(|v| v.as_str()).map(String::from);
                cfg.files.push((
                    pattern.clone(),
                    FileDeclaration {
                        riwayah: get("riwayah"),
                        numbering: get("ayah_numbering_system"),
                        segmentation: get("word_segmentation"),
                    },
                ));
            }
        }
        Ok(cfg)
    }

    pub fn rel(&self, path: &Path) -> String {
        // Absolute without resolving symlinks, so a linked folder keeps its own name.
        let clean = |p: &Path| -> PathBuf { p.components().filter(|c| !matches!(c, std::path::Component::CurDir)).collect() };
        let abs = if path.is_absolute() { clean(path) } else { clean(&std::env::current_dir().unwrap_or_default().join(path)) };
        let root = if self.root.is_absolute() { clean(&self.root) } else { clean(&std::env::current_dir().unwrap_or_default().join(&self.root)) };
        if let Ok(r) = abs.strip_prefix(&root) {
            return r.to_string_lossy().replace('\\', "/");
        }
        let abs = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let root = self.root.canonicalize().unwrap_or_else(|_| self.root.clone());
        abs.strip_prefix(&root).unwrap_or(path).to_string_lossy().replace('\\', "/")
    }

    pub fn excluded(&self, rel: &str) -> bool {
        self.exclude.iter().any(|p| covers(p, rel))
    }

    pub fn published(&self, rel: &str) -> bool {
        self.publishes.iter().any(|p| covers(p, rel))
    }

    pub fn declaration(&self, rel: &str) -> Option<&FileDeclaration> {
        self.files.iter().find(|(p, _)| covers(p, rel)).map(|(_, d)| d)
    }
}
