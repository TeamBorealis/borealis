//! Helpers of the gated 5etools item audit (`fivetools_items.rs`): the raw 5etools entries
//! matched to imported elements, the test characters, the tally and the per-book table.

#[path = "../../../tools/regression-gate/outcome.rs"]
mod outcome;

use borealis_content::{Element, Ruleset};
use borealis_engine::commands::{self, slot_key};
use borealis_engine::state::AbilityMethod;
use borealis_engine::{invariants, CharacterSnapshot, CharacterState, Choice, Engine};
use borealis_import_5etools::convert::item::{generate_variant_items, generic_variant_items};
use borealis_import_5etools::report::ImportReport;
use borealis_import_5etools::util::id_fragment;
use borealis_import_5etools::{copy, folder};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

pub type Counts = BTreeMap<String, usize>;

/// Element types that are items.
pub const ITEM_TYPES: &[&str] = &["Weapon", "Armor", "Magic Item", "Item"];
/// Str 15, Dex 18: finesse and ranged weapons use Dex, medium armor caps Dex.
pub const SCORES: [i32; 6] = [15, 18, 14, 12, 12, 10];
/// Str below every armor's strength requirement.
pub const WEAK_SCORES: [i32; 6] = [8, 18, 14, 12, 12, 10];
pub const LEVEL: u32 = 5;

pub fn bump(map: &mut Counts, key: impl Into<String>) {
    *map.entry(key.into()).or_default() += 1;
}

/// Well-known ids of the test setup.
pub mod ids {
    use borealis_content::Ruleset;

    fn by(rs: Ruleset, classic: &'static str, modern: &'static str) -> &'static str {
        if rs == Ruleset::Dnd2024 {
            modern
        } else {
            classic
        }
    }
    pub fn fighter(rs: Ruleset) -> &'static str {
        by(rs, "ID_5ET_PHB_CLASS_FIGHTER", "ID_5ET_XPHB_CLASS_FIGHTER")
    }
    pub fn human(rs: Ruleset) -> &'static str {
        by(rs, "ID_5ET_PHB_RACE_HUMAN", "ID_5ET_XPHB_SPECIES_HUMAN")
    }
    pub fn longsword(rs: Ruleset) -> &'static str {
        by(
            rs,
            "ID_5ET_PHB_ITEM_LONGSWORD",
            "ID_5ET_XPHB_ITEM_LONGSWORD",
        )
    }
    pub fn longbow(rs: Ruleset) -> &'static str {
        by(rs, "ID_5ET_PHB_ITEM_LONGBOW", "ID_5ET_XPHB_ITEM_LONGBOW")
    }
    pub fn light_crossbow(rs: Ruleset) -> &'static str {
        by(
            rs,
            "ID_5ET_PHB_ITEM_LIGHT_CROSSBOW",
            "ID_5ET_XPHB_ITEM_LIGHT_CROSSBOW",
        )
    }
    pub fn sling(rs: Ruleset) -> &'static str {
        by(rs, "ID_5ET_PHB_ITEM_SLING", "ID_5ET_XPHB_ITEM_SLING")
    }
    pub fn pistol(rs: Ruleset) -> &'static str {
        by(rs, "ID_5ET_DMG_ITEM_PISTOL", "ID_5ET_XPHB_ITEM_PISTOL")
    }
    pub fn laser_pistol(rs: Ruleset) -> &'static str {
        by(
            rs,
            "ID_5ET_DMG_ITEM_LASER_PISTOL",
            "ID_5ET_XDMG_ITEM_LASER_PISTOL",
        )
    }
    pub fn blowgun(rs: Ruleset) -> &'static str {
        by(rs, "ID_5ET_PHB_ITEM_BLOWGUN", "ID_5ET_XPHB_ITEM_BLOWGUN")
    }
    pub fn breastplate(rs: Ruleset) -> &'static str {
        by(
            rs,
            "ID_5ET_PHB_ITEM_BREASTPLATE",
            "ID_5ET_XPHB_ITEM_BREASTPLATE",
        )
    }
    pub fn shield(rs: Ruleset) -> &'static str {
        by(rs, "ID_5ET_PHB_ITEM_SHIELD", "ID_5ET_XPHB_ITEM_SHIELD")
    }
}

// ------------------------------------------------------------------------------- verdicts

/// Outcome of one value check.
pub enum Verdict {
    Correct,
    Wrong(String),
    NotModelled,
}

#[derive(Default)]
pub struct Cat {
    checked: usize,
    correct: usize,
    wrong: Vec<(String, String)>,
    not_modelled: Vec<String>,
}

/// Value checks by category.
#[derive(Default)]
pub struct Tally {
    cats: BTreeMap<String, Cat>,
}

impl Tally {
    pub fn record(&mut self, cat: &str, id: &str, v: Verdict) {
        let (pass, why) = match &v {
            Verdict::Correct => (true, ""),
            Verdict::Wrong(why) => (false, why.as_str()),
            Verdict::NotModelled => (false, "not modelled"),
        };
        outcome::record("fivetools_items", &format!("{cat}/{id}"), pass, why);
        let c = self.cats.entry(cat.to_string()).or_default();
        c.checked += 1;
        match v {
            Verdict::Correct => c.correct += 1,
            Verdict::Wrong(why) => c.wrong.push((id.to_string(), why)),
            Verdict::NotModelled => c.not_modelled.push(id.to_string()),
        }
    }

    /// `(wrong, not modelled)` counts of a category (`None` when nothing was checked).
    pub fn misses(&self, cat: &str) -> Option<(usize, usize)> {
        self.cats
            .get(cat)
            .map(|c| (c.wrong.len(), c.not_modelled.len()))
    }

    /// Categories with wrong values, with the count.
    pub fn wrong_counts(&self) -> Vec<(&str, usize)> {
        self.cats
            .iter()
            .filter(|(_, c)| !c.wrong.is_empty())
            .map(|(cat, c)| (cat.as_str(), c.wrong.len()))
            .collect()
    }

    /// Ids of every wrong or not-modelled check, with the category.
    fn problems(&self) -> impl Iterator<Item = (&str, &str, bool)> {
        self.cats.iter().flat_map(|(cat, c)| {
            c.wrong
                .iter()
                .map(move |(id, _)| (cat.as_str(), id.as_str(), true))
                .chain(
                    c.not_modelled
                        .iter()
                        .map(move |id| (cat.as_str(), id.as_str(), false)),
                )
        })
    }

    pub fn print(&self) {
        println!("--- value checks: checked / correct / wrong / not modelled  category");
        for (cat, c) in &self.cats {
            println!(
                "{:>6} {:>6} {:>6} {:>6}  {cat}",
                c.checked,
                c.correct,
                c.wrong.len(),
                c.not_modelled.len()
            );
        }
        for (cat, c) in &self.cats {
            if c.wrong.is_empty() {
                continue;
            }
            println!("--- wrong: {cat} ({})", c.wrong.len());
            let mut why: Counts = Counts::new();
            for (_, w) in &c.wrong {
                bump(&mut why, digits_out(w));
            }
            for (w, n) in why.iter().take(12) {
                println!("    {n:>5}x {w}");
            }
            for (id, w) in c.wrong.iter().take(25) {
                println!("    {id}: {w}");
            }
        }
        for (cat, c) in &self.cats {
            if !c.not_modelled.is_empty() {
                let ex: Vec<&str> = c.not_modelled.iter().take(6).map(String::as_str).collect();
                println!(
                    "--- not modelled: {cat} ({}) e.g. {ex:?}",
                    c.not_modelled.len()
                );
            }
        }
    }
}

fn digits_out(s: &str) -> String {
    let mut out: String = s
        .chars()
        .map(|c| if c.is_ascii_digit() { '#' } else { c })
        .collect();
    out.truncate(90);
    out
}

// ----------------------------------------------------------------------------- raw entries

/// One resolved 5etools item entry.
pub struct RawItem(pub Map<String, Value>);

fn strip(code: &str) -> &str {
    code.split('|').next().unwrap_or(code)
}

impl RawItem {
    pub fn str(&self, k: &str) -> Option<&str> {
        self.0.get(k).and_then(Value::as_str)
    }
    pub fn flag(&self, k: &str) -> bool {
        self.0.get(k) == Some(&Value::Bool(true))
    }
    /// Integer field, also from strings such as `"+1"`.
    pub fn int(&self, k: &str) -> Option<i32> {
        match self.0.get(k)? {
            Value::Number(n) => n.as_f64().map(|f| f as i32),
            Value::String(s) => s.trim().trim_start_matches('+').parse().ok(),
            _ => None,
        }
    }
    pub fn num(&self, k: &str) -> Option<f64> {
        self.0.get(k).and_then(Value::as_f64)
    }
    pub fn code(&self) -> &str {
        self.str("type").map(strip).unwrap_or("")
    }
    pub fn list(&self, k: &str) -> Vec<Value> {
        self.0
            .get(k)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    }
    pub fn properties(&self) -> Vec<String> {
        self.raw_properties()
            .iter()
            .map(|s| strip(s).to_string())
            .collect()
    }
    /// Property codes with their book (`Evo|AU`); a `{"uid": ...}` object is its uid.
    pub fn raw_properties(&self) -> Vec<String> {
        self.list("property")
            .iter()
            .filter_map(|p| {
                p.as_str()
                    .or_else(|| p.get("uid").and_then(Value::as_str))
                    .map(str::to_string)
            })
            .collect()
    }
    pub fn mastery(&self) -> Option<String> {
        self.list("mastery")
            .first()
            .and_then(Value::as_str)
            .map(|s| strip(s).to_string())
    }
    pub fn attunes(&self) -> bool {
        match self.0.get("reqAttune") {
            Some(Value::Bool(b)) => *b,
            Some(Value::String(s)) => !s.trim().is_empty(),
            _ => false,
        }
    }
    /// `Some(label)` when the attunement tags name only classes other than fighter and/or
    /// spellcasting (which a level-5 fighter without subclass is not); `None` otherwise.
    pub fn fighter_cannot_attune(&self) -> Option<&'static str> {
        let tags = self.list("reqAttuneTags");
        if tags.is_empty() {
            return None;
        }
        let mut caster = false;
        for t in &tags {
            let o = t.as_object()?;
            if o.len() != 1 {
                return None;
            }
            match (
                o.get("class").and_then(Value::as_str),
                o.get("spellcasting"),
            ) {
                (Some(c), _) if !c.eq_ignore_ascii_case("fighter") => {}
                (None, Some(Value::Bool(true))) => caster = true,
                _ => return None,
            }
        }
        Some(if caster {
            "by a spellcaster"
        } else {
            "by a class"
        })
    }
    /// Quantities of the item entries of `packContents` (special entries left out).
    pub fn pack_quantities(&self) -> Vec<u32> {
        self.list("packContents")
            .iter()
            .filter_map(|c| match c {
                Value::String(_) => Some(1),
                Value::Object(o) if o.contains_key("item") => Some(
                    o.get("quantity")
                        .and_then(Value::as_u64)
                        .and_then(|q| u32::try_from(q).ok())
                        .unwrap_or(1),
                ),
                _ => None,
            })
            .collect()
    }
}

/// Raw entries by the id of the element they became.
pub struct RawIndex {
    pub by_id: HashMap<String, RawItem>,
    pub unmatched: Vec<String>,
    pub unmatched_by_source: Counts,
    pub ambiguous: usize,
    /// Generic magic variants (`+1 Weapon`) by element id: `weapon`, `armor`, `shield` or
    /// `other`.
    pub generic: Vec<(String, String)>,
}

fn variant_kind(variant: &Value) -> &'static str {
    let req = variant
        .get("requires")
        .map(Value::to_string)
        .unwrap_or_default();
    if req.contains("\"type\":\"S") && !req.contains("\"armor\"") {
        "shield"
    } else if req.contains("\"armor\":true")
        || req.contains("\"type\":\"LA")
        || req.contains("\"type\":\"MA")
        || req.contains("\"type\":\"HA")
    {
        "armor"
    } else if req.contains("\"weapon\":true") || req.contains("weaponCategory") {
        "weapon"
    } else {
        "other"
    }
}

/// Read and resolve the folder as the importer does, then match each entry to its element by
/// source prefix and name.
pub fn raw_index(data: &Path, ours: &HashMap<String, (Element, Ruleset)>) -> RawIndex {
    let loaded = folder::load(data, |_, _, _| {}).expect("load folder");
    let resolve = |prop: &str| -> Vec<Value> {
        let raw: Vec<Value> = loaded
            .props
            .get(prop)
            .map(|v| v.iter().map(|e| e.value.clone()).collect())
            .unwrap_or_default();
        copy::resolve_array(prop, &raw).entries
    };
    let mut entries = resolve("item");
    let bases = resolve("baseitem");
    let variants = resolve("magicvariant");
    entries.extend(bases.iter().cloned());
    entries.extend(generate_variant_items(&variants, &bases));
    let generic = generic_variant_items(&variants);
    let mut generic_kind: HashMap<(String, String), &'static str> = HashMap::new();
    for v in &variants {
        let name = v.get("name").and_then(Value::as_str).unwrap_or("");
        let source = v
            .pointer("/inherits/source")
            .and_then(Value::as_str)
            .unwrap_or("");
        generic_kind.insert((name.into(), source.into()), variant_kind(v));
    }
    let n_plain = entries.len();
    entries.extend(generic);

    let mut by_name: HashMap<&str, Vec<&str>> = HashMap::new();
    for (id, (e, _)) in ours {
        if ITEM_TYPES.contains(&e.ty.as_str()) && e.setter_value("category") != Some("Item Groups")
        {
            by_name
                .entry(e.name.as_str())
                .or_default()
                .push(id.as_str());
        }
    }
    let mut out = RawIndex {
        by_id: HashMap::new(),
        unmatched: Vec::new(),
        unmatched_by_source: Counts::new(),
        ambiguous: 0,
        generic: Vec::new(),
    };
    let mut taken = HashSet::new();
    for (i, v) in entries.into_iter().enumerate() {
        let Value::Object(m) = v else { continue };
        let name = m
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let Some(source) = m.get("source").and_then(Value::as_str).map(str::to_string) else {
            continue;
        };
        let prefix = format!("ID_5ET_{}_ITEM_", id_fragment(&source));
        let hits: Vec<&str> = by_name
            .get(name.as_str())
            .map(|v| {
                v.iter()
                    .copied()
                    .filter(|id| id.starts_with(&prefix))
                    .collect()
            })
            .unwrap_or_default();
        match hits.as_slice() {
            [] => {
                out.unmatched.push(format!("{name}|{source}"));
                bump(&mut out.unmatched_by_source, source);
            }
            [id] if taken.insert((*id).to_string()) => {
                if i >= n_plain {
                    let kind = generic_kind
                        .get(&(name.clone(), source.clone()))
                        .copied()
                        .unwrap_or("other");
                    out.generic.push(((*id).to_string(), kind.to_string()));
                }
                out.by_id.insert((*id).to_string(), RawItem(m));
            }
            _ => out.ambiguous += 1,
        }
    }
    out.generic.sort();
    out
}

// ------------------------------------------------------------------------- test characters

/// A settled test character.
pub struct Sub {
    pub state: CharacterState,
    pub snap: CharacterSnapshot,
}

/// The level-5 human fighter of one edition, its weak twin and its longbow twin.
pub struct Base {
    pub state: CharacterState,
    pub snap: CharacterSnapshot,
    /// Settle-invariant violations of the bare character (not blamed on items).
    pub violations: HashSet<String>,
    pub mods: [i32; 6],
    pub scores: [i32; 6],
    pub weak: Sub,
    pub with_bow: Sub,
}

pub fn pick(engine: &Engine, state: &CharacterState, target: &str) -> Option<CharacterState> {
    let snap = engine.derive(state);
    let slot = snap
        .slots
        .iter()
        .find(|s| s.chosen.is_none() && s.options.iter().any(|o| o.id == target))?;
    commands::select(engine, state, &slot.key, Choice::Element(target.into()))
        .ok()
        .map(|o| o.state)
}

pub fn with_class(engine: &Engine, rs: Ruleset, class: &str, scores: [i32; 6]) -> CharacterState {
    let state = commands::new_character(engine, rs);
    let state = commands::set_ability_method(&state, AbilityMethod::Manual);
    let state = commands::set_base_scores(engine, &state, scores)
        .expect("scores")
        .state;
    let snap = engine.derive(&state);
    let key = slot_key(&snap, "ID_LEVEL_1", "Class", 1).expect("class slot");
    commands::select(engine, &state, &key, Choice::Element(class.into()))
        .expect("class")
        .state
}

fn fighter(engine: &Engine, rs: Ruleset, scores: [i32; 6]) -> CharacterState {
    let mut state = with_class(engine, rs, ids::fighter(rs), scores);
    state = pick(engine, &state, ids::human(rs)).expect("human offered");
    for _ in 1..LEVEL {
        state = commands::level_up_main(engine, &state, None)
            .expect("level up")
            .state;
    }
    state
}

impl Base {
    pub fn new(engine: &Engine, rs: Ruleset) -> Self {
        for id in [
            ids::longsword(rs),
            ids::longbow(rs),
            ids::breastplate(rs),
            ids::shield(rs),
        ] {
            assert!(engine.has_element(rs, id), "{id} missing");
        }
        let state = fighter(engine, rs, SCORES);
        let weak_state = fighter(engine, rs, WEAK_SCORES);
        Self::from_states(engine, rs, state, weak_state)
    }

    /// A test character from its state and its weak (Str 8) twin.
    pub fn from_states(
        engine: &Engine,
        rs: Ruleset,
        state: CharacterState,
        weak_state: CharacterState,
    ) -> Self {
        let snap = engine.derive(&state);
        assert_eq!(snap.level, LEVEL);
        let found = invariants::check(engine, &state, &snap);
        let mut kinds = Counts::new();
        for v in &found {
            bump(&mut kinds, v.kind);
        }
        println!("    baseline invariant violations {rs:?} by kind: {kinds:?}");
        let violations = found.iter().map(ToString::to_string).collect();
        let mut mods = [0; 6];
        let mut scores = [0; 6];
        for i in 0..6 {
            mods[i] = snap.abilities[i].modifier;
            scores[i] = snap.abilities[i].score;
        }
        let weak = Sub {
            snap: engine.derive(&weak_state),
            state: weak_state,
        };
        let bow_state = equip_state(engine, &state, ids::longbow(rs), None).expect("longbow");
        let with_bow = Sub {
            snap: engine.derive(&bow_state),
            state: bow_state,
        };
        for s in snap.slots.iter().filter(|s| s.chosen.is_none()) {
            let imported = s
                .options
                .iter()
                .filter(|o| o.id.starts_with("ID_5ET_"))
                .count();
            println!(
                "    open slot {rs:?} {:?} type {:?}: {} options, {imported} imported",
                s.name,
                s.ty,
                s.options.len()
            );
        }
        println!(
            "test character {rs:?}: level {} AC {} speed {} scores {scores:?} mods {mods:?} prof {} ({} baseline invariant violations)",
            snap.level,
            snap.armor_class,
            snap.speed.walk,
            snap.proficiency_bonus,
            violations_len(&violations)
        );
        Self {
            state,
            snap,
            violations,
            mods,
            scores,
            weak,
            with_bow,
        }
    }
}

fn violations_len(v: &HashSet<String>) -> usize {
    v.len()
}

pub fn equip_state(
    engine: &Engine,
    state: &CharacterState,
    item: &str,
    adorner: Option<&str>,
) -> Option<CharacterState> {
    let o = commands::add_item(engine, state, item, 1).ok()?;
    let uid = o.state.inventory.items.last()?.uid.clone();
    let mut s = o.state;
    if let Some(a) = adorner {
        s = commands::set_adorner(engine, &s, &uid, Some(a)).ok()?.state;
    }
    Some(commands::equip(engine, &s, &uid, None).ok()?.state)
}

/// `item` (optionally adorned) equipped on `state`, settled.
pub fn equipped(
    engine: &Engine,
    state: &CharacterState,
    item: &str,
    adorner: Option<&str>,
) -> Option<CharacterSnapshot> {
    equip_state(engine, state, item, adorner).map(|s| engine.derive(&s))
}

/// A level-1 character with the class `el`, or a level-1 fighter with the background `el`.
pub fn level_one_with(engine: &Engine, rs: Ruleset, el: &Element) -> Option<CharacterSnapshot> {
    let state = if el.ty == "Class" {
        with_class(engine, rs, &el.id, SCORES)
    } else {
        let f = with_class(engine, rs, ids::fighter(rs), SCORES);
        pick(engine, &f, &el.id)?
    };
    Some(engine.derive(&state))
}

// -------------------------------------------------------------------------------- formulas

pub fn damage_type(code: &str) -> Option<&'static str> {
    Some(match code {
        "B" => "bludgeoning",
        "P" => "piercing",
        "S" => "slashing",
        "A" => "acid",
        "C" => "cold",
        "F" => "fire",
        "O" => "force",
        "L" => "lightning",
        "N" => "necrotic",
        "I" => "poison",
        "Y" => "psychic",
        "R" => "radiant",
        "T" => "thunder",
        _ => return None,
    })
}

pub fn dice_with_bonus(dice: &str, bonus: i32, ty: &str) -> String {
    let core = match bonus {
        0 => dice.to_string(),
        b if b < 0 => format!("{dice}-{}", -b),
        b => format!("{dice}+{b}"),
    };
    if ty.is_empty() {
        core
    } else {
        format!("{core} {ty}")
    }
}

/// The core weapon-property tag of a 5etools property code.
/// The supports tag a weapon property code (`F`, or `Evo|AU` with its book) must give: a core
/// property tag, or the Weapon Property element the import makes of a book's own property
/// (`ID_5ET_AU_ITEMPROP_EVO`).
pub fn property_tag(raw: &str) -> Option<String> {
    let (code, source) = raw.split_once('|').unwrap_or((raw, ""));
    property_id(code).map(str::to_string).or_else(|| {
        let frag = |s: &str| {
            s.trim()
                .to_uppercase()
                .replace(|c: char| !c.is_ascii_alphanumeric(), "_")
        };
        (!source.trim().is_empty())
            .then(|| format!("ID_5ET_{}_ITEMPROP_{}", frag(source), frag(code)))
    })
}

pub fn property_id(code: &str) -> Option<&'static str> {
    Some(match code {
        "A" => "ID_INTERNAL_WEAPON_PROPERTY_AMMUNITION",
        "F" => "ID_INTERNAL_WEAPON_PROPERTY_FINESSE",
        "H" => "ID_INTERNAL_WEAPON_PROPERTY_HEAVY",
        "L" => "ID_INTERNAL_WEAPON_PROPERTY_LIGHT",
        "LD" => "ID_INTERNAL_WEAPON_PROPERTY_LOADING",
        "R" => "ID_INTERNAL_WEAPON_PROPERTY_REACH",
        "T" => "ID_INTERNAL_WEAPON_PROPERTY_THROWN",
        "2H" => "ID_INTERNAL_WEAPON_PROPERTY_TWOHANDED",
        "V" => "ID_INTERNAL_WEAPON_PROPERTY_VERSATILE",
        "S" => "ID_INTERNAL_WEAPON_PROPERTY_SPECIAL",
        "RLD" => "ID_INTERNAL_WEAPON_PROPERTY_FIREARM_RELOAD",
        "BF" => "ID_INTERNAL_WEAPON_PROPERTY_FIREARM_BURST_FIRE",
        "AF" => "ID_INTERNAL_WEAPON_PROPERTY_FIREARM_AMMUNITION",
        _ => return None,
    })
}

pub fn ability_index(abbr: &str) -> Option<usize> {
    ["str", "dex", "con", "int", "wis", "cha"]
        .iter()
        .position(|a| *a == abbr)
}

// ------------------------------------------------------------------------------ books table

fn note_source(element: &str, frags: &[(String, String)]) -> Option<String> {
    if let Some((_, src)) = element.rsplit_once('|') {
        return Some(src.to_uppercase());
    }
    source_of_id(element, frags)
}

/// Source of an `ID_5ET_<SRC>_...` id (longest matching source fragment).
fn source_of_id(id: &str, frags: &[(String, String)]) -> Option<String> {
    frags
        .iter()
        .find(|(frag, _)| id.starts_with(&format!("ID_5ET_{frag}_")))
        .map(|(_, src)| src.to_uppercase())
}

/// Per source: elements by kind, skipped and partial entries, item checks that failed.
pub fn books_table(
    report: &ImportReport,
    ours: &HashMap<String, (Element, Ruleset)>,
    tally: &Tally,
) {
    let mut frags: Vec<(String, String)> = report
        .packs
        .iter()
        .map(|p| (id_fragment(&p.source), p.source.clone()))
        .collect();
    frags.sort_by_key(|(f, _)| std::cmp::Reverse(f.len()));
    let mut skipped = Counts::new();
    let mut partial = Counts::new();
    for n in &report.skipped {
        bump(
            &mut skipped,
            note_source(&n.element, &frags).unwrap_or("?".into()),
        );
    }
    for n in &report.partial {
        bump(
            &mut partial,
            note_source(&n.element, &frags).unwrap_or("?".into()),
        );
    }
    let mut wrong = Counts::new();
    let mut unmodelled = Counts::new();
    for (_, id, is_wrong) in tally.problems() {
        let src = source_of_id(id, &frags).unwrap_or("?".into());
        bump(
            if is_wrong {
                &mut wrong
            } else {
                &mut unmodelled
            },
            src,
        );
    }
    let mut items = Counts::new();
    for (e, _) in ours.values() {
        if ITEM_TYPES.contains(&e.ty.as_str()) {
            if let Some(src) = source_of_id(&e.id, &frags) {
                bump(&mut items, src);
            }
        }
    }
    println!("--- books: source ruleset elements items skipped partial item-checks-wrong item-checks-not-modelled | elements by kind");
    let mut problems = Vec::new();
    for p in &report.packs {
        let src = p.source.to_uppercase();
        let total: usize = p.elements.values().sum();
        let g = |m: &Counts| m.get(&src).copied().unwrap_or(0);
        let kinds: Vec<String> = p.elements.iter().map(|(k, n)| format!("{k} {n}")).collect();
        println!(
            "{:<14} {:<8} {total:>5} {:>5} {:>4} {:>4} {:>4} {:>4} | {}",
            p.source,
            p.ruleset,
            g(&items),
            g(&skipped),
            g(&partial),
            g(&wrong),
            g(&unmodelled),
            kinds.join(", ")
        );
        if g(&skipped) + g(&partial) + g(&wrong) > 0 {
            problems.push(format!(
                "{} (skipped {}, partial {}, item checks wrong {})",
                p.source,
                g(&skipped),
                g(&partial),
                g(&wrong)
            ));
        }
    }
    let mut by_kind = Counts::new();
    for n in &report.skipped {
        bump(&mut by_kind, format!("skipped {}", n.kind));
    }
    for n in &report.partial {
        bump(&mut by_kind, format!("partial {}", n.kind));
    }
    println!("import notes by kind: {by_kind:?}");
    let orphan = skipped.get("?").copied().unwrap_or(0) + partial.get("?").copied().unwrap_or(0);
    println!("notes without a source: {orphan}");
    println!(
        "sources with problems ({}): {}",
        problems.len(),
        problems.join("; ")
    );
}
