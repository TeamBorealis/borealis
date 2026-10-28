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
| `abilities` (`AbilityState`) | method, base scores, bonuses with source labels, final/modifier/save, cap and `overCap`, `pointBuy` (budget/spent/remaining/costs/min/max) or `standardArray` (values/assigned/unassigned) or roll history |
| `stats` | proficiency bonus, AC, initiative, speed, max HP, passive Perception, 18 skills (+ proficiency level), languages, proficiencies, selected features |
| `slots` + `progress` | every selection slot (section 2.1) and `{ total, remaining, nextSlotId, byTab }` |
| `spellcasting` | per spellcasting class: ability, DC, attack, model (`spellbook`/`prepared`/`known`), cantrip and prepared limits, spell list with prepared flags; plus slot rows and `multiclassSlots` (non-null when 2+ casting classes); a class's Pact Magic slots are its own `pact` row (short or long rest recovery), never in the slot rows |
| `inventory` | items, coins, total gp, carried weight / capacity (STR x 15), attunement `used/max`, storage names, **transient `warnings`** |
| `details` | the Manage text fields (name excluded: use `update_character_meta`) |
| `attacks` | manual and automatic attacks; automatic ones carry the computed `attackBonus` and `damage` (spell attack counted once everywhere) |
| `companion` | `null` unless the build has a companion |
| `options, availableOptions` | character options (multiclassing, feats, average HP, house rules) and the option definitions to render |
| `sourceRestrictions` | enabled source ids, `null` = unrestricted |
| `notices` | informational diagnostics, e.g. import `LegacyDivergence` lines |

### 2.1 Selection slots

`SelectionSlot { id, tab, groupId, groupLabel, title, elementType, ordinal, of, required, locked, chosen, requiredLevel, allowReplace, hint }`.

- `id` is stable across snapshots (owner element + rule index + ordinal). The UI uses it as the React key and as
  the `slot` URL parameter for the Next-selection flow.
- `tab` places the slot: `race`, `class`, `background`, `abilities` (ASI picks), `feats`, `languages`, `proficiencies`,
  `companion`, `spells`. Multiclass slots carry `groupId`/`groupLabel` so the UI groups them under the class slot.
- `locked` = the rule has a `default`, exactly one candidate, and the slot holds it: shown, not editable (Aurora,
  engine-optional-sole-option). Nothing else is chosen automatically: a slot's `default` is chosen once, when the slot
  appears; a sole candidate without a default stays open for the user. `apply_selection` / `clear_selection` on a
  locked slot fail with `invalid_argument`. `required=false` = optional pick (collapsed).
- `progress.remaining` counts required, unchosen slots; `nextSlotId` follows the engine's Selection Flow priority
  (race, class, archetype, multiclass, background, ASI, language, proficiency, feat, companion, spell).
- `get_slot_options` returns the *currently valid* candidates (source restrictions, requirements, duplicates already
  applied; failing multiclass prerequisites are hidden). `disabledReason` may mark visible-but-unselectable rows.
  `apply_selection` must re-validate and fail with `requirement_failed` if the element is no longer valid.
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
| Standard array | `assign_standard_array` assigns **or swaps**; a value can never be duplicated or lost; `value: null` unassigns. Base scores of unassigned abilities are reported as 8. |
| rolls | `roll_abilities` (all, or one ability) is only valid for `roll_3d6`/`roll_4d6`; each roll is appended to `abilities.rolls` (dice + total). Backend owns the RNG. |
| Level down | `level_down` removes the most recent level, drops an emptied multiclass slot, and sets XP to the new level's threshold. |
| Attunement | equipping a 4th attunable item succeeds **unattuned** and adds a warning string to `inventory.warnings`; `set_attuned` beyond the limit fails with `requirement_failed`. The warning list is transient: it is present only in the snapshot returned by the mutation that caused it. |
| Buying | `add_item(buy: true)` pays from the purse with make-change; insufficient funds -> `requirement_failed` (the UI then offers "add anyway" via `buy: false`). Total copper is invariant under change-making. |
| HP and prepared spells | HP rolls are persisted inputs (`set_hp_roll`); new levels use the average when the option is on; prepared-spell maximum is clamped to at least 1. |
| Source restrictions | before `set_source_restrictions` the UI calls `preview_source_restrictions` and warns about picks that would be dropped. |

## 3. Commands

### App

| Command | Args (camelCase keys) | Returns |
|---|---|---|
| `app_version` | none | `string` |

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
| `set_hp_roll` | `characterId, { classSlotId: string; level: number; value: number }` | `CharacterSnapshot` |

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
| `preview_ruleset_switch` | `characterId, { target: Ruleset }` | `RulesetSwitchPlan` |
| `switch_ruleset` | `characterId, { target: Ruleset }` | `CharacterSnapshot` |

### Magic

| Command | Args (camelCase keys) | Returns |
|---|---|---|
| `set_spell_prepared` | `characterId, { classSlotId: string; spellId: string; prepared: boolean },` | `CharacterSnapshot` |

### 2024 play state and Convert to 2024

Every command returns the full `CharacterSnapshot`; the new blocks live in `snapshot.play` (`PlayState`, camelCase) and on `attacks[]` (`mastery`, `critRange`, `dieFloor`, `offhandDamage`).

| Command | Args (camelCase keys) | Returns |
|---|---|---|
| `preview_convert_to_2024` | `characterId` | `ConversionPreview` `{characterId,name,items:[{category,label,outcome:kept\|mapped\|dropped\|needs_choice,fromId,toId,reason,what,crossEdition,kind}],counts,before,after}` (saves nothing; 2014 only, else `requirement_failed`) |
| `apply_convert_to_2024` | `characterId` | `{newCharacterId, snapshot}`: saves a new library character ("<name> (2024)", `meta.converted_from` = old id) and opens it; the original file is untouched |
| `spend_resource` / `restore_resource` | `characterId, resourceId, uses?` (default 1) | `CharacterSnapshot` (`requirement_failed` when too few uses, `not_found` for an unknown id) |
| `short_rest` / `long_rest` | `characterId` | `CharacterSnapshot` (rests end toggles; Long Rest refills and lowers exhaustion by 1 in 2024) |
| `set_toggle` | `characterId, toggleId, on` | `CharacterSnapshot` (Rage etc.; `play.toggles[].active`, `play.defenses`) |
| `set_exhaustion` | `characterId, level` | `CharacterSnapshot` |
| `set_heroic_inspiration` | `characterId, has` | `CharacterSnapshot` |
| `choose_weapon_masteries` | `characterId, weaponIds: string[]` | replaces all picks in slot order (slots = `Weapon` selects named "...Mastery"); `invalid_argument` for duplicates, `requirement_failed` for too many / not allowed |
| `set_wild_shape_forms` | `characterId, formIds: string[]` | `CharacterSnapshot` (limits in `play.wildShape`) |
| `choose_equipment_package` | `characterId, packageId, option` | `CharacterSnapshot` (once per package; items + coins added) |

`PlayState` fields: `speeds, resources, resourceCosts, toggles, exhaustion, inspiration, inspirationSources, martialArts, masteries, defenses{resistances,immunities,vulnerabilities,conditionImmunities}, senses, advantages, initiativeAdvantage, notes, carry, wildShape, equipmentPackages, compatibility{strict,entries}, features[{id,name,ty,source,level,grantedBy,description,sheetSummary,displayed}], freeCasts, saveDcs`.
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
| `update_companion` | `characterId, { patch: Partial<Pick<CompanionState, "name" \| "portrait">> }` | `CharacterSnapshot` |

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
| `list_sources` | none | `ContentSource[]` |
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
  `alignment`, `deity` and the companion portrait live in the file's `extra.ui` block. `companion` in the snapshot is
  non-null when a Companion slot is chosen or a name/portrait was set (stats are 0/10 until companion stat blocks exist).
- Manual attacks have ids `manual-<index>`; `upsert_attack` without `id` appends, `move_attack` swaps within the manual
  attacks, automatic attacks (`auto: true`) answer `requirement_failed`. The attack's `card` flag is not stored yet.
- `set_source_restrictions` narrows option lists only: the engine never unregisters stored picks, so
  `preview_source_restrictions.affected` lists current picks whose source would be switched off (they stay, flagged by
  the UI), not picks that would vanish.
- `set_allow_cross_edition`: `true` allows foreign-edition content in new picks, `false` forces strict mode, `null`
  resets to the ruleset default (strict for 2014, open for 2024). Stored choices are never unregistered.
  `get_compatibility_report` lists registered foreign-edition elements with the adapter notes.
- Packs: `core` is always on; other bundled packs may be switched off, user packs removed. Enabled state and indexes are
  stored in `content-prefs.json` in the data folder. `set_pack_enabled`, `remove_pack`, `import_pack`, `import_5etools`,
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
  - `WeaponFacts`: `damage`, `damageType`, `properties[]`, `mastery | null`, `range | null`, `proficiency` (Simple/Martial), `kind` (Melee/Ranged).
  - `SpellFacts`: `castingTime`, `range`, `duration`, `verbal`, `somatic`, `material`, `materialText`, `concentration`, `ritual`.
- `SpellEntry.casting: SpellFacts | null` (spellcasting rows).
- `InventoryItem.weapon` gains `mastery | null`, `range | null`; new `InventoryItem.armor: ArmorFacts | null`.
- `CharacterDetails` gains free-text `treasure` and `questItems`, written with `updateDetails({ treasure, questItems })` and stored in the character's UI extras (additive, defaults to "").

## 5. Selecting the implementation

`createApi()` (`src/api/index.ts`) returns `TauriApi` inside the Tauri webview (`window.__TAURI_INTERNALS__`) and the
`MockApi` otherwise; `VITE_USE_MOCK=1` forces the mock inside Tauri for UI development. `MockApi` is a complete,
tested reference of the *behaviour* the UI expects (it implements every command, the rules in 2.2 and the events).

## 6. Known gaps / decisions for the Rust side

- `save_character_as` / `export_*` need `tauri-plugin-dialog` for the native save dialog (path omitted).
- `import_character` takes bytes; the Rust importer decides `.dnd5e` vs native JSON by content.
- Delete is permanent: the UI confirms first; a `restore_character` command is not in v1.
- Companion editing is limited to name/portrait (`update_companion`); companion pickers arrive as `companion` tab slots.
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
