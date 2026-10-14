//! Build a reference character with the real engine; the checks live in `checks.rs`.

use std::collections::BTreeMap;

use borealis_content::Ruleset;
use borealis_engine::commands::{self, slot_key};
use borealis_engine::ids::OPT_AVERAGE_HP;
use borealis_engine::snapshot::CharacterSnapshot;
use borealis_engine::{CharacterState, Choice, Engine};

use crate::checks::check;
use crate::pack::{name_matches, norm, Loaded};
use crate::schema::{CharacterSpec, Head};
use crate::Row;

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

/// What a pending selection selects.
#[derive(Debug, Clone)]
enum Target {
    /// This element id.
    Id(String),
    /// An option with this name (`A|B`).
    Name(String),
}

/// A selection made as soon as a matching open slot offers it.
#[derive(Debug, Clone)]
struct Pending {
    label: String,
    slot: Option<String>,
    target: Target,
}

impl Pending {
    fn id(label: &str, id: String) -> Self {
        Self {
            label: label.to_string(),
            slot: None,
            target: Target::Id(id),
        }
    }
}

/// Select the pending target in the first open matching slot; `None` = not offered yet.
fn try_pick(
    engine: &Engine,
    s: &CharacterState,
    p: &Pending,
) -> Option<Result<CharacterState, String>> {
    let snap = engine.derive(s);
    let (slot, choice) = snap.slots.iter().find_map(|x| {
        if x.chosen.is_some()
            || x.list_choice.is_some()
            || p.slot
                .as_deref()
                .is_some_and(|f| !norm(&x.name).contains(&norm(f)))
        {
            return None;
        }
        let element = x.options.iter().find(|o| match &p.target {
            Target::Id(id) => &o.id == id,
            Target::Name(n) => name_matches(n, &o.name),
        });
        if let Some(o) = element {
            return Some((x, Choice::Element(o.id.clone())));
        }
        // a `List` entry ("Strength +1" for the option "Strength")
        let Target::Name(n) = &p.target else {
            return None;
        };
        x.list_items
            .iter()
            .find(|(_, text)| name_matches(n, text) || name_matches(n, list_label(text)))
            .map(|(i, _)| (x, Choice::ListItem(*i)))
    })?;
    Some(
        commands::select(engine, s, &slot.key, choice.clone())
            .map(|o| o.state)
            .map_err(|e| format!("selecting {choice:?} for {}: {e}", p.label)),
    )
}

/// A list entry without its trailing signed amount: "Strength +1" -> "Strength".
fn list_label(text: &str) -> &str {
    text.rsplit_once(' ')
        .filter(|(_, n)| n.starts_with(['+', '-']) && n[1..].chars().all(|c| c.is_ascii_digit()))
        .map_or(text, |(label, _)| label)
}

/// Apply every pending selection that is offered now, repeating while one succeeds.
fn settle(
    engine: &Engine,
    mut s: CharacterState,
    pending: &mut Vec<Pending>,
) -> Result<CharacterState, String> {
    loop {
        let mut progressed = false;
        let mut i = 0;
        while i < pending.len() {
            match try_pick(engine, &s, &pending[i]) {
                Some(r) => {
                    s = r?;
                    pending.remove(i);
                    progressed = true;
                }
                None => i += 1,
            }
        }
        if !progressed {
            return Ok(s);
        }
    }
}

/// A `picks` entry: a pack element of that name (any type) by id, else an option name.
fn pick_target(l: &Loaded, option: &str) -> Result<Target, String> {
    if option.starts_with("id:") {
        return resolve(l, "", option).map(Target::Id);
    }
    Ok(l.elements
        .iter()
        .find(|e| name_matches(option, &e.name))
        .map_or_else(
            || Target::Name(option.to_string()),
            |e| Target::Id(e.id.clone()),
        ))
}

/// A built character: its snapshot and the inventory uid of every equip key.
pub struct Built {
    pub snap: CharacterSnapshot,
    pub uids: BTreeMap<String, String>,
}

/// Race, subrace and background ids (library id or pack name).
fn origin(l: &Loaded, spec: &CharacterSpec) -> Result<(Vec<String>, Vec<Pending>), String> {
    let mut now = Vec::new();
    if let Some(id) = &spec.race_id {
        now.push(id.clone());
    }
    if let Some(n) = &spec.race {
        now.push(resolve(l, "Race|Species", n)?);
    }
    if let Some(id) = &spec.background_id {
        now.push(id.clone());
    }
    if let Some(n) = &spec.background {
        now.push(resolve(l, "Background", n)?);
    }
    let mut later = Vec::new();
    if let Some(n) = &spec.subrace {
        let id = resolve(l, "Sub Race|Subrace|Subspecies", n)?;
        later.push(Pending::id("subrace", id));
    }
    Ok((now, later))
}

fn new_state(engine: &Engine, ruleset: Ruleset) -> Result<CharacterState, String> {
    let s = commands::new_character(engine, ruleset);
    if engine.has_element(ruleset, OPT_AVERAGE_HP) && !s.has_option(OPT_AVERAGE_HP) {
        return commands::set_option(engine, &s, OPT_AVERAGE_HP, true)
            .map(|o| o.state)
            .map_err(|e| e.to_string());
    }
    Ok(s)
}

fn select_class(
    engine: &Engine,
    s: &CharacterState,
    class_id: &str,
) -> Result<CharacterState, String> {
    let p = Pending::id("class", class_id.to_string());
    if let Some(r) = try_pick(engine, s, &p) {
        return r;
    }
    let snap = engine.derive(s);
    let key = slot_key(&snap, "ID_LEVEL_1", "Class", 1).ok_or("no class slot")?;
    commands::select(engine, s, &key, Choice::Element(class_id.to_string()))
        .map(|o| o.state)
        .map_err(|e| format!("the engine refuses class {class_id}: {e}"))
}

/// Add, adorn and equip the items of `spec.equip`.
fn equip_all(
    engine: &Engine,
    l: &Loaded,
    mut s: CharacterState,
    spec: &CharacterSpec,
) -> Result<(CharacterState, BTreeMap<String, String>), String> {
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
        s = commands::equip(engine, &s, &uid, eq.slot)
            .map_err(|e| format!("equip {item}: {e}"))?
            .state;
        uids.insert(eq.key.clone(), uid);
    }
    Ok((s, uids))
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
    let (origin_ids, mut pending) = origin(l, spec)?;
    if let Some(n) = &spec.subclass {
        let id = resolve(l, "Archetype|Subclass", n)?;
        pending.push(Pending::id("subclass", id));
    }
    for p in &spec.picks {
        pending.push(Pending {
            label: format!("pick {:?}", p.option),
            slot: p.slot.clone(),
            target: pick_target(l, &p.option)?,
        });
    }
    let mut s = new_state(engine, ruleset)?;
    for id in &origin_ids {
        let p = Pending::id("origin", id.clone());
        s = try_pick(engine, &s, &p).unwrap_or_else(|| Err(format!("no open slot offers {id}")))?;
    }
    s = select_class(engine, &s, &class_id)?;
    s = commands::set_base_scores(engine, &s, spec.scores)
        .map_err(|e| format!("scores: {e}"))?
        .state;
    for level in 1..=spec.level {
        if level > 1 {
            s = commands::level_up_main(engine, &s, None)
                .map_err(|e| format!("level-up to {level}: {e}"))?
                .state;
        }
        s = settle(engine, s, &mut pending)?;
    }
    if let Some(p) = pending.first() {
        let what = match &p.target {
            Target::Id(id) => id.clone(),
            Target::Name(n) => n.clone(),
        };
        return Err(format!(
            "{} {what} was never offered up to level {}",
            p.label, spec.level
        ));
    }
    let (s, uids) = equip_all(engine, l, s, spec)?;
    Ok(Built {
        snap: engine.derive(&s),
        uids,
    })
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
