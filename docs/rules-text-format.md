# Rules-text packs (`borealis-docs/1`)

Searchable SRD rules text for offline search (local-search spec, sections 3 and 4; work package S1.1). A rules-text
pack is a folder under `content/` holding:

| File | Purpose |
|---|---|
| `docs.json` | the sections (format below) |
| `ATTRIBUTION.md` | the CC-BY-4.0 attribution the SRD requires, the modifications made, scope |
| `COVERAGE.md` | counts and the heading-coverage check against the PDF |

There is **no `pack.json`**: a rules-text pack is not a `borealis-pack/1` element pack, is never added to the content
store (`BUNDLED_PACKS`, `SRD_PACKS` do not list it) and only the search index reads it. It is bundled into the app
the same way as the element packs (a `bundle.resources` entry in `src-tauri/tauri.conf.json`).

Conventions follow `specs/content-format.md`: snake_case keys, a `format` id with a major version, unknown keys rejected,
ruleset tags `dnd2014` / `dnd2024` / `any`, `license` and `attribution` mandatory.

## `docs.json`

```json
{
  "format": "borealis-docs/1",
  "info": {
    "id": "srd-5.2.1-rules-text", "name": "SRD 5.2.1 Rules Text", "version": "1.0.0",
    "license": "CC-BY-4.0", "attribution": "...", "description": "...",
    "source": {"document": "SRD 5.2.1", "pdf": "SRD_CC_v5.2.1.pdf", "pdf_sha256": "..."}
  },
  "ruleset": "dnd2024",
  "sections": [
    {"id": "playing-the-game/combat/cover", "path": ["Playing the Game", "Combat", "Cover"], "page": 15, "html": "<p>...</p>"}
  ]
}
```

* `id`: slug of the heading path (`/`-joined, lower case, ASCII); unique in the pack; a repeated path gets `-2`, `-3`.
  Search derives the citation key `rules:<pack id>/<id>` from it.
* `path`: heading texts from the chapter (H1) to the section's own heading (H4 at most). The last entry is the title.
  The SRD 5.1 PDF prints no heading for the Adventuring, Combat and Gamemastering chapters; the tool supplies those
  three parents. In the SRD 5.2.1 Rules Glossary every entry sits directly under `Rules Glossary`.
* `page`: 1-based PDF page of the heading. In both SRD PDFs this equals the printed page number.
* `html`: the section's own text (its sub-sections are separate entries) as a fragment of `p`, `ul/li`,
  `table/tr/th/td`, `b`, `i`. Plain text and escaped `& < >` only; no attributes, no links, no scripts. May be empty
  when a heading is directly followed by a sub-heading.
* Sections are in document order. Chunking (> 350 words), embedding and `html_to_text` happen in search, not here.

The JSON schema is `crates/borealis-content/schema/docs-format.schema.json`
(`borealis_content::schema::validate_docs`); `crates/borealis-content/tests/rules_text_packs.rs` checks both bundled
packs (schema, page and path on every section, unique ids, attribution, spot checks).

## Building

`content-tools/rules-text/build.py` (Python 3 standard library plus poppler `pdftohtml` and `pdftotext`) reads the
official SRD PDFs and writes the two packs. It is deterministic code over the PDF text layer (fonts, sizes, colours,
positions) and never uses AI or any source other than the SRD PDFs. Headings are the GillSans SemiBold dark-red lines
(39/27/21/18 pt = H1-H4). Chapters modelled as typed elements (classes, species, feats, spells, magic items,
monsters) are left out; `config.py` lists them per pack and COVERAGE.md repeats the list.
