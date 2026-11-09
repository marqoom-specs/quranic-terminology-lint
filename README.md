# Quranic terminology lint

Two offline checks for software that handles the Quran:

- **`quranic-data`** reads the data a project stores and publishes: data files,
  API responses and schemas, database schemas. It finds which riwayah and ayah
  numbering the data follows, and reports what another project would read
  wrongly.
- **`quranic-terminology`** checks the names in code against the Quranic
  vocabulary naming standard. It is optional, for projects that want their
  code to use the standard's names too.

Both are single binaries with no runtime dependencies, and read the Marqoom
standards compiled into them.

## Set up

Add the hooks to your project's `.pre-commit-config.yaml`, then run
`pre-commit install`:

```yaml
repos:
  - repo: https://github.com/marqoom-specs/quranic-terminology-lint
    rev: v0.5.0
    hooks:
      - id: quranic-data
      - id: quranic-terminology
```

pre-commit builds the checks with Cargo the first time, and installs a Rust
toolchain if you don't have one. To run them yourself or in CI:

```sh
cargo install --git https://github.com/marqoom-specs/quranic-terminology-lint --tag v0.5.0
```

## Check your data

Two projects exchange Quran data through numbers and codes: ayah numbers, a
riwayah, a numbering system. When those are missing or spelled privately, the
data still loads, and the consumer pairs it with the wrong ayahs. `quranic-data`
reads the data itself and reports these places:

```text
Quran data
  repo/data/quran_warsh.json
      ayahs keyed by surah number · 114 surahs · 6214 ayahs · fits madani_last (113/114 surahs)
      states nothing; file name says `warsh` → warsh_an_nafi
      same file at repo/munajjam_desktop/assets/data/quran_warsh.json

Meaning — what a consumer would read wrongly
  ! warning  ayah numbers that are not Kufi, and nothing says so: rule 072 reads unstated numbers as
             Kufi, so a consumer would pair them with the wrong ayahs (surah 2 has 285 ayahs here
             and 286 in Kufi). The count fits `madani_first`, `madani_last`, `makki`, `dimashqi`.
             at exports/button/002_alignment.json
             →  state the riwayah or the numbering
             basis: rule 072

Values — codes no other project can resolve
  ✗ error    `hafsh` is not a riwayah in the registry, so no other project can resolve it; the
             nearest code is `hafs_an_asim`
             at exports/batch/002_البقرة.json:5 [/riwaya]
                openapi.json:301 [/components/schemas/…/properties/riwaya] ×3
             →  write `hafs_an_asim`, or declare the mapping `"hafsh": "hafs_an_asim"` if that is
                what it means
             basis: registries: qiraat
…
Fix first
  1. ✗ values that aren't registry codes
       in exports/batch/002_البقرة.json, exports/batch/khatmah_manifest.json, openapi.json
       → write `hafs_an_asim` for `hafsh` (if that is what they mean)
  2. ! ayah numbers that aren't Kufi, with nothing saying so
       in exports/button/002_alignment.json, repo/data/quran_warsh.json, repo/data/surahs.json
       → state `riwayah` (or `ayah_numbering_system`) in each file
…
3 errors, 12 warnings, 3 notes; 4 field names the dictionary gives differently
```

A problem found in several files is shown once, with every place it occurs, and
a file bundled twice is reported once, under its first path. With more than
three kinds of problem, the report ends with "Fix first": one line for each
kind, the most serious first, where it stays on screen. `--format markdown`
puts it at the top.

It reads `.json` (data files, saved API responses, OpenAPI documents), `.csv`,
`.tsv` and `.sql`. It does not read application code.

| It checks | How |
| --- | --- |
| Ayah numbering | Counts each surah's ayahs and matches them against the six systems in the `ayah_counts` registry; compares that with what the data states, or with rule 072's Kufi default when it states nothing |
| Riwayah and qiraah values | Resolves every value of a `riwayah`, `qiraah`, `rawi` or numbering field against the registries: a code, a recorded spelling, a rawi's name that could be two riwayahs (`duri`), or unknown (`hafsh`) |
| Ayah and word keys | Fields holding `2:255` or `37:1:2` without a stated numbering, text or segmentation |
| The basmalah inside an ayah | Word positions shift by four when the basmalah is joined to the first ayah's text |
| Places of prostration | The ۩ marks in Arabic text against the `sajdah` registry, ayah by ayah; a surah counted differently from Kufi only by its number of marks, since the registry gives Kufi numbers |
| Recitations | Lists that name a reciter but not the riwayah the audio follows |
| Field names | `verses_count`, `chapter_id`, `riwaya` against the dictionary; reported last, once per name, as warnings in published files and notes elsewhere |

```sh
quranic-data                      # the current directory
quranic-data data/ openapi.json   # chosen files and folders
quranic-data --format markdown    # a report to paste into an issue
quranic-data --format json        # everything, for tools
quranic-data registry             # check the registries themselves
```

Exit status: 0 no errors, 1 errors found (`--strict` counts warnings too), 2 a
problem with the command, a path or the configuration.

### Saved API responses and schemas

The check reads files, so save what your API returns, or its OpenAPI document,
and keep one example of each published file in the repository. FastAPI, for
example, serves its document at `/openapi.json`. Checking real output in CI is
more reliable than any reading of the code that produces it.

### Declaring your data

A project can describe its data in the `data` section of `.terminology.json`
instead of renaming anything:

```json
{
  "data": {
    "publishes": ["exports/**/*.json", "openapi.json"],
    "files": {
      "data/quran_warsh.json": {
        "riwayah": "warsh_an_nafi",
        "word_segmentation": "KFGQPC Warsh text split on spaces; the basmalah is joined to 1:1"
      },
      "data/surahs.json": { "ayah_numbering_system": "madani_last" }
    },
    "mappings": { "hafsh": "hafs_an_asim" },
    "exclude": ["fixtures/legacy/**"]
  }
}
```

`files` states the riwayah, numbering or word segmentation of a file that
doesn't say it itself. `mappings` resolves the project's own spellings.

Files listed under `publishes` are read the way their consumers read them:
the consumers never see your `.terminology.json`, so its `files` and `mappings`
don't apply to them. Published data has to state its riwayah and use registry
codes in the data itself.

### Registry evidence

When data that states its numbering disagrees with the `ayah_counts` registry
in a few surahs, the report lists them under "Registry evidence". Several
independent editions disagreeing with the registry in the same surah is a
reason to recheck that row against the source, not to change the data.

## Check names in code

`quranic-terminology` checks that the names in your code follow the Quranic
vocabulary naming standard. Each name that breaks a rule is reported with the
name to use instead, in the same case style, and every rule it breaks:

```text
src/model.ts:2: error getSuraAyat → getSurahAyahs (rule 019: Ta marbutah outside idafah; rule 049: Forming the code plural)
src/model.ts:2: error suraNo → surahNumber (rule 019: Ta marbutah outside idafah; rule 068: Reference numbers)
src/model.ts:2: error verseKey → ayahKey (rule 001: One code name per concept)
src/model.ts:3: error loadTajweed → loadTajwid (rule 021: Long vowels)
src/model.ts:3: error tajweedRules → tajwidRulings (rule 001: One code name per concept)
…
13 errors, 0 warnings in 1 file
```

A commit with an error is stopped; warnings never block. The suggestions are
terminology corrections, not proof that a rename is compatible with an external
API.

```sh
quranic-terminology                   # the configured paths, or the current directory
quranic-terminology src               # chosen files and folders
quranic-terminology --format table    # each name once, with how often it occurs
quranic-terminology --format json     # every finding, with suggested_identifier
quranic-terminology --unsafe-fixes    # apply the supported renames
```

The text report shows 20 names and the table 10 rows; `--limit` shows more,
and JSON always has every finding. Exit status: 0 no errors, 1 errors found,
2 a problem with the command, a path, a file or the configuration.

### Unsafe fixes

To apply renames automatically, use the `quranic-terminology-unsafe-fix` hook
in place of `quranic-terminology`, or pass `--unsafe-fixes`:

```yaml
      - id: quranic-terminology-unsafe-fix
```

It renames names in Python and plain JavaScript or TypeScript (`.js`, `.ts`,
`.mjs`, `.cjs`), and corrects plain words in Python comments and in Markdown
or text files. A rename is textual: it can break imports, exports, bindings
and references stored in strings, and it doesn't check scope or collisions.
The hook stops the commit after a fix, so you can review the diff, run your
tests and stage the changes. Use your editor's rename refactoring when a change
has to follow code relationships.

It never edits strings, data files, file names, compatibility paths, warnings,
or syntax it can't read with certainty: template literals, a `/` that could be
division or a regular expression, unfinished strings, JSX and other languages.
In a Python file with f-strings only comments are fixed, and a Python file that
doesn't tokenize isn't fixed at all. In documents, code in backticks, fenced
and indented examples, block quotes and camelCase words are left alone, and a
document with HTML, template braces or front matter isn't edited.

### What it checks

Each identifier is split into words (`ayahKey`, `ayah_key` and `AYAH_KEY` are
the same name) and checked against the standard's rules. The term files list
each concept's canonical name; most misspellings are predicted by the rules
from that name, so a finding names the rule that was broken.

| Rule | Checks | Example |
| --- | --- | --- |
| 001 | One code name per concept | `chapter` → `surah`, `verse` → `ayah` |
| 009 | No transliteration marks or apostrophes | `qira'ah` → `qiraah` (warning) |
| 019, 020 | Ta marbutah | `sura` → `surah`, `hamzah_al_wasl` → `hamzat_al_wasl` |
| 021 | Long vowels are one letter | `tajweed` → `tajwid`, `tafseer` → `tafsir` |
| 022 | The nisbah ending | `makkiyy` → `makki` |
| 026 | Final hamzah and ayn after sukun | `rub_al_hizb` → `rubu_al_hizb` |
| 030–033 | The definite article | `asbab_an_nuzul` → `asbab_al_nuzul`, `al_fatihah` → `fatihah`, `rasm_al_uthmani` → `rasm_uthmani`, `rubu_hizb` → `rubu_al_hizb` |
| 046 | Deprecated names | `omitted_alif` → `dagger_alif` |
| 049 | Plurals add `s` | `ayat` → `ayahs`, `suras` → `surahs` |
| 050 | `_type` is for mark types | `waqf_type` (warning) |
| 055–057 | Numbering systems, personal and surah names | `qaloun` → `qalun`, `kufan` → `kufi` |
| 068, 069 | `number` and `position` | `surah_no` → `surah_number`, `ayah_idx` (warning) |
| 073 | Timing names don't include the recording | `hafs_word_timing` (warning) |

The other rules govern how a name is derived from vocalised Arabic, or how
concepts are modelled. They apply when a term is added to the term files, not
to code.

### What it leaves alone

- Comments, strings with spaces (`"Surah al-Fatihah"`), text between HTML
  tags, and Markdown or text files are read as prose. Prose may use display
  names (`Tajweed`), English equivalents (`verse`), `al-` and apostrophes, and
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

## Configuration

Both checks read `.terminology.json` from the project root (the nearest one
above the current directory, or `--config`). The `data` section belongs to the
data check, described above; the other keys to the names check:

```json
{
  "paths": ["src"],
  "exclude": ["src/generated/**", "QuranCorpus/**"],
  "ignore_words": ["segment"],
  "allow_gloss_in": ["src/api/v1/**"],
  "compatibility": ["src/legacy/**"],
  "external_names": ["\\bChapters\\b", "vendor_sura_id"],
  "_comment": "Chapters is an export of our Quran API dependency."
}
```

| Key | Use it for |
| --- | --- |
| `paths` | What to check when no paths are given. pre-commit passes the staged files instead; use the hook's `files: ^src/` to limit it. |
| `exclude` | Files that are never read: generated code, copied datasets, notices that must stay as supplied. |
| `ignore_words` | A word or compound with an unrelated meaning, also inside camelCase or snake_case names and with an `s` plural. |
| `allow_gloss_in` | Files that must use English equivalents, such as a published API that says `verse`. |
| `compatibility` | Names you can't rename without a migration. Reported, but they don't block. |
| `external_names` | [Regular expressions](https://docs.rs/regex/latest/regex/#syntax) for names someone else owns, such as a publisher's column `aya_text_emlaey`. |

In patterns, in both sections, `*` matches within a folder name and `**`
across folders, and a pattern for a folder covers everything in it. A line
containing `terminology: ignore` is skipped. Keys starting with `_` or
`$` are ignored, so you can record the reasons.

## Changing the terms and registries

The names check's terms are in `terms/`:

- `concepts.tsv`: each concept's code name, display name, origin, Arabic
  name, and the names no rule predicts (other spellings, English
  equivalents, Arabic plurals, deprecated names).
- `members.tsv`: surahs, qiraat, rawis, riwayahs, turuq, ayah numbering
  systems and tajwid rulings.

Don't list a spelling that a rule already predicts; a test in
`src/names/terms.rs` checks this for the common cases.

The data check reads `concepts.tsv` too, and the registries in `registries/`:
`ayah_counts.tsv`, `ayah_numbering.tsv`, `qiraat.tsv`, `sajdah.tsv` and
`surahs.tsv`, copied unchanged from the terminology standard. Replace them with
the standard's files rather than editing them here, and run
`quranic-data registry` afterwards.

Both are compiled into the binaries, so a change ships with a release, and
projects pick it up with `pre-commit autoupdate`.

## Development

```sh
cargo test --release                  # unit tests, and both commands run on temporary projects
cargo clippy --release --all-targets
cargo build --release
./target/release/quranic-terminology                 # the names check on its own code
./target/release/quranic-data examples/data/failing  # fails, on purpose
pre-commit validate-manifest
tests/hooks.sh                        # the hooks, installed into other projects by pre-commit
```

The code is in `src/`: `src/data/` is the data check, `src/names/` the names
check, and `src/bin/` holds the two commands. `skills/quranic-data-review/` is
an agent skill for the review the data check can't do alone.

MIT licensed.
