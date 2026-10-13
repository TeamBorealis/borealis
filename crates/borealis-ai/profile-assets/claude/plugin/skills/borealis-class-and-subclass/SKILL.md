---
name: borealis-class-and-subclass
description: Use when building a complete 1-20 class or a subclass bound to an existing class. Covers class element layout, features per level, spellcasting feature and slot stats, the multiclass node, subclass selection level per edition, and the mandatory class_sweep loop.
---
# Building a class (1-20) and subclasses

First read borealis-content-authoring and borealis-editions. Pick the edition (`dnd2014` or `dnd2024`) before writing anything:
it changes IDs, ASI handling and the subclass level. Study an SRD class of the same edition with `search_content` / `get_element`
(for example the Fighter for a martial class, the Wizard for a caster) and mirror its structure.

## Files in a class bundle
One bundle, `{ "format": "borealis-content/1", "ruleset": ..., "elements": [...] }`, containing:
1. the `Class` element,
2. one `Class Feature` element per feature (a feature that only exists as a description still needs an element),
3. optionally the subclass (`Archetype`) and its `Archetype Feature` elements (can be a second bundle),
4. any new options (invocation-style choices) the class introduces.

## The Class element
```json
{
  "id": "ID_HB_CLASS_RUNEWARDEN", "name": "Runewarden", "type": "Class", "source": "My Homebrew",
  "ruleset": "dnd2024", "supports": ["Class"],
  "description": "<p>Primary ability: Strength. Hit Point Die: d10.</p>",
  "setters": [{"name": "hd", "value": "d10"}, {"name": "short", "value": "Strength; Medium armor, Shields"}],
  "rules": [
    {"kind": "grant", "id": "ID_PROFICIENCY_SAVINGTHROW_STRENGTH", "type": "Proficiency", "requirements": "!ID_HB_MULTICLASS_RUNEWARDEN"},
    {"kind": "select", "type": "Proficiency", "name": "Skill Proficiency (Runewarden)", "number": 2,
     "supports": "ID_PROFICIENCY_SKILL_ATHLETICS|ID_PROFICIENCY_SKILL_ARCANA|ID_PROFICIENCY_SKILL_HISTORY",
     "requirements": "!ID_HB_MULTICLASS_RUNEWARDEN"},
    {"kind": "grant", "id": "ID_HB_CLASS_FEATURE_RUNEWARDEN_RUNE_CARVING", "type": "Class Feature"},
    {"kind": "grant", "id": "ID_HB_CLASS_FEATURE_RUNEWARDEN_EXTRA_ATTACK", "type": "Class Feature", "level": 5}
  ],
  "multiclass": {
    "id": "ID_HB_MULTICLASS_RUNEWARDEN", "prerequisite": "Strength 13", "requirements": "[str:13]",
    "setters": [{"name": "multiclass proficiencies", "value": "Medium armor, Shields"}],
    "rules": [{"kind": "grant", "id": "ID_INTERNAL_GRANT_MULTICLASS", "type": "Grants"},
              {"kind": "grant", "id": "ID_PROFICIENCY_ARMOR_PROFICIENCY_MEDIUM_ARMOR", "type": "Proficiency"}]
  },
  "data": {"hit_die": 10, "primary_abilities": ["strength"], "primary_ability_mode": "single",
           "saving_throws": ["strength", "constitution"], "subclass_level": 3,
           "asi_levels": [4, 8, 12, 16], "epic_boon_level": 19,
           "progression": {"proficiency_bonus": [2,2,2,2,3,3,3,3,4,4,4,4,5,5,5,5,6,6,6,6]}}
}
```
Rules to remember:
- `hd` setter is `d6`, `d8`, `d10` or `d12`.
- Every level-1 starting grant/select (saves, skills, armor, weapons, equipment) carries `"requirements": "!<MULTICLASS_ID>"` so multiclassing into the class does not repeat them.
- The `multiclass` node has its own `id`, a `requirements` test on the entry ability (`[str:13]`, `([str:13]||[dex:13])`, `[int:13]`), and
  grants `ID_INTERNAL_GRANT_MULTICLASS` plus the reduced proficiency bundle. The engine derives the multiclass element from it; do not author one.
- Improvements: in 2014 the engine generates the ASI/feat choices from the class (do not author them); list the levels in `data.asi_levels` and grant the
  class's ASI feature as the SRD class does. In 2024 author one `Ability Score Improvement` Class Feature holding one `select_feat` rule per slot
  (`{"kind":"select_feat","level":4,"name":"Ability Score Improvement (Runewarden 4)","category":"General","default":"ID_2024_FEAT_ABILITY_SCORE_IMPROVEMENT"}`)
  and an `Epic Boon` feature with `{"kind":"select_feat","level":19,"name":"Epic Boon (Runewarden)","category":"Epic Boon"}`. Standard slots are 4/8/12/16
  (Fighter and Rogue get extra); copy the SRD class's feature with `get_element` to be sure.
- Give every level 1-20 a purpose: at minimum list a feature or a numeric bump for each class level, even if it is only "Subclass feature" placeholders
  granted by the subclass.

## Features per level
Each feature element is `type: "Class Feature"`, with `data: {"class": "<Name>", "level": n}` and its mechanics in `rules`. Grant it from the Class element with the
matching `level`. Put limited-use abilities in a `resource` rule; put scaling numbers in `data.progression` columns and reference them (`"max": {"type":"table","key":"rages"}`).
A feature whose benefit is purely narrative needs no rules, only a good description.

## Spellcasting class
Add a `Spellcasting` Class Feature that holds the stats, plus a `spellcasting` node on that feature:
```json
"spellcasting": {"name": "Runewarden", "ability": "Wisdom", "prepare": true, "allow_replace": true,
                 "list": {"supports": "Runewarden", "known": true}}
```
- `name` must equal the class name exactly (case-sensitive); `ability` is the full ability name.
- Slots: `{"kind":"stat","name":"runewarden:spellcasting:slots:1","value":"2"}` at level 1, then increments at later `level`s (value is the extra slots gained at that level, not the total).
  Repeat for each slot level 1-9 that the class reaches. Prepared limit: `runewarden:spellcasting:prepare` the same way.
- Cantrips and spell choices: level-gated `select` rules, `"type":"Spell"`, `"supports":"$(spellcasting:list), 0"`, `"spellcasting":"Runewarden"`.
- Multiclass slot pooling: add `{"kind":"grant","id":"ID_INTERNAL_GRANT_MULTICLASS_SPELLCASTING_SLOTS_FULL","type":"Grants","requirements":"ID_INTERNAL_GRANT_MULTICLASS"}`
  (use `_HALF`, `_THIRD`, ... for slower casters; `_SOLO` for pact magic).
- Spells must carry the class name in their `supports` (`["Runewarden"]`) or the class list is empty. If the class uses existing spells, extend them with an `append`
  or tell the user which spells need the tag.
Check the table values against `data.progression` arrays (`slot_1`..`slot_9`, `cantrips`, `prepared`).

## Subclass bound to a parent class
The parent class must have a feature that opens the choice, granted at the subclass level:
```json
{"id":"ID_HB_CLASS_FEATURE_RUNEWARDEN_SUBCLASS","name":"Runewarden Subclass","type":"Class Feature",
 "rules":[{"kind":"select","type":"Archetype","name":"Runewarden Subclass","supports":"Runewarden Subclass"}],
 "data":{"class":"Runewarden","level":3}}
```
The subclass:
```json
{"id":"ID_HB_SUBCLASS_RUNEWARDEN_STORMCALLER","name":"Stormcaller","type":"Archetype","source":"My Homebrew",
 "ruleset":"dnd2024","supports":["Runewarden Subclass"],
 "rules":[{"kind":"grant","id":"ID_HB_SUBCLASS_FEATURE_RUNEWARDEN_STORMCALLER_THUNDER_RUNE","type":"Archetype Feature","level":3},
          {"kind":"grant","id":"ID_HB_SUBCLASS_FEATURE_RUNEWARDEN_STORMCALLER_STORM_STEP","type":"Archetype Feature","level":7}],
 "data":{"class":"ID_HB_CLASS_RUNEWARDEN","class_name":"Runewarden","level_gained":3}}
```
- The string in `supports` of the subclass and in the `select` of the parent must be identical.
- Subclass level: 2024 is always 3. 2014 varies by class (SRD: Cleric, Sorcerer, Warlock at 1; Druid and Wizard at 2; the rest at 3). The select's
  `level` and the feature grant must match; the first subclass feature normally arrives at that level, then roughly three more spread over the next levels (6-7, 10-11, 14-15 in 2014; 6, 10, 14 or similar in 2024).
- 2014 parent tags differ per class (for example "Arcane Tradition"); for a new 2014 class pick one noun phrase and reuse it exactly.
- Adding a subclass to an existing SRD class: do not edit the class. Use `search_content` to find the parent's subclass tag in its subclass-choice feature (`get_element`), put that tag in your subclass `supports`, and set `data.class`.
- Subclass spells (always-prepared lists) use `grant_spell` rules in 2024 or a `spellcasting` node with `extend` in 2014.

## Mandatory verification loop
1. Run `validate_element` on the bundle; fix all issues.
2. Run `class_sweep` with `class_json_bundle`, `edition` (`"2014"` or `"2024"`) and, if a subclass exists, `subclass` (its id).
3. Read the result: `passed`, `levels_reached` (must be 20), `engine_errors`, and the per-level errors. Fix the root cause (missing grant, dangling id, bad
   `supports` string, slot column typo), then run `class_sweep` again. Repeat until `passed` is true and `engine_errors` is 0.
4. Run the sweep once per subclass you wrote.
Do not claim a class is finished unless the last sweep passed. Report the tool's numbers, not your own estimates.
