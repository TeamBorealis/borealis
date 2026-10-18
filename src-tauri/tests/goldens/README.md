# SRD golden characters (IM7)

Hand-checked golden characters for acceptance stage 3e (`specs/acceptance-imports-1.md`).
There are 26: 12 classes x 2 editions (SRD 5.1 = 2014 rules, SRD 5.2.1 = 2024 rules), plus 2 multiclass characters.

**Attribution.** This work includes material taken from the System Reference Document 5.1 ("SRD 5.1") by Wizards of
the Coast LLC, available at https://dnd.wizards.com/resources/systems-reference-document, and material from the System
Reference Document 5.2.1 ("SRD 5.2.1") by Wizards of the Coast LLC, available at https://www.dndbeyond.com/srd. Both
are licensed under the Creative Commons Attribution 4.0 International License, available at
https://creativecommons.org/licenses/by/4.0/legalcode.

This folder holds data only. It has no code and nothing reads it yet. Note that the repo `.gitignore` has an unanchored `specs/` rule, which also matches `goldens/specs/`. New files there need `git add -f`; files already tracked are unaffected. A later agent maps `specs/` to Borealis selections
and compares the built sheet with `expected/`.

## Process (decision IM7)

1. **Agent A** (golden-A) wrote every build spec (`specs/<id>.json`). It then derived each expected sheet
   (`expected/<id>.json`) by hand from the SRD rules text. It never ran the Borealis engine or app and never read engine
   code. Each number has a `why` string. Every rule reading that needed judgment is in `derivation-notes.md`,
   with the SRD section it comes from.
2. **Agent B** re-derives the sheets independently, **from `specs/` and the SRD documents only**. Agent B must not open
   `expected/` or `derivation-notes.md` until its own sheets are written. The two sets must match exactly. A mismatch
   is resolved against the SRD text, never against engine output.
3. **The user** spot-checks a few sheets. After that the files are frozen. Pass = the engine matches exactly.

## Roster

| id | Build | Breakpoints covered |
|---|---|---|
| srd51-barbarian-5 | Half-Orc Barbarian 5, Path of the Berserker | subclass, ASI, Extra Attack, Fast Movement |
| srd51-bard-3 | Half-Elf Bard 3, College of Lore | subclass, Expertise, 2nd-level slots, Jack of All Trades |
| srd51-cleric-1 | Hill Dwarf Cleric 1, Life Domain | L1 subclass, heavy armor, Dwarven Toughness |
| srd51-druid-8 | Human Druid 8, Circle of the Land (Forest) | 2 ASIs, 4th-level slots, circle spells, Wild Shape |
| srd51-fighter-11 | Human Fighter 11, Champion | Extra Attack (2), Grappler feat, Remarkable Athlete, 2 styles |
| srd51-monk-17 | Lightfoot Halfling Monk 17, Way of the Open Hand | Diamond Soul, Quivering Palm, 4 ASIs |
| srd51-paladin-20 | Dragonborn Paladin 20, Oath of Devotion | capstone, Aura of Protection, Improved Divine Smite |
| srd51-ranger-5 | High Elf Ranger 5, Hunter | Extra Attack, Archery, racial cantrip |
| srd51-rogue-11 | Lightfoot Halfling Rogue 11, Thief (point buy) | Reliable Talent, 4 Expertise, 3 ASIs |
| srd51-sorcerer-17 | Tiefling Sorcerer 17, Draconic Bloodline (red) | 9th-level slots, 4 Metamagic, Elemental Affinity |
| srd51-warlock-3 | Half-Elf Warlock 3, The Fiend, Pact of the Tome | pact boon, invocations, pact slots |
| srd51-wizard-8 | Rock Gnome Wizard 8, School of Evocation | 2 ASIs, 4th-level slots, spellbook |
| srd51-multiclass-paladin2-warlock3 | Human Paladin 2 / Warlock 3 | multiclass slots + separate Pact Magic |
| srd521-barbarian-20 | Goliath Barbarian 20, Path of the Berserker | Epic Boon, Primal Champion (score cap 25) |
| srd521-bard-11 | Human Bard 11, College of Lore | Magical Secrets, 6th-level slots, Expertise x4 |
| srd521-cleric-5 | Dwarf Cleric 5, Life Domain (Protector) | 3rd-level slots, Sear Undead, bought gear |
| srd521-druid-1 | Gnome Druid 1 (Magician) | level 1, Primal Order skill bonus |
| srd521-fighter-8 | Orc Fighter 8, Champion | 3 ASIs (one Grappler), 2 Fighting Style feats |
| srd521-monk-5 | Halfling Monk 5, Warrior of the Open Hand | Extra Attack, Alert, Focus |
| srd521-paladin-3 | Dragonborn Paladin 3, Oath of Devotion | subclass, Channel Divinity, Defense |
| srd521-ranger-11 | Elf (Wood Elf) Ranger 11, Hunter | Superior Hunter's Prey, Roving speeds, Expertise |
| srd521-rogue-17 | Human Rogue 17, Thief | Thief's Reflexes, Slippery Mind, 5 ASIs |
| srd521-sorcerer-5 | Tiefling Sorcerer 5, Draconic Sorcery | 3rd-level slots, Draconic Resilience |
| srd521-warlock-8 | Elf (High Elf) Warlock 8, Fiend Patron, Pact of the Blade (point buy) | 4th-level pact slots, 6 invocations |
| srd521-wizard-3 | Gnome (Rock Gnome) Wizard 3, Evoker | subclass, Scholar expertise |
| srd521-multiclass-wizard5-cleric3 | Human Wizard 5 / Cleric 3 (Thaumaturge) | multiclass slots, per-class prepared spells |

## Build spec schema (`specs/<id>.json`, `schema: "borealis-golden-spec/1"`)

A spec fixes every choice, so only one sheet can come out of it. All names are the English SRD names.
Specs hold choices only. They never hold derived totals.

| Field | Meaning |
|---|---|
| `id`, `ruleset` (`SRD 5.1` / `SRD 5.2.1`), `edition` (`2014` / `2024`), `summary`, `character_level` | identity |
| `race` (2014) | `name`, `subrace`, fixed `ability_score_increases`, base `languages`, `choices` (Half-Elf +1s, Skill Versatility, extra language, dwarf tool, wizard cantrip, Draconic Ancestry...) |
| `species` (2024) | `name`, `size`, `choices` (lineage / legacy / ancestry, the trait's spellcasting ability, Keen Senses or Skillful skill, Versatile feat and that feat's own choices) |
| `background` | 2014: Acolyte with `skill_proficiencies`, `languages`, `equipment`, `feature`. 2024: `ability_scores_listed`, the chosen `ability_score_increases` (+2/+1), `origin_feat` plus `origin_feat_choices` (spell list ability, cantrips, level 1 spell), `skill_proficiencies`, `tool_proficiency`, `equipment_option` (A or B) |
| `languages` (2024) | every language known (Common + 2 standard + any from class features) |
| `ability_scores` | `method` (standard array or 27-point buy) and the `assignment` per ability **before** any increase. Point-buy specs also list `points_spent` |
| `hit_points` | the HP rule used: **fixed average**, i.e. maximum hit die at character level 1, then the class's fixed value per level, plus Con modifier |
| `classes` | each class with `level`, `subclass`, and `order` when multiclassed (first class = order 1) |
| `multiclass_prerequisites` | the prerequisite rule that was checked (multiclass only) |
| `level_progression` | one entry per character level: `class`, `class_level`, the `hit_points` rule for that level, and `choices` made at that level. A choice has a `feature` plus `choice`, `skills`, `tools`, `expertise`, `increases` (an ASI; 2024 also names `feat: "Ability Score Improvement"`), `feat` (a feat taken), `add` (an extra Weapon Mastery kind), `spells` or `note` |
| `spells` | the final spell state per casting source: `ability`, `cantrips`, then `known` (2014 known casters), `prepared` (prepared casters, **not** counting always-prepared spells), `spellbook` (wizards), and grants such as `level_1` (Magic Initiate) and `always_prepared` (species) |
| `equipment` | the starting-equipment option taken per class and background (only the first class gives gear when multiclassing), purchases made with gold, `worn_armor`, `shield`, `wielded`, and `carried_weapons` |
| `pact_weapon` | the weapon conjured with Pact of the Blade (2024 warlock) |

Gear is normal starting equipment only, so there are no magic items. Gold is spent only where the spec lists purchases.

## Expected sheet schema (`expected/<id>.json`, `schema: "borealis-golden-expected/1"`)

Every value is `{ "value": ..., "why": "..." }` unless noted.

- `abilities.<Ability>`: `score`, `modifier`, `why` (base, then each increase in order).
- `proficiency_bonus`, `hit_points_max`, `armor_class` (with `source`), `initiative`.
- `speeds`: only speeds that are always on (`walk`, plus `climb` / `swim` / `fly` when a feature grants them
  permanently). Speeds that need an action or a limited use are left out: Dragon Wings, Draconic Flight, Large Form.
- `saving_throws.<Ability>`: `value`, `proficient`, `why`. Always-on bonuses are included (Aura of Protection on the paladin itself).
- `skills.<Skill>`: all 18 skills. `proficiency` is `none` / `half` (Jack of All Trades or Remarkable Athlete) /
  `proficient` / `expertise`. Flat always-on bonuses are included (Magician, Thaumaturge).
- `passive_perception` = 10 + the Perception total.
- `spellcasting[]`: one entry per casting source (class, species trait or feat) with `ability`, `spell_save_dc` and
  `spell_attack_bonus`.
- `spell_slots.value`: `{ "<level>": count }` from the Spellcasting feature (or the multiclass table). `pact_slots`:
  `count`, `slot_level`, `recharge` (null when the character has none).
- `attacks[]`: one entry per weapon kind in `carried_weapons`, every damaging cantrip, and a Dragonborn Breath Weapon.
  - `to_hit` = ability modifier + proficiency (when proficient) + always-on bonuses (the Archery style on ranged weapons).
    Save-based entries have `save` (`ability`, `dc`) instead.
  - `damage` uses the one-handed die. Versatile weapons add `damage_two_handed`. Finesse weapons use the better of
    Str and Dex. Thrown melee weapons use the same modifier as in melee.
  - Included because they are always on: Martial Arts die and Dex (monk weapons, when the die is larger), Improved
    Divine Smite (2014 paladin melee weapons), Agonizing Blast, Elemental Affinity (2014 Draconic fire spells), Pact of
    the Blade's Cha (2024). Eldritch Blast also lists `beams`.
  - Left out because they are situational or need a resource: Rage damage, Sneak Attack, Hunter's Mark, Colossus
    Slayer, Divine Smite, Sacred Weapon, Frenzy, Savage Attacker, Great Weapon Fighting rerolls, Hex, Brutal Strike.
    Their sizes are given in `class_values`.
  - Cantrip damage dice scale with total character level (5 / 11 / 17).
  - An Unarmed Strike is listed only for monks.
- `resources[]`: limited-use pools (`name`, `max`, `recharge`, `why`). This covers class pools (Rage, Ki / Focus
  Points, Sorcery Points, Channel Divinity, Bardic Inspiration, Second Wind, Action Surge, Lay on Hands, Wild Shape...),
  species uses (Breath Weapon, Relentless Endurance, Stonecunning...), and feat uses (a Magic Initiate free cast).
- `class_values[]`: fixed numbers that are not pools, such as rage damage, sneak attack dice, martial arts die,
  feature save DCs, and attacks per Attack action.
- `features`: `class.<Class>[]`, `subclass.<Subclass>[]`, `race[]` (2014) or `species[]` (2024), `background[]`,
  and `feats[]`. Each entry is `{name, level, choice?}`.
  - **Class and subclass features** use the SRD feature heading names (2014: the class `###` headings and the subclass
    headings; 2024: the `Level N: <name>` headings). Each name is listed once, at the lowest class level that grants it.
    The one exception is `Ability Score Improvement`, which is listed at every level that grants it.
    Sub-options (Fighting Style options, Ki actions, Metamagic options, invocations) go in `choice`, not as separate
    entries. Flavor headings (Tenets) and spellcasting sub-headings are not features.
  - **Race / species traits** use the trait names from the SRD, excluding Ability Score Increase, Age, Alignment, Size,
    Speed, Languages and Creature Type. A trait that starts at a higher level (Large Form, Draconic Flight) carries that
    character level.
  - **Background** lists the 2014 background feature (Shelter of the Faithful). 2024 backgrounds have no feature,
    because their feat goes in `feats`.
  - **Feats** lists every feat by name with the character level it was taken at. This includes 2024 Ability Score
    Improvement feats, Fighting Style feats, Epic Boons, and origin feats from backgrounds, Human Versatile or
    Lessons of the First Ones.
- `spells.<source>`: rule-derived limits (`cantrips_known_max`, `spells_known_max` or `prepared_max`,
  `spellbook_size`) and the spells a feature keeps `always_prepared` (domain, oath, circle, patron, species and feat spells).
