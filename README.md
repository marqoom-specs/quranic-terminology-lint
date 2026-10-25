# Quranic terminology lint

Checks that the names in your code follow the Quranic vocabulary naming
standard. Add it to pre-commit once and every commit is checked: `aya_number`,
`tajweed_rules` or `al_fatihah` are rejected with the name to use instead and
the rule behind it.

```text
src/model.ts:12: error getSuraNo → getSurahNumber (rule 068: Reference numbers)
src/model.ts:18: error tajweed_rules → tajwid_rules (rule 021: Long vowels)
```

It runs offline, has no dependencies, and needs Python 3.10+. Check-only is
the default. Findings show the file, line, full suggested name in the original
case style, and the rule. Suggestions are terminology corrections, not proof
that a rename is compatible with an external API.

## Set up

Add one of the two hooks to your project's `.pre-commit-config.yaml`, then run
`pre-commit install`. `pre-commit autoupdate` moves you to the latest release.

**Check only.** A commit with an error is stopped. Files are unchanged.

```yaml
repos:
  - repo: https://github.com/marqoom-specs/quranic-terminology-lint
    rev: v0.4.0
    hooks:
      - id: quranic-terminology
```

**Unsafe fix.** Opt in to automatic replacements with this hook:

```yaml
repos:
  - repo: https://github.com/marqoom-specs/quranic-terminology-lint
    rev: v0.4.0
    hooks:
      - id: quranic-terminology-unsafe-fix
```

This applies textual code renames in Python and simple JavaScript/TypeScript
(`.js`, `.ts`, `.mjs`, `.cjs`), plus supported prose corrections. Code renames
can break imports, exports, bindings and references stored in strings. The tool
does not perform scope analysis or prevent name collisions. Review the diff,
run your tests and stage the changes before retrying the commit. Use your
editor's rename refactoring when a change needs to follow code relationships.

Unsafe fix still skips strings, JSON/CSV/TSV data, file names, compatibility
paths, and unsupported or ambiguous syntax. This includes JavaScript template
literals, regex/division syntax, unfinished strings, JSX/TSX and other languages.
Python interpolation and malformed syntax are handled conservatively; exact
fix availability can differ between Python versions.

Prose corrections cover plain words in Python comments and supported
Markdown/text (`.md`, `.txt`). Fenced and indented examples, block quotes,
Markdown lines containing backticks, and camelCase words in prose are left
alone. Documentation with HTML, template braces or front matter is skipped.
MDX, XML and other unsupported formats can produce findings without edits.
Use `exclude` for copied texts and notices that must remain exactly as supplied.

### Migrating from v0.3.0

Version 0.4.0 names a word-level audio span `word_timing`, as rule 073 does.
v0.3.0 reported `word_timing` and preferred `word_timestamp`; both are accepted
now, so a project that already renamed keeps passing. A bare `silah` is no
longer reported as `madd_al_silah`.

### Migrating from v0.2.1

Version 0.3.0 removes the prose-only `--fix` option and the
`quranic-terminology-fix` hook. They are **not aliases** for unsafe fix: an old
configuration fails instead of silently opting in to code renames.

- To keep checking without automatic edits, use `quranic-terminology` and
  remove `--fix` from any arguments or scripts.
- To opt in to automatic edits, use `quranic-terminology-unsafe-fix` or pass
  `--unsafe-fixes` to the CLI. No extra hook arguments are needed.

The JSON summary retains `fixed_names` and `fixed_files`; the obsolete
`held_back` field has been removed. Skipped or unsupported findings still
appear in the report; unsafe fix does not promise to resolve every finding.

To check the whole project once, run
`pre-commit run quranic-terminology --all-files`.

To run it directly or in CI:

```sh
pip install git+https://github.com/marqoom-specs/quranic-terminology-lint@v0.4.0
quranic-terminology-lint                # configured paths, or the current directory
quranic-terminology-lint src --by table # group findings by terminology rule
quranic-terminology-lint src --json     # complete results, including suggested_identifier
quranic-terminology-lint src --unsafe-fixes # supported code and prose replacements
```

The default report shows 20 findings with locations; the table shows 10 rows.
Use `--limit` to show more. JSON always includes every finding. Exit status:
0 no errors (warnings do not block), 1 errors found, 2 a configuration or file
problem. A fix reports the original findings and still returns 1 when errors
were found, so you can review edits and run the check again.

## Adopting in an existing project

Start with a source directory, inspect the findings, and record exceptions for
external APIs, legacy schemas, generated files and copied datasets. A term can
violate this naming standard while still being required by a dependency.

```json
{
  "paths": ["src"],
  "exclude": ["src/generated/**", "data/**", "QuranCorpus/**"],
  "compatibility": ["src/legacy/**"],
  "allow_gloss_in": ["src/api/**"],
  "external_names": ["\\bChapters\\b"],
  "_comment": "Chapters is an export owned by our Quran API dependency."
}
```

`paths` sets the direct CLI default. Pre-commit passes staged file names, which
override it; use the hook's `files: ^src/` filter to restrict staged checks to
that directory. `exclude`, `compatibility`, and the naming exceptions apply to
both modes.

## What it checks

Each identifier is split into words (`ayahKey`, `ayah_key` and `AYAH_KEY` are
the same name) and checked against the standard's rules. The term files list
each concept's canonical name. Most misspellings are predicted by the rules
from that name, so a finding names the rule that was broken.

| Rule | Checks | Example |
| --- | --- | --- |
| 001 | One code name per concept | `chapter` → `surah`, `verse` → `ayah` |
| 009 | No transliteration marks or apostrophes | `qira'ah` → `qiraah` |
| 019, 020 | Ta marbutah | `sura` → `surah`, `hamzah_al_wasl` → `hamzat_al_wasl` |
| 021 | Long vowels are one letter | `tajweed` → `tajwid`, `tafseer` → `tafsir` |
| 022 | The nisbah ending | `makkiyy` → `makki` |
| 026 | Final hamzah and ayn after sukun | `rub_al_hizb` → `rubu_al_hizb` |
| 030–033 | The definite article | `asbab_an_nuzul` → `asbab_al_nuzul`, `al_fatihah` → `fatihah`, `rasm_al_uthmani` → `rasm_uthmani`, `rubu_hizb` → `rubu_al_hizb` |
| 046 | Deprecated names | `waqf_jaiz` → `waqf_jaiz_mustawi_al_tarafayn` |
| 049 | Plurals add `s` | `ayat` → `ayahs`, `suras` → `surahs` |
| 050 | `_type` is for mark types | `waqf_type` (warning) |
| 055–057 | Numbering systems, personal and surah names | `qaloun` → `qalun`, `kufan` → `kufi` |
| 068, 069 | `number` and `position` | `surah_no` → `surah_number`, `ayah_idx` (warning) |
| 073 | Timing names do not include the recording | `hafs_word_timing` (warning) |

The other rules govern how a name is derived from vocalised Arabic, or how
concepts are modelled. They apply when a term is added to the term files, not
to code.

**Errors** match a spelling or alias covered by the naming standard. They can
still need an exception for an unrelated meaning or external API. **Warnings**
are suggestions to review and never block a commit.

### What it leaves alone

- Comments, strings with spaces (`"Surah al-Fatihah"`), text between HTML
  tags, and Markdown or text files are read as prose. Prose may use display
  names (`Tajweed`), English equivalents (`verse`), `al-` and apostrophes. It
  is only checked for misspellings such as `Sura`.
- Ordinary English words that a term also translates (`part`, `pause`,
  `reading`, `stop`, `timing`) are never reported on their own.
- General concepts with English names (`token`, `line`, `page`) are not held
  to the Arabic spelling rules.
- ALL-CAPS Unicode character names in prose (`ARABIC FATHA`, rule 043).
- In CSV and TSV files only the header row is checked, and in JSON only the
  keys. Rows and values are data, often copied from a source.
- Scholarly transliteration in prose (`riwāyāt`).
- URLs and file names (`uthmani-hafs.json`), virtualenvs of any name,
  `node_modules`, build output, and files over 2 MB.

## Exceptions

Commit a `.terminology.json` at the project root:

```json
{
  "paths": ["src"],
  "exclude": ["src/generated/**"],
  "ignore_words": ["segment"],
  "allow_gloss_in": ["src/api/v1/**"],
  "compatibility": ["src/legacy/schema.sql"],
  "external_names": ["vendor_sura_id"],
  "_comment": "Why each exception exists."
}
```

| Key | Use it for |
| --- | --- |
| `paths` | What to check when no paths are given. |
| `exclude` | Files that are never read. |
| `ignore_words` | A word or compound with an unrelated meaning, including inside camelCase/snake_case identifiers and its `s` plural. |
| `allow_gloss_in` | Files that must use English equivalents, such as a published API that says `verse`. |
| `compatibility` | Names you cannot rename without a migration. Reported, but they do not block. |
| `external_names` | Regular expressions for names someone else owns, such as a publisher's column `aya_text_emlaey`. |

A line containing `terminology: ignore` is skipped. Keys starting with `_` or
`$` are ignored, so you can record the reasons.

## Changing the terms

The terms live in `src/quranic_terminology_lint/terms/`:

- `concepts.tsv`: each concept's code name, display name, origin, Arabic
  name, and the names no rule predicts (other spellings, English
  equivalents, Arabic plurals, deprecated names).
- `members.tsv`: surahs, qiraat, rawis, riwayahs, turuq, ayah numbering
  systems and tajwid rulings.

Do not list a spelling that a rule already predicts; `tests/test_rules.py`
checks this for the common cases. A change to the terms is released like any
other change, and projects pick it up with `pre-commit autoupdate`.

## Development

```sh
python -m pip install -e '.[test]'
python -m pytest
quranic-terminology-lint          # the linter checks its own code
pre-commit validate-manifest
python -m build
```

MIT licensed.
