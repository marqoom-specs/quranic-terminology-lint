//! The names check, run as a user runs it: the binary in a project folder.

use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

const BIN: &str = env!("CARGO_BIN_EXE_quranic-terminology");

/// A throwaway project folder, outside any repository's `.terminology.json`.
struct Project(PathBuf);

struct Run {
    code: i32,
    out: String,
    err: String,
}

impl Project {
    fn new() -> Project {
        static N: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir()
            .join("quranic-terminology-tests")
            .join(format!("names-{}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Project(dir)
    }

    fn write(&self, name: &str, text: impl AsRef<[u8]>) -> &Project {
        let path = self.0.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
        self
    }

    fn config(&self, config: Value) -> &Project {
        self.write(".terminology.json", config.to_string())
    }

    fn read(&self, name: &str) -> String {
        std::fs::read_to_string(self.0.join(name)).unwrap()
    }

    fn run(&self, args: &[&str]) -> Run {
        let o = Command::new(BIN).args(args).current_dir(&self.0).output().unwrap();
        Run { code: o.status.code().unwrap_or(-1), out: String::from_utf8_lossy(&o.stdout).into(), err: String::from_utf8_lossy(&o.stderr).into() }
    }

    fn json(&self, args: &[&str]) -> (i32, Value) {
        let r = self.run(&[args, &["--json"]].concat());
        let value = serde_json::from_str(&r.out).unwrap_or_else(|e| panic!("{e}: {}{}", r.out, r.err));
        (r.code, value)
    }

    fn git(&self, args: &[&str]) {
        let ok = Command::new("git").args(args).current_dir(&self.0).output().unwrap().status.success();
        assert!(ok, "git {args:?}");
    }

    fn repo(&self, files: &[(&str, &str)]) -> &Project {
        self.git(&["init", "-q"]);
        self.git(&["config", "user.email", "t@example.invalid"]);
        self.git(&["config", "user.name", "T"]);
        for (name, text) in files {
            self.write(name, text);
        }
        self.git(&["add", "."]);
        self.git(&["commit", "-qm", "base", "--allow-empty"]);
        self
    }
}

/// Checks one file and returns the exit code and the JSON result.
fn check(name: &str, text: &str, config: Option<Value>) -> (i32, Value) {
    let p = Project::new();
    if let Some(c) = config {
        p.config(c);
    }
    p.write(name, text).json(&[name])
}

fn found(result: &Value) -> Vec<&str> {
    result["findings"].as_array().unwrap().iter().map(|f| f["found"].as_str().unwrap()).collect()
}

// --- what is reported ----------------------------------------------------------

#[test]
fn an_error_blocks_and_names_the_rule() {
    let (code, result) = check("model.py", "aya_number = 1\n", None);
    assert_eq!(code, 1);
    let f = &result["findings"][0];
    assert_eq!((&f["found"], &f["preferred"], &f["rules"]), (&json!("aya"), &json!("ayah"), &json!(["019"])));
    assert_eq!(f["message"], "rule 019: Ta marbutah outside idafah");
    assert_eq!(f["arabic"], "الآيَة");
    assert_eq!(result["findings"].as_array().unwrap().len(), 1);
}

#[test]
fn findings_give_the_current_arabic_name() {
    for (written, preferred, arabic) in [("ya_saghirah", "small_yaa", "اليَاء الفَارِسِيَّة"), ("alaqat_al_huruf", "letter_relation", "التَّجَاوُر")] {
        let (_, result) = check("model.py", &format!("{written} = 1\n"), None);
        let findings = result["findings"].as_array().unwrap();
        assert_eq!(findings.len(), 1, "{written}");
        assert_eq!((&findings[0]["preferred"], &findings[0]["arabic"]), (&json!(preferred), &json!(arabic)));
    }
}

#[test]
fn canonical_names_pass() {
    let (code, result) = check("model.py", "ayah = 1\nsurahs = []\nword_timing = 0\n", None);
    assert_eq!(code, 0);
    assert_eq!(found(&result), Vec::<&str>::new());
}

#[test]
fn warnings_do_not_block() {
    let (code, result) = check("model.py", "ayah_idx = 1\n", None);
    assert_eq!((code, &result["summary"]["warnings"]), (0, &json!(1)));
}

#[test]
fn allowances() {
    for (text, config, name) in [
        ("Tajweed and the verse of al-Fatihah", json!({}), "README.md"),
        ("sura = 1", json!({"compatibility": ["schema.sql"]}), "schema.sql"),
        ("vendor_sura_id = 1", json!({"external_names": ["vendor_sura_id"]}), "model.py"),
        ("sura = 1 # terminology: ignore", json!({}), "model.py"),
        ("verse = 1", json!({"allow_gloss_in": ["model.py"]}), "model.py"),
        ("sura = 1", json!({"ignore_words": ["sura"]}), "model.py"),
        ("# Load surah al-Fatihah, the first chapter\nsurah = 1", json!({}), "model.py"),
        ("label = \"Surah al-Fatihah\"", json!({}), "model.py"),
        ("const label = \"Chapter 1\"; // al-Fatihah", json!({}), "view.ts"),
        ("<h1>Chapter 1: al-Fatihah</h1>", json!({}), "page.html"),
    ] {
        assert_eq!(check(name, text, Some(config)).0, 0, "{text}");
    }
}

#[test]
fn a_gloss_exception_covers_compounds_but_not_misspellings() {
    let (code, result) = check("model.py", "verse_count = verse_timing = verseTimings = 1\naya = 2\n", Some(json!({"allow_gloss_in": ["model.py"]})));
    assert_eq!(code, 1);
    assert_eq!(result["summary"]["allowed_glosses"], 3);
    assert_eq!(found(&result), ["aya"]);
    let (code, result) = check("model.py", "verse_timing = 1\n", None);
    assert_eq!((code, &result["findings"][0]["preferred"]), (1, &json!("ayah_timing")));
}

#[test]
fn names_in_strings_markup_and_prose_are_still_checked() {
    for (text, name) in [
        ("row = {\"aya_number\": 1}", "model.py"),
        ("{\"sura\": 1}", "data.json"),
        ("<div class=\"sura-title\"></div>", "page.html"),
        ("# The aya text", "model.py"),
        ("The Sura list", "README.md"),
    ] {
        assert_eq!(check(name, text, None).0, 1, "{text}");
    }
}

#[test]
fn data_files_check_names_not_values() {
    for (text, name, code) in [
        ("surah_number,name\n1,Al-Fatihah\n2,al_baqarah\n", "surahs.csv", 0),
        ("sura_number,name\n1,Fatihah\n", "surahs.csv", 1),
        ("{\"surah_name\": \"al_baqarah\", \"list\": [\"sura\"]}", "data.json", 0),
        ("{\"sura_name\": \"Baqarah\"}", "data.json", 1),
    ] {
        assert_eq!(check(name, text, None).0, code, "{text}");
    }
}

#[test]
fn prose_transliteration_is_fine() {
    assert_eq!(check("README.md", "The riwāyāt of the Uthmānī mushaf", None).0, 0);
}

#[test]
fn compatibility_paths_are_reported_but_do_not_block() {
    let (code, result) = check("legacy/schema.sql", "sura = 1", Some(json!({"compatibility": ["legacy/**"]})));
    assert_eq!(code, 0);
    assert_eq!(result["summary"]["known"], 1);
    assert_eq!(result["findings"][0]["known"], true);
    assert_eq!(check("db/schema.sql", "sura = 1", Some(json!({"compatibility": ["legacy/**"]}))).0, 1);
}

#[test]
fn long_canonical_names_pass_and_deprecated_ones_fail() {
    for name in ["waqf_jaiz_mustawi_al_tarafayn", "waqfJaizMustawiAlTarafayn"] {
        let (code, result) = check("model.py", &format!("{name} = 1"), None);
        assert_eq!((code, found(&result).len()), (0, 0));
    }
    let (code, result) = check("model.py", "waqf_jaiz_mustawi_al_tarafayn = 1\nwaqf_jaiz = 2", None);
    assert_eq!(code, 1);
    assert_eq!(result["findings"].as_array().unwrap().len(), 1);
    assert_eq!((&result["findings"][0]["rules"], &result["findings"][0]["line"]), (&json!(["046"]), &json!(2)));
}

// --- references: file names and URLs are not names --------------------------------

#[test]
fn references_do_not_hide_their_neighbours() {
    for (text, name) in [
        ("// source: quran-tajweed uthmani-hafs.json 2:26\nsura = 1", "fixture.rs"),
        ("{\"source\": \"quran-tajweed uthmani-hafs.json 2:26\", \"aya\": 1}", "fixture.json"),
        ("| source | quran-tajweed uthmani-hafs.json 2:26 |\naya", "notes.md"),
        ("source = \"https://example.org/sura/aya.json\"\naya = 1", "model.py"),
        ("source = \"uthmani-hafs.json\"; aya = 1", "model.py"),
        ("See [source](https://example.org/uthmani-hafs.json).\naya", "notes.md"),
    ] {
        let (code, result) = check(name, text, None);
        assert_eq!(code, 1, "{text}");
        assert_eq!(result["summary"]["reference_literals"], 1, "{text}");
        let errors: Vec<&Value> = result["findings"].as_array().unwrap().iter().filter(|f| f["severity"] == "error").collect();
        assert_eq!(errors.len(), 1, "{text}");
        assert!(["aya", "sura"].contains(&errors[0]["found"].as_str().unwrap()));
        assert_eq!(errors[0]["line"], if text.contains('\n') { 2 } else { 1 });
        assert!(!found(&result).contains(&"uthmani"));
    }
}

#[test]
fn names_that_are_not_references_still_fail() {
    for text in ["field = \"aya\"", "object.aya = 1", "aya.json = 1"] {
        let (code, result) = check("model.py", text, None);
        assert_eq!((code, &result["summary"]["reference_literals"]), (1, &json!(0)), "{text}");
    }
}

#[test]
fn an_external_name_stays_whole() {
    let (code, result) = check("model.py", "source = \"uthmani-hafs.json sura\"\naya = 1", Some(json!({"external_names": ["uthmani-hafs\\.json sura"]})));
    assert_eq!(code, 1);
    assert_eq!(result["summary"]["external_names"], 1);
    assert_eq!(found(&result), ["aya"]);
}

#[test]
fn python_strings_and_comments_are_lexed() {
    for (text, line) in [
        ("source = \"\"\"\nuthmani-hafs.json\n\"\"\"\naya = 1", 4),
        ("source = \"a \\\"quote\\\" uthmani-hafs.json\"\naya = 1", 2),
        ("# source: uthmani-hafs.json\naya = 1", 2),
    ] {
        let (code, result) = check("model.py", text, None);
        assert_eq!((code, &result["summary"]["reference_literals"]), (1, &json!(1)), "{text}");
        let places: Vec<(&str, i64)> = result["findings"].as_array().unwrap().iter().map(|f| (f["found"].as_str().unwrap(), f["line"].as_i64().unwrap())).collect();
        assert_eq!(places, [("aya", line)], "{text}");
    }
}

#[test]
fn python_code_and_unparsed_source_get_no_exemptions() {
    for (text, name) in [
        ("url = f\"https://example.org/{sura}.json\"", "sura"),
        ("url = rf\"https://example.org/{aya.json}\"", "aya"),
        ("metadata = {\"source\": True}; aya.json = 1", "aya"),
        ("source = \"\"\"unterminated\naya.json", "aya"),
        ("broken = \"\nsource = \"aya.json\"", "aya"),
        ("url = f\"https://example.org/{sura:{aya}}\"", "aya"),
    ] {
        let (code, result) = check("model.py", text, None);
        assert_eq!((code, &result["summary"]["reference_literals"]), (1, &json!(0)), "{text}");
        assert!(found(&result).contains(&name), "{text}");
    }
}

#[test]
fn a_one_line_json_file_is_quick() {
    let data: serde_json::Map<String, Value> = (1..60)
        .flat_map(|s| (1..100).map(move |a| (format!("{s}:{a}"), json!({"surah_number": s, "ayah_number": a}))))
        .collect();
    let p = Project::new();
    p.write("timings.json", Value::Object(data).to_string());
    let started = std::time::Instant::now();
    assert_eq!(p.json(&["timings.json"]).0, 0);
    assert!(started.elapsed().as_secs_f64() < 3.0);
}

// --- which files ---------------------------------------------------------------

#[test]
fn excluded_paths_are_skipped_however_they_are_named() {
    let p = Project::new();
    p.write("src/a space.py", "sura = 1").config(json!({"exclude": ["src/**"]}));
    for args in [["src"], ["src/a space.py"]] {
        let (code, result) = p.json(&args);
        assert_eq!((code, &result["summary"]["files_scanned"]), (0, &json!(0)));
    }
}

#[test]
fn a_file_given_twice_is_read_once() {
    let p = Project::new();
    p.write("a space.py", "sura = 1");
    let (code, result) = p.json(&["a space.py", ".", "."]);
    assert_eq!((code, &result["summary"]["files_scanned"]), (1, &json!(1)));
}

#[test]
fn the_configuration_is_found_above_and_can_be_overridden() {
    let p = Project::new();
    p.write("src/a.py", "sura = 1").write("child/ok.py", "ayah = 1").config(json!({"paths": ["src"]}));
    let child = Project(p.0.join("child"));
    assert_eq!(child.json(&[]).0, 1);
    assert_eq!(child.json(&["ok.py"]).0, 0);
}

#[test]
fn invalid_configurations_stop_the_check() {
    for config in [json!([]), json!({"paths": "src"}), json!({"exclude": [3]}), json!({"unknown": []}), json!({"external_names": ["("]}), json!({"data": ["x"]})] {
        let p = Project::new();
        p.config(config.clone());
        assert_eq!(p.run(&[]).code, 2, "{config}");
    }
}

#[test]
fn the_data_section_and_comments_are_left_alone() {
    for config in [json!({"data": {"publishes": ["exports/**"]}}), json!({"_comment": "why", "$schema": "x"})] {
        let p = Project::new();
        p.config(config).write("a.py", "ayah = 1");
        assert_eq!(p.run(&["a.py"]).code, 0);
    }
}

#[test]
fn missing_inputs_are_errors() {
    let p = Project::new();
    assert_eq!(p.run(&["missing.py"]).code, 2);
    assert_eq!(p.run(&["--config", "missing.json"]).code, 2);
}

#[cfg(unix)]
#[test]
fn tool_folders_symlinks_and_virtualenvs_are_skipped() {
    let p = Project::new();
    p.write("node_modules/bad.py", "sura = 1").write("bad.bin", "sura");
    std::os::unix::fs::symlink("node_modules/bad.py", p.0.join("link.py")).unwrap();
    std::os::unix::fs::symlink("missing", p.0.join("broken.py")).unwrap();
    let (code, result) = p.json(&[".", "node_modules/bad.py", "link.py", "broken.py"]);
    assert_eq!((code, &result["summary"]["files_scanned"]), (0, &json!(0)));

    let venv = Project::new();
    venv.write("env-ci/pyvenv.cfg", "home = /usr").write("env-ci/lib/bad.py", b"\xff");
    assert_eq!(venv.json(&["."]).0, 0);

    let alias = Project::new();
    alias.write("real/model.py", "aya_number = 1");
    std::os::unix::fs::symlink("real", alias.0.join("alias")).unwrap();
    assert_eq!(alias.json(&[alias.0.join("alias/model.py").to_str().unwrap()]).0, 1);
}

#[test]
fn nothing_to_check_is_noted() {
    let p = Project::new();
    p.config(json!({"exclude": ["model.py"]})).write("model.py", "aya_number = 1");
    let r = p.run(&["model.py"]);
    assert_eq!(r.code, 0);
    assert!(r.err.contains("no files to check"));
}

#[test]
fn large_files_are_skipped() {
    let p = Project::new();
    p.write("large.py", "sura ".repeat(400_001));
    let (code, result) = p.json(&["large.py"]);
    assert_eq!((code, result["summary"]["skipped_large_files"].as_array().unwrap().len()), (0, 1));
}

#[test]
fn unreadable_files_are_errors() {
    let p = Project::new();
    p.write("bad.py", b"\xff");
    let r = p.run(&["."]);
    assert_eq!(r.code, 2);
    assert!(r.err.contains("bad.py: not UTF-8"), "{}", r.err);

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let p = Project::new();
        p.write("locked.py", "ayah = 1");
        std::fs::set_permissions(p.0.join("locked.py"), std::fs::Permissions::from_mode(0o000)).unwrap();
        if std::fs::read(p.0.join("locked.py")).is_err() {
            assert_eq!(p.run(&["locked.py"]).code, 2); // Running as root can read it anyway.
        }
    }
}

// --- reports ---------------------------------------------------------------------

#[test]
fn the_table_groups_names() {
    let p = Project::new();
    for n in 0..3 {
        p.write(&format!("model{n}.py"), "aya = 1\nsura = 2\nayah_idx = 3\n");
    }
    let r = p.run(&[".", "--format", "table"]);
    assert_eq!(r.code, 1);
    assert!(r.out.contains("3 files · 6 errors · 3 warnings"), "{}", r.out);
    assert!(r.out.lines().any(|l| l.split_whitespace().collect::<Vec<_>>() == ["aya", "ayah", "019", "3"]));
    assert!(r.out.find("Errors").unwrap() < r.out.find("Warnings (they don't block)").unwrap());
    assert!(!r.out.contains("model0"));
}

#[test]
fn the_table_is_limited_and_json_is_complete() {
    let p = Project::new();
    p.write("model.py", "aya = sura = basmala = 1");
    let r = p.run(&[".", "--format", "table", "--limit", "1"]);
    assert_eq!(r.code, 1);
    assert!(r.out.contains("Showing 1 of 3"));
    let (code, result) = p.json(&[".", "--limit", "1"]);
    assert_eq!((code, result["findings"].as_array().unwrap().len()), (1, 3));

    let clean = Project::new();
    clean.write("model.py", "ayah = 1");
    let r = clean.run(&[".", "--format", "table"]);
    assert_eq!(r.code, 0);
    assert!(r.out.contains("0 errors · 0 warnings") && !r.out.contains("Found"));
}

#[test]
fn by_file_shows_each_place() {
    let p = Project::new();
    p.write("model.py", "aya_number = 1\naya = 2 # aya\n");
    let r = p.run(&["model.py", "--format", "text"]);
    assert_eq!(r.code, 1);
    assert!(r.out.contains("model.py:1: error aya_number → ayah_number (rule 019: Ta marbutah outside idafah)"), "{}", r.out);
    assert!(r.out.contains("model.py:2: error aya → ayah ×2"));
    assert!(p.run(&["model.py", "--format", "text", "--limit", "1"]).out.contains("… and 1 more"));
}

#[test]
fn the_report_shows_the_whole_corrected_name() {
    let p = Project::new();
    p.write("model.ts", "const getSuraNo = 1;\n");
    assert!(p.run(&["model.ts"]).out.contains("model.ts:1: error getSuraNo → getSurahNumber"));
    let (_, result) = p.json(&["model.ts"]);
    assert!(result["findings"].as_array().unwrap().iter().all(|f| f["suggested_identifier"] == "getSurahNumber"));
    let (_, result) = check("model.py", "AYA_AYA = 1\n", None);
    assert_eq!(result["findings"][0]["suggested_identifier"], "AYAH_AYAH");
}

#[test]
fn the_default_report_is_bounded_but_json_is_not() {
    let p = Project::new();
    p.write("model.py", "aya = 1\n".repeat(25));
    let r = p.run(&["model.py"]);
    assert_eq!(r.code, 1);
    assert!(r.out.contains("… and 5 more") && !r.out.contains("model.py:21:"));
    assert_eq!(p.json(&["model.py"]).1["findings"].as_array().unwrap().len(), 25);
}

// --- fixes -----------------------------------------------------------------------

#[test]
fn checking_never_writes() {
    let source = "# the aya list\naya_number = 1\n";
    for args in [vec![], vec!["--json"], vec!["--format", "table"]] {
        let p = Project::new();
        p.write("a.py", source);
        assert_eq!(p.run(&[&["a.py"], &args[..]].concat()).code, 1);
        assert_eq!(p.read("a.py"), source);
    }
    for (name, source) in [
        ("model.py", "ayah = 100\naya = 2\nprint(ayah, aya)\n"),
        ("model.py", "from quran import Chapters\nprint(Chapters.__name__)\n"),
        ("model.ts", "const url = \"https://example.com\"; const aya = 1;\nconsole.log(aya);\n"),
        ("model.ts", "/* note */ const aya = 1;\nconsole.log(aya);\n"),
        ("model.ts", "const text = `\naya\n// the sura list\n`;\n"),
        ("model.xml", "<!-- Tanzil Quran Text (Uthmani) -->\n<aya />\n"),
        ("model.mdx", "export const aya = 1;\n\nThe Sura list\n"),
    ] {
        let p = Project::new();
        p.write(name, source);
        assert_eq!(p.run(&[name]).code, 1, "{source}");
        assert_eq!(p.read(name), source);
    }
}

#[test]
fn an_unknown_option_stops_before_writing() {
    for args in [vec!["--fix"], vec!["--fix", "--unsafe-fixes"]] {
        let p = Project::new();
        p.write("a.py", "# the aya list\naya_number = 1\n");
        let r = p.run(&[&["a.py"], &args[..]].concat());
        assert_eq!(r.code, 2);
        assert!(r.err.contains("unknown option --fix"), "{}", r.err);
        assert_eq!(p.read("a.py"), "# the aya list\naya_number = 1\n");
    }
}

#[test]
fn unsafe_fixes_keep_the_case_style() {
    let p = Project::new();
    p.repo(&[("old.ts", "export const riwaya = 1;\n")]);
    p.write("a.ts", "const ayaNo = getSuraName(x); // the aya list\nconst AYA_NO = 1;\n");
    assert_eq!(p.run(&["a.ts", "--unsafe-fixes"]).code, 1);
    assert_eq!(p.read("a.ts"), "const ayahNumber = getSurahName(x); // the ayah list\nconst AYAH_NUMBER = 1;\n");
    assert_eq!(p.run(&["a.ts"]).code, 0);

    let lib = Project::new();
    lib.repo(&[("lib.ts", "export function getSuraName() {}\n")]);
    assert!(lib.read("lib.ts").contains("getSuraName"));
    lib.run(&["lib.ts", "--unsafe-fixes"]);
    assert_eq!(lib.read("lib.ts"), "export function getSurahName() {}\n");
}

#[test]
fn unsafe_fixes_report_their_edits_and_settle() {
    let p = Project::new();
    p.write("a.py", "aya_no = 1  # the aya\n");
    let (code, result) = p.json(&["a.py", "--unsafe-fixes"]);
    assert_eq!(code, 1);
    assert_eq!(p.read("a.py"), "ayah_number = 1  # the ayah\n");
    assert_eq!((&result["summary"]["fixed_names"], &result["summary"]["fixed_files"]), (&json!(2), &json!(1)));
    let (code, again) = p.json(&["a.py", "--unsafe-fixes"]);
    assert_eq!((code, &again["summary"]["fixed_names"]), (0, &json!(0)));
    assert!(p.write("b.py", "aya = 1\n").run(&["b.py", "--unsafe-fixes"]).out.contains("Fixed 1 name in 1 file."));
}

#[test]
fn unsafe_fixes_edit_prose_but_not_code_in_documents() {
    let p = Project::new();
    p.write("README.md", "The Sura list.\n\nCall `get_sura()`:\n\n```\nsura = 1\n```\n").write("a.py", "x = 1  # the aya and `aya_id`\n");
    p.run(&[".", "--unsafe-fixes"]);
    assert_eq!(p.read("README.md"), "The Surah list.\n\nCall `get_sura()`:\n\n```\nsura = 1\n```\n");
    assert_eq!(p.read("a.py"), "x = 1  # the ayah and `aya_id`\n");
    for source in [
        "````python\n```\naya = 1\n````\n",
        "```python\n~~~\naya = 1\n```\n",
        "    aya = 1\n",
        "> aya = 1\n",
        "Use ``aya = 1`` here.\n",
        "---\nname: aya\n---\n",
        "<script>\nconst aya = 1;\n</script>\n",
    ] {
        let p = Project::new();
        p.write("README.md", source).run(&["README.md", "--unsafe-fixes"]);
        assert_eq!(p.read("README.md"), source);
    }
}

#[test]
fn unsafe_fixes_never_touch_strings_data_warnings_or_compatibility() {
    let p = Project::new();
    let source = "import { x } from \"./findRubAlHizb\";\nname = {\"sura_id\": \"Douri\"};\nayah_idx = 1\n";
    p.write("a.ts", source).write("d.json", "{\"sura\": 1}\n").write("legacy.py", "sura = 1\n").config(json!({"compatibility": ["legacy.py"]}));
    p.run(&[".", "--unsafe-fixes"]);
    assert_eq!((p.read("a.ts"), p.read("d.json"), p.read("legacy.py")), (source.into(), "{\"sura\": 1}\n".into(), "sura = 1\n".into()));

    let quotes = "  [\"Al-Jumu'a\", \"Friday\"],\n  label = 'the \"sura\" list'\n";
    let p = Project::new();
    p.write("names.ts", quotes).run(&["names.ts", "--unsafe-fixes"]);
    assert_eq!(p.read("names.ts"), quotes);
}

#[test]
fn unsafe_fixes_decline_syntax_they_cannot_read() {
    for (name, source) in [
        ("model.ts", "const data = `\n// aya\n`;\nconst sura = 1;\n"),
        ("model.ts", "const pattern = /aya/;\nconst sura = 1;\n"),
        ("model.ts", "const data = \"unterminated\naya\n"),
        ("model.xml", "<!-- Tanzil Quran Text (Uthmani) -->\n<aya />\n"),
        ("model.mdx", "export const aya = 1;\n\nThe Sura list\n"),
        ("model.tsx", "const aya = <div>sura</div>;\n"),
        ("model.py", "\"\"\"unterminated\n# the aya\n"),
    ] {
        let p = Project::new();
        p.write(name, source).run(&[name, "--unsafe-fixes"]);
        assert_eq!(p.read(name), source, "{source}");
    }
}

#[test]
fn unsafe_fixes_keep_line_endings() {
    let p = Project::new();
    p.write("model.py", b"# the aya\r\nayah = 1\r\n").run(&["model.py", "--unsafe-fixes"]);
    assert_eq!(p.read("model.py"), "# the ayah\r\nayah = 1\r\n");
}

#[test]
fn a_git_rename_keeps_existing_names() {
    let p = Project::new();
    p.repo(&[("old.py", "sura = 1\n")]);
    p.git(&["mv", "old.py", "new.py"]);
    assert_eq!(p.run(&["new.py"]).code, 1);
    assert_eq!(p.read("new.py"), "sura = 1\n");
}
