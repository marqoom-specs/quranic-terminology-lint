//! What a check reports.

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    Error,
    Warning,
    Note,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Note => "note",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Group {
    /// Ayah numbers, riwayahs and the like that a consumer would misread.
    Meaning,
    /// Values that do not resolve to a registry code.
    Values,
    /// Non-canonical names of fields, columns and tables.
    Names,
}

#[derive(Debug, Clone)]
pub struct Finding {
    pub severity: Severity,
    pub group: Group,
    pub code: &'static str,
    pub file: String,
    pub line: Option<usize>,
    /// JSON path, column or schema location.
    pub at: String,
    pub message: String,
    pub fix: Option<String>,
    pub basis: &'static str,
    pub count: usize,
    /// For a value finding: the value, and the code it most likely means.
    pub value: Option<(String, Option<String>)>,
}

/// One body of Quran data found in a file, with what was learned about it.
#[derive(Debug, Clone)]
pub struct Inventory {
    pub file: String,
    pub at: String,
    pub shape: String,
    pub surahs: usize,
    pub ayahs: u32,
    /// The systems that fit best, how many surahs they fit, out of how many compared.
    pub numbering: Vec<String>,
    /// For each system, in registry order, the surahs whose count it fits.
    pub fits: Vec<usize>,
    pub matched: usize,
    pub compared: usize,
    pub stated_riwayah: Option<String>,
    pub stated_numbering: Option<String>,
    pub hint: Option<String>,
    /// What the sajdah marks in the text show, when the text has them.
    pub sajdah: Option<SajdahMarks>,
}

/// Sajdah marks in a text, compared with the registry's places.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SajdahMarks {
    pub marked: usize,
    /// Marks at the registry's places, in Kufi text.
    pub at_places: usize,
    /// In text not numbered in Kufi: surahs with as many marks as the
    /// registry has places. Their ayah numbers can't be compared.
    pub other_numbering: Vec<u32>,
    pub other_marks: usize,
    /// Surahs where the marks differ from the registry.
    pub differ: Vec<u32>,
}

/// A surah where data that states its numbering disagrees with the registry.
#[derive(Debug, Clone)]
pub struct Evidence {
    pub system: String,
    pub surah: u32,
    pub registry: u32,
    pub data: u32,
    pub file: String,
}

#[derive(Debug, Default)]
pub struct FileResult {
    pub file: String,
    /// A hash of the file's bytes: identical files are reported once.
    pub digest: u64,
    pub findings: Vec<Finding>,
    pub inventory: Vec<Inventory>,
    pub evidence: Vec<Evidence>,
    pub bytes: u64,
}

/// Order of importance within a severity: what makes a consumer read the
/// wrong ayah comes first, names last.
pub fn rank(code: &str) -> u8 {
    match code {
        "numbering-contradicts-statement" => 0,
        "numbering-unstated-not-kufi" => 1,
        "value-unknown" | "value-ambiguous" | "value-qiraah-as-riwayah" => 2,
        "numbering-renumbered" => 3,
        "numbering-not-conventional" => 4,
        "recitation-without-riwayah" => 5,
        "basmalah-attached" | "basmalah-in-first-ayah" => 6,
        "word-keys-unstated" => 7,
        "value-rawi" | "value-alternative" => 8,
        "word-order-only" => 9,
        "sajdah-differs" => 10,
        "counts-differ-from-registry" => 11,
        "name-noncanonical" => 30,
        _ => 20,
    }
}

/// A short name for a kind of finding, for the summary.
pub fn title(code: &str) -> Option<&'static str> {
    Some(match code {
        "numbering-contradicts-statement" => "ayah counts that contradict the numbering the data states",
        "numbering-unstated-not-kufi" => "ayah numbers that aren't Kufi, with nothing saying so",
        "numbering-renumbered" => "renumbered to Kufi without saying so",
        "numbering-not-conventional" => "a numbering that isn't the riwayah's usual one, unstated",
        "numbering-unstated" => "riwayah and numbering unstated (read as Kufi)",
        "counts-differ-from-registry" => "ayah counts that differ from the registry in a few surahs",
        "recitation-without-riwayah" => "recitations without the riwayah of their audio",
        "basmalah-attached" => "the basmalah joined to 1:1",
        "basmalah-in-first-ayah" => "the basmalah inside first ayahs",
        "word-keys-unstated" => "word keys without the text and segmentation they count by",
        "ayah-keys-unstated" => "ayah keys without a stated numbering",
        "word-order-only" => "words identified only by their order",
        "sajdah-differs" => "sajdah marks that differ from the registry",
        "name-noncanonical" => "names the dictionary gives differently",
        "value-unknown" => "values that aren't registry codes",
        "value-ambiguous" => "rawi names that fit more than one riwayah",
        "value-qiraah-as-riwayah" => "a qiraah where a riwayah is meant",
        "value-rawi" => "a rawi where a riwayah is meant",
        "value-alternative" => "recorded spellings where other projects expect the code",
        _ => return None,
    })
}

/// A fix that holds for every finding of a kind, when their own fixes differ.
pub fn general_fix(code: &str) -> Option<&'static str> {
    Some(match code {
        "numbering-unstated-not-kufi" | "numbering-not-conventional" | "numbering-renumbered" => "state `riwayah` (or `ayah_numbering_system`) in each file",
        "numbering-contradicts-statement" => "correct the stated numbering, or the data",
        "value-unknown" | "value-alternative" | "value-rawi" | "value-ambiguous" | "value-qiraah-as-riwayah" => "write the registry codes",
        _ => return None,
    })
}

/// The line of the first occurrence of `needle` in `text`, 1-based.
pub fn line_of(text: &str, needle: &str) -> Option<usize> {
    let i = text.find(needle)?;
    Some(text[..i].bytes().filter(|b| *b == b'\n').count() + 1)
}
