//! The Marqoom registries, compiled into the binary so the check runs offline.
//!
//! `registries/*.tsv` are copied unchanged from the terminology standard's
//! registries; `concepts.tsv` is the one the names lint already ships.

use crate::data::text::{ar_norm, norm};
use std::collections::HashMap;

const AYAH_COUNTS: &str = include_str!("../../registries/ayah_counts.tsv");
const AYAH_NUMBERING: &str = include_str!("../../registries/ayah_numbering.tsv");
const QIRAAT: &str = include_str!("../../registries/qiraat.tsv");
const SAJDAH: &str = include_str!("../../registries/sajdah.tsv");
const SURAHS: &str = include_str!("../../registries/surahs.tsv");
const CONCEPTS: &str = include_str!("../../terms/concepts.tsv");

/// The six ayah numbering systems, in the column order of `ayah_counts.tsv`.
pub const SYSTEMS: [&str; 6] = ["madani_first", "madani_last", "makki", "kufi", "basri", "dimashqi"];

#[derive(Debug, Clone)]
pub struct Member {
    pub kind: String,
    pub code: String,
    pub parent: String,
    pub rawi: String,
    pub numbering: String,
    pub arabic: String,
    pub alts: Vec<String>,
}

pub struct Registry {
    /// Per-surah ayah counts, index = surah number - 1, columns in `SYSTEMS` order.
    pub counts: Vec<[u32; 6]>,
    /// The total al-Dani states for each system, and the other totals he reports.
    pub totals: [u32; 6],
    pub also_reported: [Vec<u32>; 6],
    /// Qiraat, rawis and riwayahs.
    pub members: Vec<Member>,
    /// Alternative spellings of each numbering system code.
    pub numbering_alts: HashMap<String, String>,
    /// Places of prostration as (surah, ayah) in Kufi numbering.
    pub sajdah: Vec<(u32, u32)>,
    /// Non-canonical names in data keys → the canonical name (from concepts.tsv).
    pub key_names: HashMap<String, String>,
}

/// Splits a TSV registry into its header (the last commented line that has
/// tabs, or the first data line) and its data rows.
fn table(text: &str) -> (Vec<String>, Vec<Vec<String>>) {
    let mut header: Vec<String> = Vec::new();
    let mut rows = Vec::new();
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix('#') {
            if rows.is_empty() && rest.contains('\t') {
                header = rest.trim_start().split('\t').map(|s| s.trim().to_string()).collect();
            }
            continue;
        }
        let cells: Vec<String> = line.split('\t').map(|s| s.trim().to_string()).collect();
        if header.is_empty() {
            header = cells;
        } else {
            rows.push(cells);
        }
    }
    (header, rows)
}

fn col<'a>(header: &[String], row: &'a [String], name: &str) -> &'a str {
    header
        .iter()
        .position(|h| h == name)
        .and_then(|i| row.get(i))
        .map(|s| s.as_str())
        .unwrap_or("")
}

fn list(cell: &str) -> Vec<String> {
    cell.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()).map(String::from).collect()
}

/// Ordinary English words that a concept records as a name; alone they are
/// far more often something else (mirrors ORDINARY_WORDS in the names lint).
const ORDINARY: &[&str] = &[
    "segment", "reading", "stop", "sign", "mark", "dot", "place", "count", "number", "line", "page",
    "word", "letter", "root", "text", "character", "glyph", "translation", "position", "index",
    "order", "part", "unit", "quarter", "half", "eighth", "chain", "path", "way", "style", "pace",
    "school", "pos", "simple", "timing", "section", "group", "station", "portion", "pause",
    "conversion", "commentary", "assimilation", "concealment", "prolongation", "orthography",
    "vocalization", "transmitter", "para", "mad", "lin", "wid", "rub", "exegesis", "reciter",
];

impl Registry {
    pub fn load() -> Registry {
        let (h, rows) = table(AYAH_COUNTS);
        let mut counts = vec![[0u32; 6]; 114];
        for row in &rows {
            let surah: usize = col(&h, row, "surah").parse().unwrap_or(0);
            if (1..=114).contains(&surah) {
                for (i, sys) in SYSTEMS.iter().enumerate() {
                    counts[surah - 1][i] = col(&h, row, sys).parse().unwrap_or(0);
                }
            }
        }

        let (h, rows) = table(AYAH_NUMBERING);
        let mut totals = [0u32; 6];
        let mut also_reported: [Vec<u32>; 6] = Default::default();
        let mut numbering_alts = HashMap::new();
        for row in &rows {
            let code = col(&h, row, "code");
            if let Some(i) = SYSTEMS.iter().position(|s| *s == code) {
                totals[i] = col(&h, row, "total").parse().unwrap_or(0);
                also_reported[i] = list(col(&h, row, "also_reported"))
                    .iter()
                    .filter_map(|s| s.parse().ok())
                    .collect();
                for alt in list(col(&h, row, "alternative_spellings")) {
                    numbering_alts.insert(norm(&alt), code.to_string());
                }
                numbering_alts.insert(format!("ayah_numbering_{code}"), code.to_string());
                numbering_alts.insert(format!("{code}_numbering"), code.to_string());
            }
        }
        // Spellings the dictionary records for the systems (members.tsv in the names lint).
        for (alt, code) in [
            ("kufan", "kufi"), ("basran", "basri"), ("damascene", "dimashqi"), ("shami", "dimashqi"),
            ("makkan", "makki"), ("meccan", "makki"), ("madani_awwal", "madani_first"),
            ("madani_akhir", "madani_last"), ("first_madinan", "madani_first"),
            ("last_madinan", "madani_last"), ("medinan_first", "madani_first"),
            ("medinan_last", "madani_last"),
        ] {
            numbering_alts.entry(alt.to_string()).or_insert_with(|| code.to_string());
        }

        let (h, rows) = table(QIRAAT);
        let members = rows
            .iter()
            .map(|row| Member {
                kind: col(&h, row, "kind").to_string(),
                code: col(&h, row, "code").to_string(),
                parent: col(&h, row, "parent").to_string(),
                rawi: col(&h, row, "rawi").to_string(),
                numbering: col(&h, row, "numbering").to_string(),
                arabic: col(&h, row, "arabic").to_string(),
                alts: list(col(&h, row, "alternative_spellings")),
            })
            .collect();

        let (h, rows) = table(SAJDAH);
        let sajdah = rows
            .iter()
            .filter_map(|row| {
                let (s, a) = col(&h, row, "ayah").split_once(':')?;
                Some((s.parse().ok()?, a.parse().ok()?))
            })
            .collect();

        let _ = SURAHS; // kept for the registry check and future surah-name values

        let (h, rows) = table(CONCEPTS);
        let mut key_names = HashMap::new();
        for row in &rows {
            let code = col(&h, row, "code");
            for field in ["other_spellings", "english", "former_names"] {
                for alt in list(col(&h, row, field)) {
                    let alt = norm(&alt);
                    if !ORDINARY.contains(&alt.as_str()) && alt != code {
                        key_names.entry(alt).or_insert_with(|| code.to_string());
                    }
                }
            }
        }
        // Spellings the naming rules predict (ta marbutah, long vowels) for the
        // concepts most often found in data keys.
        for (alt, code) in [
            ("aya", "ayah"), ("ayat", "ayahs"), ("ayas", "ayahs"), ("sura", "surah"), ("suras", "surahs"),
            ("surat", "surah"), ("riwaya", "riwayah"), ("riwayat", "riwayahs"), ("riwayas", "riwayahs"),
            ("qiraa", "qiraah"), ("qiraat", "qiraahs"), ("qiraa_at", "qiraahs"), ("sajda", "sajdah"),
            ("basmala", "basmalah"), ("tajweed", "tajwid"), ("verses", "ayahs"), ("chapters", "surahs"),
            ("juzz", "juz"), ("ajza", "juzs"),
        ] {
            key_names.entry(alt.to_string()).or_insert_with(|| code.to_string());
        }

        Registry { counts, totals, also_reported, members, numbering_alts, sajdah, key_names }
    }

    pub fn member(&self, kind: &str, code: &str) -> Option<&Member> {
        self.members.iter().find(|m| m.kind == kind && m.code == code)
    }

    /// The numbering system conventionally paired with a riwayah's qiraah.
    pub fn numbering_of_riwayah(&self, riwayah: &str) -> Option<&str> {
        let r = self.member("riwayah", riwayah)?;
        let q = self.member("qiraah", &r.parent)?;
        (!q.numbering.is_empty()).then_some(q.numbering.as_str())
    }

    pub fn riwayahs_of(&self, field: fn(&Member) -> &str, value: &str) -> Vec<String> {
        self.members
            .iter()
            .filter(|m| m.kind == "riwayah" && field(m) == value)
            .map(|m| m.code.clone())
            .collect()
    }

    /// Arabic names (without diacritics) of riwayahs, rawis and qiraat.
    pub fn arabic_index(&self) -> HashMap<String, (String, String)> {
        let mut out = HashMap::new();
        for m in &self.members {
            out.insert(ar_norm(&m.arabic), (m.kind.clone(), m.code.clone()));
        }
        out
    }

    pub fn system_index(code: &str) -> Option<usize> {
        SYSTEMS.iter().position(|s| *s == code)
    }
}
