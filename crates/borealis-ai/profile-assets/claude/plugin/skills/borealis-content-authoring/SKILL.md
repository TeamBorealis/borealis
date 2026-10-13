---
name: borealis-content-authoring
description: Use when writing or editing any Borealis content element (spell, feat, item, species, background, class feature, monster). Covers the JSON element model, rule kinds, ID conventions, reusing core IDs, supports linking, data.progression tables, the description HTML subset and a pre-answer checklist.
---
# Authoring Borealis content

Content is JSON. A file is `{ "format": "borealis-content/1", "ruleset": "dnd2024", "info": {"name": "..."}, "elements": [ ... ] }`.
When you answer, return either a whole file or an array of elements. Always validate with `validate_element` first (see borealis-tools).

## Element model
Required on every element: `id`, `name`, `type`, `source`. Common optional fields:
`ruleset` (`dnd2014` | `dnd2024` | `any`), `supports` (array of tag strings or ids), `description` (HTML string),
`setters` (array of `{name, value}`, values are strings), `rules` (array), `sheet`, `spellcasting`, `multiclass`
(class only), `data` (free-form typed payload), `maps_to`, `renamed_from`.
Put your own homebrew in a distinct `source` string (for example `"My Homebrew"`), never `SRD ...`.

```json
{
  "id": "ID_HB_FEAT_STEADY_NERVES",
  "name": "Steady Nerves",
  "type": "Feat",
  "source": "My Homebrew",
  "ruleset": "dnd2024",
  "supports": ["General Feat", "Feat"],
  "description": "<p>You gain a +2 bonus to initiative.</p>",
  "setters": [{"name": "category", "value": "General"}],
  "rules": [{"kind": "stat", "name": "initiative", "value": "2"}],
  "data": {"category": "General", "prerequisite": {"level_min": 4}}
}
```
Imitate existing elements: use `search_content` then `get_element` on a similar SRD element and copy its shape.

## Core rule kinds
- `grant`: gives another element. Needs `id` and `type` of the target. Optional `level`, `requirements`.
  `{"kind":"grant","id":"ID_PROFICIENCY_SKILL_STEALTH","type":"Proficiency"}`
- `select`: player picks `number` elements of `type` whose tags match `supports`. Optional `name`, `level`, `requirements`.
  `{"kind":"select","type":"Proficiency","name":"Skill Proficiency (Rogue)","number":4,"supports":"ID_PROFICIENCY_SKILL_STEALTH|ID_PROFICIENCY_SKILL_ACROBATICS"}`
  A `supports` string is a boolean expression: `,` is AND, `|` is OR, `!` is NOT, tags are case-sensitive.
  Spell selects add `"spellcasting": "<Class name>"` and use `$(spellcasting:list), 0` (cantrips) or `$(spellcasting:list), $(spellcasting:slots)`.
- `stat`: adds to a named statistic. `{"kind":"stat","name":"speed","value":"10"}`. Same-name stats at different `level`s add up,
  so write increments, not totals. Optional `bonus`, `alt`, `level`, `requirements`, `equipped`.

`level` means class level inside class/subclass/class-feature rules and character level in species and feat rules.
`requirements` is an expression: element ids joined with `&&`, `||`, `!`; tests like `[str:13]`, `[character:4]`.

## 2024 custom kinds
Any object with a `kind` is accepted. The engine understands a named set (see the pack's ENGINE-NEEDS list). The ones you will
use most: `resource` (limited-use pool with `id`, `name`, `max`, `recharge`), `grant_spell` (`spell`, `mode`: always_prepared, `ability`),
`grant_feat` (`feat`, `params`), `select_feat` (`category`: Origin | General | Fighting Style | Epic Boon), `weapon_mastery` (`count`, `filter`),
`ability_increase` (feat bonus with `cap`), `toggle` (state like Rage with nested `effects`), `extra_damage`, `save_dc`, `expertise`,
`damage_scaling`, `healing`. Before using one, run `get_element` on an SRD element that uses it and mirror its attribute names exactly.
Never invent a new kind for something a core kind plus a `resource` can already do; unknown kinds validate but do nothing in the engine.

## IDs
- Uppercase, `ID_` prefix, words joined by `_`, no spaces or apostrophes: `ID_HB_SPELL_FROST_LANCE`.
- Homebrew prefix `ID_HB_`; SRD 2014 uses `ID_WOTC_PHB_...`, SRD 2024 uses `ID_2024_...`. Do not reuse those prefixes for new content.
- Pattern by type: `ID_<P>_CLASS_<NAME>`, `ID_<P>_CLASS_FEATURE_<CLASS>_<NAME>`, `ID_<P>_SUBCLASS_<CLASS>_<NAME>` (2024) or
  `ID_<P>_ARCHETYPE_<CLASS>_<NAME>` (2014), `..._SUBCLASS_FEATURE_...`, `ID_<P>_SPELL_<NAME>`, `ID_<P>_FEAT_<NAME>`, `ID_<P>_MAGIC_ITEM_<NAME>`,
  `ID_<P>_MONSTER_<NAME>`, `ID_<P>_MULTICLASS_<CLASS>` for the multiclass node.
- IDs must be unique across the installed content. Search before choosing one.
- Engine plumbing (`ID_INTERNAL_*`, derived multiclass and ASI elements) is generated: never author it, only reference allowed ones.

## Reusing core IDs
Skills, abilities, saves, weapon/armor/tool proficiencies, languages, damage types, conditions and sizes already exist. Look them up with
`list_core_ids` (categories: skills, abilities, proficiencies, languages, damage_types, conditions, sizes) and reference them with `grant`/`select`.
Never redefine them. Stat names are lower-case and colon-separated: `strength`, `stealth:proficiency`, `hp`, `speed`, `<class>:spellcasting:slots:3`.

## supports linking
`supports` on an element advertises tags other rules can select. A subclass lists its parent's subclass tag
(`"supports": ["Fighter Subclass"]`), and the parent class feature holds a `select` with the same string. Spells list class names
(`["Cleric","Wizard"]`). Feats list `["Feat", "<Category> Feat"]`. A typo in the string silently produces an empty choice list, so
copy the string from the parent verbatim.

## data.progression tables
Class tables live in `data.progression` as arrays of 20 numbers, index 0 = level 1: `proficiency_bonus`, `cantrips`, `prepared`,
`slot_1`..`slot_9`, plus class-specific columns such as `rages`, `sneak_attack`, `focus_points`. Keep every array exactly 20 long
and make them agree with the `stat` rules that realise them. A feature's `resource.max` can point at a column:
`{"type":"table","key":"second_wind"}`.

## Description HTML subset
Use only `<p>`, `<b>`, `<i>`/`<em>`, `<ul><li>`, and simple `<table><tr><th>/<td>`. Start named sub-features with `<p><b>Name.</b> text</p>`.
No scripts, styles, classes or links. Use `<div element="ID_X"/>` only to transclude another element. Write descriptions in your own words;
never paste published rulebook text.

## Checklist
1. Searched for existing/similar elements and reused core IDs.
2. Unique `ID_` ids with correct pattern; `source` is not an SRD source.
3. `ruleset` set (`dnd2014` or `dnd2024`) on the file or each element.
4. Rules use the right kind; mirrored attribute names from an SRD example.
5. `supports` strings copied from the parent verbatim.
6. Progression arrays have 20 entries and agree with stat rules.
7. Descriptions use the HTML subset only.
8. `validate_element` returns `ok: true`; fix every schema issue and every report diagnostic, then re-run.
