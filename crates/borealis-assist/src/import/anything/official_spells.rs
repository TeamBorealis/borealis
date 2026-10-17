//! Official spells the import refers to that neither the import nor the installed content has
//! (a subclass table printing a spell of another official book, a model-written grant with a
//! guessed official id). Every spell target of a grant is resolved against the library and
//! the import at derive time (so validation, self-checks and finalize all see the same pack):
//!
//! * a target whose id is unknown but whose name is a known spell is pointed at that spell;
//! * otherwise, when the spell's level is known from the source (a cantrip grant, a cantrip the
//!   text names, a table row whose other spells fix the row's spell level), the grant points at
//!   a placeholder Spell that keeps only the printed name, marked as official content to
//!   install (no rules text, no stats: they are not in the source);
//! * when the level is not known the grant is removed (an unlevelled placeholder would show
//!   on the sheet as a cantrip) and reported.
//!
//! A cantrip an element's own text gives ("you learn the X cantrip", "you know the X and Y
//! cantrips") that no rule of it or of its family grants is granted too, the same way (a known
//! spell, else a placeholder), when the name is certain: a spell the library or the import has,
//! or a name printed as one (emphasised, title case, or listed next to a known spell). Any other
//! name is left ungranted with a note ([`NOT_GRANTED`]).
//!
//! Each case leaves a note starting with [`UNRESOLVED_OFFICIAL`] on the referring element, so
//! the report and the review can show it; no grant is silently dropped or left dangling.

use std::collections::{BTreeSet, HashMap, HashSet};

use borealis_content::ContentStore;
use serde_json::{json, Value};

use super::super::derived::Derived;
use super::super::fewshot::in_edition;
use super::super::types::{ImportPlan, PlannedElement, Role, SpellGrant};
use super::text::{candidates, key, named_cantrips, strip_join};
use super::{grant_rule, CANTRIP};
use crate::import::ids::{make_id, slug};

/// Start of the note on an element that refers to an official spell nobody has.
pub(crate) const UNRESOLVED_OFFICIAL: &str = "unresolved official reference: ";
/// Words of the [`UNRESOLVED_OFFICIAL`] note on an element whose text names a cantrip it gives
/// that no rule grants and that is not certain to be a spell name (it is not granted).
pub(crate) const NOT_GRANTED: &str = "no rule grants it";
/// `source` of a placeholder spell.
pub(crate) const PLACEHOLDER_SOURCE: &str = "Official content (not installed)";
/// The id part before a spell's name in a guessed official id (`ID_PHB_SPELL_ICE_KNIFE`).
const SPELL_MARK: &str = "_SPELL_";
/// Id prefix of a placeholder when the referring element's id shows none.
const FALLBACK_PREFIX: &str = "IMPORT";
/// Words kept lower case inside a title-cased spell name.
const SMALL_WORDS: [&str; 12] = [
    "a", "an", "and", "at", "by", "for", "from", "in", "of", "on", "or", "the",
];
/// Setters a Spell needs to load; a placeholder leaves all but the level empty (not printed).
const EMPTY_SETTERS: [&str; 4] = ["school", "time", "duration", "range"];

fn str_at<'a>(v: &'a Value, k: &str) -> &'a str {
    v.get(k).and_then(Value::as_str).unwrap_or("")
}

/// The field of rule `r` that names a spell element, if it is a spell grant.
fn target_field(r: &Value) -> Option<&'static str> {
    match str_at(r, "kind") {
        "grant" if str_at(r, "type") == "Spell" => Some("id"),
        "grant_spell" | "spell_cast_alt_cost" => Some("spell"),
        _ => None,
    }
}

/// The spell name a guessed id carries (`ID_PHB_SPELL_ICE_KNIFE` -> "ice knife").
///
/// A target written as the spell's name instead of an id ("Fire Bolt") is that name.
fn name_from_id(id: &str) -> Option<String> {
    let id = id.trim();
    if !id.starts_with("ID_") && id.chars().any(char::is_alphabetic) {
        return Some(id.to_lowercase());
    }
    let (_, tail) = id.rsplit_once(SPELL_MARK)?;
    let words: Vec<&str> = tail.split('_').filter(|w| !w.is_empty()).collect();
    (!words.is_empty()).then(|| words.join(" ").to_lowercase())
}

/// A printed name in title case ("ice knife" -> "Ice Knife").
fn title(name: &str) -> String {
    name.split_whitespace()
        .enumerate()
        .map(|(i, w)| {
            let lower = w.to_lowercase();
            if i > 0 && SMALL_WORDS.contains(&lower.as_str()) {
                return lower;
            }
            let mut c = lower.chars();
            c.next()
                .map(|f| f.to_uppercase().chain(c).collect())
                .unwrap_or_default()
        })
        .collect::<Vec<String>>()
        .join(" ")
}

fn spell_level(el: &Value) -> Option<u32> {
    el.get("setters")?
        .as_array()?
        .iter()
        .find(|s| str_at(s, "name") == "level")
        .and_then(|s| str_at(s, "value").trim().parse().ok())
}

/// Spells the pack can point at: the library's of the edition, the import's own and the
/// placeholders made so far, by name key and by id.
struct Known<'a> {
    store: &'a ContentStore,
    by_key: HashMap<String, (String, Option<u32>)>,
    ids: HashMap<String, (String, Option<u32>)>,
    /// Every element id of the import (any type).
    own: HashSet<String>,
}

impl<'a> Known<'a> {
    fn new(plan: &ImportPlan, store: &'a ContentStore, d: &Derived) -> Self {
        let mut k = Self {
            store,
            by_key: HashMap::new(),
            ids: HashMap::new(),
            own: HashSet::new(),
        };
        for e in store
            .by_type("Spell")
            .map(|s| &s.element)
            .filter(|e| in_edition(e, plan.edition))
        {
            let level = e.setter_value("level").and_then(|v| v.trim().parse().ok());
            k.add(&e.name, &e.id, level);
        }
        for doc in d.docs.iter().chain(&d.extra) {
            k.own.insert(str_at(doc, "id").to_string());
            if str_at(doc, "type") == "Spell" {
                k.add(str_at(doc, "name"), str_at(doc, "id"), spell_level(doc));
            }
        }
        k
    }

    fn add(&mut self, name: &str, id: &str, level: Option<u32>) {
        self.by_key.insert(key(name), (id.to_string(), level));
        self.ids.insert(id.to_string(), (key(name), level));
    }

    /// True when `id` is an element of the library or the import.
    fn has(&self, id: &str) -> bool {
        self.own.contains(id) || self.ids.contains_key(id) || self.store.contains_id(id)
    }

    /// The known spell `name` names; a printed plural ("produce flames") also finds the
    /// singular when only that is known.
    fn find(&self, name: &str) -> Option<&(String, Option<u32>)> {
        let k = key(name);
        if k.is_empty() {
            return None;
        }
        self.by_key
            .get(&k)
            .or_else(|| k.strip_suffix('s').and_then(|s| self.by_key.get(s)))
    }

    fn level_of(&self, id: &str) -> Option<u32> {
        self.ids.get(id).and_then(|(_, l)| *l).or_else(|| {
            self.store
                .get(id)
                .and_then(|s| s.element.setter_value("level"))
                .and_then(|v| v.trim().parse().ok())
        })
    }

    /// The name key of spell `id` (a guessed id: the name it carries).
    fn key_of(&self, id: &str) -> String {
        self.ids
            .get(id)
            .map(|(k, _)| k.clone())
            .or_else(|| name_from_id(id).map(|n| key(&n)))
            .unwrap_or_default()
    }
}

/// The spell level of `g`, a grant of `p`'s spell table the plan could not resolve, as the
/// table fixes it: a cantrip grant; the level of another spell of the same row; or the row's
/// rank when every row with a known spell has the spell level of its rank (subclass tables
/// give one row per spell level, 1st upwards).
fn table_level(p: &PlannedElement, g: &SpellGrant, known: &Known) -> Option<u32> {
    if g.level == CANTRIP {
        return Some(0);
    }
    let row_level = |level: u32| {
        p.spells
            .iter()
            .filter(|s| s.level == level)
            .filter_map(|s| s.id.as_deref().and_then(|id| known.level_of(id)))
            .next()
    };
    if let Some(l) = row_level(g.level) {
        return Some(l);
    }
    let rows: Vec<u32> = p
        .spells
        .iter()
        .map(|s| s.level)
        .filter(|l| *l != CANTRIP)
        .collect::<BTreeSet<u32>>()
        .into_iter()
        .collect();
    let rank = |level: u32| rows.iter().position(|l| *l == level).map(|i| i as u32 + 1);
    let checked: Vec<bool> = rows
        .iter()
        .filter_map(|r| row_level(*r).map(|l| Some(l) == rank(*r)))
        .collect();
    (!checked.is_empty() && checked.iter().all(|ok| *ok))
        .then(|| rank(g.level))
        .flatten()
}

/// 0 when a sentence of `doc`'s text names `name` as a cantrip ("you learn the a and b
/// cantrips").
fn cantrip_in_text(doc: &Value, name: &str) -> Option<u32> {
    let text = super::super::fidelity::plain(str_at(doc, "description")).to_lowercase();
    let name = name.to_lowercase();
    text.split(['.', ';', '\n'])
        .any(|s| s.contains(&name) && s.contains("cantrip"))
        .then_some(0)
}

/// The import's id prefix as `doc`'s own id shows it (`ID_<prefix>_<TYPE>_...`).
fn prefix_of(doc: &Value) -> String {
    let ty = format!("_{}_", slug(str_at(doc, "type")));
    str_at(doc, "id")
        .strip_prefix("ID_")
        .and_then(|rest| rest.split_once(&ty))
        .map_or_else(|| FALLBACK_PREFIX.to_string(), |(p, _)| p.to_string())
}

/// One placeholder spell: its id, name, level and the elements that refer to it.
struct Placeholder {
    id: String,
    name: String,
    level: u32,
    owners: Vec<String>,
}

#[derive(Default)]
struct Made {
    by_key: HashMap<String, Placeholder>,
}

impl Made {
    /// The placeholder for `name` (made on first use) referred to by `owner`.
    fn get(&mut self, name: &str, level: u32, owner: &Value, known: &Known) -> String {
        let owner_name = str_at(owner, "name").to_string();
        if let Some(p) = self.by_key.get_mut(&key(name)) {
            if !p.owners.contains(&owner_name) {
                p.owners.push(owner_name);
            }
            return p.id.clone();
        }
        let shown = title(name);
        let mut used: HashSet<String> = known.own.clone();
        used.extend(self.by_key.values().map(|p| p.id.clone()));
        let prefix = prefix_of(owner);
        let mut id = make_id(&prefix, "Spell", &shown, &used);
        while known.has(&id) {
            used.insert(id.clone());
            id = make_id(&prefix, "Spell", &shown, &used);
        }
        self.by_key.insert(
            key(name),
            Placeholder {
                id: id.clone(),
                name: shown,
                level,
                owners: vec![owner_name],
            },
        );
        id
    }

    fn elements(self) -> Vec<Value> {
        let mut all: Vec<Placeholder> = self.by_key.into_values().collect();
        all.sort_by(|a, b| a.id.cmp(&b.id));
        all.into_iter().map(placeholder).collect()
    }
}

/// A placeholder Spell: the printed name, the level the source fixes, a note naming where the
/// import refers to it; no rules text and no stats (the source prints none).
fn placeholder(p: Placeholder) -> Value {
    let mut setters = vec![json!({"name": "level", "value": p.level.to_string()})];
    setters.extend(
        EMPTY_SETTERS
            .iter()
            .map(|n| json!({"name": n, "value": ""})),
    );
    json!({
        "id": p.id,
        "name": p.name,
        "type": "Spell",
        "source": PLACEHOLDER_SOURCE,
        "description": format!(
            "<p>An official spell this import refers to ({}). Its rules are not in this source and \
    the official content that prints them is not installed: install or import that content for them.</p>",
            p.owners.join(", ")
        ),
        "setters": setters,
    })
}

fn rules_mut(doc: &mut Value) -> Option<&mut Vec<Value>> {
    let o = doc.as_object_mut()?;
    let rules = o.entry("rules").or_insert_with(|| json!([]));
    if !rules.is_array() {
        *rules = json!([]);
    }
    rules.as_array_mut()
}

fn placeholder_note(name: &str, id: &str) -> String {
    format!(
        "{UNRESOLVED_OFFICIAL}\"{}\" is an official spell in neither this import nor the installed content: granted as a placeholder that keeps only its name ({id})",
        title(name)
    )
}

fn dropped_note(name: &str) -> String {
    format!(
        "{UNRESOLVED_OFFICIAL}\"{}\" is an official spell in neither this import nor the installed content and the source gives no spell level for it: not granted, only named in the text",
        title(name)
    )
}

/// Grants of `p`'s spell table the plan could not resolve, added to its element `doc`.
fn table_grants(
    p: &PlannedElement,
    doc: &mut Value,
    plan: &ImportPlan,
    known: &Known,
    made: &mut Made,
    notes: &mut Vec<String>,
) {
    for g in p.spells.iter().filter(|g| g.id.is_none()) {
        let granted = doc
            .get("rules")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|r| r.get(target_field(r)?).and_then(Value::as_str))
            .any(|id| known.key_of(id) == key(&g.name));
        if granted {
            continue;
        }
        let mut shaped = g.clone();
        let target = match known.find(&g.name) {
            Some((id, _)) => id.clone(),
            None => match table_level(p, g, known).or_else(|| cantrip_in_text(doc, &g.name)) {
                Some(level) => {
                    if level == 0 {
                        // A cantrip is known from the feature on (no class level, not prepared).
                        shaped.level = CANTRIP;
                    }
                    let id = made.get(&g.name, level, doc, known);
                    notes.push(placeholder_note(&g.name, &id));
                    id
                }
                None => {
                    notes.push(dropped_note(&g.name));
                    continue;
                }
            },
        };
        if let Some(rules) = rules_mut(doc) {
            rules.push(grant_rule(plan.edition, &shaped, &target));
        }
    }
}

/// Fewest letters of a name read off the end of an id.
const MIN_SUFFIX_NAME: usize = 3;

/// The cantrip of `named` (the ones the element's text gives) whose name ends the id `id` of an
/// unknown shape ("ID_XYZ_EMBER_WHISTLE" for "ember whistle").
fn named_suffix(id: &str, named: &[String]) -> Option<String> {
    let k = key(id);
    named
        .iter()
        .filter(|n| key(n).len() >= MIN_SUFFIX_NAME && k.ends_with(&key(n)))
        .max_by_key(|n| key(n).len())
        .map(|n| n.to_lowercase())
}

/// The level a model-written grant `r` of `doc` fixes for an unknown spell `name`.
fn grant_level(
    r: &Value,
    doc: &Value,
    name: &str,
    planned: Option<&PlannedElement>,
    known: &Known,
) -> Option<u32> {
    if str_at(r, "mode") == "known_cantrip" {
        return Some(0);
    }
    if let Some(l) = cantrip_in_text(doc, name) {
        return Some(l);
    }
    let p = planned?;
    let g = p.spells.iter().find(|g| key(&g.name) == key(name))?;
    table_level(p, g, known)
}

/// Every spell target of `doc`'s rules resolved: by name to a known spell, else to a
/// placeholder, else the rule is removed. Returns the notes.
fn resolve_targets(
    doc: &mut Value,
    planned: Option<&PlannedElement>,
    known: &Known,
    made: &mut Made,
) -> Vec<String> {
    let mut notes = Vec::new();
    let snapshot = doc.clone();
    let named: Vec<String> = named_in(&snapshot, known)
        .into_iter()
        .map(|n| n.name)
        .collect();
    for path in ["/rules", "/multiclass/rules"] {
        let Some(rules) = doc.pointer_mut(path).and_then(Value::as_array_mut) else {
            continue;
        };
        let mut kept = Vec::with_capacity(rules.len());
        for mut r in rules.drain(..) {
            let Some(field) = target_field(&r) else {
                kept.push(r);
                continue;
            };
            let id = str_at(&r, field).to_string();
            let placeheld = made.by_key.values().any(|p| p.id == id);
            let Some(name) = (!known.has(&id) && !placeheld)
                .then(|| name_from_id(&id).or_else(|| named_suffix(&id, &named)))
                .flatten()
            else {
                kept.push(r);
                continue;
            };
            let target = match known.find(&name) {
                Some((found, _)) => {
                    notes.push(format!(
                        "spell grant {id} names no element; it is the spell \"{}\" ({found})",
                        title(&name)
                    ));
                    found.clone()
                }
                None => match grant_level(&r, &snapshot, &name, planned, known) {
                    Some(level) => {
                        let pid = made.get(&name, level, &snapshot, known);
                        notes.push(placeholder_note(&name, &pid));
                        pid
                    }
                    None => {
                        notes.push(dropped_note(&name));
                        continue;
                    }
                },
            };
            r[field] = json!(target);
            kept.push(r);
        }
        *rules = kept;
    }
    notes
}

/// Resolve every spell grant of the import (see the module docs); placeholders go to
/// `d.extra`, one note per reference to `d.notes`.
pub(crate) fn official_spells(plan: &ImportPlan, store: &ContentStore, d: &mut Derived) {
    let known = Known::new(plan, store, d);
    let mut made = Made::default();
    let mut notes: Vec<(String, String)> = Vec::new();
    for doc in &mut d.docs {
        let id = str_at(doc, "id").to_string();
        let planned = plan.elements.iter().find(|p| p.id == id);
        let mut own = Vec::new();
        if let Some(p) = planned.filter(|p| p.spells.iter().any(|g| g.id.is_none())) {
            table_grants(p, doc, plan, &known, &mut made, &mut own);
        }
        own.extend(resolve_targets(doc, planned, &known, &mut made));
        notes.extend(own.into_iter().map(|m| (id.clone(), m)));
    }
    notes.extend(text_grants(plan, &known, &mut made, &mut d.docs));
    d.extra.extend(made.elements());
    d.notes.extend(notes);
}

// ---------------------------------------------------------------------------------------------
// Cantrips an element's own text gives

/// Element types whose own text can give a cantrip (traits, features, feats, items, races).
const GIVING_TYPES: [&str; 12] = [
    "Race",
    "Sub Race",
    "Racial Trait",
    "Class Feature",
    "Archetype Feature",
    "Feat",
    "Feat Feature",
    "Background",
    "Background Feature",
    "Magic Item",
    "Item",
    "Weapon",
];
/// Most words of a spell name read from a sentence.
const MAX_NAME_WORDS: usize = 5;
/// Words that make a "the ... cantrip" phrase no spell name ("the chosen cantrip").
const NOT_NAME_WORDS: [&str; 26] = [
    "cantrip",
    "cantrips",
    "following",
    "chosen",
    "same",
    "selected",
    "listed",
    "above",
    "below",
    "new",
    "other",
    "one",
    "two",
    "three",
    "that",
    "this",
    "these",
    "those",
    "your",
    "its",
    "their",
    "any",
    "each",
    "bonus",
    "extra",
    "additional",
];
/// Most grant links followed from an element to its family (a cycle never loops).
const MAX_FAMILY_DEPTH: usize = 4;

/// A name printed in the "the ... cantrip" phrase of `text`, as a spell: marks, a parenthetical
/// note and a glued footnote marker left out.
fn printed_name(piece: &str) -> String {
    let all = candidates(piece);
    let base = all
        .get(1)
        .filter(|_| piece.contains(" ("))
        .unwrap_or(&all[0]);
    candidates(base).last().cloned().unwrap_or_default()
}

/// `name` reads as a spell's name: one to [`MAX_NAME_WORDS`] words of letters (an apostrophe
/// or hyphen inside), none of them a word of [`NOT_NAME_WORDS`].
fn name_like(name: &str) -> bool {
    let words: Vec<String> = name.split_whitespace().map(str::to_lowercase).collect();
    !words.is_empty()
        && words.len() <= MAX_NAME_WORDS
        && words.iter().all(|w| {
            w.chars()
                .all(|c| c.is_alphabetic() || c == '\'' || c == '’' || c == '-')
                && !NOT_NAME_WORDS.contains(&w.as_str())
        })
}

/// `name` is printed as a name in the element's text: emphasised (`<em>`, `<i>`), or with every
/// word but a small one capitalised (as the list printed it).
fn printed_as_name(name: &str, html: &str) -> bool {
    let low = html.to_lowercase();
    let n = name.to_lowercase();
    if ["</em>", "</i>"]
        .iter()
        .any(|c| low.contains(&format!(">{n}{c}")))
    {
        return true;
    }
    name.split_whitespace().enumerate().all(|(i, w)| {
        w.chars().next().is_some_and(char::is_uppercase)
            || (i > 0 && SMALL_WORDS.contains(&w.to_lowercase().as_str()))
    })
}

/// The pieces of a printed list ("a, b, and c"): split on commas, semicolons and joins.
fn list_pieces(list: &str, known: &Known) -> Vec<String> {
    let mut out = Vec::new();
    for piece in list.split([',', ';']).map(strip_join) {
        let piece = piece.strip_prefix("the ").unwrap_or(piece).trim();
        if piece.is_empty() {
            continue;
        }
        if known.find(&printed_name(piece)).is_some() {
            out.push(piece.to_string());
            continue;
        }
        out.extend(
            piece
                .split(" and ")
                .flat_map(|x| x.split(" or "))
                .flat_map(|x| x.split(" & "))
                .map(|x| x.trim())
                .map(|x| x.strip_prefix("the ").unwrap_or(x).to_string())
                .filter(|x| !x.is_empty()),
        );
    }
    out
}

/// A cantrip `doc`'s text gives: its printed name, and whether that name is certain to be a
/// spell (known, or printed as a name, or listed next to a known spell).
struct Named {
    name: String,
    certain: bool,
}

/// The cantrips `doc`'s own text says it gives ("you learn the a and b cantrips").
fn named_in(doc: &Value, known: &Known) -> Vec<Named> {
    let html = str_at(doc, "description");
    let text = super::super::fidelity::plain(html);
    let lines: Vec<&str> = text.lines().collect();
    let mut out: Vec<Named> = Vec::new();
    for list in named_cantrips(&lines) {
        let names: Vec<String> = list_pieces(&list, known)
            .iter()
            .map(|p| printed_name(p))
            .filter(|n| name_like(n))
            .collect();
        let any_known = names.iter().any(|n| known.find(n).is_some());
        for name in names {
            if out.iter().any(|o| key(&o.name) == key(&name)) {
                continue;
            }
            let certain = known.find(&name).is_some() || any_known || printed_as_name(&name, html);
            out.push(Named { name, certain });
        }
    }
    out
}

/// The import's element ids `doc` grants (its traits, features, sub-features).
fn granted_ids<'a>(doc: &'a Value, own: &HashSet<String>) -> Vec<&'a str> {
    doc.get("rules")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|r| str_at(r, "kind") == "grant" && str_at(r, "type") != "Spell")
        .map(|r| str_at(r, "id"))
        .filter(|id| own.contains(*id))
        .collect()
}

/// The spell rules of `doc` (grants and alternative casts).
fn spell_rules(doc: &Value) -> impl Iterator<Item = &Value> {
    doc.get("rules")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|r| target_field(r).is_some())
}

/// `doc` grants spell `name` (by any of its spell rules).
fn grants(doc: &Value, name: &str, known: &Known, made: &Made) -> bool {
    let k = key(name);
    spell_rules(doc).any(|r| {
        let id = target_field(r).map_or("", |f| str_at(r, f));
        known.key_of(id) == k || made.by_key.get(&k).is_some_and(|p| p.id == id)
    })
}

/// Indices of the elements reached from `i` through grants (`down`), or granting it (up).
fn family(docs: &[Value], i: usize, own: &HashSet<String>, down: bool) -> Vec<usize> {
    let mut seen: Vec<usize> = vec![i];
    let mut frontier = vec![i];
    for _ in 0..MAX_FAMILY_DEPTH {
        let mut next = Vec::new();
        for &f in &frontier {
            let found: Vec<usize> = if down {
                let ids = granted_ids(&docs[f], own);
                (0..docs.len())
                    .filter(|&j| ids.contains(&str_at(&docs[j], "id")))
                    .collect()
            } else {
                let id = str_at(&docs[f], "id");
                (0..docs.len())
                    .filter(|&j| granted_ids(&docs[j], own).contains(&id))
                    .collect()
            };
            next.extend(found.into_iter().filter(|j| !seen.contains(j)));
        }
        next.sort_unstable();
        next.dedup();
        if next.is_empty() {
            break;
        }
        seen.extend(&next);
        frontier = next;
    }
    seen.remove(0);
    seen
}

/// The rule granting cantrip `id`, in the shape of a spell grant its family already has (same
/// kind, list, ability and spellcasting), else as a known cantrip (on the sheet in both
/// editions, whatever the element is).
fn cantrip_rule(id: &str, template: Option<&Value>) -> Value {
    match template {
        Some(t) if str_at(t, "kind") == "grant" => {
            let mut r = json!({"kind": "grant", "id": id, "type": "Spell"});
            if let Some(v) = t.get("spellcasting") {
                r["spellcasting"] = v.clone();
            }
            r
        }
        _ => {
            let mut r = json!({"kind": "grant_spell", "spell": id, "mode": "known_cantrip"});
            for f in ["spell_list", "ability"] {
                if let Some(v) = template.and_then(|t| t.get(f)) {
                    r[f] = v.clone();
                }
            }
            r
        }
    }
}

fn not_granted_note(name: &str) -> String {
    format!(
        "{UNRESOLVED_OFFICIAL}\"{}\" is named as a cantrip its text gives, but {NOT_GRANTED} and the name is not certain to be a spell: not granted, check it",
        title(name)
    )
}

/// Every cantrip an element's own text gives that neither it nor its family (the elements it
/// grants or that grant it) grants: granted when its name is certain, noted otherwise. Returns
/// the notes.
fn text_grants(
    plan: &ImportPlan,
    known: &Known,
    made: &mut Made,
    docs: &mut [Value],
) -> Vec<(String, String)> {
    let text_only: HashSet<&str> = plan
        .elements
        .iter()
        .filter(|p| p.role == Role::Text)
        .map(|p| p.id.as_str())
        .collect();
    let mut notes = Vec::new();
    for i in 0..docs.len() {
        let doc = &docs[i];
        let id = str_at(doc, "id").to_string();
        if !GIVING_TYPES.contains(&str_at(doc, "type")) || text_only.contains(id.as_str()) {
            continue;
        }
        let named = named_in(doc, known);
        if named.is_empty() {
            continue;
        }
        let up = family(docs, i, &known.own, false);
        let down = family(docs, i, &known.own, true);
        let mut add: Vec<Value> = Vec::new();
        for n in named {
            let granted = |j: usize| grants(&docs[j], &n.name, known, made);
            let elsewhere = |j: usize| {
                granted(j)
                    || named_in(&docs[j], known)
                        .iter()
                        .any(|m| key(&m.name) == key(&n.name))
            };
            if granted(i) || up.iter().any(|&j| granted(j)) || down.iter().any(|&j| elsewhere(j)) {
                continue;
            }
            if !n.certain {
                notes.push((id.clone(), not_granted_note(&n.name)));
                continue;
            }
            let target = match known.find(&n.name) {
                Some((_, Some(level))) if *level != 0 => continue,
                Some((found, _)) => {
                    notes.push((
                        id.clone(),
                        format!(
                            "rules: its text gives the \"{}\" cantrip and no rule granted it: granted ({found})",
                            title(&n.name)
                        ),
                    ));
                    found.clone()
                }
                None => {
                    let pid = made.get(&n.name, 0, &docs[i], known);
                    notes.push((id.clone(), placeholder_note(&n.name, &pid)));
                    pid
                }
            };
            let template = std::iter::once(i)
                .chain(up.iter().copied())
                .chain(down.iter().copied())
                .find_map(|j| spell_rules(&docs[j]).next());
            add.push(cantrip_rule(&target, template));
        }
        if let Some(rules) = rules_mut(&mut docs[i]) {
            rules.extend(add);
        }
    }
    notes
}

/// `(element id, note)` for every official spell `docs` refer to that nobody has (the notes
/// [`official_spells`] leaves): for the review to flag the elements.
#[allow(dead_code)] // the review's signal (`feature_spells::official::unresolved_official`)
pub(crate) fn unresolved_official(
    plan: &ImportPlan,
    docs: &[Value],
    store: &ContentStore,
) -> Vec<(String, String)> {
    let mut d = Derived {
        docs: docs.to_vec(),
        ..Derived::default()
    };
    official_spells(plan, store, &mut d);
    d.notes
        .into_iter()
        .filter(|(_, m)| m.starts_with(UNRESOLVED_OFFICIAL))
        .collect()
}

#[cfg(test)]
#[path = "official_spells_tests.rs"]
mod tests;
