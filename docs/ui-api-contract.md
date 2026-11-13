# UI <-> backend API contract (Tauri commands)

Status: implemented by the frontend in `app/ui/src/api/` (the TypeScript is the source of truth; this file explains
it for the Rust side). Files: `contract.ts` (the `ApiMethods` interface, the `COMMANDS` name table, events),
`types/*.ts` (all payload types), `tauri.ts` (real client), `mock/` (in-memory backend used in browsers and tests).
Clean-room note: sample data in the mock is original and SRD-generic.

## 1. Conventions

- **80 commands**, one Rust `#[tauri::command]` per row of the table in section 3. Rust names are the snake_case
  names in `COMMANDS`; the UI never calls anything else.
- **Arguments** are passed as one flat JSON object whose keys become the command's *named parameters*
  (Tauri maps camelCase JS keys to snake_case Rust parameters). Example:
  `invoke("apply_selection", { characterId, slotId, elementId })` -> `fn apply_selection(character_id: String, slot_id: String, element_id: String)`.
  Where the TypeScript wraps a struct (`request`, `patch`, `options`, `query`) the Rust parameter has that name
  and deserialises the struct (serde `rename_all = "camelCase"`).
- **Results** serialise with camelCase field names (serde `rename_all = "camelCase"`); enums are lowercase/snake strings exactly
  as in the TS unions (`dnd2014`, `point_buy`, `two-handed`, ...).
- **Errors** reject with `{ code, message, details? }` (`ApiError`). Codes: `not_found`, `invalid_argument`,
  `budget_exceeded`, `requirement_failed`, `ai_unavailable`, `content_error`, `io_error`, `internal`. Messages are
  user-presentable English; the UI shows them in a toast verbatim. Never leak paths or stack traces in `message`.
- **Snapshot-returning mutations.** Every character mutation returns the complete new `CharacterSnapshot` (section 2).
  The UI replaces its copy; it never merges or patches. A mutation that fails leaves the character unchanged.
- **Reprocess is backend-owned.** After any registration change the engine recomputes everything; the snapshot's slots,
  stats and spellcasting reflect the converged state. Stale picks are dropped by the backend.
- **Opened characters.** The backend keeps the opened characters in memory keyed by `characterId`
  (`create_character` and `load_character` open one; `close_character` releases it). Mutations mark `dirty = true`.
  `save_character` is a silent save to the existing path; autosave in the UI calls the same command after 1.5 s of quiet.
- **Binary data** crosses as base64 strings (imports) or `data:` URLs (portraits). Exports return `ExportResult`:
  desktop backends write the file (native save dialog when no path is given) and return `path`; when there is no
  filesystem access (mobile share sheet, browser) return `dataBase64` and the UI triggers the download/share.

## 2. The `CharacterSnapshot`

| Field | Meaning |
|---|---|
| `id, name, ruleset, portrait, group, favorite, dirty` | identity and library metadata (`ruleset` = `dnd2014` or `dnd2024`) |
| `level` (`LevelState`) | total level, XP, `nextLevelXp`, `levelUpAvailable` (XP crossed a threshold; **never auto-level**), `canLevelUp/Down`, `canMulticlass` (+ reason), and per-class `ClassLevelInfo` (slot id, class, level, hit die, subclass, resolved per-level HP values) |
| `abilities` (`AbilityState`) | method, base scores, bonuses with source labels, final/modifier/save, `check` (the ability check bonus with every-check and D20 Test modifiers; the Play tab rolls it), cap and `overCap`, `pointBuy` (budget/spent/remaining/costs/min/max) or `standardArray` (values/assigned/unassigned) or roll history |
| `stats` | proficiency bonus, AC, initiative, speed, max HP, passive Perception, 18 skills (+ proficiency level), languages, proficiencies, selected features |
| `slots` + `progress` | every selection slot (section 2.1) and `{ total, remaining, nextSlotId, byTab }` |
| `spellcasting` | per spellcasting class: ability, DC, attack, `attackRoll` (the attack with D20 Test modifiers), model (`spellbook`/`prepared`/`known`), cantrip and prepared limits, spell list with prepared flags and `ready` (castable now: cantrips, a known caster's known spells, else prepared or always prepared); plus slot rows and `multiclassSlots` (non-null when 2+ casting classes); a class's Pact Magic slots are its own `pact` row (short or long rest recovery), never in the slot rows |
| `inventory` | items, coins, total gp, carried weight / capacity (STR x 15), attunement `used/max`, storage names, **transient `warnings`** |
| `details` | the Manage text fields (name excluded: use `update_character_meta`) |
| `attacks` | manual and automatic attacks; automatic ones carry the computed `attackBonus` and `damage` (spell attack counted once everywhere); every row carries `toHit` (what an attack roll adds, D20 Test modifiers included; null for a save row or a typed bonus that is not a number) |
| `companion` | the granted companion, else the first one added; `null` without companions |
| `companions` | every companion (GitHub #13): the granted one (`id` `granted`) first, then the ones added by hand (`companion-<n>`) |
| `options, availableOptions` | character options (multiclassing, feats, average HP, house rules) and the option definitions to render |
| `sourceRestrictions` | enabled source ids, `null` = unrestricted |
| `notices` | informational diagnostics, e.g. import `LegacyDivergence` lines |

### 2.1 Selection slots

`SelectionSlot { id, tab, groupId, groupLabel, title, elementType, ordinal, of, required, locked, chosen, requiredLevel, allowReplace, hint }`.

- `id` is stable across snapshots (owner element + rule index + ordinal). The UI uses it as the React key and as
  the `slot` URL parameter for the Next-selection flow.
- `tab` places the slot: `race`, `class`, `background`, `abilities` (ASI picks), `feats`, `languages`, `proficiencies`,
  `companion`, `spells`. Multiclass slots carry `groupId`/`groupLabel` so the UI groups them under the class slot. A `spells` slot that
  feeds no spellcasting class (`classSlotId` null: a feat's or species' picks) names its source in `groupLabel`
  (`Magic Initiate (Cleric)`, the name its spellcasting block takes once it exists).
- `locked` = the rule has a `default`, exactly one candidate, and the slot holds it: shown, not editable (Aurora,
  engine-optional-sole-option). Nothing else is chosen automatically: a slot's `default` is chosen once, when the slot
  appears; a sole candidate without a default stays open for the user. `apply_selection` / `clear_selection` on a
  locked slot fail with `invalid_argument`. `required=false` = optional pick (collapsed).
- `progress.remaining` counts required, unchosen slots; `nextSlotId` follows the engine's Selection Flow priority
  (race, class, archetype, multiclass, background, ASI, language, proficiency, feat, companion, spell).
- `get_slot_options` returns the *currently valid* candidates (source restrictions, requirements, duplicates already
  applied; failing multiclass prerequisites are hidden). `disabledReason` may mark visible-but-unselectable rows.
  HR3 (GitHub #24): with the house rule `ignoreFeatPrerequisites` / `ignoreMulticlassPrerequisites` on, feats /
  multiclasses whose prerequisites fail are offered too, with `prerequisitesUnmet: true` (the character's own classes
  stay excluded). The chosen pick carries `SelectionSlot.prerequisitesUnmet`, a multiclass `ClassLevelInfo.prerequisitesUnmet`
  and a play feature `prerequisitesUnmet`; the UI marks each "prerequisites not met". Switching a rule off never prunes:
  the pick is rejected (kept in the file, not applied) and a notice names it until the rule is on again or the pick changes.
  An open slot that offers nothing carries `emptyReason` (U14): `otherEdition` (`{ edition, rejected, notAllowed }`:
  options only a character of that edition can use, R4, and options the "other edition" switch would show) and
  `restricted` (options only the source restriction hides). `problem` (C1, GitHub #11) says what content behind the
  choice can't be read (its own filter, or options whose requirements can't be parsed); such a problem is not repeated
  in `notices`, and an unreadable expression of content the character neither holds nor meets is not a notice at all.
  `apply_selection` must re-validate and fail with `requirement_failed` if the element is no longer valid.
- B3 (GitHub #35): extras a DM grants. `list_added_pick_options({ kind, spellcasting? })` (`kind`: `feat`, `spell`,
  `language`, `proficiency`; a spell names its spellcasting class by `className`) returns `AddPickOption`s: the kind's
  options in the character's edition and unrestricted sources that it does not have yet, only those whose prerequisites
  hold (a spell: on the class's lists, of a level it casts) unless `ignoreFeatPrerequisites` is on, then every one
  (`qualifies: false` on the rest). `add_pick` adds one (`requirement_failed` when it is not offered, `invalid_argument`
  for a spell without a class); `remove_pick({ inventoryId })` removes it. Under the hood a pick is Aurora's own
  "Additional ..." proxy item, equipped and hidden, so it behaves like one from the equipment screen; it is listed in
  `CharacterSnapshot.addedPicks` (`applied: false` while kept but withheld, `prerequisitesUnmet` under the house rule)
  and never in `inventory`. An added spell's `SpellEntry.added` and an added feat's play feature `added` are true.
- B-55 (`specs/version-rows-and-filters.md`): `get_slot_options` returns one row per name (type family + normalized
  name). A row with two or more offered options lists them in `versions` (`versions[0]` is the row itself: the chosen
  version, else the default order: character's edition, imported before bundled, source A-Z, id). Paging is by row and
  `total` counts rows; omit `limit` (or pass 0) to get every row. `query` matches the normalized row name.
  `switch_version` replaces a chosen row's pick with another of its versions in place, keeps the sub-choices that the
  new version offers again (same rule name and ordinal, same element or same name) and returns the dropped slot titles.
- 2024: background ability increases are ordinary slots (a +2 slot and a +1 slot with ability options), the origin feat is
  a `locked` feats slot, weapon mastery is a `weapon-mastery` slot group on the class tab. The ruleset badge and
  terminology (`Species` vs `Race`) come from `get_ruleset_profile`.

### 2.2 Rules the backend must enforce (UI also reflects them)

| Rule | Behaviour |
|---|---|
| Point buy | `set_base_score` fails with `budget_exceeded` when `spent > budget`; range 8..15 unless `pointBuyUnrestricted` (house rule) or method `manual` (settings min/max). |
| Prerequisite house rules (HR3) | `ignoreFeatPrerequisites`: feats skip their requirements and structured prerequisites. `ignoreMulticlassPrerequisites`: the multiclassing gate and the Multiclass picker read ability score tests as met (`!<own class>` still applies). Stored in the character file as `house_rules.ignore_feat_prerequisites` / `ignore_multiclass_prerequisites` (absent when off). |
| Standard array | `assign_standard_array` assigns **or swaps**; a value can never be duplicated or lost; `value: null` unassigns. Base scores of unassigned abilities are reported as 8. |
| rolls | `roll_abilities` (all, or one ability) is only valid for `roll_3d6`/`roll_4d6`; each roll is appended to `abilities.rolls` (dice + total). Backend owns the RNG. |
| Level down | `level_down` removes the most recent level, drops an emptied multiclass slot, and sets XP to the new level's threshold. |
| Attunement | equipping a 4th attunable item succeeds **unattuned** and adds a warning string to `inventory.warnings`; `set_attuned` beyond the limit fails with `requirement_failed`. The warning list is transient: it is present only in the snapshot returned by the mutation that caused it. |
| Buying | `add_item(buy: true)` pays from the purse with make-change; insufficient funds -> `requirement_failed` (the UI then offers "add anyway" via `buy: false`). Total copper is invariant under change-making. |
| HP and prepared spells | HP rolls are persisted inputs (`set_hp_roll`); new levels use the average when the option is on. With it off, each level uses its roll or, without one (`value: null` clears it, stored as 0), the average; `ClassLevelInfo.hpAverage` says which. Prepared-spell maximum is clamped to at least 1. |
| Source restrictions | before `set_source_restrictions` the UI calls `preview_source_restrictions` and warns about picks that would be dropped. |

## 3. Commands

### App

| Command | Args (camelCase keys) | Returns |
|---|---|---|
| `app_version` | none | `string` |
| `open_link` | `{ url: string }` | `void`: opens one of the app's own web pages (`ui/src/lib/links.ts` = `src-tauri/src/links.rs`: website, changelog, GitHub Releases, Discord invite, privacy page, download page) in the system browser on every platform (U15, B-86); any other address is `invalid_argument` |

### Characters (library)

| Command | Args (camelCase keys) | Returns |
|---|---|---|
| `list_characters` | none | `CharacterSummary[]` |
| `create_character` | `{ request: NewCharacterRequest }` | `CharacterSnapshot` |
| `load_character` | `Cid` | `CharacterSnapshot` |
| `get_snapshot` | `Cid` | `CharacterSnapshot` |
| `save_character` | `Cid` | `CharacterSnapshot` |
| `save_character_as` | `characterId, { name?: string; path?: string }` | `CharacterSnapshot` |
| `close_character` | `characterId, { discard: boolean }` | `void` |
| `delete_character` | `Cid` | `void` |
| `duplicate_character` | `Cid` | `CharacterSummary` |
| `update_character_meta` | `characterId, { name?: string; group?: string; favorite?: boolean },` | `CharacterSnapshot` |
| `set_portrait` | `characterId, { dataUrl: string \| null }` | `CharacterSnapshot` |
| `import_character` | `{ fileName: string; contentBase64: string }` | `ImportReport` |
| `export_character` | `Cid` | `ExportResult` |

### Builder: selections

| Command | Args (camelCase keys) | Returns |
|---|---|---|
| `get_slot_options` | `SlotOptionsArgs` | `Page<SlotOption>` |
| `apply_selection` | `characterId, { slotId: string; elementId: string }` | `CharacterSnapshot` |
| `clear_selection` | `characterId, { slotId: string }` | `CharacterSnapshot` |
| `switch_version` | `characterId, { slotId: string; elementId: string }` | `SwitchVersionResult` (`{ snapshot, dropped }`) |
| `replace_spell` | `characterId, { slotId: string }` | `CharacterSnapshot` |

### Builder: levels

| Command | Args (camelCase keys) | Returns |
|---|---|---|
| `set_xp` | `characterId, { xp: number }` | `CharacterSnapshot` |
| `level_up` | `characterId, { classSlotId: string }` | `CharacterSnapshot` |
| `level_down` | `Cid` | `CharacterSnapshot` |
| `add_multiclass` | `Cid` | `CharacterSnapshot` |
| `set_hp_roll` | `characterId, { classSlotId: string; level: number; value: number \| null }` (null = use the average) | `CharacterSnapshot` |

### Builder: abilities (backend enforces budgets)

| Command | Args (camelCase keys) | Returns |
|---|---|---|
| `set_ability_method` | `characterId, { method: AbilityMethod }` | `CharacterSnapshot` |
| `set_base_score` | `characterId, { ability: AbilityKey; value: number }` | `CharacterSnapshot` |
| `roll_abilities` | `characterId, { ability?: AbilityKey }` | `CharacterSnapshot` |
| `assign_standard_array` | `characterId, { ability: AbilityKey; value: number \| null },` | `CharacterSnapshot` |
| `reset_abilities` | `Cid` | `CharacterSnapshot` |

### Builder: options, restrictions, ruleset

| Command | Args (camelCase keys) | Returns |
|---|---|---|
| `set_character_options` | `characterId, { options: Partial<CharacterOptions> }` | `CharacterSnapshot` |
| `preview_source_restrictions` | `characterId, { enabledSources: string[] \| null }` | `RestrictionPreview` |
| `set_source_restrictions` | `characterId, { enabledSources: string[] \| null }` | `CharacterSnapshot` |
| `set_allow_cross_edition` | `characterId, { allow: boolean \| null }` | `CharacterSnapshot` |
| `get_compatibility_report` | `characterId` | `CompatibilityReport` = `{ strict, entries: [{ element, from, converted, notes }] }` |
| `get_ruleset_profile` | `{ ruleset: Ruleset }` | `RulesetProfile` |
| `class_counts_by_edition` | none | `ClassCounts` = `{ dnd2014, dnd2024 }`: classes a new character of each edition is offered (U14) |
| `start_over_in_edition` | `characterId, { ruleset: Ruleset }` | `CharacterSnapshot`: a NEW character of the other edition with this one's name, details, options, level, scores and sources, saved and opened; the old file is not changed (U14) |
| `preview_ruleset_switch` | `characterId, { target: Ruleset }` | `RulesetSwitchPlan` |
| `switch_ruleset` | `characterId, { target: Ruleset }` | `CharacterSnapshot` |

### Magic

| Command | Args (camelCase keys) | Returns |
|---|---|---|
| `set_spell_prepared` | `characterId, { classSlotId: string; spellId: string; prepared: boolean },` | `CharacterSnapshot` |

### 2024 play state and Convert to 2024

Every command returns the full `CharacterSnapshot`; the new blocks live in `snapshot.play` (`PlayState`, camelCase) and on `attacks[]` (`masteries`, `critRange`, `dieFloor`, `offhandDamage`). A weapon may have several mastery properties (RL10): `attacks[].masteries` is `[{name, text, mastered}]` (one entry per property, omitted when none) and each `play.masteries[]` pick is `{weapon, name, properties: [{name, text}]}`.

| Command | Args (camelCase keys) | Returns |
|---|---|---|
| `preview_convert_to_2024` | `characterId` | `ConversionPreview` `{characterId,name,items:[{category,label,outcome:kept\|mapped\|dropped\|needs_choice,fromId,toId,reason,what,crossEdition,kind}],counts,before,after}` (saves nothing; 2014 only, else `requirement_failed`) |
| `apply_convert_to_2024` | `characterId` | `{newCharacterId, snapshot}`: saves a new library character ("<name> (2024)", `meta.converted_from` = old id) and opens it; the original file is untouched |
| `spend_resource` / `restore_resource` | `characterId, resourceId, uses?` (default 1) | `CharacterSnapshot` (`requirement_failed` when too few uses, `not_found` for an unknown id) |
| `short_rest` / `long_rest` | `characterId` | `CharacterSnapshot` (rests end toggles; Long Rest refills and lowers exhaustion by 1 in 2024). Spell tracker: a Short Rest gives back the Pact Magic slots and short-rest trackers; a Long Rest every slot and every short- or long-rest tracker, and ends concentration |
| `set_toggle` | `characterId, toggleId, on` | `CharacterSnapshot` (Rage etc.; `play.toggles[].active`, `play.defenses`) |
| `set_exhaustion` | `characterId, level` | `CharacterSnapshot` |
| `set_heroic_inspiration` | `characterId, has` | `CharacterSnapshot` |
| `choose_weapon_masteries` | `characterId, weaponIds: string[]` | replaces all picks in slot order (slots = `Weapon` selects named "...Mastery"); `invalid_argument` for duplicates, `requirement_failed` for too many / not allowed |
| `set_wild_shape_forms` | `characterId, formIds: string[]` | `CharacterSnapshot` (limits in `play.wildShape`) |
| `choose_equipment_package` | `characterId, packageId, option` | `CharacterSnapshot` (once per package; items + coins added; option `none` declines it, "I'll buy my own", and leaves it takeable) |
| `spend_spell_slot` / `restore_spell_slot` | `characterId, level, pact?, uses?` (default 1) | `CharacterSnapshot`: relative, like `spend_resource` (one more used / one given back, never below none; so quick presses add up and a press after a rest never undoes it). With `pact` the Pact Magic pool. `requirement_failed` without such slots or when too few are left |
| `set_concentration` | `characterId, spellId: string \| null` | `CharacterSnapshot` (a known or prepared concentration spell, replacing any other; `null` ends it; else `requirement_failed`) |
| `add_tracker` / `update_tracker` | `characterId, (trackerId,) name, max, reset: short_rest \| long_rest \| never, countsUp?` | `CharacterSnapshot` (bidi and zero-width characters are stripped from the name; name 1-60 characters, max 1-999, at most 50 trackers, else `invalid_argument`; an edit clamps the used count; unknown id `not_found`) |
| `remove_tracker` | `characterId, trackerId` | `CharacterSnapshot` |
| `spend_tracker` / `restore_tracker` | `characterId, trackerId, uses?` (default 1) | `CharacterSnapshot`: relative; `requirement_failed` when too few pips are left |
| `take_damage` / `regain_hit_points` | `characterId, amount` (1-9999, else `invalid_argument`) | `CharacterSnapshot` (B-194): relative; damage comes off temporary hit points first and hit points stop at 0; healing stops at the maximum |
| `set_temp_hit_points` | `characterId, amount` (0-9999) | `CharacterSnapshot`: replaces the temporary hit points (they don't stack); 0 ends them |
| `spend_hit_die` | `characterId, classId, roll` (1..die, else `invalid_argument`) | `CharacterSnapshot`: one die spent, heals the roll plus the Constitution modifier (2024: at least 1), never above the maximum; `requirement_failed` when none is left, `not_found` for a class without hit dice |
| `restore_hit_die` | `characterId, classId, uses?` (default 1) | `CharacterSnapshot`: gives spent dice back, heals nothing |

`PlayState` fields: `speeds, resources, resourceCosts, toggles, exhaustion, inspiration, inspirationSources, martialArts, masteries, defenses{resistances,immunities,vulnerabilities,conditionImmunities}, senses, advantages, initiativeAdvantage, notes, carry, wildShape, equipmentPackages, compatibility{strict,entries}, features[{id,name,ty,source,level,grantedBy,description,sheetSummary,displayed}], freeCasts, saveDcs, spellSlots[{level,max,used}], pactSlots{level,max,used}|null, concentration{id,name,level}|null, concentrationSpells, trackers[{id,name,max,used,reset,countsUp}], hitDice[{classId,className,die,total,used,notation}], hitPoints{max,current,temp}`. A Long Rest restores every hit point, ends temporary hit points and gives back spent hit dice (2024: all; 2014: up to half the total, at least one, largest dice first). Each resource also carries `die` (`1d6`, from a stat named after the resource with ` die`, `:die`, `:dice`, `:size` or `:dice:size`; null when none).
The spell tracker blocks (GitHub #2) are clamped to the current maximums; every mutation also writes that clamp back to the
saved character (`build.play.spell_slots_used`, `pact_slots_used`, `concentration`, `trackers`, all omitted when empty).
`build.play` loads leniently: a bad entry is dropped or repaired with a warning instead of failing the character, and keys
this version does not know (in `build`, `build.play` and each tracker) are kept and written back.
They are the engine blocks with keys camelCased (types in `ui/src/api/types/play.ts`).

### Equipment

| Command | Args (camelCase keys) | Returns |
|---|---|---|
| `search_items` | `ItemQuery & { characterId?: string }` | `Page<ItemSummary>` |
| `add_item` | `characterId, { itemId: string; quantity: number; buy: boolean; baseItemId?: string }` | `CharacterSnapshot` |
| `remove_item` | `characterId, { inventoryId: string; all: boolean }` | `CharacterSnapshot` |
| `equip_item` | `characterId, { inventoryId: string; slot: EquipRequest }` | `CharacterSnapshot` |
| `unequip_item` | `characterId, { inventoryId: string }` | `CharacterSnapshot` |
| `set_attuned` | `characterId, { inventoryId: string; attuned: boolean }` | `CharacterSnapshot` |
| `store_item` | `characterId, { inventoryId: string; storage: 0 \| 1 \| 2 }` | `CharacterSnapshot` |
| `move_item` | `characterId, { inventoryId: string; direction: "up" \| "down" }` | `CharacterSnapshot` |
| `extract_pack` | `characterId, { inventoryId: string }` | `CharacterSnapshot` |
| `adjust_coins` | `characterId, { coin: CoinKey; delta: number }` | `CharacterSnapshot` |
| `convert_coins` | `characterId, { to: CoinKey }` | `CharacterSnapshot` |
| `update_item_details` | `characterId, inventoryId, patch: { displayName?, notes?, listInInventory? }` | `CharacterSnapshot` |
| `set_storage_names` | `characterId, { names: [string, string] }` | `CharacterSnapshot` |

### Explain

| Command | Args (camelCase keys) | Returns |
|---|---|---|
| `explain_stat` | `characterId, { stat: string }` | `StatTrace` |

`StatTrace` (`types/trace.ts`) is the engine's deterministic provenance for one statistic (`ac`, `initiative`,
`strength:score`, ...; the name is case-insensitive): `{ stat, base, steps[], value, diagnostics[] }`. Each step has
`group, kind (seed|derived|rule), label, elementId?, elementName?, ruleKind, value, rawValue?, bonus?, condition?,
included, reason?, inputs[]`. Invariant: `value` equals the sum of the `included` top-level step values; `inputs` explain
a referenced statistic and are not added again. An unknown statistic yields an empty trace with value 0. Prose
("explain this number") is layered on top by the AI side; the trace never changes a result. The mock returns one
seed step carrying the snapshot value.

### Manage

| Command | Args (camelCase keys) | Returns |
|---|---|---|
| `update_details` | `characterId, { patch: Partial<CharacterDetails> }` | `CharacterSnapshot` |
| `upsert_attack` | `characterId, { attack: Omit<Attack, "id"> & { id?: string } }` | `CharacterSnapshot` |
| `remove_attack` | `characterId, { attackId: string }` | `CharacterSnapshot` |
| `move_attack` | `characterId, { attackId: string; direction: "up" \| "down" }` | `CharacterSnapshot` |
| `update_companion` | `characterId, { companionId?: string; patch: CompanionPatch }` | `CharacterSnapshot` |
| `add_companion` | `characterId, { creatureId?: string \| null; name?: string }` | `CharacterSnapshot` |
| `remove_companion` | `characterId, { companionId: string }` | `CharacterSnapshot` |

### Sheet

| Command | Args (camelCase keys) | Returns |
|---|---|---|
| `render_sheet` | `characterId, { options?: SheetOptions }` | `SheetPreview` |
| `export_sheet` | `characterId, { options?: SheetOptions }` | `ExportResult` |

### Compendium

| Command | Args (camelCase keys) | Returns |
|---|---|---|
| `search_compendium` | `{ query: CompendiumQuery }` | `Page<ElementSummary>` |
| `get_element` | `{ elementId: string }` | `ElementDetail` |
| `compendium_facets` | none | `CompendiumFacets` |

### Content

| Command | Args (camelCase keys) | Returns |
|---|---|---|
| `list_packs` | none | `ContentPack[]` |
| `import_pack` | `{ request: ImportPackRequest }` | `ImportPackResult` |
| `set_pack_enabled` | `{ packId: string; enabled: boolean }` | `ContentPack[]` |
| `remove_pack` | `{ packId: string }` | `ContentPack[]` |
| `list_sources` | none | `ContentSource[]` (`edition`, `editionSetByUser`, `editionEditable`: IM14) |
| `set_source_edition` | `{ sourceId: string; edition: Ruleset }` | `ContentSource[]`: an imported book's edition, stored in `content-prefs.json` (`sourceEditions`), applied at load, never written into the pack (IM14) |
| `list_indexes` | none | `ContentIndex[]` |
| `add_index` | `{ url: string }` | `ContentIndex[]` |
| `remove_index` | `{ url: string }` | `ContentIndex[]` |
| `update_indexes` | none | `UpdateIndexesResult` |
| `clear_downloaded_content` | none | `ContentPack[]` |

### 5etools local import

| Command | Args (camelCase keys) | Returns |
|---|---|---|
| `scan_5etools_folder` | `path` | `FiveToolsScan` = `{ sources: [{ abbr, name, ruleset, category, counts }] }` |
| `import_5etools` | `request: { path, sources: string[], includeFluff }` | `FiveToolsImportReport` = `{ converted, replaced, partial: [{element, kind, reason}], skipped: [{element, kind, reason}], packs: ContentPack[] }` |

The user supplies a path to their own local 5etools `data/` folder (typed path; a native folder dialog is optional on
desktop). `scan_5etools_folder` is read-only. `import_5etools` emits `borealis://import_5etools_progress` (`ContentProgress`,
phase `converting` then `done`), installs one pack per source into the library with `kind: "local-import"` and
`redistributable: false` (UI badge: "Local import — not redistributable"), and triggers `content-reloaded`. No default
download URL exists; nothing is fetched from the network.

Fields added by the 5etools import gaps work:

- `FiveToolsSource.category`: `"book" | "adventure" | "homebrew" | "other"`, from `books.json` / `adventures.json` or a homebrew
  file's `_meta`; `"other"` when listed nowhere. `ruleset` may be `"mixed"` (entries of both editions). The UI groups sources by it.
- `ImportNote.kind` (on every `partial` and `skipped` note): entity kind of the element (`"feat"`, `"language"`, `"subclass"`, ...).
- `FiveToolsImportReport.replaced`: number of packs that already existed and were replaced in place (re-import).

### Settings

| Command | Args (camelCase keys) | Returns |
|---|---|---|
| `get_settings` | none | `AppSettings` |
| `save_settings` | `{ patch: Partial<AppSettings> }` | `AppSettings` |

### AI

| Command | Args (camelCase keys) | Returns |
|---|---|---|
| `ai_status` | none | `AiStatus` |
| `ai_start_task` | `{ request: AiTaskRequest }` | `AiTaskHandle` |
| `ai_cancel_task` | `{ taskId: string }` | `void` |
| `ai_validate_homebrew` | `{ json: Record<string, unknown> }` | `ValidationReport` |
| `ai_add_homebrew_to_pack` | `{ json: Record<string, unknown>; packId?: string }` | `ContentPack` |
| `ai_host_enable` / `ai_host_disable` / `ai_host_status` / `ai_host_begin_pairing` | none | `AiHostStatus` |
| `ai_host_revoke` | `{ deviceId: string }` | `AiHostStatus` |

`SlotOption` (the item type of `get_slot_options`) is a `SlotOptionBase` (an `ElementSummary` plus optional
`disabledReason`, edition marks and `bundled`) plus optional `versions: SlotOptionBase[]` (B-55); `Cid` in the
table means a single `characterId` string argument.

## 4. Events (push, backend -> UI)

Emit with `app.emit(name, payload)`; the UI subscribes through `Api.subscribe`.

| Event name | Payload | When |
|---|---|---|
| `borealis://ai-event` | `AiEvent` (`started`, `delta`, `tool`, `result`, `error`, `done`) | during an AI task, in order, tagged with `taskId` |
| `borealis://content-progress` | `ContentProgress` | during `update_indexes` / long imports |
| `borealis://import_5etools_progress` | `ContentProgress` | during `import_5etools` |
| `borealis://content-reloaded` | `{ reason }` | after content was hot-reloaded; the UI refetches packs and open pickers |

AI protocol: `ai_start_task` returns `{ taskId }` **immediately** (it must not wait for the model). Events may arrive
before the invoke promise resolves; the UI buffers events until it knows the task id, so ordering within a task is the
only requirement. Text arrives as `delta` chunks (append-only). Homebrew tasks finish with one `result` event carrying a
draft element JSON (`kind: "homebrew"`), then `done`. `ai_cancel_task` ends the stream with `done { cancelled: true }`.
`ai_validate_homebrew` runs the real content schema; `ai_add_homebrew_to_pack` writes to the user's homebrew pack and
triggers `content-reloaded`. When no CLI is available `ai_status` returns `availability: "unavailable"` (or
`needs_pairing` on mobile) and `ai_start_task` fails with `ai_unavailable`; the UI shows the explanation and disables sending.

### Import anything

Event `borealis://import-event` (`import:event`): `{draftId, stage: detect|plan|generate|validate|repair|ready|error, progress 0..1, message, element?}`.
`ready` is the last event of a start/revise run; the UI then calls `import_get_draft`. Events may arrive before `import_start` returns (the UI buffers).

| Method | Command | Args -> result |
|---|---|---|
| `importStart` | `import_start` | `{sources:[{kind:"text",text}\|{kind:"file",path,name,mime}\|{kind:"bytes",name,mime,base64}], hint?:{kind?,edition?,parentClassId?}}` -> `{draftId}` |
| `importGetDraft` | `import_get_draft` | `{draftId}` -> `ImportDraft` |
| `importChooseParent` | `import_choose_parent` | `{draftId, classId}` -> `ImportDraft` (parentChoice cleared) |
| `importRevise` | `import_revise` | `{draftId, instruction, elementId?}` -> void; streams events, ends with `ready` |
| `importUpdateElement` | `import_update_element` | `{draftId, elementId, json}` -> `ImportDraft` (element re-validated; may become `failed`) |
| `importFinalize` | `import_finalize` | `{draftId, packName, prefix}` -> `ContentPack` (the spec's "PackInfo"; `redistributable:false`) |
| `importDiscard` | `import_discard` | `{draftId}` -> void |
| `importReportPreview` | `import_report_preview` | `{draftId?, packId?, note?, flagged?, includeSource?}` (exactly one of draftId/packId) -> `ImportReportPreview` (desktop) |
| `importReportSend` | `import_report_send` | `{previewId}` -> `{id}` (desktop) |

`ImportDraft`: `{draftId, stage, detected:{kinds,edition,names,confidence}, parentChoice?:{candidates:[{classId,name}]}, parentClassId?, elements:[ImportElement], sweep:{levelsPassed,errors}, suggestedPackName, suggestedPrefix}`.
`ImportElement`: `{id, type, name, level?, status: ok|repaired|needs_review|failed, sourceExcerpt, json, balanceFlags[], validationIssues[]}`;
`level` groups class/subclass features in review. `import_finalize` must fail while `parentChoice` is set or any element is `failed`.
File sources: the UI sends `file.path` when the webview provides it, else the bare file name; the backend needs a way to receive picked bytes.

Import bug reports (opt-in, decisions BR1-BR3; nothing is ever sent without the user pressing Send):
`ImportReportPreview`: `{previewId, payload, bytes, selfChecksFailed, sourceAvailable, sourceBytes|null, sourceLimit, endpoint}`.
`payload` is the exact `borealis-import-bug-report/1` JSON that `import_report_send` posts to
`https://borealisbuilder.com/api/import-report`: app version/OS/arch, edition, origin (`draft`|`pack`), counts, grouped error
`codes` (`rule_error`, `unlocated`, `unsupported`, `engine`, `invented_rule_kind`, `element_failed`, `user_flagged`, ...), the
import report (messages cut, long quotes removed), engine diagnostics (draft level sweep), element STRUCTURE (id, type, name,
rules, setters, requirements, sheet labels; descriptions and any prose longer than a short label removed), `flagged` ids, the
user's `note` (<= 2000 chars) and `source` (null unless `includeSource`; drafts only, <= 1 MB of source text). The backend keeps
the last 8 previews; `import_report_send` posts the stored bytes (HTTPS, no redirects) and forgets them; failures are `io_error`,
an expired preview `not_found`. Payloads over 1.4 MB are refused with `invalid_argument`.

### Homebrew creator

The 1.5.0 creator (decisions HC1-HC4; `specs/homebrew-creator-spec.md` is the source of truth, section 15.2). Types:
`ui/src/api/types/homebrew.ts` (TS) and `src-tauri/src/homebrew_dto.rs` (Rust, checked against the generated contract by
`src-tauri/tests/contract_homebrew.rs`; the form is `borealis_authoring::form::FormState`). Wrappers:
`src-tauri/src/commands_homebrew.rs`, delegating to `homebrew_packs.rs` (packs, delete), `homebrew_draft_api.rs`
(drafts), `homebrew_refs.rs` (picker rows) and `homebrew_creator.rs` (catalogs); the AI path keeps `homebrew_api.rs`.
Commands that write packs run through `commands_content.rs::blocking`.

**Feature switch (HC3).** `AppSettings.creatorEnabled` (default `false`, no settings control) shows START > HOMEBREW and the
creator's entry points; until 1.5.0 the UI reaches these commands only with it on (dev builds and the web mock's `?creator=1`).

**Not implemented yet.** Until a command's work item lands (spec section 21), it rejects with `requirement_failed` and
`details: { notImplemented: "<command>" }`. Implemented in the backend: the catalogs and kinds, packs, the element list,
drafts (new, open, update, save, discard, list), delete, duplicate and reference options (WB-04/05/06), the
validation of drafts with `homebrew_check_draft` (WB-07), Try it (WB-08), and bulk edit, undo, where-used and spell
lists (WB-23, `homebrew_expert.rs`, `homebrew_undo.rs`), and the pack file (WB-24, `homebrew_transfer.rs`). Copies (WB-22) are implemented, deep ones too. `homebrew_new_draft` with
`parentId` makes a child (WB-16). The web mock
implements packs, elements and drafts in memory (kinds are sample forms).

| Command | Args (camelCase keys) | Returns |
|---|---|---|
| `homebrew_kinds` | none | `HomebrewKindDescriptor[]` (fields, sub-tabs, bindings, editions; implemented, WB-04 descriptors) |
| `homebrew_block_catalog` | none | `HomebrewBlockSpec[]` (implemented, WB-05) |
| `homebrew_stat_catalog` | none | `HomebrewStatGroup[]` (implemented, WB-05; plus a group `yourContent` for names loaded packs use) |
| `homebrew_create_pack` | `{ request: HomebrewPackRequest }` (`name, code, edition: dnd2014\|dnd2024\|any, author?, description?, license?`) | `ContentPack` (`kind: "homebrew"`) |
| `homebrew_update_pack` | `{ packId, patch: HomebrewPackPatch }` | `ContentPack` |
| `homebrew_list_elements` | `{ packId }` | `HomebrewElementSummary[]` |
| `homebrew_new_draft` | `{ packId, kind, copyFrom?, parentId?, parentKey?, deepCopy? }` | `HomebrewDraftView` |
| `homebrew_open_draft` | `{ packId, elementId?, draftId? }` | `HomebrewDraftView` (resumes the element's edited draft; `draftId` resumes a never-saved element's draft) |
| `homebrew_update_draft` | `{ draftId, form: HomebrewForm }` | `HomebrewDraftView` (autosaved; preview, problems, generated data) |
| `homebrew_check_draft` | `{ draftId }` | `HomebrewProblem[]` (engine checks, the 1-20 sweep, choices that offer nothing, balance hints; never errors; runs off the main thread) |
| `homebrew_save_draft` | `{ draftId }` | `HomebrewSaveResult` (`{ pack, elementIds }`); `invalid_argument` with `details` = the problems when errors exist |
| `homebrew_discard_draft` | `{ draftId }` | `void` |
| `homebrew_list_drafts` | none | `HomebrewDraftSummary[]` |
| `homebrew_delete_elements` | `{ packId, elementIds, confirm }` | `HomebrewDeleteResult` (`{ deleted, usedBy, pack, undoId }`) |
| `homebrew_child_use` | `{ draftId, childKey }` | `HomebrewCharacterRef[]`: the local characters that use the saved child `childKey` of the draft or a saved child inside it (asked before it is removed: the save deletes it); a child the draft made, or an unknown key, is used by no one |
| `homebrew_duplicate_element` | `{ packId, elementId }` | `HomebrewDraftView` |
| `homebrew_bulk_edit` | `{ packId, elementIds, patch: HomebrewBulkPatch }` (`edition?`, `addTags?`, `removeTags?` together; or `moveToPackId`; or `duplicate: true`) | `HomebrewBulkResult` (`{ pack, elementIds, undoId }`); `invalid_argument` with `details` = the problems when an element can't take the change (nothing changes) |
| `homebrew_undo` | `{ undoId }` | `ContentPack` (a delete or bulk edit undone; `requirement_failed` when a pack changed since, `not_found` once used) |
| `homebrew_reference_options` | `{ target: HomebrewPickerTarget, query, filters: HomebrewPickerFilters, page: { offset, limit }, draftId? }` | `Page<HomebrewPickerRow>` |
| `homebrew_where_used` | `{ elementId }` | `HomebrewElementRef[]` (loaded elements that grant, select, require or list it; strongest use first) |
| `homebrew_spell_list` | `{ packId, listTag, expandedOf? }` | `HomebrewSpellListView` (every loaded spell and the pack's own, `onList`, `removable`; with `expandedOf` a subclass's expanded list, `expandedOf` set in the view) |
| `homebrew_set_spell_list` | `{ packId, listTag, expandedOf?, add, remove }` | `HomebrewSpellListView` (one write; own spells change their tags, loaded spells get the pack's append; an expanded list changes the own subclass's `spellcasting.extends` (or its own feature's that holds them), or the pack's append's `spellcasting` for a loaded subclass) |
| `homebrew_keep_spell_list_checks` | `{ packId, kept: HomebrewKeptChecks }` | `void` (a pack's unsaved SPELL LISTS checks, `{ list, pending: [{ id, on }] }`, kept on the device in `<data>/homebrew-drafts/spell-lists/<packId>.json`, never synced; an empty `list` or `pending` forgets them) |
| `homebrew_kept_spell_list_checks` | `{ packId }` | `HomebrewKeptChecks \| null` |
| `homebrew_trial_start` | `{ draftId, setup: HomebrewTrialSetup }` | `HomebrewTrialView` (`{ trialId, snapshot, ladder, problems, changed }`) |
| `homebrew_trial_update` | `{ trialId, change: HomebrewTrialChange }` | `HomebrewTrialView` |
| `homebrew_trial_end` | `{ trialId }` | `void` |
| `homebrew_trial_options` | `{ trialId, slotId, query?, limit?, offset? }` | `Page<SlotOption>` (a trial choice's options, as `get_slot_options`) |
| `homebrew_export_pack` | `{ packId, leaveOutCopiedText }` | `HomebrewExportResult` (`{ fileName, path, shared, dataBase64, leftOut }`: desktop saves through a save dialog, `path` null when cancelled; Android opens the share sheet, `shared`; elsewhere `dataBase64` for the web view's share sheet or a download) |

- **Pack file (WB-24, `homebrew_transfer.rs`, spec 14.2).** `homebrew_export_pack` writes a `.borealis-pack.zip`: the pack
  folder `<packId>/` with `pack.json`, its content file and `creator.json` (no drafts, no `.trash`, no `pack-meta.json`).
  With `leaveOutCopiedText` the elements `creator.json` marks `nonSrdText` export without their text (description and sheet
  texts empty; a creature's stat-block texts read "Text left out of this shared copy." and its flavour text goes); the marks
  stay, so the imported pack is still not redistributable. Only `kind: "homebrew"` packs export (`requirement_failed`).
  `HomebrewElementSummary.copiedText` marks the elements that still hold copied text (the export dialog's count). Import
  goes through `preview_import_pack`: one `.zip` holding a `creator.json` beside a `pack.json` is a creator pack, installed
  unconverted as `kind: "homebrew"`; its preview carries `homebrew: { code, codeTaken, copyCode, copyName, replaceUsedBy, replaceDrops }`, no `books`
  (`replaceUsedBy`: the local characters that use an element of the installed pack the file lacks, the delete check's
  scan; `replaceDrops`: those elements' names; both empty when Replace loses nothing, and the UI's Replace asks first
  otherwise).
  When its pack is installed (`replacesExisting`), `install_import_preview` replaces it (the old folder is copied to
  `<data>/sync/trash/<ms>-<packId>`, swept after 30 days) or, with `asCopy: true`, installs a copy: code `copyCode`, pack id
  `homebrew-<copyCode>`, name `copyName`, every `ID_<CODE>_` token of the pack and every element id given again with the
  references following (other packs' ids untouched), `creator.json` re-keyed with its provenance, no retired ids. A code
  another pack uses (`codeTaken`, pack not installed) installs only as a copy. `content-reloaded` reason
  `homebrew-pack-imported`. The creator workspace opens the open element again on that reason and on `device-sync`, so a
  draft whose element changed asks "Keep my draft / Use the saved version" (spec 10.1, 14.4).
- **Expert features (WB-23, `homebrew_expert.rs`).** *Where used* runs the delete scan's reference scanner
  (`borealis_authoring::pack::use_of`: ids as whole tokens, so `!ID_A,ID_B` counts) over every loaded element:
  `grants` (a grant or a custom rule naming it), `selects` (a choice offering it by id, or by a tag it carries, of its
  type), `requires`, `lists` (a spell list that offers the spell, an `extend`, or the id anywhere else: a package,
  `data.owner`). *SPELL LISTS*: own spells change their tags through their form (`data.classes` follows), loaded
  spells get the pack's append `{id, supports: [tag]}` (HC5, never an in-place edit); only own spells and own appends
  come off. A subclass's expanded list (P2b; picker type `Expanded Spell List` in `homebrew_reference_options`: the
  subclasses with one, and a non-bundled pack's subclasses whose class casts) is a list too: an own subclass's
  `spellcasting.extends` (`{name: <class list>, extend: true}`), or the pack's append `{id, spellcasting}` on a loaded
  subclass (adopted by a subclass that has no spellcasting of its own). *Bulk edit* changes each element through its form (`form::bulk_patched`: the tags field, a spell's lists;
  the edition field; a class keeps its edition, R4); a move keeps ids, `creator.json` entries and editions (an element
  that followed its pack's edition takes it along) and retires the ids in the old pack; refused when an element has
  unsaved changes; duplicates use WB-22's copier. Deletes and bulk edits return an `undoId`: `homebrew_undo` writes
  every pack it touched back as it was (content and `creator.json`), as one change, when no pack changed since; made
  elements' ids are retired. Undo records live in memory (the last 20). *DATA view*: `generated.tree` is
  `HomebrewDataNode[]` (one top line per element, keys in file order, setters as `name: value`, descriptions `html`,
  expressions `expr`, rules with their block sentence); `generated.json` is the element (one) or the elements (several)
  as the content file writes them. `HomebrewElementSummary` carries `tags` and `text` (summary + description as plain
  text, 400 characters) for the list filter.
- **Try it (WB-08, `homebrew_trial.rs`).** A trial is a test character on a private engine that holds the draft's
  elements (with the ids they would get now); it is never saved, listed or opened as a character, and never changes the
  open character. `setup: {}` takes the defaults: the draft's (else its pack's) edition, the SRD picks of that edition
  (Fighter, Human, Soldier, the first SRD subclass), the standard array by the class's main abilities, and the level
  the draft is first on the character. The draft takes its slot by element type (class/species/background at level 1,
  else the first choice that offers it, a feat at the first ability score improvement, a spell through its class's
  lists, an item added and equipped, anything else as a test grant). A choice holding the draft comes back `locked`
  and refuses changes (`invalid_argument`). `change` is a level (clamped to 1-20), a setup (another edition or
  `asSecondClass` starts from that setup's picks; new `abilityScores` keep the choices) or `{slotId, elementId}`
  (`null` clears). `asSecondClass` builds Fighter 1, then the draft from level 2 through the `Multiclass` slot and
  reports a missed multiclass prerequisite (it is taken anyway). `changed` lists snapshot paths and `<list>:<name>`
  entries that differ from the same character without the draft. Two trials are kept at once; one idle for 30 minutes
  is dropped, and every trial command then answers `not_found`.
- **Ids never reach the screen.** They travel in `HomebrewRef`, picker rows and summaries; the UI shows names.
- **Form.** `HomebrewForm { kind, fields, blocks, children, preserved }`: `fields` holds one value per
  `HomebrewFieldSpec.key` of the kind; `blocks` are rule blocks (`{ key, kind, sentence, fields }`); `children` are the child
  elements edited inside (a class's features); `preserved` lists the parts kept byte-for-byte (spec 3.4). The value shapes of
  builders (WB-02), rich text (WB-03), blocks (WB-05) and per-kind fields (WB-04) are typed by those items.
- **Expression builders (WB-02).** A field or block field with the `condition`, `tagFilter`, `equipped` or `formula`
  control holds a `HomebrewExprValue<M>` `{ value: { shape: "model", model } | { shape: "kept", text, reason }, original? }`
  (types in `types/homebrew.ts`; a sample of every variant in `api/__fixtures__/homebrew-expr-builders.json`, written by
  `crates/borealis-authoring/tests/expr_dto.rs`). The UI edits `model` and sends `original` back unchanged: the backend
  writes the original text while the model means the same, else canonical text. Kept text is read-only (Remove only).
  Formula fields are read by different engine readers; `HomebrewFormulaDialect` says which terms a field offers.
- **Block catalog (WB-05).** `homebrew_block_catalog` lists the core blocks (`grant`, `select`, `list` = a select of
  type `List`, `stat`), one block per custom rule kind the engine reads (`custom_rules::KNOWN_KINDS`, 69) and the toggle
  effect `speed_multiply` (`effectOnly`). A block's `fields` are `HomebrewBlockField`s: exactly the rule fields the
  engine reads, never an unmodelled one; `level` and `requirements` are not fields but the flags `levelGated` ("From
  level") and `onlyIf` ("Only if", the condition builder). `bindings[0]` is the field's path in the rule (`max.expr`);
  `formula` fields carry the `dialect` of their reader; `shownAsNote` fields are text the engine shows verbatim. The
  control vocabulary is documented on `HomebrewControl`. A block's `sentence` (in `HomebrewBlock`) comes from
  `borealis_authoring::blocks::sentence`. The web mock serves the same catalogs (`api/mock/homebrew-catalog.ts`, generated
  by `src-tauri/tests/contract_homebrew.rs`).
- **Statistic catalog (WB-05).** `homebrew_stat_catalog` groups every statistic name the bundled packs use, in plain words;
  `classScoped` entries are patterns (`{class}:spellcasting:slots:{n}`) written from a class picker and a number. Names
  the loaded packs use that the catalog doesn't know come last, in the group `yourContent`.
- **Form values (WB-04).** Rust `borealis_authoring::form` reads an element into a form and writes it back (an unchanged
  form writes the element byte for byte). A field's value has the shape of its control: `text`/`multiline`/`combo` a
  string (`""` = not set), `check` a boolean, `tags`/`chips` a string list, `reference` a `HomebrewRef` or null,
  `condition` a `HomebrewExprValue`, `richText` the description tree (`DocJson`), `sheet` a `HomebrewSheetValue`,
  `setters` `HomebrewSetterRow[]` (expert rows), `mapsTo` a `HomebrewMapsToValue` (sample: `api/__fixtures__/homebrew-form.json`,
  written by `crates/borealis-authoring/tests/form_dto.rs`). `HomebrewForm.key` identifies a form inside the draft
  (children are matched by it; a new child gets a new key). `preserved` (on the form and on each block) lists the
  parts kept from the original as `{key, label}`: the UI may only remove entries; the backend keeps the bytes and the
  element id. `HomebrewFieldSpec.readOnly` marks a shown-only field (Source); the `blocks` control marks where the
  block list goes.
- **Blocks in a form (WB-04/WB-05 join).** Each rule is read by the block catalog (`form::CatalogBlockCodec`):
  `HomebrewBlock.kind` is the `HomebrewBlockSpec.kind` of its dialog (`list` for a select of type `List`), `fields` holds
  one value per offered field plus the gate fields `level` and `requirements`, in the shape of the field's control
  (`HomebrewBlockValue<C>` in `types/homebrew.ts`: builder controls a `HomebrewExprValue`, `formulas` a list of them,
  `reference` a `HomebrewRef`, `references` a list, `blocks` nested `HomebrewBlock`s with no `preserved`, the rest the
  stored JSON). Fields the dialog doesn't offer, and stored values its controls can't hold, are the block's `preserved`
  parts; a rule the catalog has no block for is kept whole (no fields, one part). An unchanged block field writes its
  stored JSON back; an edited one is written canonically. The backend rewrites each block's `sentence` from its values
  after every update. A new block, or a block whose kind changes, needs a new `key` (a changed kind drops the old kept
  parts).
- **Storage (WB-06).** A creator pack is `<data>/library/homebrew-<code>/`: `pack.json`, `content.json` (the pack's
  `Source` element first, then the elements in creation order), `pack-meta.json` (`kind: homebrew`) and `creator.json`
  (`{ format: "borealis-creator/1", code, createdWith, elements: { <id>: { kind, createdAt, copiedFrom?, nonSrdText } },
  retiredIds }`). Packs made elsewhere (`ai_add_homebrew_to_pack`, "Import anything") get a `creator.json` on their first
  save; other packs are copy-only (`requirement_failed`, HC5). Every file is replaced atomically, in the order
  `creator.json`, `content.json`, `pack.json`; each save bumps the patch version. A save re-adds only its pack to the loaded
  content (`content-reloaded` reason `homebrew-saved`; a turned-off pack reloads everything, `homebrew-saved-full-reload`).
  Drafts are `<data>/homebrew-drafts/<draftId>.json` (`{ format: "borealis-hb-draft/1", draftId, packId, elementId, kind,
  form, baseIds, baseDigest, updatedAt, copiedFrom? }`, the full form with its memo), written by the first
  `homebrew_update_draft` (opening writes none), never synced or bundled (HC7, `is_excluded`). A draft or `creator.json`
  that can't be read is set aside as `.bad`, never deleted. `baseChanged` is true when the saved elements the draft started
  from changed since (another device, an edit outside the app). Ids are given on the first save and never change; deleted
  ids go to `retiredIds` and a copy of the deleted elements to the pack's `.trash/`. Delete without `confirm` first scans
  the characters folder and the open characters: when any uses an element, nothing is deleted and `usedBy` lists them.
  `homebrew_update_draft` ignores what the UI sends for read-only fields (Source). Picker rows are the loaded elements plus,
  with `draftId`, the pack's never-saved elements (`draft: true`, with the id they would get now); spells carry `level`,
  `school` and `spell`.
- **Problems (WB-07)** are plain sentences (no ids, keys or jargon) with a form `path` (`fields.level`,
  `blocks.2.fields.number`, `blocks.0.fields.effects.1.fields.name`, `children.0.fields.name`; null = the element), the
  `elementId` of a saved element (null for one not saved yet) and a `code`: an `IssueCode` (`dangling_reference`), a rule
  problem (`content.unknown_rule_kind`, `content.unknown_rule_field`), or the creator's own (`form.required`, `form`,
  `block.required`, `block.range`, `block.dice`, `block.choice`, `block.shape`, `lint.duplicate_name`,
  `lint.empty_description`, `lint.escape`, `lint.unknown_pool`, `lint.not_offered`, `lint.select_empty`,
  `lint.spell_list_empty`, `form.class_edition`, `form.edition_adapted`, `engine.refused`, `sweep.<finding>`,
  `balance.<metric>`); sentences per code: `borealis_authoring::check::messages` (the edition codes:
  `borealis_authoring::form::edition_problems`).
  `homebrew_update_draft` sends the live pipeline's problems (spec 8.1 steps 1-5: form, schema, the content validator over
  the loaded content plus the draft, rule problems, lints); `homebrew_check_draft` the engine checks (steps 6-7), all
  warnings or info. Errors block Save (`homebrew_save_draft` runs the live pipeline, then the validator on the saved
  pack); warnings and info never do. Balance hints are `info`.
- **Item forms (WB-12).** Kinds `item`, `weapon`, `armor`, `magicItem`, `itemPack`, `equipmentPackage` (spec 4.9).
  Their fields use `section` (SectionSecondary heading), `min`/`max` (`number`) and `picker` (`reference`/`references`:
  element types, `multiple` for several). Values: `number` a whole number, `references` a `HomebrewRef[]`, `tagFilter` a
  `HomebrewExprValue<HomebrewTagFilter>`, `contents` a `HomebrewContentsRow[]` (an item pack's `extract.items`),
  `packageOptions` a `HomebrewPackageOption[]` (an Equipment Package's `data.options`); rows' `kept` travel back
  unchanged. Each field writes every location that holds it: a weapon's category, properties, damage type and mastery
  are `supports` ids (plus the `damage` setter's `type`, the `mastery` setter, and the 2024 `data` mirrors when the
  element has them); an armor's Armor Class and stealth disadvantage are its `stat ac:armored:armor` / `ac:shield` and
  `grant ID_INTERNAL_GRANTS_STEALTH_DISADVANTAGE` rules, and a magic armor's bonus its AC rule. Rules a field writes are
  not blocks (the block list holds the others). Bad values (dice, ranges, costs, coins) are `form` problems at the field.
  A magic item's `nameFormat` (control `nameFormat`, the `name-format` setter as a string) is text with value chips:
  `{{parent}}` (the base item's name) and `{{enhancement}}` (its bonus) are shown by their words and inserted from
  buttons; braces can't be typed (spec 7.4). The web mock serves the six descriptors from `api/mock/homebrew-item-kinds.ts`, written by
  `crates/borealis-authoring/tests/form_items.rs`.
- **Spell form (WB-11).** Kind `spell` (element type `Spell`, spec 4.1). Its fields bind the engine setter and, only when
  the element already has it, the 2024 mirror in `data` (`level` = setter `level` + `data.level`, `school`, `verbal`/
  `somatic`/`material` = `hasVerbalComponent`... + `data.components.v/s/m`, `concentration`, `ritual`, `materialCost` =
  `data.material_cost.amount`, `materialConsumed` = `.consumed`, `reactionTrigger` = `data.reaction_trigger`). `level` is
  a combo of `"0"` (Cantrip) to `"9"`. `spellLists` (control `spellLists`, a string list) holds every support tag of the
  spell, the way Aurora content puts a spell on a class list (`supports` + `data.classes`); the spell form has no `tags`
  field. The lists it offers are the picker rows of the type `"Spell List"` (`homebrew_reference_options` with
  `target.types: ["Spell List"]`: one row per `spellcasting.list.supports` list tag of the loaded content and the
  pack's drafts, `id` = `name` = the list; an expression list such as `(Cleric||Wizard)` or
  `Wizard,(Abjuration||Evocation)` gives each list tag it names, never a school, a level, `Ritual` or a spell id), plus
  the lists the spell already has, plus new names. A list that no loaded class declares and no spell choice picks from
  is a warning at `fields.spellLists` (`lint.unknown_spell_list`, naming a near spelling), and a spell on no list a hint
  (`lint.no_spell_list`); the web mock words both the same way. `HomebrewFieldSpec.default`
  is the value a new element starts with (`homebrew_new_draft` fills it in: a new spell is a 1st-level action, Self,
  Instantaneous, V and S; its school is the author's choice), and `section` groups fields under a SectionSecondary heading.
  The web mock serves the real descriptor of every kind whose form has landed (`api/mock/homebrew-kinds.ts`, written by
  `src-tauri/tests/contract_homebrew.rs`; `BOREALIS_UPDATE_SNAPSHOTS=1` rewrites it). Tab labels are upper case (`GENERAL`).
- **Feat form (WB-13).** Kind `feat` (element type `Feat`, spec 4.2), for 2014 and 2024 feats. `category` (combo with
  custom text: Origin, General, Fighting Style, Epic Boon) binds the setter `category`, the support tags
  `"<Category> Feat"` and `"Feat"` (what a 2024 feat choice offers by) and, when the element has it, `data.category`;
  empty = a 2014 feat (no category setter or tags). `requirements` (control `condition`) is one condition for
  `requirements` and, when the element has it, `data.prerequisite` (`level_min`, `abilities_any`, `requires_feature`:
  the part of the condition every character must meet; terms only `data.prerequisite` held are read into the condition).
  `prerequisiteText` is the display `prerequisite`: left empty, or as it was written from the condition, it is written
  from the condition ("Level 4+, Strength or Dexterity 13+"); typed, it is kept. `repeatable` binds the setter
  `allow duplicate` (a feat stored with it keeps it) and `data.repeatable.allowed` (a new feat that can be taken again
  gets `{allowed: true}`); `repeatDistinct` ("Spell List") is `data.repeatable.distinct_param` (`spell_list`). The feat
  form has no `allowDuplicate` field. Kind `featFeature` (element type `Feat Feature`, group `child`, `parentKinds:
  ["feat"]`) has the shared parts only; it is not offered by New element.
- **Species forms (WB-15).** Kinds `species` (element type `Species`, 2024), `race` (`Race`), `subRace` (`Sub Race`)
  and `raceVariant` (`Race Variant`, all 2014), spec 4.4. Fields that hold rules: `size` (chips of Tiny..Gargantuan: one
  is a `grant` of `ID_SIZE_<SIZE>`, two or more a `select_size`), `speed`, `flySpeed`, `swimSpeed`, `climbSpeed`,
  `burrowSpeed` (control `number`, feet, 0 = none: `stat innate speed[:mode]` `bonus base`), `darkvision` (the grant of
  `ID_VISION_DARKVISION`, plus `stat darkvision:range` when not 60), `blindsight`, `tremorsense`, `truesight` (`sense`
  rules), `traits` (control `children` since P2a, `picker.types ["Racial Trait"]`: a `grant` each); 2014 kinds also
  `strength`..`charisma` (number -10..10: `stat <ability>`), `languages` (references of `Language`: grants) and
  `languageChoice` (number: a `select` of `Standard||Exotic` languages; its stored label is kept). `creatureType` is
  `data.creature_type`; `data.size`, `data.speed` and `data.traits` are written when the element has them. `subRace`
  and `raceVariant` have `parent` (required text): the tag the race's choice offers it by, its first support tag; a
  `parent` no loaded race's choice offers is the warning `lint.not_offered` at `fields.parent`. `race` has
  `maleNames`, `femaleNames`, `familyNames` (chips: `names` setters with `type`) and `subraces` (combo: `""` none,
  `required`, `optional`: the race's own `select` of type `Sub Race`; a new one is named "<Race> Subrace" and offers the
  tag of the race's name, a stored one keeps its name and tag; a choice with a number or a level stays a block). A rule a field doesn't write exactly
  (a trait from level 5, an increase with a condition) stays a block; the rules come back in their stored order. Child
  kinds `racialTrait` (`parentKinds ["species","race","subRace","raceVariant"]`) and `speciesTraitOption`
  (`["species","racialTrait"]`, 2024) have the shared parts only and are not offered by New element. `min`/`max` and
  `picker` are the field spec parts the item forms use (above).
- **Children (WB-16).** `homebrew_new_draft {packId, kind, parentId, copyFrom?}` makes a child: `parentId` is a draft of
  the pack (its `draftId`) or a saved element of it (its draft is opened or resumed); the child (new, or a copy of the
  loaded element `copyFrom`) is the parent form's last child, listed in the parent's children field for its type (a
  class feature at level 1 on the LEVELS grid, a class's starting-equipment package). The answer is the parent's
  `HomebrewDraftView` (its draft is written); `invalid_argument` when the parent's form has no place for that kind. A
  children field (controls `children`, `levels`, `package`) holds `HomebrewChildEntry[]` `{ level?, child?, element? }`:
  `child` is the key of a child form of the draft, `element` a `HomebrewRef` to an element not edited here (a copied
  class still gives the original's features). A child no children field lists and no rule of the parent gives (a trait
  granted with a condition) is a problem at `children.<i>`.
- **Children inside children (P2a).** A child form has children of its own: a background variant's `feature`
  (`children` of `backgroundFeature`, granted by the variant) and a racial trait's `lineage` (`children` of
  `speciesTraitOption`, 2024: one `select` of type `Species Trait Option` named after the trait, offering the options by
  id `ID_A|ID_B`; the SRD's choices by tag stay blocks). `homebrew_new_draft {parentId, parentKey}` makes the child inside
  the child form `parentKey` of the parent's draft (`not_found` for a key the draft has none of); the answer is the
  parent's draft. A saved element opens with its children and theirs (keys `e`, `e/0`, `e/0/0`); a nested child takes
  its edition from the element above when it has none. The species `traits` field is a `children` field too (P2a, was
  `references`): traits made inside the species, or ones that exist given by reference (`element` entries); the
  `data.traits` mirror is kept. A `children` field offers "Give one that exists" (the reference picker of its type).
  A field limited to the other edition (`editions`) is shown while its value isn't empty nor its starting value, with a
  note: it still writes its rules.
- **Class option groups (P2a, spec 4.8).** The "Lets the player choose" block of type `Option` shows the group (its
  `supports` as one tag), the options of that group (`homebrew_reference_options` with `tags: [group]`) and New option:
  `homebrew_new_draft {kind: "classOption"}`, `homebrew_update_draft` (name, `group`, `className` = the class's name,
  `edition`), `homebrew_save_draft` (the option is saved at once).
- **Renamed class (P2a, spec 4.7).** Saving a class whose subclass choice's tag changed (renamed: "<Class> Subclass";
  or a 2014 label changed) rewrites that tag on the pack's saved subclasses of it (`data.class` = the class, or the 2024
  tag with no `data.class`), and their `data.class_name`, so the class keeps offering them.
- **Class form (WB-16).** Kind `class` (element type `Class`, spec 4.5), tabs GENERAL, PROFICIENCIES, LEVELS,
  SPELLCASTING (WB-17), MULTICLASS, EQUIPMENT, RULES, DESCRIPTION. A new class (`homebrew_new_draft {kind: "class"}`) has
  the pack's edition (2024 for a Both-editions pack) and three generated children on the grid: Ability Score Improvement
  (4, 8, 12, 16, 19; 2024: 4, 8, 12, 16), the subclass choice (3) and, 2024 only, the Epic Boon (19). Fields: `hitDie`
  (setter `hd` + 2024 mirrors), `primaryAbilities` (`data.primary_abilities`), the first-class-only `savingThrows`,
  `armor`, `weapons`, `tools` (grants with `requirements: "!<multiclass id>"`) and `skills` (control `skillChoice`,
  `HomebrewSkillChoice { count, skills: HomebrewRef[], tagged? }`: the select `"Skill Proficiency (<Class>)"`; `tagged`
  is a 2014 tag filter kept as is); `levels` (control `levels`, class features each at a level); `table` (control
  `classTable`, `HomebrewTableColumn[] { key, label, values[20] }` = `data.progression`, which `table` maximums read);
  `multiclass` (the section on/off), `mcRequirements`, `mcPrerequisite` (written from the condition while empty or as
  written), `mcProficiencies`, `mcProficiencyText` (likewise); `equipment` (control `package`, one entry: the
  Equipment Package child edited inline with its `options`). The multiclass id is generated (`ID_<CODE>_MULTICLASS_
  <CLASS>`, a copy's too) and every "first class only" names it. Kind `classFeature` (child of `class`) has the shared
  parts plus `asiLevels` (control `numbers`), `subclassLabel` and `epicBoon` (2024; `editions: ["dnd2024"]`): their
  rules are written from the class's name (`"Ability Score Improvement (<Class> N)"`, 2014 `"Improvement Option
  (<Class> N)"`; select `Archetype` supporting `"<Class> Subclass"` or, 2014, the label; `"Epic Boon (<Class>)"`). A
  feature's `data.class`/`data.level` follow the class. A class feature's block holds `fields.againAt` (levels): the
  block is written again at those levels ("also gained again at levels"; the base level in its name and supports is
  renumbered). `HomebrewFieldSpec.editions` limits a field to editions.
- **Spellcasting section (WB-17).** Kinds `classFeature` and `feat` have a SPELLCASTING tab (spec 4.6) with the fields
  `casts` (the `spellcasting` node on/off), `casterName` (expert; empty = the class's name: a class's feature follows its
  class), `ability`, `preparation` (`prepared` = `prepare` + `list.known`, `spellbook` = `prepare` only, `known`),
  `spellList` (`list.supports`, empty = the spellcasting name; a `combo` with `allowCustom` whose suggestions are the
  `"Spell List"` picker rows, `picker.types: ["Spell List"]`), `swap` (`allow_replace`), `casterType` (`""`, `full`,
  `halfUp`, `halfDown`, `third`, `pact`, `custom`: the `<name>:spellcasting:slots:N` stats, 2014 pact `slots:count`/`slot`,
  2024 pact `pact_magic`), `slotTable` (control `slotTable`, `number[][]` 20 x 9 for `custom`, else `[]`),
  `multiclassSlots` (`auto` = the type's `ID_INTERNAL_GRANT_MULTICLASS_SPELLCASTING_SLOTS_*`, a specific one, or `none`),
  `prepared` (control `levelCounts`, `number[]` 20: `prepare` stats), `prepareFormula` (2014: `level`, `half`, `""`),
  `cantrips` and `known` (control `levelCounts`, `HomebrewLevelCounts { levels, statistic, replace? }`: the choices and
  count stats) and `preparedChange` (2024 `prepared_spell_change`). The class kind's SPELLCASTING tab edits the section
  of its Spellcasting feature (a `classFeature` child at level 1, made with `homebrew_new_draft {parentId}`); a 2024
  class's `data.progression` spellcasting columns (`cantrips`, `prepared`, `slot_N`, `pact_slots`, `pact_slot_level`)
  and `data.spellcasting` (when present) are written from that section whenever it was edited. No new command.
- **Subclasses and class options (WB-18).** Kind `subclass` (type `Archetype`; tabs GENERAL, LEVELS, SPELLCASTING,
  RULES, DESCRIPTION): `parentClass` (`reference`, picker `Class`: `data.class` + `data.class_name`), and three
  read-only fields the backend writes from the class picked (`homebrew_update_draft`, from the class's `select` of type
  `Archetype`, also a class of the pack not saved yet): `subclassTag` (the support tag that choice offers subclasses
  by), `levelGained` (`data.level_gained`) and `subclassLevels` (`number[]`, the levels the class gives subclass features
  at; derived, never stored). The UI shows read-only fields from the last answer's form. `levels` is the LEVELS grid of
  `subclassFeature` children (type `Archetype Feature`; a child gets `data.class` = the class's name, `data.subclass`,
  `data.level`; it has the spellcasting section, named after the subclass). Kind `classOption` (type `Option`):
  `group` (required combo with custom text: the first support tag + `data.group` when present), `className`
  (`data.class_name`), `repeatable` (`data.repeatable`: a stored boolean keeps its shape, a new one is
  `{allowed: true}`), the shared condition `requirements` (`[warlock:2]`). House-rule settings (an `Option` with a
  `default` setter) open in the generic form. No new command or type. A `group` that no loaded or drafted choice names,
  that no other loaded option is in, for an option no rule gives by its id, is the warning `lint.group_not_offered` at
  `fields.group` (naming a near spelling); the web mock has no such check (its sample content holds no choices).
- **Small types and the generic form (WB-19).** One `small` kind per type of spec 4.10. Fields that bind several
  locations: `language.rarity` (`standard`/`exotic`/`secret`: the setters, the tag `Standard`/`Exotic`/`Secret`, 2024
  `Rare` for exotic, and `data.rarity`), `proficiency.covers` (`references`, picker `Proficiency`: its plain `grant`s
  of `Proficiency`), `condition.listed` (`check`: the tag `Condition` and `data.tag`), `rulesText.glossaryGroup`
  (`data.tag` and the tag of that name). Deity `alignment`, `domains`, `symbol` are required. The generic form
  (`other`): `type` is a combo with custom text whose `picker.types` is `["Element Type"]`, and `setters` (rows with
  attributes) has `picker.types` `["Setter Name"]`: these are pseudo picker types of `homebrew_reference_options`
  (one row per word, the word as id and name). `["Element Type"]` lists the open types (the fixed Companion,
  Familiar and Property types, then the loaded types with no form of their own); `["Setter Name", "<type>"]` lists
  the setter names the type requires and the ones its loaded elements use. A new or retyped generic element whose
  type has its own form, or is `Source`/`Multiclass`, has an error at `fields.type` (code `form.type`).
- **Suggestions from loaded content (1.5.1).** Two more pseudo picker types of `homebrew_reference_options`, one row
  per word: `["Item Category"]` lists the `category` setter values of loaded and drafted elements of the item forms'
  types (first spelling, sorted); the item forms' `category` combo (custom text) has `picker.types` `["Item Category"]`
  and offers them after its fixed choices. `["Tag", "<type>", ...]` lists the tags (`supports`, no `ID_` ids) loaded
  and drafted elements of those types carry; the shared `tags` field has `picker.types` `["Tag", <the kind's element
  types>]` and its chips suggest them (the generic form adds its `type`). The web mock reads the pack's saved elements.
- **Background form (WB-14).** Kind `background` (type `Background`, both editions; a new one has its pack's edition,
  2024 in a pack of both, and shows that edition's fields: `editions`). A tab whose fields are all of the other
  edition is not shown (CHARACTERISTICS on a 2024 background). 2024: `abilityScores` (closed chips: three abilities,
  `ability_distribution` with `pool`, `options [[2,1],[1,1,1]]`, `cap 20`, + `data.ability_scores`), `originFeat`
  (control `featGrant`, `HomebrewFeatGrant` `{feat, params}`: `grant_feat` with `params` by the engine's param key,
  + `data.origin_feat`, `data.origin_feat_params`). Both: `skills` and `tools` (`references` of Proficiency grants,
  + `data.skills`, `data.tool`), `choices` (control `choices`, `HomebrewChoiceRow[]`: `select` of type Language or
  Proficiency with `number` and a tag filter), `equipment` (`package`: `equipment_package` + `data.equipment_package`).
  2014: `languages`, `feature` (`children` of `backgroundFeature`, granted), `variants` (`children` of
  `backgroundVariant`, offered by an optional `select` of type `Background Variant` by id), `personalityTraits`,
  `ideals`, `bonds`, `flaws` (control `writtenList`, `HomebrewWrittenList` `{count, items}`: `select` of type `List`,
  items numbered 1..n), `equipmentNote` (rich text, the `equipment` part). New pseudo picker type
  `["Feat Parameter", "<feat id>"]` (the feat's `select_param` names) and `["Feat Parameter", "<feat id>", "<name>"]`
  (that choice's options). `HomebrewPickerTarget.tags` is now sent in descriptors (skills: tag `Skill`).
- **Creature form (WB-20).** Kind `creature` (type `Monster`, both editions; a new one has its pack's edition, 2024 in
  a pack of both). The stat block lives in `data` (`MonsterData`); every key is a field, read and written through the
  typed model. Tabs GENERAL, STATISTICS, ACTIONS, LEGENDARY, RULES, DESCRIPTION. Fields hold the model's own shape
  (snake_case inside: `homebrewCreature.ts`) except four wrappers: `armorClass` `{entries, note}` (`data.ac`,
  `data.ac_note`), `hitPoints` `{average, formula, note}` (`average: null` is worked out from the dice), `senses`
  (`passive_perception: null` is 10 + Perception or the Wisdom modifier) and `challenge` `{cr, note, xp, xpLair,
  proficiencyBonus}` (`xp` / `proficiencyBonus` null follow the CR). `statBlock` (`data.ruleset`, expert, read-only)
  follows the edition. New controls: `armorClass`, `hitPoints`, `speeds`, `initiative` (2024), `abilities`, `bonuses`
  (saves or skills, keys in `choices`), `damage`, `senses`, `challenge`, `entries` (traits, actions, bonus actions,
  reactions: name, text, usage, condition, attack, save, spellcasting with spell refs `{name, id?, note?}`: a spell
  that isn't loaded is typed by its name alone, each spell has an editable note; names and notes are trimmed and a blank
  note left out), `legendary`, `lair`. Problems:
  `form.required` (size, creature type, alignment), `form` (creature type missing, unreadable dice or CR), and the stat
  block validator's `invalid_monster_data` / `non_standard_monster_xp` at the field (`fields.armorClass`, an entry at
  `fields.actions.2`) with a sentence per part; the content schema's duplicates inside such a stat block are dropped.
  The preview is the compendium's monster entry: the whole stat block in its `descriptionHtml` (type line, armor class,
  hit points, initiative, speed, the six scores, saves, skills, damage and condition lines, senses, languages,
  challenge, then the sections); its facts are only `Type` and `Source`.
- **Copies (WB-22, spec 11, 12.3).** `homebrew_new_draft {packId, copyFrom, deepCopy?}` copies any loaded element (not a
  book): `kind` is not read (the copy opens in its element's form). The copy has no id until its first save, is named
  "<Name> (copy)", keeps the rules, tags, description and edition (its `edition` field is the original's), and drops the
  2024 mirror keys no field writes (the `data` keys the engine reads or a parent rewrites stay), the 2014/2024
  counterpart (`mapsTo` null), what it replaces, its earlier type name and its source reference. `deepCopy: true` copies
  the parts too, ids cleared: a class's features, starting equipment and (2024 shape, "<Class> Subclass") its subclasses
  with their features; a species' or race's traits and their lineage options; a background's feature, variants and
  package; a subclass's or feat's own features. The parts are the form's `children`; subclasses are not in the view: they
  are kept with the draft and saved with it (`elementIds` lists them after the form's), and `HomebrewDraftView.savedWith`
  (absent when none) names them in saving order, which the editor shows as "Saved with it: Evoker.". On save every reference to a
  copied original names its copy, and a copied lineage choice offers the copied options by id (they lose the original's
  tag, so the original never offers them). Without `deepCopy` the copy grants the originals. A 2024 subclass choice's tag
  is written from the class name without the characters a tag can't hold ("Wizard (copy)" offers "Wizard copy
  Subclass"). `homebrew_duplicate_element` copies a class deep (its subclasses in the pack too); a duplicate keeps the
  original's `copiedFrom`. `creator.json` records `copiedFrom {id, packId, srd}` for every copied element, and
  `nonSrdText` when a non-bundled original had a description; the pack then lists `redistributable: false`.
  `HomebrewPickerFilters.srdOnly` ("Start from a template") lists bundled elements only. Problems: a class read from an
  element keeps its edition (`form.class_edition`, error at `fields.edition`); an element of one edition in a pack of the
  other is `form.edition_adapted` (warning).
- **Events:** saves end with the existing `borealis://content-reloaded`.

### AI tasks

`ai_start_task` modes and what they need (`params` is a free JSON object, camelCase):

| mode | needs | result `kind` (`data`) |
|---|---|---|
| `chat` | `prompt`, optional `history` | none; text arrives as `delta` chunks |
| `rules_qa` | `prompt`; optional `characterId` (ruleset, source restriction, summary) | `rules_qa` (`CitedAnswerData`): retrieval is BM25 over the loaded packs (disabled packs are not loaded) filtered by ruleset; ids the model cites that were not retrieved are stripped (`strippedCitations`) |
| `explain` | `characterId`, `params.stat` (or `prompt`) | `explain`: `{ stat, value, text, source: model\|template, fallbackReason, trace }`. Works **without a provider** (offline template) |
| `build_advisor` | `characterId`, `prompt` = goals | `level_up`: `{ currentLevel, nextLevel, picks: [{ rank, option, rationale, effects, ... }], summary, dropped }` |
| `backstory` / `npc` | optional `characterId`; `params.tone/length/kind`; `prompt` = ideas | `narrative`: `{ title, text, traits, ideals, bonds, flaws, hooks, npc }` |
| `homebrew` | | not wired as a task: `invalid_argument` |

Non-chat tasks send their readable text as one `delta` before the `result`; structured tasks never stream raw JSON.
Event order is always `started`, then `delta`/`tool`, then at most one `result` or `error`, then `done`. Cancelling sends
`done { cancelled: true }` and no `result`. `ai_start_task` rejects with `ai_unavailable` when the mode needs a provider
and none is installed (only `explain` can run offline). Result keys are camelCase (the Rust snake_case keys are renamed;
ids as keys are untouched).

Provider detection: `ai_status.providers` lists installed CLIs, preferred first. `settings.ai.provider` (`claude`,
`codex`; `none` = whichever is installed, claude first) picks the preference and `settings.ai.overridePath` is an explicit
executable for that backend.

AI host (desktop, LAN): `ai_host_enable` starts the host and opens a pairing window (needs an installed provider);
`pairingCode` is a 6-digit code valid for `pairingExpiresInSecs`; `ai_host_begin_pairing` reissues it. Paired devices are
stored (token hashes only) in `ai-host.json` whenever `ai_host_status`/`revoke`/`disable` run. Mobile builds compile
without the host: the host commands answer `invalid_argument` / `supported: false`. The `monsters` packs
(`srd-5.1-monsters`, `srd-5.2.1-monsters`) are bundled with the app.

### 3.1 Behaviour notes: play, equipment, sheet

- `replace_spell` empties the slot of a learned spell (and unprepares it) for classes that swap spells; the UI then
  picks the new spell with `apply_selection`. Other classes get `requirement_failed`.
- `ClassSpellcasting.model`: `prepared` = prepares from the whole class list (cleric, druid), `spellbook` = prepares
  from learned spells (wizard), `known` = no preparation. `cantripsMax` counts the class's open cantrip slots.
- `equip_item`: attunable items are attuned while the limit allows; at the limit the item is equipped unattuned and
  `inventory.warnings` says so. `set_attuned(true)` at the limit is `requirement_failed`. `versatile` holds a
  versatile weapon in both hands (`equipped: "two-handed"`) and frees the off hand.
- `add_item`: identical plain stacks merge; `buy` pays the catalogue price with make-change or fails with
  `requirement_failed`; `baseItemId` layers `itemId` (a magic item) on that base item. `adjust_coins` with a negative
  `delta` withdraws with make-change; `convert_coins` re-expresses all money with `to` as the highest coin (electrum
  only when asked for).
- `export_sheet`: PDF by default (`options.format: "html"` for HTML). Desktop shows the native save dialog, writes the
  file and returns `path`; a cancelled dialog returns neither `path` nor `dataBase64`. Mobile returns `dataBase64`.
- `search_compendium`: name matches rank above description matches (exact, prefix, word prefix, substring), ties sort
  by type then name; `limit` 0 means 50 (max 500). `get_element.descriptionHtml` is sanitised on the Rust side
  (allow-list tags, no attributes).

### 3.2 Behaviour notes: manage, packs, imports

- `update_details` / `update_companion` take a partial object; unknown keys, non-text values or text over 20 000
  characters are `invalid_argument`. Details map onto the character file (identity, appearance, personality, notes);
  the companion portrait lives in the file's `extra.ui` block. `alignment` and `deity` are the character's level 1
  Alignment and Deity selections (B-191, as Aurora's Manage pickers): a value naming an element that slot offers (case
  and spaces ignored; "True Neutral" names Neutral) selects it, an empty value empties the slot, and any other text
  empties the slot and is kept as custom text in `extra.ui`. The snapshot's `details.alignment` / `deity` are the chosen
  element's name, else that custom text (the sheet prints the same), and `detailChoices.alignment` / `deity` list the
  element names the two slots offer (empty without such a slot or elements). A locked slot is `invalid_argument`. `companion` in the snapshot is
  non-null when a Companion slot is chosen or a name/portrait was set (stats are 0/10 until companion stat blocks exist).
- Several companions (GitHub #13, U16(c)): `update_companion` without `companionId` (or with `granted`) changes the
  granted companion as before; its name stays in `details.companion_name` and its picture in `extra.ui`. Notes, traits
  and the armor class, hit point and speed bonuses of every companion, and the companions added by hand, are saved
  under the top-level `companions` key, which is left out when empty (a file with zero or one companion and no edits
  is written byte for byte as before). `add_companion` takes a `Companion` or `Monster` element (or none; a
  non-creature is `invalid_argument`, an unknown id `not_found`) and answers `requirement_failed` past 12 added
  companions; `remove_companion` refuses the granted one (`requirement_failed`). Bonuses are already included in
  `armorClass`, `hp` and `speed`; the user's traits are listed in `customTraits` and again after the creature's own
  in `traits`. Limits: names 60 characters, 30 traits (text 2000), notes 4000, bonuses -99..99.
- Manual attacks have ids `manual-<index>`; `upsert_attack` without `id` appends, `move_attack` swaps within the manual
  attacks, automatic attacks (`auto: true`) answer `requirement_failed`. The attack's `card` flag is not stored yet.
- `set_source_restrictions` narrows option lists only: the engine never unregisters stored picks, so
  `preview_source_restrictions.affected` lists current picks whose source would be switched off (they stay, flagged by
  the UI), not picks that would vanish.
- `set_allow_cross_edition`: `true` allows foreign-edition content in new picks, `false` forces strict mode, `null`
  resets to the ruleset default (strict for 2014, open for 2024). Stored choices are never unregistered.
  `get_compatibility_report` lists registered foreign-edition elements with the adapter notes.
- Packs: `core` is always on; other bundled packs may be switched off, user packs removed. Enabled state, indexes and
  the user's book editions (IM14, `sourceEditions`: book name key -> `dnd2014`/`dnd2024`; unknown keys a newer app
  wrote are kept) are stored in `content-prefs.json` in the data folder. `preview_import_pack` lists the set's `books`
  (`{ name, detected, edition, setByUser }`) and `install_import_preview` takes an optional
  `editions: Record<bookName, Ruleset>` stored before the install (and `asCopy` for a homebrew pack file, see Homebrew
  creator, Pack file). A re-import without the preview never changes an
  installed book's edition: a changed guess is stored as the book's previous edition. `set_pack_enabled`, `remove_pack`, `import_pack`, `import_5etools`,
  `clear_downloaded_content` and `update_indexes` all hot-reload the ContentStore and Engine and emit
  `borealis://content-reloaded` (`reason`: `pack-enabled`, `pack-removed`, `pack-imported`, `5etools-imported`,
  `indexes-updated`, `downloads-cleared`); a pack that fails to load is rolled back with a `content_error`.
- `import_pack`: XML goes through `borealis-migrate` (pack id = file name slug, `kind: "imported"`), JSON must be a
  Borealis content file. `report.converted === 0` returns `pack: null` and installs nothing. Progress goes to
  `borealis://content-progress` (`converting`, `reloading`, `done`).
- `import_5etools` runs on a background thread, emits `borealis://import_5etools_progress` (`converting` per element
  group, then `reloading`, `done`), installs `5etools-<abbr>` packs with `kind: "local-import"`, `redistributable: false`.
- Indexes: none ship by default; `add_index` accepts `https://` URLs only and does no network access (the UI asks
  for consent first: the source is not verified by Borealis). Two kinds of index are read:
  - Borealis JSON (`borealis-index/1`, legacy `aurora-index/1`): every file entry needs `extra.attributes.sha256`; an
    optional ed25519 `<url>.sig` is pinned on first use (`verified: true`). Changed files are downloaded, SHA-256
    checked and installed as `kind: "downloaded"` packs `dl-<index>-<file>`.
  - Aurora XML `.index` (what Aurora users paste): nested indexes followed (depth <= 8, cycle guard, <= 2 000 files),
    `<obsolete>` skipped, no per-file hash needed, never `verified`. Every file is fetched over HTTPS only; all files of
    one top-level index are converted together (the folder-drop path) into one `downloaded` pack `dl-<stem>-<hash8>`
    named after the index. Each file's SHA-256 is recorded on first download; later changes are applied but listed
    in `UpdateIndexesResult.changed`, and hosts not seen before are listed in `newHosts`. A partial download never
    replaces an installed pack with less content.
  Redirects must stay on HTTPS. Unusable entries and indexes are listed in `failed`; `ContentIndex.problem` gives the
  reason per index (e.g. `the server answered 404 Not Found for 3 files`). Phases: `checking`, `downloading`,
  `converting`, `reloading`, `done`.

### Equipment and Magic columns (UI v3)

All additive; read from content setters (`armorClass`, `strength`, `stealth`, `armor`, `damage`, `mastery`, `range`, `time`, `duration`, `has*Component`, `materialComponent`, ...).

- `ElementSummary` (so `ItemSummary`, compendium rows, slot options and `ElementDetail`) gains optional `spell: SpellFacts`, `armor: ArmorFacts`, `weapon: WeaponFacts`, each omitted when it does not apply. Weight and cost stay on `ItemSummary` (`weight`, `costCp`).
  - `ArmorFacts`: `group`, `acText`, `acBase`, `addsDex`, `dexCap | null`, `strength | null`, `stealthDisadvantage`.
  - `WeaponFacts`: `damage`, `damageType`, `properties[]`, `mastery | null` (every property name, comma-joined: `Topple, Push`), `range | null`, `proficiency` (Simple/Martial), `kind` (Melee/Ranged).
  - `SpellFacts`: `castingTime`, `range`, `duration`, `verbal`, `somatic`, `material`, `materialText`, `concentration`, `ritual`.
- `SpellEntry.casting: SpellFacts | null` (spellcasting rows).
- `InventoryItem.weapon` gains `mastery | null`, `range | null`; new `InventoryItem.armor: ArmorFacts | null`.
- `CharacterDetails` gains free-text `treasure` and `questItems`, written with `updateDetails({ treasure, questItems })` (additive, defaults to ""). Since B-218 they are stored in the character's own `details.treasure` / `details.quest` (what an Aurora import fills and the sheet prints); text an older version kept in the UI extras is still shown until the field is next edited.

## 5. Selecting the implementation

`createApi()` (`src/api/index.ts`) returns `TauriApi` inside the Tauri webview (`window.__TAURI_INTERNALS__`) and the
`MockApi` otherwise; `VITE_USE_MOCK=1` forces the mock inside Tauri for UI development. `MockApi` is a complete,
tested reference of the *behaviour* the UI expects (it implements every command, the rules in 2.2 and the events).

## 6. Known gaps / decisions for the Rust side

- `save_character_as` / `export_*` need `tauri-plugin-dialog` for the native save dialog (path omitted).
- `import_character` takes bytes; the Rust importer decides `.dnd5e` vs native JSON by content.
- Delete is permanent: the UI confirms first; a `restore_character` command is not in v1.
- The granted companion's creature is picked in the `companion` tab slots; an added companion's creature is any
  `companion` or `monster` element (`update_companion` with `creatureId`).
- Spell-slot expenditure (play tracking) is intentionally out of scope; only slot totals are exposed.

## 7. `--mcp` mode, AI profile and first-run AI setup

- `borealis --mcp [--content-dir <dir>]... [--packs a,b]` runs the read-only Borealis MCP server on stdio and exits when stdin
  closes; no window is created. Without `--content-dir` it looks for bundled content next to the executable
  (`../Resources/content`, `./content`). Handled in `main.rs` before Tauri starts (`mcp_mode`).
- Startup (desktop only): the crate's `profile-assets` ship as the `ai-profile` resource and are laid down read-only in
  `<data>/ai-profile`; `ensure_profile` installs when missing, refreshes when the app version changed, verifies otherwise and
  repairs (logged with `tracing::warn!`) on tamper. Every AI session then gets `SessionOptions.profile` with the MCP command
  `<current exe> --mcp --content-dir <bundled> --content-dir <library> --packs <enabled pack ids>`. Mobile skips all of it.
- Commands (desktop; phones get `mobile: true`):
  - `ai_setup_status() -> { providers: [{ backend, installed, path, version, loggedIn, installCommand, loginCommand }], npmPath, profile: { state, detail }, completed, mobile }`.
    Login checks: `claude auth status` (JSON `loggedIn`) and `codex login status` (exit code), 8 s timeout each; `loggedIn` is `null` when not installed.
  - `ai_setup_confirm_install({ provider }) -> { token, command }`: one-time consent token, only after the UI dialog is accepted. `requirement_failed` when npm is missing.
  - `ai_setup_install({ provider, consentToken })`: runs `npm install -g @anthropic-ai/claude-code` / `@openai/codex`; refused with `requirement_failed`
    for a missing, wrong-provider or reused token. Output streams on `borealis://ai-setup-event` (`{kind:"output",provider,line}`, last `{kind:"done",provider,ok,message}`).
  - `ai_setup_login({ provider, openTerminal? }) -> { command, opened }`: `claude auth login` / `codex login`; opens Terminal on macOS only. No credentials are handled.
  - `ai_setup_complete({ skipped? })`: writes `<data>/ai-setup.json`; the first-run redirect to `/setup/ai` happens while it is absent.
- UI: route `/setup/ai` (`AiSetupPage`), shown once on first desktop launch, reachable from Settings -> "AI setup...". MockApi option `aiSetup`.


### Import anything: backend notes
* `bytes` sources carry file contents read by the webview (drop, picker, camera); `file` is a native-dialog path (desktop "Browse...").
  Limits (Rust): max 12 sources, image <= 20 MB raw (downscaled to the 5 MB model limit), PDF <= 256 MB (text layer read
  directly; scanned pages split into parts under the 20 MB model limit), Word `.docx` / OpenDocument `.odt` <= 64 MB (text,
  headings and tables extracted; counts as text), text <= 2000000 chars, 320 MB total.
  Text MAY be mixed with images/PDFs in one `import_start`: with files present, every text source (and any text file) is concatenated into **notes**
  (<= 4000 chars, else `invalid_argument`) that ride along in each model step as `<user_notes>` hints ("this is a 2024 Fighter subclass"); the files stay the source.
  Text-only imports are unchanged (the text is the source). Notes apply to the start run only (not stored in the draft for revise). Other violations: `invalid_argument`, nothing is stored.
* Drafts persist in `<data>/import-drafts/<id>.json`. `import_choose_parent` returns the draft at once (elements pending) and then streams events ending in `ready`.
* `import_finalize` installs a `user-import` pack (`redistributable:false`), emits `content-reloaded` (`pack-imported`); prefix is `[a-z0-9-]{1,8}`.
* `ai_validate_homebrew` takes an element or `{elements:[...]}`; balance flags come back in `warnings`. `ai_add_homebrew_to_pack` files it in pack `homebrew` (or `packId`, must be a homebrew pack).
