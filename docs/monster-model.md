# Monster data model

Monsters are ordinary content elements with `type: "Monster"`. The stat block lives in the element `data` map and is
typed by `borealis_content::MonsterData` (`crates/borealis-content/src/monster.rs`, `monster_types.rs`,
`monster_validate.rs`). The embedded JSON schema has a matching `$defs/monster`, applied to any `Monster` element that
carries `data` (legacy XML-migrated monsters without `data` keep using setters and are untouched).

```rust
let m = MonsterData::from_element(&element)?;      // Err(NotAMonster | Data) for other types / bad data
let element = m.to_element(id, name, source);       // sets type, ruleset and data
let hard: Vec<MonsterIssue> = m.validate();         // what the content store reports
let soft: Vec<MonsterIssue> = m.soft_checks();      // HP average, passive Perception, attack damage averages
```

The store reports every `validate()` problem as `Severity::Error` / `IssueCode::InvalidMonsterData` with path
`data.<field>`. Monsters with `data` skip the legacy `required_setters("Monster")` check.

## Fields (all snake_case; unknown fields are rejected)

| Field | Type | Notes |
|---|---|---|
| `ruleset` | `dnd2014` / `dnd2024` / `any` | required; also set as `Element.ruleset` by `to_element` |
| `size` | `[tiny..gargantuan]` | non-empty; two entries for "Medium or Small" |
| `swarm_of` | size | swarms: `size` is the swarm, `swarm_of` the member (`Medium swarm of Tiny beasts`) |
| `creature_type` | enum of the 14 types | lowercase, singular |
| `tags` | `[string]` | the parenthetical: `["goblinoid"]`, `["human","shapechanger"]`, `["any race"]` |
| `alignment` | string | verbatim: `neutral evil`, `unaligned`, `any non-good alignment` |
| `ac` | `[{value, source?, condition?}]` | one entry per form; `source` is the parenthetical, `condition` e.g. `in humanoid form` |
| `hp` | `{average, formula?}` | formula without spaces, `19d12+133`, `2d6-2`; absent when fixed |
| `speed` | `{walk?, burrow?, climb?, fly?, swim?, hover?, note?}` | feet; `note` for form dependent speeds |
| `initiative` | `{modifier, score}` | 2024 stat blocks (`Initiative +7 (17)`); soft check: score = 10 + modifier |
| `abilities` | `{str,dex,con,int,wis,cha}` | 1..=30 |
| `saves` | `{str..cha: bonus}` | only saves that differ from the ability modifier (proficient); 2024 prints MOD and SAVE for all six, `save_bonus(ability)` returns the printed SAVE |
| `skills` | `{skill: bonus}` | skill names lowercase (`sleight of hand`); `skill_notes` keeps parentheticals |
| `damage_vulnerabilities` / `damage_resistances` / `damage_immunities` | `[{types[], text?}]` | one entry per `;` clause; `text` holds the clause verbatim when it has qualifiers ("... from nonmagical attacks") |
| `condition_immunities` | `[string]` | lowercase |
| `gear` | `[string]` | 2024 `Gear` line, one entry per item |
| `senses` | `{blindsight?, darkvision?, tremorsense?, truesight?, deafened_range?, blind_beyond?, passive_perception, notes?}` | feet |
| `languages` | `[string]` | one per comma item, verbatim; `telepathy` (feet) is split out |
| `cr` | string | `"0" "1/8" "1/4" "1/2" "1".."30"`; integers also deserialize; never a float (`Cr` type) |
| `xp`, `proficiency_bonus` | numbers | both stored as printed; `validate()` checks the bonus against `cr`, `warnings()` the XP |
| `xp_lair` | number | 2024 `XP 18,000, or 20,000 in lair`; must not be below `xp` |
| `traits` / `actions` / `bonus_actions` / `reactions` | `[Feature]` | printed order |
| `legendary` | `{per_round?, lair_uses?, preamble, actions[]}` | `per_round` parsed from "can take 3 legendary actions" (5.1) or `Legendary Action Uses: 3` (5.2.1); `lair_uses` from `(4 in Lair)` |
| `description` | string | flavour paragraph printed with the block (SRD 5.1 appendices only) |

`Feature = {name, text, usage?, cost?, condition?, attack?}`:
`name` has all parentheticals removed; `usage` is `{"recharge":{"min":5,"max":6}}`, `{"per_day":3}` or
`{"on_rest":"short_or_long"}`; `cost` is the legendary action cost; `condition` is any other parenthetical
("Humanoid Form Only"); `attack` is a derived view `{kind, to_hit, reach?, range?, long_range?, damage[{average,dice?,type}]}`
of standard attack lines. `text` is always the authoritative wording. Spell lists inside a feature are kept as
`\n`-separated lines (`At will: ...`, `1st level (4 slots): ...`); in 5.1 they are not resolved to spell ids.

2024 (SRD 5.2.1) additions to `Feature`, all optional and derived from `text`:
`usage.per_day_in_lair {normal, lair}` (`Legendary Resistance (3/Day, or 4/Day in Lair)`); attack `kind` also
`melee | ranged | melee_or_ranged` (the 2024 `Melee Attack Roll:` wording); `save {ability, dc, damage[], half_on_success}`
for the first `<Ability> Saving Throw: DC n ... Failure: ... Success: Half damage.` in the text;
`spellcasting {ability?, save_dc?, attack_bonus?, lists[{frequency, spells[{name, id?, note?}]}]}` where `frequency` is
`"at_will"` or `{"per_day":{"uses":2,"each":true}}` (`2/Day Each`) and `id` is the `ID_2024_SPELL_*` of the bundled
SRD 5.2.1 spell when it exists. Elements also carry `maps_to {dnd2014, kind: approximate}` to the matching SRD 5.1 monster
and `renamed_from` (5.1 name) when the 2024 name differs.

## Validation

Hard (`validate`): sizes non-empty; AC > 0; HP > 0 and formula `NdS[+-M]`; ability scores 1..=30; at least one speed;
no empty damage clause; `proficiency_bonus` equals the CR's bonus (2, +1 per four CR above 4); non-empty feature
names/text; recharge is a d6 range; legendary actions need `per_round`.

Warning (`warnings`, reported by the store as `non_standard_monster_xp` warnings, decision HB4): `xp` differs from the CR
to XP table (`Cr::XP_TABLE`, identical in SRD 5.1 and 5.2.1; CR 0 accepts 0 or 10). Homebrew prints such XP; the stored
value is kept as printed and never recomputed.

Soft (`soft_checks`, not run by the store): HP average equals floor of the formula average; passive Perception equals
10 + Perception bonus (or the Wis modifier); each parsed attack damage average equals its dice average.

## Ids and packs

`ID_SRD51_MONSTER_<NAME>` (`ID_SRD521_MONSTER_<NAME>` for 5.2.1), name uppercased, non-alphanumerics collapsed to `_`.
Bundled packs: `content/srd-5.2.1-monsters` (330 stat blocks: `ID_SRD521_MONSTER_<NAME>`, ruleset `dnd2024`, built by
`content-tools/srd521-monsters/`, see its `COVERAGE.md`) and `content/srd-5.1-monsters` (317 stat blocks, see its `COVERAGE.md`), built by
`content-tools/srd51-monsters/` (`extract_text.py` -> `parse_monsters.py` -> `build_pack.py`, plus `qa_monsters.py`).
