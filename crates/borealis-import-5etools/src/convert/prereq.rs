//! 5etools `prerequisite` arrays -> requirement expressions (`engine.md` 5.1).
//!
//! The array is a list of alternatives (OR); the keys of one alternative are all required
//! (AND); list values inside one key are alternatives again ("an elf or a half-elf"). Every
//! key is turned into existing requirement terms: `[character:N]`, `[<class>:N]`, ability
//! tests and element ids (races, subraces, subclasses, backgrounds, feats, features,
//! optional features, spells, proficiencies). A requirement on a named feature of a
//! one-alternative prerequisite uses the engine's structured `data.prerequisite.requires_feature`
//! (matched by element name, so it also holds for bundled classes). What cannot be checked
//! stays in the display text with "(not checked)" and is treated as met, so the option is
//! still offered.

use super::prereq_index::{linked_named, srd_ids, PrereqIndex};
use super::Shared;
use crate::core_ids;
use crate::srd::SrdKind;
use crate::text::RefKind;
use crate::util::key;
use regex::Regex;
use serde_json::Value;
use std::collections::BTreeSet;
use std::sync::OnceLock;

/// Marker appended to prerequisite text that is shown but not enforced.
pub const NOT_CHECKED: &str = "(not checked)";

/// A converted prerequisite.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Prereq {
    /// Display text.
    pub text: String,
    /// Requirements expression (empty when nothing can be enforced).
    pub requirements: String,
    /// A feature (by element name) the character must have: `data.prerequisite.requires_feature`.
    pub requires_feature: Option<String>,
    /// Character requirements that are shown but not enforced (reported as partial).
    pub unmapped: Vec<String>,
    /// Shown but deliberately not enforced: campaign settings, infusion item targets.
    pub informational: Vec<String>,
}

/// What a prerequisite conversion can look up.
#[derive(Debug, Clone, Copy)]
pub struct Cx<'a> {
    /// Links and SRD index.
    pub sh: &'a Shared,
    /// Name lookups.
    pub ix: &'a PrereqIndex,
    /// Id of the element being converted (excluded from its own category tests).
    pub own_id: &'a str,
}

/// One converted key of an alternative.
enum Part {
    /// A requirement term.
    Expr(String),
    /// A named feature (structured check when possible) with its id-based fallback.
    Feature(String, Option<String>),
    /// Nothing to enforce (a setting or an item target).
    Info,
    /// Always met here (no other feat of an exclusive category exists).
    Met,
    /// A character requirement that cannot be expressed.
    Unmapped,
}

fn or_ids(ids: &BTreeSet<String>) -> Option<String> {
    match ids.len() {
        0 => None,
        1 => ids.iter().next().cloned(),
        _ => Some(format!(
            "({})",
            ids.iter().cloned().collect::<Vec<_>>().join("||")
        )),
    }
}

fn group(parts: &[String]) -> Option<String> {
    match parts.len() {
        0 => None,
        1 => Some(parts[0].clone()),
        _ => Some(format!("({})", parts.join("||"))),
    }
}

fn list_of(v: &Value) -> Vec<&Value> {
    match v {
        Value::Array(a) => a.iter().collect(),
        Value::Null => Vec::new(),
        other => vec![other],
    }
}

fn level_part(cx: &Cx<'_>, v: &Value) -> Option<(String, String)> {
    if let Some(n) = v.as_u64() {
        return Some((format!("[character:{n}]"), format!("Level {n}+")));
    }
    let n = v.get("level").and_then(Value::as_u64)?;
    let Some(class) = v.pointer("/class/name").and_then(Value::as_str) else {
        return Some((format!("[character:{n}]"), format!("Level {n}+")));
    };
    let test = format!("[{}:{n}]", class.to_lowercase());
    let Some(sub) = v.pointer("/subclass/name").and_then(Value::as_str) else {
        return Some((test, format!("{class} level {n}+")));
    };
    let ids = cx
        .ix
        .subclasses
        .get(&(key(class), key(sub)))
        .and_then(or_ids)?;
    Some((
        format!("{test}&&{ids}"),
        format!("{class} ({sub}) level {n}+"),
    ))
}

fn ability_part(v: &Value) -> Option<(String, String)> {
    let alts: Vec<(String, String)> = v
        .as_array()?
        .iter()
        .filter_map(|o| {
            let pairs: Vec<(String, i64, &str)> = o
                .as_object()?
                .iter()
                .filter_map(|(ab, n)| Some((ab.clone(), n.as_i64()?, core_ids::ability_name(ab)?)))
                .collect();
            if pairs.is_empty() {
                return None;
            }
            let joined = pairs
                .iter()
                .map(|(a, n, _)| format!("[{a}:{n}]"))
                .collect::<Vec<_>>()
                .join("&&");
            let expr = if pairs.len() > 1 {
                format!("({joined})")
            } else {
                joined
            };
            let text = pairs
                .iter()
                .map(|(_, n, name)| format!("{name} {n} or higher"))
                .collect::<Vec<_>>()
                .join(" and ");
            Some((expr, text))
        })
        .collect();
    let exprs: Vec<String> = alts.iter().map(|(e, _)| e.clone()).collect();
    let text = alts
        .iter()
        .map(|(_, t)| t.clone())
        .collect::<Vec<_>>()
        .join(" or ");
    Some((group(&exprs)?, text))
}

fn race_ids(cx: &Cx<'_>, item: &Value) -> BTreeSet<String> {
    let name = key(item.get("name").and_then(Value::as_str).unwrap_or_default());
    if name == "small race" {
        let mut ids = cx.ix.small_races.clone();
        for n in &cx.ix.small_names {
            ids.extend(srd_ids(cx.sh, SrdKind::Race, n));
        }
        return ids;
    }
    if let Some(sub) = item.get("subrace").and_then(Value::as_str) {
        let mut ids = cx
            .ix
            .subraces
            .get(&(name.clone(), key(sub)))
            .cloned()
            .unwrap_or_default();
        for srd in [format!("{sub} {name}"), format!("{name} ({sub})")] {
            ids.extend(srd_ids(cx.sh, SrdKind::Subrace, &srd));
        }
        return ids;
    }
    let mut ids = cx.ix.races.get(&name).cloned().unwrap_or_default();
    ids.extend(srd_ids(cx.sh, SrdKind::Race, &name));
    // "Vampire (Ixalan)": a subrace written into the race name
    if ids.is_empty() {
        if let Some((race, sub)) = name.strip_suffix(')').and_then(|n| n.split_once(" (")) {
            let sub_item = serde_json::json!({"name": race, "subrace": sub});
            return race_ids(cx, &sub_item);
        }
    }
    ids
}

/// `name|source|display` -> `(name, source)`.
fn uid(s: &str, default_source: &str) -> (String, String) {
    let mut it = s.split('|');
    let name = it.next().unwrap_or_default().trim().to_string();
    let source = it
        .next()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(default_source)
        .to_string();
    (name, source)
}

fn linked(cx: &Cx<'_>, kind: RefKind, s: &str) -> BTreeSet<String> {
    let (name, source) = uid(s, kind.default_source());
    match cx.sh.link(kind, &name, &source) {
        Some(id) => BTreeSet::from([id]),
        None => linked_named(cx.sh, kind, &name),
    }
}

/// A feat reference: the feat, or the versions of it (`Name; Version`) the display text names
/// (`strike of the giants|bgg|Strike of the Giants (Fire Strike)`), or the feat and all its
/// versions.
fn feat_ids(cx: &Cx<'_>, s: &str) -> BTreeSet<String> {
    let (name, _) = uid(s, "PHB");
    let name = key(&name);
    let prefix = format!("{name}; ");
    let display = key(s.split('|').nth(2).unwrap_or_default());
    let versions: Vec<(&String, &String)> = cx
        .sh
        .links
        .iter()
        .filter(|((k, n, _), _)| *k == RefKind::Feat && n.starts_with(&prefix))
        .map(|((_, n, _), id)| (n, id))
        .collect();
    let named: BTreeSet<String> = versions
        .iter()
        .filter(|(n, _)| display.contains(&n[prefix.len()..]))
        .map(|(_, id)| (*id).clone())
        .collect();
    if !named.is_empty() {
        return named;
    }
    let mut ids = linked(cx, RefKind::Feat, s);
    ids.extend(versions.into_iter().map(|(_, id)| id.clone()));
    ids.extend(srd_ids(cx.sh, SrdKind::Feat, &name));
    ids
}

fn spell_ids_named(cx: &Cx<'_>, name: &str) -> BTreeSet<String> {
    let name = key(name);
    let mut ids: BTreeSet<String> = cx
        .ix
        .spells
        .iter()
        .filter(|s| s.name == name)
        .map(|s| s.id.clone())
        .collect();
    ids.extend(srd_ids(cx.sh, SrdKind::Spell, &name));
    ids
}

/// `{choose: "level=0|class=Warlock|spell attack=m;r;o", entry}` -> the spells that match.
fn spell_choice_ids(cx: &Cx<'_>, filter: &str, entry: &str) -> Option<BTreeSet<String>> {
    let needs_damage = entry.to_lowercase().contains("damage");
    let mut rows: Vec<_> = cx.ix.spells.iter().collect();
    for clause in filter.split('|') {
        let (k, v) = clause.split_once('=')?;
        let values: Vec<String> = v.split(';').map(key).collect();
        match key(k).as_str() {
            "level" => rows.retain(|s| values.contains(&s.level.to_string())),
            "class" => rows.retain(|s| s.classes.iter().any(|c| values.contains(c))),
            "spell attack" => {
                rows.retain(|s| s.attacks.iter().any(|a| values.contains(&a.to_lowercase())))
            }
            _ => return None,
        }
    }
    if needs_damage {
        rows.retain(|s| s.damage);
    }
    let mut ids = BTreeSet::new();
    for s in rows {
        ids.insert(s.id.clone());
        ids.extend(srd_ids(cx.sh, SrdKind::Spell, &s.name));
    }
    (!ids.is_empty()).then_some(ids)
}

fn spell_part(cx: &Cx<'_>, v: &Value) -> Option<String> {
    let mut alts = Vec::new();
    for item in list_of(v) {
        let ids = match item {
            Value::String(s) => {
                let (base, flag) = s.split_once('#').unwrap_or((s.as_str(), ""));
                let (name, _) = uid(base, "PHB");
                if flag == "x" {
                    // "hex spell or a warlock feature that curses"
                    let spell = name.split('/').next().unwrap_or_default();
                    let mut ids = spell_ids_named(cx, spell);
                    ids.extend(
                        cx.ix
                            .features
                            .iter()
                            .filter(|f| f.class == "warlock" && f.sub && f.name.contains("curse"))
                            .map(|f| f.id.clone()),
                    );
                    ids
                } else {
                    spell_ids_named(cx, &name)
                }
            }
            Value::Object(_) => {
                let filter = item.get("choose").and_then(Value::as_str)?;
                let entry = item
                    .get("entry")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                spell_choice_ids(cx, filter, entry)?
            }
            _ => return None,
        };
        alts.push(or_ids(&ids)?);
    }
    group(&alts)
}

fn proficiency_part(v: &Value) -> Option<String> {
    let mut alts = Vec::new();
    for item in list_of(v) {
        let mut all = Vec::new();
        for (k, val) in item.as_object()? {
            let name = val.as_str()?;
            let id = match k.as_str() {
                "armor" => core_ids::armor_prof_id(name).map(str::to_string),
                "weapon" | "weaponGroup" => core_ids::weapon_prof_id(name),
                "skill" => core_ids::skill_id(name),
                "tool" => core_ids::tool_prof_id(name),
                _ => None,
            }?;
            all.push(id);
        }
        if all.is_empty() {
            return None;
        }
        alts.push(all.join("&&"));
    }
    let wrapped: Vec<String> = alts
        .iter()
        .map(|a| {
            if alts.len() > 1 && a.contains("&&") {
                format!("({a})")
            } else {
                a.clone()
            }
        })
        .collect();
    group(&wrapped)
}

fn category_ids(cx: &Cx<'_>, v: &Value) -> BTreeSet<String> {
    list_of(v)
        .into_iter()
        .filter_map(Value::as_str)
        .filter_map(|c| cx.ix.feat_categories.get(c))
        .flatten()
        .filter(|id| *id != cx.own_id)
        .cloned()
        .collect()
}

/// None of these ids (always met when there are none).
fn none_of(ids: &BTreeSet<String>) -> Part {
    if ids.is_empty() {
        return Part::Met;
    }
    Part::Expr(
        ids.iter()
            .map(|id| format!("!{id}"))
            .collect::<Vec<_>>()
            .join("&&"),
    )
}

/// "No other dragonmark": no Dragonmark feat (category `D`) and no dragonmarked race or
/// subrace (`Mark of ...`).
fn no_other_dragonmark(cx: &Cx<'_>) -> Part {
    let mut ids = category_ids(cx, &Value::from(vec!["D"]));
    let marked = |name: &String| name.starts_with("mark of ");
    ids.extend(
        cx.ix
            .races
            .iter()
            .filter(|(n, _)| marked(n))
            .flat_map(|(_, s)| s.iter().cloned()),
    );
    ids.extend(
        cx.ix
            .subraces
            .iter()
            .filter(|((_, sub), _)| marked(sub))
            .flat_map(|(_, s)| s.iter().cloned()),
    );
    ids.remove(cx.own_id);
    none_of(&ids)
}

fn fighting_style_gate(text: &str) -> Option<String> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r#"(?i)level (\d+) ([a-z][a-z ]*?) "?fighting style"?"#).expect("valid regex")
    });
    let c = re.captures(text)?;
    Some(format!("[{}:{}]", c[2].trim().to_lowercase(), &c[1]))
}

/// Feature names -> the structured check name (one name, spellcasting-like names collapse
/// to `Spellcasting`, which the engine resolves to "has a spellcasting block").
fn feature_check_name(names: &[String]) -> Option<String> {
    if names
        .iter()
        .all(|n| n == "spellcasting" || n == "pact magic")
    {
        return Some("Spellcasting".into());
    }
    match names {
        [one] => Some(crate::util::title_case(one)),
        _ => None,
    }
}

fn feature_part(cx: &Cx<'_>, names: Vec<String>) -> Part {
    let fallback = or_ids(&cx.ix.features_named(&names));
    match feature_check_name(&names) {
        Some(n) => Part::Feature(n, fallback),
        None => fallback.map_or(Part::Unmapped, Part::Expr),
    }
}

fn convert_key(cx: &Cx<'_>, k: &str, v: &Value) -> Part {
    let expr = |o: Option<String>| o.map_or(Part::Unmapped, Part::Expr);
    let ids = |set: BTreeSet<String>| expr(or_ids(&set));
    let strings = || -> Vec<String> {
        list_of(v)
            .into_iter()
            .filter_map(|x| x.as_str().map(str::to_string))
            .collect()
    };
    match k {
        "race" => {
            let alts: Option<Vec<String>> = list_of(v)
                .into_iter()
                .map(|r| or_ids(&race_ids(cx, r)))
                .collect();
            expr(alts.and_then(|a| group(&a)))
        }
        "background" => ids(list_of(v)
            .into_iter()
            .filter_map(|b| b.get("name").and_then(Value::as_str))
            .flat_map(|n| {
                let mut s = linked_named(cx.sh, RefKind::Background, n);
                s.extend(srd_ids(cx.sh, SrdKind::Background, n));
                s
            })
            .collect()),
        "feat" => ids(strings().iter().flat_map(|f| feat_ids(cx, f)).collect()),
        "optionalfeature" => ids(strings()
            .iter()
            .flat_map(|f| linked(cx, RefKind::OptionalFeature, f))
            .collect()),
        "pact" => ids(strings()
            .iter()
            .filter_map(|p| cx.ix.optfeatures.get(&format!("pact of the {}", key(p))))
            .flatten()
            .cloned()
            .collect()),
        "spell" => expr(spell_part(cx, v)),
        "proficiency" => expr(proficiency_part(v)),
        "feature" => feature_part(cx, strings().iter().map(|s| key(s)).collect()),
        "spellcasting" | "spellcasting2020" | "spellcastingFeature" | "spellcastingPrepared"
            if v.as_bool() != Some(false) =>
        {
            feature_part(cx, vec!["spellcasting".into(), "pact magic".into()])
        }
        "featCategory" => ids(category_ids(cx, v)),
        "exclusiveFeatCategory" => none_of(&category_ids(cx, v)),
        "other" => match v.as_str() {
            Some(s) if key(s).contains("no other dragonmark") => no_other_dragonmark(cx),
            _ => Part::Unmapped,
        },
        "otherSummary" => expr(
            v.get("entry")
                .and_then(Value::as_str)
                .and_then(fighting_style_gate),
        ),
        "campaign" | "item" => Part::Info,
        _ => Part::Unmapped,
    }
}

/// Display text of a reference string (`name|source|display`, `spell#c`).
fn ref_text(s: &str) -> String {
    let (base, flag) = s.split_once('#').unwrap_or((s, ""));
    let parts: Vec<&str> = base.split('|').collect();
    let shown = if parts.len() > 2 {
        crate::text::strip_tags(parts[2])
    } else {
        crate::util::title_case(&crate::text::strip_tags(parts[0]))
    };
    match flag {
        "c" => format!("{shown} cantrip"),
        "x" => format!(
            "{} spell or a warlock feature that curses",
            shown.split('/').next().unwrap_or_default()
        ),
        _ => shown,
    }
}

fn object_text(o: &serde_json::Map<String, Value>) -> String {
    if let Some(t) = o
        .get("displayEntry")
        .or_else(|| o.get("entry"))
        .and_then(Value::as_str)
    {
        return crate::text::strip_tags(t);
    }
    if let Some(name) = o.get("name").and_then(Value::as_str) {
        return match o.get("subrace").and_then(Value::as_str) {
            Some(sub) => format!(
                "{} ({})",
                crate::util::title_case(name),
                crate::util::title_case(sub)
            ),
            None => crate::util::title_case(name),
        };
    }
    o.iter()
        .map(|(k, v)| {
            let what = match k.as_str() {
                "weaponGroup" => "weapons",
                "armor" if v.as_str() == Some("shield") => return "shields".to_string(),
                other => other,
            };
            format!("{} {what}", v.as_str().unwrap_or_default())
        })
        .collect::<Vec<_>>()
        .join(" and ")
}

/// Display text of one key.
fn describe(key: &str, value: &Value) -> String {
    let flat = match value {
        Value::Bool(true) => String::new(),
        Value::String(s) => ref_text(s),
        Value::Array(a) => a
            .iter()
            .map(|x| match x {
                Value::String(s) => ref_text(s),
                Value::Object(o) => object_text(o),
                other => other.to_string(),
            })
            .collect::<Vec<_>>()
            .join(" or "),
        Value::Object(o) => o
            .get("entry")
            .or_else(|| o.get("entrySummary"))
            .and_then(Value::as_str)
            .map_or_else(|| value.to_string(), crate::text::strip_tags),
        other => other.to_string(),
    };
    let label = match key {
        "spellcasting" | "spellcasting2020" | "spellcastingFeature" | "spellcastingPrepared" => {
            "Spellcasting".to_string()
        }
        "other" | "otherSummary" => return flat,
        "exclusiveFeatCategory" => "No other feat of category".to_string(),
        "featCategory" => "A feat of category".to_string(),
        "optionalfeature" => "Feature".to_string(),
        k => crate::util::title_case(k),
    };
    if flat.is_empty() {
        label
    } else {
        format!("{label}: {flat}")
    }
}

#[derive(Default)]
struct Alt {
    exprs: Vec<String>,
    feature: Option<(String, Option<String>)>,
    text: Vec<String>,
}

fn convert_alt(cx: &Cx<'_>, map: &serde_json::Map<String, Value>, out: &mut Prereq) -> Alt {
    let mut alt = Alt::default();
    for (k, v) in map {
        match k.as_str() {
            "level" => {
                if let Some((e, t)) = level_part(cx, v) {
                    alt.exprs.push(e);
                    alt.text.push(t);
                    continue;
                }
            }
            "ability" => {
                if let Some((e, t)) = ability_part(v) {
                    alt.exprs.push(e);
                    alt.text.push(t);
                    continue;
                }
            }
            _ => {}
        }
        let text = describe(k, v);
        let part = if matches!(k.as_str(), "level" | "ability") {
            Part::Unmapped
        } else {
            convert_key(cx, k, v)
        };
        match part {
            Part::Expr(e) => {
                alt.exprs.push(e);
                alt.text.push(text);
            }
            Part::Met => alt.text.push(text),
            Part::Feature(name, fallback) => {
                alt.feature = Some((name, fallback));
                alt.text.push(text);
            }
            Part::Info => {
                alt.text.push(format!("{text} {NOT_CHECKED}"));
                out.informational.push(text);
            }
            Part::Unmapped => {
                alt.text.push(format!("{text} {NOT_CHECKED}"));
                out.unmapped.push(text);
            }
        }
    }
    alt
}

fn join_alt(exprs: &[String]) -> Option<String> {
    let wrapped: Vec<String> = exprs
        .iter()
        .map(|e| {
            if exprs.len() > 1 && e.contains("||") && !(e.starts_with('(') && e.ends_with(')')) {
                format!("({e})")
            } else {
                e.clone()
            }
        })
        .collect();
    (!wrapped.is_empty()).then(|| wrapped.join("&&"))
}

/// Convert a `prerequisite` array.
#[must_use]
pub fn prerequisites(items: &[Value], cx: &Cx<'_>) -> Prereq {
    let mut out = Prereq::default();
    let mut alts: Vec<Alt> = items
        .iter()
        .filter_map(Value::as_object)
        .map(|m| convert_alt(cx, m, &mut out))
        .collect();
    out.text = alts
        .iter()
        .filter(|a| !a.text.is_empty())
        .map(|a| a.text.join(", "))
        .collect::<Vec<_>>()
        .join(" or ");
    // A single alternative can use the structured, name-based feature check; with several
    // alternatives the feature must become an id test inside the expression.
    if alts.len() == 1 {
        out.requires_feature = alts[0].feature.take().map(|(name, _)| name);
    }
    let mut exprs = Vec::new();
    for alt in &mut alts {
        if let Some((name, fallback)) = alt.feature.take() {
            match fallback {
                Some(f) => alt.exprs.push(f),
                None => out.unmapped.push(format!("Feature: {name}")),
            }
        }
        match join_alt(&alt.exprs) {
            Some(e) => exprs.push(e),
            // an alternative with nothing checkable is always met
            None => {
                exprs.clear();
                break;
            }
        }
    }
    out.requirements = match exprs.len() {
        0 => String::new(),
        1 => exprs.remove(0),
        _ => exprs
            .iter()
            .map(|e| {
                if e.contains("&&") || e.contains("||") {
                    format!("({e})")
                } else {
                    e.clone()
                }
            })
            .collect::<Vec<_>>()
            .join("||"),
    };
    out.unmapped.sort();
    out.unmapped.dedup();
    out.informational.sort();
    out.informational.dedup();
    out
}

/// Convert `items` onto `el`: display text, requirements, the structured feature check and
/// notes for what is not checked (partial notes only for real character requirements).
pub fn apply_prerequisites(
    sh: &Shared,
    st: &super::State,
    items: &[Value],
    el: &mut borealis_content::Element,
    partial: &mut Vec<String>,
) {
    let own_id = el.id.clone();
    let p = prerequisites(
        items,
        &Cx {
            sh,
            ix: &st.prereq,
            own_id: &own_id,
        },
    );
    el.prerequisite = p.text;
    el.requirements = p.requirements;
    if let Some(f) = p.requires_feature {
        el.data.insert(
            "prerequisite".into(),
            serde_json::json!({ "requires_feature": f }),
        );
    }
    for u in &p.unmapped {
        partial.push(format!("prerequisite not enforced: {u}"));
        super::note_unmapped(el, "prerequisite", u);
    }
    for i in &p.informational {
        super::note_unmapped(el, "prerequisite-unchecked", i);
    }
}

#[cfg(test)]
#[path = "prereq_tests.rs"]
mod tests;
