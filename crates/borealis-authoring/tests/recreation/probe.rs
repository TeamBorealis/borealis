//! The test characters of the re-creation suite (spec 18.2): what "a test character that takes it" means per kind,
//! run the same way on two libraries (the bundled one and the one with every element re-created from its form).
//!
//! * a class levels 1 to 20 (the WB-07 sweep's ladder), a subclass on the class that offers it; at every level the
//!   character makes the picks a player makes (the first option of each open choice not already taken), so the
//!   options the class offers are applied, not only listed;
//! * an option a ladder offers (a fighting style, an invocation, a spell) is taken at the level it is offered;
//! * a species, race, background, feat (any element a choice of a fighter offers at level 1 or 4) is taken there; and
//!   an option the choices of a taken species, race or background offer (a subrace, a lineage, a language) is taken
//!   on that character;
//! * an item is added to a level-1 fighter's inventory and equipped when it can be (its attack and armor class);
//! * every other element: a plain derive, as `load_into_engine` does.
//!
//! Each probe yields the snapshots it derived as JSON. Deterministic: same library, same snapshots.

use borealis_content::Ruleset;
use borealis_engine::commands::{self, slot_key};
use borealis_engine::snapshot::SlotSnapshot;
use borealis_engine::{CharacterSnapshot, CharacterState, Choice, Engine, SlotKey};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

/// Highest level of a ladder.
pub const MAX_LEVEL: u32 = 20;
/// Scores of every test character: 13+ everywhere, so ability prerequisites and multiclass entries are met.
const SCORES: [i32; 6] = [15, 14, 14, 13, 13, 13];
/// Rounds of picks per level (a pick can open another choice).
const FILL_ROUNDS: usize = 4;
/// Options tried per choice before it is left open.
const FILL_TRIES: usize = 8;
/// Choices the test characters leave to the probe (the class, origin and subclass are what is tested).
const NOT_FILLED: [&str; 8] = [
    "Archetype",
    "Class",
    "Race",
    "Sub Race",
    "Species",
    "Background",
    "Alignment",
    "Deity",
];
/// Levels at which a host fighter is offered origin choices and feats.
pub const HOST_LEVELS: [u32; 2] = [1, 4];

/// The host class per edition.
pub fn fighter(ruleset: Ruleset) -> &'static str {
    if ruleset == Ruleset::Dnd2024 {
        "ID_2024_CLASS_FIGHTER"
    } else {
        "ID_WOTC_PHB_CLASS_FIGHTER"
    }
}

/// What a probe does.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Probe {
    /// A class (and subclass) 1-20.
    Ladder {
        ruleset: Ruleset,
        class: String,
        subclass: Option<String>,
    },
    /// Ladder `ladder` at `level`: `option` taken at the first open choice that offers it, before the default picks.
    Option {
        ladder: Box<Probe>,
        level: u32,
        option: String,
    },
    /// A host fighter of `level` takes `id` at the choice that offers it.
    Take {
        ruleset: Ruleset,
        level: u32,
        id: String,
    },
    /// A level-1 fighter adds the item and equips it when it can.
    Item { ruleset: Ruleset, id: String },
    /// A new character, derived.
    Blank { ruleset: Ruleset },
}

impl Probe {
    /// A short name for reports.
    pub fn label(&self) -> String {
        match self {
            Self::Ladder {
                class, subclass, ..
            } => match subclass {
                Some(s) => format!("ladder {class} / {s}"),
                None => format!("ladder {class}"),
            },
            Self::Option {
                ladder,
                level,
                option,
                ..
            } => format!("{} takes {option} at level {level}", ladder.label()),
            Self::Take { level, id, .. } => format!("level-{level} fighter takes {id}"),
            Self::Item { id, .. } => format!("fighter equips {id}"),
            Self::Blank { ruleset } => format!("new {ruleset:?} character"),
        }
    }
}

/// One level of a ladder: the state before the test character's picks, the choices open then (option id -> the
/// first open choice offering it), and the snapshot after the picks.
#[derive(Clone)]
pub struct Rung {
    pub level: u32,
    pub before: CharacterState,
    pub open: HashMap<String, SlotKey>,
    pub snapshot: CharacterSnapshot,
}

/// Option id -> the first open choice of `snap` that offers it.
fn open_choices(snap: &CharacterSnapshot) -> HashMap<String, SlotKey> {
    let mut out = HashMap::new();
    for s in snap.slots.iter().filter(|s| s.chosen.is_none()) {
        for o in &s.options {
            out.entry(o.id.clone()).or_insert_with(|| s.key.clone());
        }
    }
    out
}

/// A snapshot as compared: JSON, in a canonical form ([`canonical`]).
pub fn json(s: &CharacterSnapshot) -> Value {
    let mut v = with_parent_ids(serde_json::to_value(s).expect("snapshot json"));
    if let Some(text) = v.get("sheet_summary").and_then(Value::as_str) {
        let mut lines: Vec<&str> = text.lines().collect();
        lines.sort_unstable();
        v["sheet_summary"] = Value::String(lines.join("\n"));
    }
    // a save DC is compared by its value: its formula is shown as written (`8+strength+proficiency` in the SRD
    // data, `8 + strength:modifier + proficiency` as the creator's formula builder writes it)
    if let Some(dcs) = v.get_mut("save_dcs").and_then(Value::as_array_mut) {
        for dc in dcs.iter_mut().filter_map(Value::as_object_mut) {
            dc.remove("formula");
        }
    }
    canonical(&v)
}

/// The registered elements name their parent by id, not by position (a re-created element may grant in another
/// order: the elements register in another order, nothing else changes).
fn with_parent_ids(mut v: Value) -> Value {
    let Some(list) = v.get_mut("elements").and_then(Value::as_array_mut) else {
        return v;
    };
    let ids: Vec<Value> = list.iter().map(|e| e["element"]["id"].clone()).collect();
    for e in list.iter_mut() {
        if let Some(m) = e.as_object_mut() {
            let parent = m
                .get("parent")
                .and_then(Value::as_u64)
                .and_then(|i| ids.get(usize::try_from(i).ok()?).cloned())
                .unwrap_or(Value::Null);
            m.insert("parent".into(), parent);
            m.remove("index");
        }
    }
    v
}

/// The snapshot with what only orders or names it left out: a list of objects is compared as a multiset (sorted), a
/// choice's key does not carry the position of its rule in the owner's rule list (`SlotKey::rule`), and a spell
/// choice's label is a name (the creator's spellcasting section labels the spell choices it writes "Spell (Bard)",
/// the SRD 5.1 text "Spell Known (Bard)"; the options, counts and levels are compared). Lists of plain values (a
/// slot table, ability scores) keep their order. Text is compared without its white space (a formula the creator
/// writes canonically, `5 * cleric level`, is shown in notes as it is written), and the sheet summary as its set of
/// lines (it lists features in the order they register).
pub fn canonical(v: &Value) -> Value {
    match v {
        Value::Object(m) => {
            let slot_key =
                m.contains_key("owner") && m.contains_key("ordinal") && m.contains_key("rule");
            let spell_choice =
                m.contains_key("key") && m.get("ty").and_then(Value::as_str) == Some("Spell");
            Value::Object(
                m.iter()
                    .filter(|(k, _)| !(slot_key && k.as_str() == "rule"))
                    .filter(|(k, _)| !(spell_choice && k.as_str() == "name"))
                    .map(|(k, x)| match (k.as_str(), x) {
                        ("instance", Value::String(t)) => {
                            (k.clone(), Value::String(instance_without_rules(t)))
                        }
                        _ => (k.clone(), canonical(x)),
                    })
                    .collect(),
            )
        }
        Value::Array(items) => {
            let mut out: Vec<Value> = items.iter().map(canonical).collect();
            if out.iter().all(Value::is_object) {
                out.sort_by_cached_key(Value::to_string);
            }
            Value::Array(out)
        }
        Value::String(t) => Value::String(t.split_whitespace().collect()),
        other => other.clone(),
    }
}

/// A repeatable element's instance tag without the rule positions it names (`<owner>:<rule>:<ordinal>` of the choice
/// that took it, `<owner>:g<rule>` of the grant), as a choice's key drops its `rule`: a re-created owner may write its
/// rules in another order.
fn instance_without_rules(tag: &str) -> String {
    tag.split('@')
        .map(|part| {
            let pieces: Vec<&str> = part.split(':').collect();
            match pieces.as_slice() {
                [owner, rule, ordinal] if rule.chars().all(|c| c.is_ascii_digit()) => {
                    format!("{owner}:{ordinal}")
                }
                [owner, grant] if grant.starts_with('g') => format!("{owner}:g"),
                _ => part.to_string(),
            }
        })
        .collect::<Vec<_>>()
        .join("@")
}

/// A choice the test character fills.
fn fillable(s: &SlotSnapshot) -> bool {
    s.chosen.is_none()
        && s.list_choice.is_none()
        && s.requires_selection
        && s.rejected.is_none()
        && !NOT_FILLED.contains(&s.ty.as_str())
        && (!s.options.is_empty() || !s.list_items.is_empty())
}

/// The picks a player makes: the first option of each open choice not already taken, in rounds.
pub fn fill(engine: &Engine, state: CharacterState) -> CharacterState {
    let mut state = state;
    for _ in 0..FILL_ROUNDS {
        let snap = engine.derive(&state);
        let mut taken: HashSet<String> = snap
            .slots
            .iter()
            .filter_map(|s| s.chosen.as_ref().map(|c| c.id.clone()))
            .collect();
        let mut changed = false;
        for s in snap.slots.iter().filter(|s| fillable(s)) {
            let choices = s
                .options
                .iter()
                .filter(|o| !taken.contains(&o.id))
                .map(|o| Choice::Element(o.id.clone()))
                .chain(s.list_items.iter().map(|(i, _)| Choice::ListItem(*i)))
                .take(FILL_TRIES);
            for c in choices {
                let picked = match &c {
                    Choice::Element(id) => Some(id.clone()),
                    Choice::ListItem(_) => None,
                };
                if let Ok(o) = commands::select(engine, &state, &s.key, c) {
                    state = o.state;
                    taken.extend(picked);
                    changed = true;
                    break;
                }
            }
        }
        if !changed {
            break;
        }
    }
    state
}

/// A character of `ruleset` with class `class` at level 1 and the test scores (`None` when the class can't be taken).
fn with_class(engine: &Engine, ruleset: Ruleset, class: &str) -> Option<CharacterState> {
    let state = commands::new_character(engine, ruleset);
    let snap = engine.derive(&state);
    let key = slot_key(&snap, "ID_LEVEL_1", "Class", 1)?;
    let state = commands::select(engine, &state, &key, Choice::Element(class.to_string()))
        .ok()?
        .state;
    Some(
        commands::set_base_scores(engine, &state, SCORES)
            .map(|o| o.state)
            .unwrap_or(state),
    )
}

/// Take the subclass at the first open choice that offers it; the state unchanged when none does.
fn take_subclass(engine: &Engine, state: CharacterState, sub: &str) -> (CharacterState, bool) {
    let snap = engine.derive(&state);
    let slot = snap
        .slots
        .iter()
        .find(|s| s.chosen.is_none() && s.options.iter().any(|o| o.id == sub));
    match slot {
        Some(s) => match commands::select(engine, &state, &s.key, Choice::Element(sub.into())) {
            Ok(o) => (o.state, true),
            Err(_) => (state, false),
        },
        None => (state, false),
    }
}

/// A class (and subclass) levelled 1-20, the test character's picks made at every level.
pub fn ladder(engine: &Engine, ruleset: Ruleset, class: &str, sub: Option<&str>) -> Vec<Rung> {
    let Some(mut state) = with_class(engine, ruleset, class) else {
        return Vec::new();
    };
    let mut picked = sub.is_none();
    let mut out = Vec::new();
    for level in 1..=MAX_LEVEL {
        if level > 1 {
            match commands::level_up_main(engine, &state, None) {
                Ok(o) => state = o.state,
                Err(_) => break,
            }
        }
        if let (Some(s), false) = (sub, picked) {
            let (next, ok) = take_subclass(engine, state, s);
            state = next;
            picked = ok;
        }
        let before = state.clone();
        let open = open_choices(&engine.derive(&before));
        state = fill(engine, state);
        out.push(Rung {
            level,
            before,
            open,
            snapshot: engine.derive(&state),
        });
    }
    out
}

/// The host fighters of `ruleset`: (level, state before picks, snapshot of it).
pub fn hosts(engine: &Engine, ruleset: Ruleset) -> Vec<(u32, CharacterState, CharacterSnapshot)> {
    let Some(mut state) = with_class(engine, ruleset, fighter(ruleset)) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for level in 1..=*HOST_LEVELS.iter().max().unwrap_or(&1) {
        if level > 1 {
            match commands::level_up_main(engine, &state, None) {
                Ok(o) => state = o.state,
                Err(_) => break,
            }
        }
        if HOST_LEVELS.contains(&level) {
            out.push((level, state.clone(), engine.derive(&state)));
        }
    }
    out
}

/// Choice types whose options are a character's origin: a host takes each, and the choices that opens are probed.
pub const ORIGIN_TYPES: [&str; 3] = ["Race", "Species", "Background"];

/// `state` takes origin `id` at choice `key`: the rung of a one-level ladder (the choices it opens, before and after
/// the test character's picks).
pub fn origin_rung(
    engine: &Engine,
    state: &CharacterState,
    key: &SlotKey,
    id: &str,
) -> Option<Rung> {
    let before = commands::select(engine, state, key, Choice::Element(id.to_string()))
        .ok()?
        .state;
    let open = open_choices(&engine.derive(&before));
    let snapshot = engine.derive(&fill(engine, before.clone()));
    Some(Rung {
        level: 1,
        before,
        open,
        snapshot,
    })
}

/// The open choice of `snap` that offers `id`.
pub fn offering(snap: &CharacterSnapshot, id: &str) -> Option<SlotKey> {
    snap.slots
        .iter()
        .find(|s| s.chosen.is_none() && s.options.iter().any(|o| o.id == id))
        .map(|s| s.key.clone())
}

/// Take `id` at choice `key` of `state`, make the picks, derive. `None` when the engine refuses it.
pub fn take_at(
    engine: &Engine,
    state: &CharacterState,
    key: &SlotKey,
    id: &str,
) -> Option<CharacterSnapshot> {
    let next = commands::select(engine, state, key, Choice::Element(id.to_string()))
        .ok()?
        .state;
    Some(engine.derive(&fill(engine, next)))
}

/// A level-1 fighter adds item `id` and equips it when it can.
pub fn item(engine: &Engine, ruleset: Ruleset, id: &str) -> Vec<CharacterSnapshot> {
    let Some(state) = with_class(engine, ruleset, fighter(ruleset)) else {
        return Vec::new();
    };
    let Ok(added) = commands::add_item(engine, &state, id, 1) else {
        return vec![engine.derive(&state)];
    };
    let state = added.state;
    let carried = engine.derive(&state);
    let uid = carried
        .inventory
        .items
        .iter()
        .find(|i| i.item.id == id)
        .filter(|i| i.equippable)
        .map(|i| i.uid.clone());
    let Some(uid) = uid else {
        return vec![carried];
    };
    match commands::equip(engine, &state, &uid, None) {
        Ok(o) => vec![carried, engine.derive(&o.state)],
        Err(_) => vec![carried],
    }
}

/// A new character, derived.
pub fn blank(engine: &Engine, ruleset: Ruleset) -> CharacterSnapshot {
    engine.derive(&commands::new_character(engine, ruleset))
}
