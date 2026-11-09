import json
from pathlib import Path
import socket
import subprocess

import pytest

from quranic_terminology_lint import cli


@pytest.fixture(autouse=True)
def isolated(tmp_path, monkeypatch):
    monkeypatch.chdir(tmp_path)


def run(capsys, text, config=None, name="model.py"):
    Path(name).parent.mkdir(parents=True, exist_ok=True)
    Path(name).write_text(text)
    if config is not None:
        Path(".terminology.json").write_text(json.dumps(config))
    status = cli.main([name, "--json"])
    return status, json.loads(capsys.readouterr().out)


def test_error_blocks_and_names_the_rule(capsys):
    status, result = run(capsys, "aya_number = 1\n")
    assert status == 1
    [finding] = result["findings"]
    assert (finding["found"], finding["preferred"], finding["rules"]) == ("aya", "ayah", ["019"])
    assert finding["message"] == "rule 019: Ta marbutah outside idafah"
    assert finding["arabic"] == "الآيَة"



@pytest.mark.parametrize("found, preferred, arabic", [
    ("ya_saghirah", "small_yaa", "اليَاء الفَارِسِيَّة"),
    ("alaqat_al_huruf", "letter_relation", "التَّجَاوُر"),
])
def test_findings_give_the_current_arabic_name(capsys, found, preferred, arabic):
    _, result = run(capsys, f"{found} = 1\n")
    [finding] = result["findings"]
    assert (finding["preferred"], finding["arabic"]) == (preferred, arabic)

def test_clean_and_offline(capsys, monkeypatch):
    def network_forbidden(*args, **kwargs):
        raise AssertionError("network accessed")
    monkeypatch.setattr(socket, "socket", network_forbidden)
    status, result = run(capsys, "ayah = 1\nsurahs = []\nword_timing = 0\n")
    assert status == 0 and not result["findings"]


def test_warnings_do_not_block(capsys):
    status, result = run(capsys, "ayah_idx = 1\n")
    assert status == 0 and result["summary"]["warnings"] == 1


@pytest.mark.parametrize("text,config,name", [
    ("Tajweed and the verse of al-Fatihah", {}, "README.md"),
    ("sura = 1", {"compatibility": ["schema.sql"]}, "schema.sql"),
    ("vendor_sura_id = 1", {"external_names": ["vendor_sura_id"]}, "model.py"),
    ("sura = 1 # terminology: ignore", {}, "model.py"),
    ("verse = 1", {"allow_gloss_in": ["model.py"]}, "model.py"),
    ("sura = 1", {"ignore_words": ["sura"]}, "model.py"),
    ("# Load surah al-Fatihah, the first chapter\nsurah = 1", {}, "model.py"),
    ('label = "Surah al-Fatihah"', {}, "model.py"),
    ('const label = "Chapter 1"; // al-Fatihah', {}, "view.ts"),
    ("<h1>Chapter 1: al-Fatihah</h1>", {}, "page.html"),
])
def test_allowances(capsys, text, config, name):
    assert run(capsys, text, config, name)[0] == 0


def test_gloss_exception_includes_compounds_but_not_misspellings(capsys):
    status, result = run(capsys, "verse_count = verse_timing = verseTimings = 1\naya = 2\n",
                         {"allow_gloss_in": ["model.py"]})
    assert status == 1
    assert result["summary"]["allowed_glosses"] == 3
    assert [f["found"] for f in result["findings"]] == ["aya"]


def test_gloss_compound_blocks_outside_exception(capsys):
    status, result = run(capsys, "verse_timing = 1\n")
    assert status == 1
    assert result["findings"][0]["preferred"] == "ayah_timing"


@pytest.mark.parametrize("text,name", [
    ('row = {"aya_number": 1}', "model.py"),
    ('{"sura": 1}', "data.json"),
    ('<div class="sura-title"></div>', "page.html"),
    ("# The aya text", "model.py"),
    ("The Sura list", "README.md"),
])
def test_names_in_strings_markup_and_prose_still_checked(capsys, text, name):
    assert run(capsys, text, None, name)[0] == 1


def test_compatibility_is_reported_but_does_not_block(capsys):
    status, result = run(capsys, "sura = 1", {"compatibility": ["legacy/**"]}, "legacy/schema.sql")
    assert status == 0
    assert result["summary"]["known"] == 1 and result["findings"][0]["known"]
    # Only listed paths are exempt; other schema files still block.
    status, _ = run(capsys, "sura = 1", {"compatibility": ["legacy/**"]}, "db/schema.sql")
    assert status == 1


def test_selection_parity(capsys):
    Path("src").mkdir()
    Path("src/a space.py").write_text("sura = 1")
    Path(".terminology.json").write_text(json.dumps({"exclude": ["src/**"]}))
    for args in (["src"], ["src/a space.py"]):
        assert cli.main([*args, "--json"]) == 0
        assert json.loads(capsys.readouterr().out)["summary"]["files_scanned"] == 0


def test_dedup(capsys):
    Path("a space.py").write_text("sura = 1")
    assert cli.main(["a space.py", ".", ".", "--json"]) == 1
    assert json.loads(capsys.readouterr().out)["summary"]["files_scanned"] == 1


def test_config_relative_and_explicit_override(capsys, monkeypatch):
    Path("src").mkdir()
    Path("src/a.py").write_text("sura = 1")
    Path("child").mkdir()
    Path("child/ok.py").write_text("ayah = 1")
    Path(".terminology.json").write_text(json.dumps({"paths": ["src"]}))
    monkeypatch.chdir("child")
    assert cli.main(["--json"]) == 1
    capsys.readouterr()
    assert cli.main(["ok.py", "--json"]) == 0


@pytest.mark.parametrize("config", [[], {"paths": "src"}, {"exclude": [3]},
                                    {"unknown": []}, {"external_names": ["("]}])
def test_invalid_config(config):
    Path(".terminology.json").write_text(json.dumps(config))
    assert cli.main([]) == 2


def test_comment_keys_are_allowed(capsys):
    Path(".terminology.json").write_text('{"_comment": "why", "$schema": "x"}')
    Path("a.py").write_text("ayah = 1")
    assert cli.main(["a.py"]) == 0


def test_missing_inputs():
    assert cli.main(["missing.py"]) == 2
    assert cli.main(["--config", "missing.json"]) == 2


def test_skips(capsys):
    Path("node_modules").mkdir()
    Path("node_modules/bad.py").write_text("sura = 1")
    Path("bad.bin").write_bytes(b"sura")
    Path("link.py").symlink_to("node_modules/bad.py")
    Path("broken.py").symlink_to("missing")
    assert cli.main([".", "node_modules/bad.py", "link.py", "broken.py", "--json"]) == 0
    assert json.loads(capsys.readouterr().out)["summary"]["files_scanned"] == 0


def test_symlinked_ancestor_is_scanned(capsys, tmp_path):
    Path("real").mkdir()
    Path("real/model.py").write_text("aya_number = 1")
    Path("alias").symlink_to("real")
    assert cli.main([str(tmp_path / "alias/model.py"), "--json"]) == 1


def test_nothing_checked_is_noted(capsys):
    Path(".terminology.json").write_text(json.dumps({"exclude": ["model.py"]}))
    Path("model.py").write_text("aya_number = 1")
    assert cli.main(["model.py"]) == 0
    assert "no eligible files" in capsys.readouterr().err


def test_size_limit(capsys):
    Path("large.py").write_text("sura " * 400001)
    assert cli.main(["large.py", "--json"]) == 0
    assert len(json.loads(capsys.readouterr().out)["summary"]["skipped_large_files"]) == 1


def test_read_failure(monkeypatch):
    Path("bad.py").write_text("ayah = 1")
    original = Path.read_text
    def denied(path, *args, **kwargs):
        if path.name == "bad.py":
            raise PermissionError("denied")
        return original(path, *args, **kwargs)
    monkeypatch.setattr(Path, "read_text", denied)
    assert cli.main(["bad.py"]) == 2


def test_invalid_utf8():
    Path("bad.py").write_bytes(b"\xff")
    assert cli.main(["bad.py"]) == 2


def test_table(capsys):
    for number in range(3):
        Path(f"model{number}.py").write_text("aya = 1\nsura = 2\nayah_idx = 3\n")
    assert cli.main([".", "--by", "table"]) == 1
    output = capsys.readouterr().out
    assert "3 files · 6 errors · 3 warnings" in output
    lines = [line.split() for line in output.splitlines()]
    assert ["aya", "ayah", "019", "3"] in lines
    assert output.index("Errors") < output.index("Warnings (do not block)")
    assert "model0" not in output


def test_table_limit_and_complete_json(capsys):
    Path("model.py").write_text("aya = sura = basmala = 1")
    assert cli.main([".", "--by", "table", "--limit", "1"]) == 1
    assert "Showing 1 of 3" in capsys.readouterr().out
    assert cli.main([".", "--limit", "1", "--json"]) == 1
    assert len(json.loads(capsys.readouterr().out)["findings"]) == 3


def test_clean_table_has_no_empty_headers(capsys):
    Path("model.py").write_text("ayah = 1")
    assert cli.main([".", "--by", "table"]) == 0
    output = capsys.readouterr().out
    assert "0 errors · 0 warnings" in output and "Found" not in output


def test_by_file(capsys):
    Path("model.py").write_text("aya_number = 1\naya = 2 # aya\n")
    assert cli.main(["model.py", "--by", "file"]) == 1
    output = capsys.readouterr().out
    assert "model.py:1: error aya_number → ayah_number (rule 019: Ta marbutah outside idafah)" in output
    assert "model.py:2: error aya → ayah ×2" in output
    assert cli.main(["model.py", "--by", "file", "--limit", "1"]) == 1
    assert "… and 1 more" in capsys.readouterr().out


def test_default_report_shows_location_and_full_replacement(capsys):
    Path("model.ts").write_text("const getSuraNo = 1;\n")
    assert cli.main(["model.ts"]) == 1
    assert "model.ts:1: error getSuraNo → getSurahNumber" in capsys.readouterr().out
    assert cli.main(["model.ts", "--json"]) == 1
    findings = json.loads(capsys.readouterr().out)["findings"]
    assert all(f["suggested_identifier"] == "getSurahNumber" for f in findings)


def test_suggestion_corrects_repeated_parts_and_preserves_case(capsys):
    _, result = run(capsys, "AYA_AYA = 1\n")
    assert result["findings"][0]["suggested_identifier"] == "AYAH_AYAH"


def test_default_report_is_bounded_without_truncating_json(capsys):
    Path("model.py").write_text("aya = 1\n" * 25)
    assert cli.main(["model.py"]) == 1
    output = capsys.readouterr().out
    assert "… and 5 more" in output
    assert "model.py:21:" not in output
    assert cli.main(["model.py", "--json"]) == 1
    assert len(json.loads(capsys.readouterr().out)["findings"]) == 25


@pytest.mark.parametrize("text,name,status", [
    ("surah_number,name\n1,Al-Fatihah\n2,al_baqarah\n", "surahs.csv", 0),
    ("sura_number,name\n1,Fatihah\n", "surahs.csv", 1),
    ('{"surah_name": "al_baqarah", "list": ["sura"]}', "data.json", 0),
    ('{"sura_name": "Baqarah"}', "data.json", 1),
])
def test_data_files_check_names_not_values(capsys, text, name, status):
    assert run(capsys, text, None, name)[0] == status


def test_prose_transliteration(capsys):
    assert run(capsys, "The riwāyāt of the Uthmānī mushaf", None, "README.md")[0] == 0


def test_virtualenv_of_any_name_is_skipped(capsys):
    Path("env-ci/lib").mkdir(parents=True)
    Path("env-ci/pyvenv.cfg").write_text("home = /usr")
    Path("env-ci/lib/bad.py").write_bytes(b"\xff")
    assert cli.main([".", "--json"]) == 0


def test_invalid_utf8_names_the_file(capsys):
    Path("bad.py").write_bytes(b"\xff")
    assert cli.main(["."]) == 2
    assert "bad.py: not UTF-8" in capsys.readouterr().err


def test_one_line_json_is_linear(capsys):
    import time
    Path("timings.json").write_text(json.dumps(
        {f"{s}:{a}": {"surah_number": s, "ayah_number": a} for s in range(1, 60) for a in range(1, 100)}))
    started = time.monotonic()
    assert cli.main(["timings.json", "--json"]) == 0
    assert time.monotonic() - started < 3  # quadratic span lookup took over 10 s here


def git(*args):
    subprocess.run(["git", *args], check=True, capture_output=True)


def repo(files):
    git("init", "-q")
    git("config", "user.email", "t@example.invalid")
    git("config", "user.name", "T")
    for name, text in files.items():
        Path(name).write_text(text)
    git("add", ".")
    git("commit", "-qm", "base", "--allow-empty")


def test_check_preserves_files_and_unsafe_fix_preserves_name_style(capsys):
    repo({"old.ts": "export const riwaya = 1;\n"})
    Path("a.ts").write_text("const ayaNo = getSuraName(x); // the aya list\nconst AYA_NO = 1;\n")
    original = Path("a.ts").read_text()
    assert cli.main(["a.ts"]) == 1
    assert Path("a.ts").read_text() == original
    capsys.readouterr()
    assert cli.main(["a.ts", "--unsafe-fixes"]) == 1
    assert Path("a.ts").read_text() == (
        "const ayahNumber = getSurahName(x); // the ayah list\nconst AYAH_NUMBER = 1;\n")
    assert "Fixed 4 names in 1 files" in capsys.readouterr().out
    assert cli.main(["a.ts"]) == 0


def test_fix_prose_words_but_not_code_in_docs(capsys):
    repo({})
    Path("README.md").write_text("The Sura list.\n\nCall `get_sura()`:\n\n```\nsura = 1\n```\n")
    Path("a.py").write_text("x = 1  # the aya and `aya_id`\n")
    cli.main([".", "--unsafe-fixes"])
    assert Path("README.md").read_text() == "The Surah list.\n\nCall `get_sura()`:\n\n```\nsura = 1\n```\n"
    assert Path("a.py").read_text() == "x = 1  # the ayah and `aya_id`\n"


def test_unsafe_fixes_rename_existing_names(capsys):
    repo({"lib.ts": "export function getSuraName() {}\n"})
    assert cli.main(["lib.ts"]) == 1
    assert "getSuraName" in Path("lib.ts").read_text()
    assert cli.main(["lib.ts", "--unsafe-fixes"]) == 1
    assert Path("lib.ts").read_text() == "export function getSurahName() {}\n"


def test_unsafe_fix_outside_git_reports_edits_and_is_idempotent(capsys):
    Path("a.py").write_text("aya_no = 1  # the aya\n")
    assert cli.main(["a.py", "--unsafe-fixes", "--json"]) == 1
    assert Path("a.py").read_text() == "ayah_number = 1  # the ayah\n"
    result = json.loads(capsys.readouterr().out)
    assert result["summary"]["fixed_names"] == 2
    assert result["summary"]["fixed_files"] == 1
    assert "held_back" not in result["summary"]
    assert cli.main(["a.py", "--unsafe-fixes", "--json"]) == 0
    assert json.loads(capsys.readouterr().out)["summary"]["fixed_names"] == 0


def test_fix_never_touches_strings_data_warnings_or_compatibility(capsys):
    source = 'import { x } from "./findRubAlHizb";\nname = {"sura_id": "Douri"};\nayah_idx = 1\n'
    Path("a.ts").write_text(source)
    Path("d.json").write_text('{"sura": 1}\n')
    Path("legacy.py").write_text("sura = 1\n")
    Path(".terminology.json").write_text('{"compatibility": ["legacy.py"]}')
    cli.main([".", "--unsafe-fixes"])
    assert Path("a.ts").read_text() == source
    assert Path("d.json").read_text() == '{"sura": 1}\n'
    assert Path("legacy.py").read_text() == "sura = 1\n"


@pytest.mark.parametrize("args", [[], ["--json"], ["--by", "table"]])
def test_check_never_writes_code_or_prose(capsys, args):
    source = "# the aya list\naya_number = 1\n"
    Path("a.py").write_text(source)
    assert cli.main(["a.py", *args]) == 1
    assert Path("a.py").read_text() == source


@pytest.mark.parametrize("args", [["--fix"], ["--fix", "--unsafe-fixes"]])
def test_removed_fix_flag_fails_before_writing(capsys, args):
    source = "# the aya list\naya_number = 1\n"
    Path("a.py").write_text(source)
    with pytest.raises(SystemExit) as error:
        cli.main(["a.py", *args])
    assert error.value.code == 2
    assert "unrecognized arguments: --fix" in capsys.readouterr().err
    assert Path("a.py").read_text() == source


def test_fix_never_edits_a_string_containing_the_other_quote(capsys):
    source = '  ["Al-Jumu\'a", "Friday"],\n  label = \'the "sura" list\'\n'
    Path("names.ts").write_text(source)
    cli.main(["names.ts", "--unsafe-fixes"])
    assert Path("names.ts").read_text() == source


@pytest.mark.parametrize("name,source", [
    ("model.py", "ayah = 100\naya = 2\nprint(ayah, aya)\n"),
    ("model.py", "from quran import Chapters\nprint(Chapters.__name__)\n"),
    ("model.ts", 'const url = "https://example.com"; const aya = 1;\nconsole.log(aya);\n'),
    ("model.ts", "/* note */ const aya = 1;\nconsole.log(aya);\n"),
    ("model.ts", "const text = `\naya\n// the sura list\n`;\n"),
    ("model.xml", "<!-- Tanzil Quran Text (Uthmani) -->\n<aya />\n"),
    ("model.mdx", "export const aya = 1;\n\nThe Sura list\n"),
])
def test_check_preserves_code_strings_and_copied_data(capsys, name, source):
    Path(name).write_text(source)
    assert cli.main([name]) == 1
    assert Path(name).read_text() == source


def test_check_after_git_rename_preserves_existing_names(capsys):
    repo({"old.py": "sura = 1\n"})
    git("mv", "old.py", "new.py")
    assert cli.main(["new.py"]) == 1
    assert Path("new.py").read_text() == "sura = 1\n"


@pytest.mark.parametrize("source", [
    "const data = `\n// aya\n`;\nconst sura = 1;\n",
    "const pattern = /aya/;\nconst sura = 1;\n",
    'const data = "unterminated\naya\n',
])
def test_unsafe_fix_declines_unsupported_javascript_syntax(capsys, source):
    Path("model.ts").write_text(source)
    cli.main(["model.ts", "--unsafe-fixes"])
    assert Path("model.ts").read_text() == source


@pytest.mark.parametrize("name,source", [
    ("model.xml", "<!-- Tanzil Quran Text (Uthmani) -->\n<aya />\n"),
    ("model.mdx", "export const aya = 1;\n\nThe Sura list\n"),
    ("model.tsx", "const aya = <div>sura</div>;\n"),
])
def test_unsafe_fix_preserves_unsupported_formats(capsys, name, source):
    Path(name).write_text(source)
    assert cli.main([name, "--unsafe-fixes"]) == 1
    assert Path(name).read_text() == source


def test_python_parse_failure_prevents_fixing(capsys):
    source = '"""unterminated\n# the aya\n'
    Path("model.py").write_text(source)
    cli.main(["model.py", "--unsafe-fixes"])
    assert Path("model.py").read_text() == source


def test_fix_preserves_line_endings(capsys):
    Path("model.py").write_bytes(b"# the aya\r\nayah = 1\r\n")
    cli.main(["model.py", "--unsafe-fixes"])
    assert Path("model.py").read_bytes() == b"# the ayah\r\nayah = 1\r\n"


@pytest.mark.parametrize("source", [
    "````python\n```\naya = 1\n````\n",
    "```python\n~~~\naya = 1\n```\n",
    "    aya = 1\n",
    "> aya = 1\n",
    "Use ``aya = 1`` here.\n",
    "---\nname: aya\n---\n",
    "<script>\nconst aya = 1;\n</script>\n",
])
def test_fix_preserves_document_examples(capsys, source):
    Path("README.md").write_text(source)
    cli.main(["README.md", "--unsafe-fixes"])
    assert Path("README.md").read_text() == source
