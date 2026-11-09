---
name: quranic-data-review
description: Review the Quran data a project stores, serves or publishes — files, API responses and schemas, database schemas, SDK types — against the Marqoom registries, and find what another project would read wrongly. Use when asked to review a Quran project's data, prepare it for Marqoom, or explain quranic-data findings. Not for renaming variables in application code.
---

# Quranic data review

Marqoom settles Quranic decisions so that developers don't have to research
them. This review checks that a project's data carries those decisions where
other projects can read them: which riwayah, which ayah numbering, which codes.
It is about data that leaves the project, not about names inside its code.

## 1. Find the data

List every place Quran data is stored, sent or published, and mark each one:

- **Published**: files other projects download or receive (exports, datasets,
  release assets), API responses, OpenAPI or JSON Schema documents, SDK types.
- **API between parts of one product**: lower priority; it becomes published
  the day someone else calls it.
- **At rest**: data the project keeps for itself (bundled text, count tables,
  database schemas and migrations).

Data produced at run time is often missing from the repository. Get a real
sample: run the export, save an API response, or generate the OpenAPI document
(FastAPI: `app.openapi()`). Read code only to learn the shape of the data it
produces, never to judge its variable names. Say plainly when a sample was
built from the code's shape rather than produced by running it.

## 2. Run the deterministic check

```sh
quranic-data --format json <paths>    # cargo install --git https://github.com/marqoom-specs/quranic-terminology-lint
```

Read `quran_data` first: each body of Quran data, the numbering its counts fit,
what it states about itself, and its sajdah marks. `fix_first` names each kind
of error and warning once, with its files; `findings` has every place, most
severe first; `copies` lists files bundled more than once. Don't repeat its
findings in other words; build on them.

## 3. Review what the check can't decide

Look for these, in the code that produces or consumes the data:

1. **Silent fallbacks**: an unknown riwayah or edition that quietly becomes
   the default (`if file missing: load Hafs`). The output then claims one
   riwayah and contains another.
2. **Joins by position**: text, timings or translations paired by list index
   instead of by ayah number or key. One dropped or repeated ayah shifts every
   later one.
3. **One export, two shapes**: the same kind of file written by two code paths
   with different fields (one states the riwayah, the other doesn't).
4. **Look-alike concepts in one field**: qiraah and riwayah, a waqf mark's type
   and the waqf ruling, a word timing and a morpheme. Check the dictionary
   definitions before deciding.
5. **Disputed facts fixed in data**: 14 or 15 places of prostration, a single
   ayah count where the systems differ, without saying which position is taken.
6. **Word identity**: word positions or keys exchanged without the text and
   segmentation they count by (rules 069, 071, 072).

## 4. Report

For each finding give: the location, the evidence, what a consumer would get
wrong (with a concrete ayah), and the fix in registry codes and rule numbers.
Then:

- A draft `data` section for `.terminology.json`: `publishes`, `files` with
  each file's riwayah, numbering and segmentation, and `mappings` for the
  project's own spellings. Include only what the evidence supports.
- Questions only the developer can answer (which mushaf the page numbers come
  from, which edition the text is). Never guess a riwayah or numbering: derive
  it from the counts with the evidence shown, or ask.

When data that states its numbering disagrees with the registry in a few
surahs, report it as registry evidence for Marqoom to recheck, not as an error
in the project.
