# 5etools local import

Crate: `crates/borealis-import-5etools` (library + `borealis-import-5etools` CLI).

## Licensing

5etools data is never bundled, committed or redistributed. The importer reads the user's own
local `data/` folder at runtime and writes packs for local use only. Every produced pack
carries `origin = 5etools-import` and `redistributable = false` (in `info.extra.attributes`
of each content file, in the manifest description, and in an `import.json` sidecar), and
its manifest license is "Proprietary: user-supplied 5etools data, not redistributable".
Test fixtures (`tests/fixtures/data`) contain only entries flagged `srd` / `srd52`
(CC-BY 4.0); `Basic Rules` flags do not qualify. A test fails if an unflagged entry appears there.

## Using it

Library:

```rust
let sources = scan_folder(path)?;                       // Vec<SourceSummary>, read-only
let opts = ImportOptions::default().with_sources(["PHB", "XPHB"]);
let result = import_folder_with_progress(path, &opts, &mut |ev| { /* ProgressEvent */ })?;
for pack in &result.packs { pack.write_to(&library_dir)?; }   // result.report: ImportReport
```

- `SourceSummary { abbr, name, ruleset: "dnd2014"|"dnd2024"|"mixed", counts: {kind: n} }`. The UI
  command `scan_5etools_folder` wraps it as `FiveToolsScan { sources }`.
- `ImportOptions { sources, include_fluff, ruleset, only_srd, srd_dirs, pack_version }`. The UI
  request `{ path, sources, includeFluff }` maps to `with_sources` + `include_fluff`.
- `ImportReport { files, converted, partial[{element, kind, file, reason}], skipped[...],
  warnings, external_references, packs }` serialises deterministically (sorted, deduplicated).
  The UI's `FiveToolsImportReport` is `converted` + `partial` + `skipped` + the installed packs.
- `ProgressEvent { phase: scanning|reading|resolving|converting|writing|done, current, total, message }`.
  `import_folder` itself emits everything except `writing` (the caller writes).
- Folder-level failures (not a directory, no 5etools files) are `Err`; per-file and per-entry
  problems never abort, they land in the report.

CLI: `borealis-import-5etools <data-dir> -o <out-dir> [--source A,B] [--ruleset 2014|2024]
[--fluff] [--only-srd] [--srd-dir D]... [--report f.json]`, or `<data-dir> --scan`.
Exit codes: 0 success, 1 import failed, 2 usage error.

## Pipeline

1. `folder::load` reads the known files (`races`, `backgrounds`, `feats`, `optionalfeatures`,
   `items-base`, `items`, `magicvariants`, `languages`, `skills`, `senses`, `conditionsdiseases`,
   `books`, `fluff-*`, `class/class-*`, `class/fluff-class-*`, `spells/spells-*`,
   `spells/fluff-spells-*`, `generated/gendata-spell-source-lookup.json`, `adventures.json`
   for source names) plus every `homebrew/*.json` (5etools homebrew format: `_meta.sources`
   names the sources, entity arrays merge with the official ones so `_copy` resolves across
   them). Unreadable files are warnings.
2. `_copy` / `_mod` / `_versions` are resolved on the raw JSON (`copy.rs`), with 5etools
   semantics: `"prop": null` removes an inherited property, dotted `_mod` keys
   (`inherits.entries`) address nested properties, `"prop": "remove"` deletes one, and a magic
   variant is looked up by its `inherits.source`. Unsupported `_mod` modes become warnings,
   unresolvable copies are skipped entries.
3. Filters (source allowlist, ruleset, `only_srd`) select entities. Magic variants are applied
   to base items of the same edition to generate concrete items.
4. Lookup tables (links, ids, class features, spell class lists, fluff, languages) are built,
   then entities are converted: races, subraces, classes, subclasses, class features,
   subclass features, backgrounds, feats, optional features, spells, items, item groups,
   masteries, languages, conditions/diseases/statuses. Catalog entries covered by the core
   pack (item properties/types/entries, skills, senses) are reported as skipped.
5. One pack per source (`5etools-<abbr>`): files `source`, `species`, `classes`, `backgrounds`,
   `feats`, `options`, `spells`, `items`, `misc` (only non-empty ones). A single-ruleset pack
   puts the ruleset on the file; a mixed one tags elements.

IDs are `ID_5ET_<SOURCE>_<KIND>_<NAME...>` (upper snake case ASCII), allocated once per entity;
a collision gets a stable 8 hex digit suffix. Same input gives byte-identical output, whatever
the folder's location (tested). `external_references` lists `ID_5ET_*` ids that rules point to
but no imported element defines (for example a feat from a source that was not selected).
A class element embeds its synthetic multiclass id, which counts as defined.

## Field mapping

| 5etools | Borealis element | Notes |
|---|---|---|
| `race` (2014) | `Race` | size, speed, darkvision, resistances, languages, skills, tools, weapons as rules; ability bonuses as `stat` rules; `data.size` |
| `race` (2024 edition) | `Species` (`legacy_alias: Race`) | `data.creature_types`; no subrace select |
| `race` traits (named entries) | `Racial Trait` per trait, granted by the race | lineage / legacy options become `Species Trait Option` selects |
| `subrace` | `Sub Race`, `supports` = parent race id | race gets a `Sub Race` select when subraces exist; unnamed subrace is "Standard"; a parent property the subrace replaces (`overwrite`, or `prop: null`) keeps its rules on the parent gated `!<subrace id>` |
| `class` | `Class` + embedded `multiclass` | saves/proficiencies gated `!<multiclass id>`; hit die in setters and `data.hit_die`; subclass select at the subclass level; `data.subclass_title`, `data.starting_equipment`, `data.class_table`, `data.caster_progression`, `data.prepared_formula` |
| class spellcasting | `spellcasting` block + rules | ability, list, cantrips/spells known or prepared, slot table (full, half, third via core `SLOTS_*`) |
| `classFeature` | `Class Feature`, granted at its level | ASI features become improvement or feat selects; `data.level` |
| `subclass` | `Archetype`, `supports` = [class id, subclass title] | domain / expanded spells, subclass casters |
| `subclassFeature` | `Archetype Feature` | intro feature text merged into the subclass |
| `optionalfeature` | `Class Feature` with `data.feature_types` | progressions become selects on the class or subclass |
| `background` | `Background` + `Background Feature` | skills/tools/languages as grants or selects; characteristics as `List` selects; 2024 ability scores as `select.ability_distribution`; origin feat granted |
| `feat` | `Feat` (+ `Feat Feature`, `Species Trait Option` for spell choices) | category in `supports` and `data.category`; level/ability prerequisites in `requirements`; `additionalSpells` as spell grants or selects |
| `spell` | `Spell` | level, school, time, range, duration, components in setters; `supports` = class list from the spell lookup; `data.damage_inflict`, `saving_throw`, `scalingLevelDice`, `areaTags`, ... |
| `baseitem`, `item`, generated variants, generic variants | `Weapon` / `Armor` / `Item` / `Magic Item` | cost, weight, damage, properties, AC, rarity, attunement, charges, `data.mastery`, `data.tier`, `data.lootTables` |
| `itemGroup` | `Item` | group members linked |
| `itemMastery` | `Mastery Property` | |
| `language` | `Language` | languages the core pack defines are skipped; others get `ID_5ET_LANGUAGE_*` |
| `condition`, `disease`, `status` | `Condition` | |
| source / `books.json` | `Source` element per pack | name from `books.json`, then a built-in table, then the abbreviation |
| `{@tag}` markup, `entries` | HTML subset in `description`; short plain text in `sheet` | links become `<a class="ref" data-element="ID">` when the target was imported |
| `reprintedAs`, SRD match | `maps_to` (equivalent) | by name, using `--srd-dir` packs and the 2014/2024 reprint links |
| fluff | `<h4>Lore</h4>` section in the description | only with `include_fluff` |

Anything that has no rule equivalent is kept as text or data and never dropped silently:
`extra.attributes["5et:<what>"]` carries the unconverted detail (skill/weapon/tool/armor
entries that did not map, unparsed prerequisites, ...), and each loss is a `partial` note.

## Lossy areas

- Alternative ability score arrays: only the first is converted; a "+2 choose" is offered as two +1 picks.
- Subrace `overwrite` / `null` of ability, proficiencies, resistances, feats and additional
  spells gates the parent's rules; other replaced properties (speed, size, ...) are reported.
- Condition immunities, tremorsense, weapon-by-filter proficiencies, Weapon Mastery slot counts,
  and prerequisites other than level/ability/spellcasting are kept as text, `5et:*` attributes or `data` only.
- Class table: only numeric columns drive rules; dice and other columns live in `data.class_table`.
  Third casters use the core `SLOTS_THIRD` table.
- Images, maps and stat blocks (`type: image`, `statblock`, ...) are dropped and counted as warnings;
  entry types without a renderer are reported as warnings.
- `_mod` modes the resolver does not know are reported as warnings and left unapplied.
- Class and subclass features whose source is not selected are reported as missing on the class.
- Catalog entries (`itemProperty`, `itemType`, `itemEntry`, `skill`, `sense`) and languages already in
  the core pack are skipped, not converted.
- Variant items are generated only for base items of the same edition as the variant; each
  magic variant is also emitted as its generic item ("Weapon of Warning") for item groups/links.
- Item references without a source default to the DMG (5etools' rule), then the item's own source.
- A language choice that mixes named languages with "other"/"any" is widened to any language.
- Optional feature / feat progressions (array or object) are running totals: each level adds the
  difference.

Run the gated corpus tests with `BOREALIS_5ETOOLS_DIR` set to a full 5etools checkout. They
require at least 97% of entries converted (at most 260 skipped and 440 partial), at least 85
packs and 12,000 elements, and a clean level 1-20 sweep of at least 25 classes and 300
subclasses.
- Spell class lists need `generated/gendata-spell-source-lookup.json`; without it spells have no class `supports`.
