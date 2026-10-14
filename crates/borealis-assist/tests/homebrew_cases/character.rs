//! Build a reference character with the real engine and run its checks.

use std::collections::BTreeMap;

use borealis_content::Ruleset;
use borealis_engine::commands::{self, slot_key};
use borealis_engine::ids::OPT_AVERAGE_HP;
use borealis_engine::snapshot::CharacterSnapshot;
use borealis_engine::{CharacterState, Choice, Engine, Severity};

use crate::pack::{name_matches, norm, Loaded};
use crate::schema::{CharCheck, CharacterSpec, Head};
use crate::Row;

const ABILITIES: [&str; 6] = [
    "strength",
    "dexterity",
    "constitution",
    "intelligence",
    "wisdom",
    "charisma",
];

/// Resolve `name` (pack element of one of `types`) or `id:<library id>`.
fn resolve(l: &Loaded, types: &str, name: &str) -> Result<String, String> {
    if let Some(id) = name.strip_prefix("id:") {
        return l
            .store
            .get(id)
            .map(|_| id.to_string())
            .ok_or_else(|| format!("no library element {id}"));
    }
    l.elements
        .iter()
        .find(|e| name_matches(name, &e.name) && name_matches(types, &e.ty))
        .map(|e| e.id.clone())
        .ok_or_else(|| format!("no {types} named {name:?} in the imported pack"))
}

/// Select `id` in the first open slot that offers it.
fn pick(engine: &Engine, s: &CharacterState, id: &str) -> Option<Result<CharacterState, String>> {
    let snap = engine.derive(s);
    let slot = snap
        .slots
        .iter()
        .find(|x| x.chosen.is_none() && x.options.iter().any(|o| o.id == id))?;
    Some(
        commands::select(engine, s, &slot.key, Choice::Element(id.to_string()))
            .map(|o| o.state)
            .map_err(|e| format!("selecting {id}: {e}")),
    )
}

fn pick_required(engine: &Engine, s: &CharacterState, id: &str) -> Result<CharacterState, String> {
    pick(engine, s, id).unwrap_or_else(|| Err(format!("no open slot offers {id}")))
}

/// A built character: its snapshot and the inventory uid of every equip key.
pub struct Built {
    pub snap: CharacterSnapshot,
    pub uids: BTreeMap<String, String>,
}

/// Build the character described by `spec`.
pub fn build(
    engine: &Engine,
    l: &Loaded,
    ruleset: Ruleset,
    spec: &CharacterSpec,
) -> Result<Built, String> {
    let class_id = match (&spec.class_id, &spec.class) {
        (Some(id), _) => id.clone(),
        (None, Some(name)) => resolve(l, "Class", name)?,
        (None, None) => return Err("the character names no class".into()),
    };
    let sub_id = match &spec.subclass {
        Some(n) => Some(resolve(l, "Archetype|Subclass", n)?),
        None => None,
    };
    let mut s = commands::new_character(engine, ruleset);
    if engine.has_element(ruleset, OPT_AVERAGE_HP) && !s.has_option(OPT_AVERAGE_HP) {
        s = commands::set_option(engine, &s, OPT_AVERAGE_HP, true)
            .map_err(|e| e.to_string())?
            .state;
    }
    for id in [&spec.race_id, &spec.background_id].into_iter().flatten() {
        s = pick_required(engine, &s, id)?;
    }
    s = match pick(engine, &s, &class_id) {
        Some(r) => r?,
        None => {
            let snap = engine.derive(&s);
            let key = slot_key(&snap, "ID_LEVEL_1", "Class", 1).ok_or("no class slot")?;
            commands::select(engine, &s, &key, Choice::Element(class_id.clone()))
                .map_err(|e| format!("the engine refuses class {class_id}: {e}"))?
                .state
        }
    };
    s = commands::set_base_scores(engine, &s, spec.scores)
        .map_err(|e| format!("scores: {e}"))?
        .state;
    let mut sub_done = sub_id.is_none();
    for level in 1..=spec.level {
        if level > 1 {
            s = commands::level_up_main(engine, &s, None)
                .map_err(|e| format!("level-up to {level}: {e}"))?
                .state;
        }
        if let (false, Some(id)) = (sub_done, &sub_id) {
            if let Some(r) = pick(engine, &s, id) {
                s = r?;
                sub_done = true;
            }
        }
    }
    if let (false, Some(id)) = (sub_done, &sub_id) {
        return Err(format!(
            "subclass {id} was never offered up to level {}",
            spec.level
        ));
    }
    let mut uids = BTreeMap::new();
    for eq in &spec.equip {
        let item = resolve(l, "Weapon|Armor|Item|Magic Item", &eq.item)?;
        s = commands::add_item(engine, &s, &item, 1)
            .map_err(|e| format!("add {item}: {e}"))?
            .state;
        let uid = s
            .inventory
            .items
            .last()
            .map(|i| i.uid.clone())
            .ok_or("item not added")?;
        if let Some(a) = &eq.adorner {
            let a = resolve(l, "Magic Item|Item", a)?;
            s = commands::set_adorner(engine, &s, &uid, Some(&a))
                .map_err(|e| format!("adorn {item} with {a}: {e}"))?
                .state;
        }
        s = commands::equip(engine, &s, &uid, None)
            .map_err(|e| format!("equip {item}: {e}"))?
            .state;
        uids.insert(eq.key.clone(), uid);
    }
    Ok(Built {
        snap: engine.derive(&s),
        uids,
    })
}

fn eq<T: PartialEq + std::fmt::Debug>(what: &str, got: T, want: Option<T>) -> Option<String> {
    match want {
        Some(w) if w != got => Some(format!("{what} = {got:?}, expected {w:?}")),
        _ => None,
    }
}

fn collect(errs: Vec<Option<String>>) -> Result<(), String> {
    let errs: Vec<String> = errs.into_iter().flatten().collect();
    if errs.is_empty() {
        Ok(())
    } else {
        Err(errs.join("; "))
    }
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

/// Evaluate one check.
pub fn check(l: &Loaded, b: &Built, c: &CharCheck) -> Result<(), String> {
    let s = &b.snap;
    match c {
        CharCheck::Clean {} => {
            let mut errs: Vec<String> = s
                .diagnostics
                .iter()
                .filter(|d| d.severity == Severity::Error)
                .map(|d| format!("{}: {}", d.code, d.message))
                .collect();
            errs.extend(
                s.invalid_selections
                    .iter()
                    .map(|i| format!("invalid selection {i:?}")),
            );
            collect(errs.into_iter().map(Some).collect())
        }
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
            skill,
            proficient,
            expertise,
            total,
        } => {
            let k = s
                .skills
                .iter()
                .find(|k| name_matches(skill, &k.name))
                .ok_or_else(|| format!("no skill {skill}"))?;
            collect(vec![
                eq("proficient", k.proficient, *proficient),
                eq("expertise", k.expertise, *expertise),
                eq("total", k.total, *total),
            ])
        }
        CharCheck::Initiative { total, advantage } => collect(vec![
            eq("initiative", s.initiative, *total),
            eq("initiative advantage", s.initiative_advantage, *advantage),
        ]),
        CharCheck::Features { present, absent } => {
            let has = |n: &str| s.features.iter().any(|f| name_matches(n, &f.name));
            let mut errs: Vec<Option<String>> = present
                .iter()
                .filter(|n| !has(n))
                .map(|n| Some(format!("missing feature {n:?}")))
                .collect();
            errs.extend(
                absent
                    .iter()
                    .filter(|n| has(n))
                    .map(|n| Some(format!("unexpected feature {n:?}"))),
            );
            collect(errs)
        }
        CharCheck::Granted { ids, absent_ids } => {
            let has = |id: &str| s.elements.iter().any(|e| e.element.id == id);
            let mut errs: Vec<Option<String>> = ids
                .iter()
                .filter(|id| !has(id))
                .map(|id| Some(format!("lacks {id}")))
                .collect();
            errs.extend(
                absent_ids
                    .iter()
                    .filter(|id| has(id))
                    .map(|id| Some(format!("unexpectedly has {id}"))),
            );
            collect(errs)
        }
        CharCheck::Resource { name, max } => {
            let n = norm(name);
            let got = s
                .resources
                .iter()
                .find(|r| norm(&r.name).contains(&n) || norm(&r.id).contains(&n))
                .map(|r| r.max)
                .or_else(|| {
                    s.stats
                        .iter()
                        .find(|(k, _)| norm(k).contains(&n))
                        .map(|(_, v)| v.value)
                })
                .ok_or_else(|| format!("no resource or stat named like {name:?}"))?;
            collect(vec![eq(name, got, Some(*max))])
        }
        CharCheck::SaveDc { name, value } => {
            let n = norm(name);
            let dc = s
                .save_dcs
                .iter()
                .find(|d| norm(&element_name(l, &d.owner)).contains(&n) || norm(&d.id).contains(&n))
                .ok_or_else(|| format!("no save DC named like {name:?}"))?;
            collect(vec![eq("save dc", dc.value, Some(Some(*value)))])
        }
        CharCheck::Choice {
            name_contains,
            ty,
            owner,
            options_include_ids,
            options_include_names,
            count,
            options,
            min_options,
        } => {
            let slots: Vec<_> = s
                .slots
                .iter()
                .filter(|x| {
                    name_contains
                        .as_deref()
                        .map_or(true, |n| norm(&x.name).contains(&norm(n)))
                        && ty.as_deref().map_or(true, |t| name_matches(t, &x.ty))
                        && owner
                            .as_deref()
                            .map_or(true, |o| name_matches(o, &x.owner.name))
                        && options_include_ids
                            .iter()
                            .all(|id| x.options.iter().any(|o| &o.id == id))
                        && options_include_names
                            .iter()
                            .all(|n| x.options.iter().any(|o| name_matches(n, &o.name)))
                })
                .collect();
            if slots.is_empty() {
                return Err("no matching choice slot".into());
            }
            let first = slots[0].options.len();
            collect(vec![
                eq("matching slots", slots.len(), *count),
                eq("options", first, *options),
                min_options
                    .filter(|m| first < *m)
                    .map(|m| format!("options = {first}, expected at least {m}")),
            ])
        }
        CharCheck::Attack {
            equip,
            attack,
            damage,
            range,
            proficient,
        } => {
            let uid = b
                .uids
                .get(equip)
                .ok_or_else(|| format!("no equip entry {equip:?}"))?;
            let a = s
                .attacks
                .iter()
                .find(|a| &a.uid == uid)
                .ok_or_else(|| format!("no attack line for {equip}"))?;
            let clean = |v: &str| norm(v).replace(' ', "");
            collect(vec![
                eq("attack", a.attack, *attack),
                eq("damage", clean(&a.damage), damage.as_deref().map(clean)),
                eq("range", clean(&a.range), range.as_deref().map(clean)),
                eq("proficient", a.proficient, *proficient),
            ])
        }
    }
}

/// Rows for one character (every check fails with the build error if the build fails).
pub fn rows(engine: &Engine, l: &Loaded, ruleset: Ruleset, spec: &CharacterSpec) -> Vec<Row> {
    let built = build(engine, l, ruleset, spec);
    let mk = |h: &Head, r: Result<(), String>| Row {
        id: format!("{}/{}", spec.id, h.id),
        level: h.level,
        class: h.class,
        pass: r.is_ok(),
        detail: r.err().unwrap_or_default(),
    };
    spec.expect
        .iter()
        .map(|x| match &built {
            Ok(b) => mk(&x.head, check(l, b, &x.check)),
            Err(e) => mk(&x.head, Err(format!("character not built: {e}"))),
        })
        .collect()
}
