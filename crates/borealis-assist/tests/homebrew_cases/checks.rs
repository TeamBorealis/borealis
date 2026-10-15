//! The checks a reference character is judged by (`CharCheck`).

use std::collections::BTreeSet;

use borealis_engine::snapshot::{
    CharacterSnapshot, FreeCastSnapshot, SlotSnapshot, SpellcastingSnapshot,
};
use borealis_engine::Severity;

use crate::character::Built;
use crate::pack::{name_matches, norm, Loaded};
use crate::schema::CharCheck;

const ABILITIES: [&str; 6] = [
    "strength",
    "dexterity",
    "constitution",
    "intelligence",
    "wisdom",
    "charisma",
];

type Errs = Vec<Option<String>>;

fn eq<T: PartialEq + std::fmt::Debug>(what: &str, got: T, want: Option<T>) -> Option<String> {
    match want {
        Some(w) if w != got => Some(format!("{what} = {got:?}, expected {w:?}")),
        _ => None,
    }
}

fn collect(errs: Errs) -> Result<(), String> {
    let errs: Vec<String> = errs.into_iter().flatten().collect();
    if errs.is_empty() {
        Ok(())
    } else {
        Err(errs.join("; "))
    }
}

/// `want` items that `has` lacks (`missing`) plus `unwanted` items it has.
fn presence(
    want: &[String],
    unwanted: &[String],
    has: impl Fn(&str) -> bool,
    missing_msg: &str,
    unwanted_msg: &str,
) -> Errs {
    let mut errs: Errs = want
        .iter()
        .filter(|n| !has(n))
        .map(|n| Some(format!("{missing_msg} {n:?}")))
        .collect();
    errs.extend(
        unwanted
            .iter()
            .filter(|n| has(n))
            .map(|n| Some(format!("{unwanted_msg} {n:?}"))),
    );
    errs
}

fn ability_index(name: &str) -> Result<usize, String> {
    let n = norm(name);
    ABILITIES
        .iter()
        .position(|a| *a == n || a[..3] == n)
        .ok_or_else(|| format!("unknown ability {name:?}"))
}

fn element_name(l: &Loaded, id: &str) -> String {
    l.store
        .element(id)
        .map(|e| e.name.clone())
        .unwrap_or_default()
}

fn clean(s: &CharacterSnapshot) -> Result<(), String> {
    let mut errs: Errs = s
        .diagnostics
        .iter()
        // a grant cycle is a warning (the engine cut it) but the pack is malformed
        .filter(|d| d.severity == Severity::Error || d.code == "content.grant_cycle")
        .map(|d| Some(format!("{}: {}", d.code, d.message)))
        .collect();
    errs.extend(
        s.invalid_selections
            .iter()
            .map(|i| Some(format!("invalid selection {i:?}"))),
    );
    collect(errs)
}

/// An active `ability_check` advantage naming the skill.
fn skill_advantage(s: &CharacterSnapshot, skill: &str) -> bool {
    s.advantages
        .iter()
        .any(|a| a.active && a.roll == "ability_check" && norm(&a.text).contains(&norm(skill)))
}

fn skill(
    s: &CharacterSnapshot,
    skill: &str,
    proficient: Option<bool>,
    expertise: Option<bool>,
    total: Option<i32>,
    advantage: Option<bool>,
) -> Result<(), String> {
    let k = s
        .skills
        .iter()
        .find(|k| name_matches(skill, &k.name))
        .ok_or_else(|| format!("no skill {skill}"))?;
    collect(vec![
        eq("proficient", k.proficient, proficient),
        eq("expertise", k.expertise, expertise),
        eq("total", k.total, total),
        eq("advantage", skill_advantage(s, skill), advantage),
    ])
}

/// Registered element (id or name) or a spell of a spellcasting block.
fn granted(s: &CharacterSnapshot, check: &CharCheck) -> Result<(), String> {
    let CharCheck::Granted {
        ids,
        absent_ids,
        names,
        absent_names,
    } = check
    else {
        unreachable!("granted")
    };
    let spells = || s.spellcasting.iter().flat_map(|c| &c.spells);
    let has_id =
        |id: &str| s.elements.iter().any(|e| e.element.id == id) || spells().any(|x| x.id == id);
    let has_name = |n: &str| {
        s.elements.iter().any(|e| name_matches(n, &e.element.name))
            || spells().any(|x| name_matches(n, &x.name))
    };
    let mut errs = presence(ids, absent_ids, has_id, "lacks", "unexpectedly has");
    errs.extend(presence(
        names,
        absent_names,
        has_name,
        "lacks",
        "unexpectedly has",
    ));
    collect(errs)
}

/// Uses per rest of a free cast (`grant_spell.free_cast`) named like `name`: the spell's own name
/// or the name of the element that grants it. `element_name` resolves a granting element id.
fn free_cast_max(
    casts: &[FreeCastSnapshot],
    name: &str,
    element_name: &dyn Fn(&str) -> Option<String>,
) -> Option<i32> {
    let n = norm(name);
    casts
        .iter()
        .filter(|f| {
            norm(&f.name) == n
                || norm(&f.source) == n
                || element_name(&f.source).is_some_and(|e| norm(&e) == n)
        })
        .find_map(|f| f.uses)
}

/// The max of the resource or stat named like `name`; a per-rest free cast counts as a resource.
fn resource_max(
    s: &CharacterSnapshot,
    name: &str,
    element_name: &dyn Fn(&str) -> Option<String>,
) -> Option<i32> {
    let n = norm(name);
    // an exact resource or stat name first: "ac" is the stat, not a resource whose name merely
    // contains the letters ("Face of ...")
    let exact = s
        .resources
        .iter()
        .find(|r| norm(&r.name) == n || norm(&r.id) == n)
        .map(|r| r.max)
        .or_else(|| s.stats.get(name).map(|v| v.value))
        .or_else(|| {
            s.stats
                .iter()
                .find(|(k, _)| norm(k) == n)
                .map(|(_, v)| v.value)
        });
    exact
        .or_else(|| free_cast_max(&s.free_casts, name, element_name))
        .or_else(|| {
            s.resources
                .iter()
                .find(|r| norm(&r.name).contains(&n) || norm(&r.id).contains(&n))
                .map(|r| r.max)
                .or_else(|| {
                    s.stats
                        .iter()
                        .find(|(k, _)| norm(k).contains(&n))
                        .map(|(_, v)| v.value)
                })
        })
}

fn resource(
    l: &Loaded,
    s: &CharacterSnapshot,
    name: &str,
    max: Option<i32>,
    absent: Option<bool>,
) -> Result<(), String> {
    // `a|b`: the pool may be printed under either name
    let element_name = |id: &str| l.store.element(id).map(|e| e.name.clone());
    let got = name
        .split('|')
        .find_map(|alt| resource_max(s, alt, &element_name));
    match (got, absent == Some(true)) {
        (Some(v), true) => Err(format!("{name:?} exists (max {v}), expected none")),
        (None, true) => Ok(()),
        (None, false) => Err(format!("no resource or stat named like {name:?}")),
        (Some(v), false) => collect(vec![eq(name, v, max)]),
    }
}

fn save_dc(l: &Loaded, s: &CharacterSnapshot, name: &str, value: i32) -> Result<(), String> {
    let n = norm(name);
    let dc = s
        .save_dcs
        .iter()
        .find(|d| norm(&element_name(l, &d.owner)).contains(&n) || norm(&d.id).contains(&n))
        .ok_or_else(|| format!("no save DC named like {name:?}"))?;
    collect(vec![eq("save dc", dc.value, Some(Some(value)))])
}

/// Number of entries a slot offers: the entries of a `List` rule, else the element options.
fn option_count(x: &SlotSnapshot) -> usize {
    if x.list_items.is_empty() {
        x.options.len()
    } else {
        x.list_items.len()
    }
}

/// Does the slot offer an element with this id?
fn offers_id(x: &SlotSnapshot, id: &str) -> bool {
    x.options.iter().any(|o| o.id == id)
}

/// Does the slot offer an element, or a `List` entry, matching this name (`A|B`)?
fn offers_name(x: &SlotSnapshot, n: &str) -> bool {
    x.options.iter().any(|o| name_matches(n, &o.name))
        || x.list_items.iter().any(|(_, t)| name_matches(n, t))
}

/// The rule a slot belongs to: the slots of a `select` with `number: N` share it.
fn rule_of(x: &SlotSnapshot) -> (&str, u32, Option<&str>) {
    (x.key.owner.as_str(), x.key.rule, x.key.instance.as_deref())
}

fn choice(s: &CharacterSnapshot, check: &CharCheck) -> Result<(), String> {
    let CharCheck::Choice {
        name_contains,
        ty,
        owner,
        options_include_ids,
        options_include_names,
        options_exclude_ids,
        options_exclude_names,
        name_excludes,
        count,
        selects,
        picks,
        options,
        min_options,
    } = check
    else {
        unreachable!("choice")
    };
    let slots: Vec<_> = s
        .slots
        .iter()
        .filter(|x| {
            name_contains
                .as_deref()
                .map_or(true, |n| norm(&x.name).contains(&norm(n)))
                && !name_excludes
                    .iter()
                    .any(|n| norm(&x.name).contains(&norm(n)))
                && ty.as_deref().map_or(true, |t| name_matches(t, &x.ty))
                && owner
                    .as_deref()
                    .map_or(true, |o| name_matches(o, &x.owner.name))
                && options_include_ids.iter().all(|id| offers_id(x, id))
                && options_include_names.iter().all(|n| offers_name(x, n))
        })
        .collect();
    if slots.is_empty() {
        return Err("no matching choice slot".into());
    }
    let first = option_count(slots[0]);
    let rules: BTreeSet<_> = slots.iter().map(|x| rule_of(x)).collect();
    let first_picks = slots
        .iter()
        .filter(|x| rule_of(x) == rule_of(slots[0]))
        .count();
    let offered = |f: &dyn Fn(&SlotSnapshot) -> bool| slots.iter().any(|x| f(x));
    let mut errs = vec![
        eq("matching slots", slots.len(), *count),
        eq("matching selects", rules.len(), *selects),
        eq("picks", first_picks, *picks),
        eq("options", first, *options),
        min_options
            .filter(|m| first < *m)
            .map(|m| format!("options = {first}, expected at least {m}")),
    ];
    errs.extend(
        options_exclude_ids
            .iter()
            .map(|id| offered(&|x| offers_id(x, id)).then(|| format!("offers excluded {id}"))),
    );
    errs.extend(
        options_exclude_names
            .iter()
            .map(|n| offered(&|x| offers_name(x, n)).then(|| format!("offers excluded {n:?}"))),
    );
    collect(errs)
}

fn attack(b: &Built, check: &CharCheck) -> Result<(), String> {
    let CharCheck::Attack {
        equip,
        attack,
        damage,
        range,
        proficient,
    } = check
    else {
        unreachable!("attack")
    };
    let uid = b
        .uids
        .get(equip)
        .ok_or_else(|| format!("no equip entry {equip:?}"))?;
    let a = b
        .snap
        .attacks
        .iter()
        .find(|a| &a.uid == uid)
        .ok_or_else(|| format!("no attack line for {equip}"))?;
    let squash = |v: &str| norm(v).replace(' ', "");
    collect(vec![
        eq("attack", a.attack, *attack),
        eq("damage", squash(&a.damage), damage.as_deref().map(squash)),
        eq("range", squash(&a.range), range.as_deref().map(squash)),
        eq("proficient", a.proficient, *proficient),
    ])
}

fn speed(s: &CharacterSnapshot, check: &CharCheck) -> Result<(), String> {
    let CharCheck::Speed {
        walk,
        climb,
        swim,
        fly,
        burrow,
    } = check
    else {
        unreachable!("speed")
    };
    let v = &s.speed;
    collect(vec![
        eq("walk", v.walk, *walk),
        eq("climb", v.climb, *climb),
        eq("swim", v.swim, *swim),
        eq("fly", v.fly, *fly),
        eq("burrow", v.burrow, *burrow),
    ])
}

fn size(s: &CharacterSnapshot, value: &str) -> Result<(), String> {
    let sizes: Vec<&str> = s
        .elements
        .iter()
        .filter(|e| e.element.ty == "Size")
        .map(|e| e.element.name.as_str())
        .collect();
    if sizes.iter().any(|n| name_matches(value, n)) {
        Ok(())
    } else {
        Err(format!("size {sizes:?}, expected {value:?}"))
    }
}

fn vision(s: &CharacterSnapshot, check: &CharCheck) -> Result<(), String> {
    let CharCheck::Vision {
        darkvision,
        blindsight,
        tremorsense,
        truesight,
    } = check
    else {
        unreachable!("vision")
    };
    let range = |n: &str| {
        s.senses
            .iter()
            .filter(|x| norm(&x.name) == n)
            .map(|x| x.range)
            .max()
            .unwrap_or(0)
    };
    collect(vec![
        eq("darkvision", range("darkvision"), *darkvision),
        eq("blindsight", range("blindsight"), *blindsight),
        eq("tremorsense", range("tremorsense"), *tremorsense),
        eq("truesight", range("truesight"), *truesight),
    ])
}

fn languages(s: &CharacterSnapshot, check: &CharCheck) -> Result<(), String> {
    let CharCheck::Languages {
        ids,
        names,
        text,
        absent_ids,
    } = check
    else {
        unreachable!("languages")
    };
    let langs: Vec<_> = s
        .elements
        .iter()
        .filter(|e| e.element.ty == "Language")
        .map(|e| &e.element)
        .collect();
    let has_id = |id: &str| langs.iter().any(|e| e.id == id);
    let mut errs = presence(
        ids,
        absent_ids,
        has_id,
        "lacks language",
        "unexpected language",
    );
    errs.extend(presence(
        names,
        &[],
        |n| langs.iter().any(|e| name_matches(n, &e.name)),
        "lacks language",
        "",
    ));
    let prose = norm(
        &s.features
            .iter()
            .map(|f| format!("{} {}", f.name, f.description))
            .collect::<Vec<_>>()
            .join(" "),
    );
    errs.extend(presence(
        text,
        &[],
        |t| langs.iter().any(|e| norm(&e.name).contains(&norm(t))) || prose.contains(&norm(t)),
        "no language or feature text mentions",
        "",
    ));
    collect(errs)
}

/// `str` and `Strength` name the same ability.
fn same_ability(a: &str, b: &str) -> bool {
    let (x, y) = (norm(a), norm(b));
    x == y || (x.len() >= 3 && y.len() >= 3 && x[..3] == y[..3])
}

/// The spellcasting block a check names.
fn caster<'a>(s: &'a CharacterSnapshot, name: Option<&str>) -> Option<&'a SpellcastingSnapshot> {
    s.spellcasting
        .iter()
        .find(|c| name.map_or(true, |n| name_matches(n, &c.name)))
}

fn spellcasting(s: &CharacterSnapshot, check: &CharCheck) -> Result<(), String> {
    let CharCheck::Spellcasting {
        name,
        present,
        ability,
        dc,
        attack,
        prepare_limit,
        slots,
        no_slots,
        cantrips,
        spells_include_ids,
        spells_include_names,
        spells_exclude_ids,
        spells_exclude_names,
    } = check
    else {
        unreachable!("spellcasting")
    };
    let block = caster(s, name.as_deref());
    let c = match (block, *present == Some(false)) {
        (None, true) => return Ok(()),
        (Some(c), true) => return Err(format!("unexpected spellcasting block {:?}", c.name)),
        (None, false) => return Err(format!("no spellcasting block {name:?}")),
        (Some(c), false) => c,
    };
    let has_id = |id: &str| c.spells.iter().any(|x| x.id == id);
    let has_name = |n: &str| c.spells.iter().any(|x| name_matches(n, &x.name));
    let empty = c.slots.iter().all(|n| *n == 0) && c.pact.is_none();
    let mut errs = vec![
        ability
            .as_deref()
            .filter(|a| !same_ability(a, &c.ability))
            .map(|a| format!("ability = {:?}, expected {a:?}", c.ability)),
        eq("dc", c.dc, *dc),
        eq("attack", c.attack, *attack),
        eq("prepare limit", c.prepare_limit, prepare_limit.map(Some)),
        slots.as_ref().and_then(|w| {
            eq(
                "slots",
                &c.slots[..w.len().min(9)],
                Some(&w[..w.len().min(9)]),
            )
        }),
        eq("no slots", empty, *no_slots),
        eq(
            "cantrips",
            c.spells.iter().filter(|x| x.level == 0).count(),
            *cantrips,
        ),
    ];
    errs.extend(presence(
        spells_include_ids,
        spells_exclude_ids,
        has_id,
        "lacks spell",
        "lists excluded spell",
    ));
    errs.extend(presence(
        spells_include_names,
        spells_exclude_names,
        has_name,
        "lacks spell",
        "lists excluded spell",
    ));
    collect(errs)
}

fn defenses(s: &CharacterSnapshot, check: &CharCheck) -> Result<(), String> {
    let CharCheck::Defenses {
        resistances,
        immunities,
        vulnerabilities,
        condition_immunities,
        absent,
    } = check
    else {
        unreachable!("defenses")
    };
    let d = &s.defenses;
    let lists = [
        ("resistance", &d.resistances, resistances),
        ("immunity", &d.immunities, immunities),
        ("vulnerability", &d.vulnerabilities, vulnerabilities),
        (
            "condition immunity",
            &d.condition_immunities,
            condition_immunities,
        ),
    ];
    let mut errs = Errs::new();
    for (what, have, want) in &lists {
        let has = |n: &str| have.iter().any(|e| name_matches(n, &e.name));
        errs.extend(presence(want, &[], has, &format!("lacks {what}"), ""));
    }
    let anywhere = |n: &str| {
        lists
            .iter()
            .any(|(_, have, _)| have.iter().any(|e| name_matches(n, &e.name)))
    };
    errs.extend(presence(&[], absent, anywhere, "", "unexpected defence"));
    collect(errs)
}

fn advantage(
    s: &CharacterSnapshot,
    roll: Option<&str>,
    text: &[String],
    absent: Option<bool>,
) -> Result<(), String> {
    let found = s.advantages.iter().any(|a| {
        roll.map_or(true, |r| name_matches(r, &a.roll))
            && text.iter().all(|t| norm(&a.text).contains(&norm(t)))
    });
    match (found, absent == Some(true)) {
        (true, false) | (false, true) => Ok(()),
        (false, false) => Err(format!("no advantage like {roll:?} {text:?}")),
        (true, true) => Err(format!("unexpected advantage like {roll:?} {text:?}")),
    }
}

fn item(
    b: &Built,
    equip: &str,
    equipped: Option<bool>,
    attuned: Option<bool>,
) -> Result<(), String> {
    let uid = b
        .uids
        .get(equip)
        .ok_or_else(|| format!("no equip entry {equip:?}"))?;
    let line = b
        .snap
        .inventory
        .items
        .iter()
        .find(|i| &i.uid == uid)
        .ok_or_else(|| format!("no inventory line for {equip}"))?;
    collect(vec![
        eq("equipped", line.equipped.is_some(), equipped),
        eq("attuned", line.attuned, attuned),
    ])
}

/// Evaluate one check.
pub fn check(l: &Loaded, b: &Built, c: &CharCheck) -> Result<(), String> {
    let s = &b.snap;
    match c {
        CharCheck::Clean {} => clean(s),
        CharCheck::HitPoints { value } => collect(vec![eq("hp", s.hit_points, Some(*value))]),
        CharCheck::ProficiencyBonus { value } => {
            collect(vec![eq("proficiency", s.proficiency_bonus, Some(*value))])
        }
        CharCheck::Save {
            ability,
            proficient,
            total,
        } => {
            let a = &s.abilities[ability_index(ability)?];
            collect(vec![
                eq("save proficient", a.save_proficient, *proficient),
                eq("save", a.save, *total),
            ])
        }
        CharCheck::Skill {
            skill: k,
            proficient,
            expertise,
            total,
            advantage,
        } => skill(s, k, *proficient, *expertise, *total, *advantage),
        CharCheck::Initiative { total, advantage } => collect(vec![
            eq("initiative", s.initiative, *total),
            eq("initiative advantage", s.initiative_advantage, *advantage),
        ]),
        CharCheck::Features { present, absent } => {
            let has = |n: &str| s.features.iter().any(|f| name_matches(n, &f.name));
            collect(presence(
                present,
                absent,
                has,
                "missing feature",
                "unexpected feature",
            ))
        }
        CharCheck::Granted { .. } => granted(s, c),
        CharCheck::Resource { name, max, absent } => resource(l, s, name, *max, *absent),
        CharCheck::SaveDc { name, value } => save_dc(l, s, name, *value),
        CharCheck::Choice { .. } => choice(s, c),
        CharCheck::Attack { .. } => attack(b, c),
        CharCheck::AbilityScores { value } => {
            let got: Vec<i32> = s.abilities.iter().map(|a| a.score).collect();
            collect(vec![eq("scores", got, Some(value.to_vec()))])
        }
        CharCheck::Speed { .. } => speed(s, c),
        CharCheck::Size { value } => size(s, value),
        CharCheck::Vision { .. } => vision(s, c),
        CharCheck::Languages { .. } => languages(s, c),
        CharCheck::ArmorClass { value } => collect(vec![eq("ac", s.armor_class, Some(*value))]),
        CharCheck::Spellcasting { .. } => spellcasting(s, c),
        CharCheck::Defenses { .. } => defenses(s, c),
        CharCheck::Advantage { roll, text, absent } => advantage(s, roll.as_deref(), text, *absent),
        CharCheck::Attunement { attuned, limit } => collect(vec![
            eq("attuned", s.inventory.attuned, *attuned),
            eq("attunement limit", s.inventory.attunement_limit, *limit),
        ]),
        CharCheck::Item {
            equip,
            equipped,
            attuned,
        } => item(b, equip, *equipped, *attuned),
        CharCheck::AnyOf { any_of } => {
            let mut why = Vec::new();
            for alt in any_of {
                match check(l, b, alt) {
                    Ok(()) => return Ok(()),
                    Err(e) => why.push(e),
                }
            }
            Err(format!("no alternative passes: {}", why.join(" | ")))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cast(spell_name: &str, source: &str, uses: Option<i32>) -> FreeCastSnapshot {
        FreeCastSnapshot {
            spell: "ID_SPELL_X".into(),
            name: spell_name.into(),
            uses,
            uses_rule: "1".into(),
            recharge: "long_rest".into(),
            also_with_slots: false,
            cast_level: None,
            source: source.into(),
        }
    }

    #[test]
    fn a_free_cast_is_found_by_its_granting_element_name() {
        let casts = [cast("Glow Orb", "ID_TRAIT_EMBER_CALL", Some(1))];
        let names = |id: &str| (id == "ID_TRAIT_EMBER_CALL").then(|| "Ember-Call".to_string());
        assert_eq!(free_cast_max(&casts, "ember-call", &names), Some(1));
        assert_eq!(free_cast_max(&casts, "glow orb", &names), Some(1));
        assert_eq!(free_cast_max(&casts, "other", &names), None);
    }

    #[test]
    fn a_free_cast_with_unresolved_uses_is_not_a_count() {
        let casts = [cast("Glow Orb", "ID_T", None)];
        assert_eq!(free_cast_max(&casts, "glow orb", &|_| None), None);
    }
}
