---
name: borealis-editions
description: Use when content must work in the 2014 rules, the 2024 rules, or both. Covers the authoring differences that matter (ability score source, origin feats, species vs race, weapon mastery, prepared casting, subclass level, multiclassing) and the ruleset / maps_to fields.
---
# 2014 vs 2024 for authors

Every character uses exactly one edition profile. Ask or infer which edition the user wants; if unclear, ask once. Never mix the two
inside one element. Set `"ruleset": "dnd2014"` or `"dnd2024"` (or `"any"` for edition-neutral vocabulary) on the file root and, if a file
mixes, on each element. Search with `ruleset: "2014"` or `"2024"` so you study the right examples.

## Differences that change what you write
| Area | 2014 | 2024 |
|---|---|---|
| ID prefix in SRD | `ID_WOTC_PHB_...` | `ID_2024_...` |
| Ability score source | Race/subrace bonuses plus class ASI | Background gives the creation bonus (`ability_distribution` rule); species grant none |
| Level-up improvement | Class ASI feature: +2/+1+1 or a feat | A General feat slot (`select_feat`, default the Ability Score Improvement feat); Epic Boon slot at 19 |
| Species/race type | `Race` (+ `Sub Race`) | `Species` (+ `Species Trait Option` for lineages); traits are `Racial Trait` |
| Background | Skills, tools, feature | Skills, tool, 3-ability bonus pool, an origin feat (`grant_feat`), equipment package |
| Origin feats | Do not exist | `Feat` with `category: "Origin"` (`supports` `["Origin Feat","Feat"]`), no prerequisite |
| Feat categories | One list | Origin, General (level 4+), Fighting Style, Epic Boon (level 19+) in `data.category` and a `category` setter |
| Weapon mastery | None | Weapons carry a mastery property id; classes grant `weapon_mastery` slots (`count`, `filter`) |
| Spell preparation | Mixed: known casters, prepared by level+modifier | Fixed per-class prepared count from a table column; `wizard:spellcasting:prepare` is a table value, not level + modifier |
| Ritual casting | Needs a class feature | Any prepared ritual spell can be cast as a ritual |
| Half-casters rounding | Down | Up for multiclass slot level |
| Subclass level | Class-specific (1, 2 or 3) | Always 3 |
| Language | From race/background | Common plus two free picks at creation |
| Armor | Proficiency | Training |
| Exhaustion | Six-level table | Linear penalty per level |
| Inspiration | Advantage flag | Heroic Inspiration reroll |

Profile hooks (exhaustion model, rounding, ritual rule, armor training) are engine behaviour, not content: never try to encode them in a rule.

## Authoring consequences
- Species in 2024: no ability bonuses, no languages, size and speed as `grant`/`stat`; lineage choices become `select` of `Species Trait Option` with levels 1/3/5 spell grants at character level.
- Backgrounds in 2024: include the three-ability `ability_distribution`, a `grant_feat` for an Origin feat, skill grants, one tool grant or select, and an `equipment_package`.
- Feats in 2024: set both `data.category` + `category` setter, `supports`, and `data.prerequisite` plus an equivalent `requirements` expression (`[character:4]`).
  Half-feats use `ability_increase` with `cap` 20 (Epic Boons 30).
- Spells are nearly the same shape in both: `setters` for level, school, casting time, range, components, duration, concentration, ritual; `supports` lists classes.
  2024 additionally keeps `data.higher_level` text.
- Classes: see borealis-class-and-subclass. In 2024 give a Weapon Mastery feature to martial classes if the design has it.
- Do not put 2024-only rule kinds in 2014 content; the 2014 engine will ignore them.

## ruleset and maps_to
- `ruleset` on the file or element selects which edition's character may use it. `any` is for shared vocabulary only.
- `maps_to` links an element to its counterpart in the other edition so imports, edition switches and search can follow it:
  `"maps_to": {"dnd2014": "ID_WOTC_PHB_ARCHETYPE_CHAMPION", "kind": "equivalent"}` on a 2024 element (use key `dnd2024` on a 2014 element).
  `kind` is `equivalent`, `approximate` or `none`. Only set it after confirming the counterpart id with `get_element`; never guess an id.
- `renamed_from` holds the old name/id when the same thing was renamed.
- When the user wants "the same thing in both editions", author two separate elements with different ids and cross-link them with `maps_to`.

## 2014 content on a 2024 character (information for explaining behaviour)
Races play as species (racial bonuses ignored or moved), old backgrounds ask for a bonus pool and origin feat, 2014 feats count as General, old subclasses on
a 2024 class shift their features to level 3, spells and items work. A 2014 class is not usable on a 2024 character. Warn the user when their request implies a
combination the engine will adapt rather than run natively.
