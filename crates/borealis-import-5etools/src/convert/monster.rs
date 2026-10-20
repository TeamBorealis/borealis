//! Creatures (`bestiary/bestiary-*.json`, property `monster`) as typed `Monster` elements:
//! the element type and `data` shape ([`MonsterData`]) of the bundled SRD monster packs, read
//! field by field (name, source, size, type and tags, alignment, AC, HP, speed, ability
//! scores, saves, skills, senses, languages, CR and XP, and traits / actions / bonus actions
//! / reactions / legendary actions as text, lair actions and regional effects from the
//! creature's legendary group). A Monster Manual creature links to the bundled SRD stat block
//! of the same name and edition (`maps_to` kind `equivalent`). Deterministic; no AI is involved.
//!
//! A creature whose stat block cannot be read exactly (an ability score or challenge rating
//! that is not a number, a creature type that is a choice, ...) is skipped with the reason in
//! the report. A creature without a challenge rating (a summon) whose armor class or hit points
//! are printed as a formula keeps the formula as display text (decision IM12a); with a
//! challenge rating such text is still a skip. Details the model has no field for are listed
//! as partial.

use super::monster_lair::LegendaryGroups;
use super::monster_text::{alignment, name_usage, senses, spellcasting};
use super::{Conv, Ent, Shared, State};
use crate::ids::IdKind;
use crate::srd::SrdKind;
use crate::text::{strip_tags, Renderer};
use borealis_content::monster::{
    Abilities, Ability, ArmorClass, CreatureSize, CreatureType, DamageClause, DamageType, Feature,
    HitPoints, Initiative, LegendaryActions, Skill, Speeds,
};
use borealis_content::{Cr, MonsterData, Ruleset};
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::collections::BTreeMap;

/// Kind label of creatures in the import report.
pub const KIND: &str = "monster";
/// Prefix of a skip reason: the creature's stat block cannot be read exactly.
pub const SKIP: &str = "creature stat block: ";
/// Prefix of a partial note: a part printed with the stat block that the model does not hold
/// (variant sidebars, mythic actions, a lair whose legendary group is not in the data).
pub const SECTION: &str = "creature section: ";
/// Prefix of a partial note: a field kept in part (a printed remark the model has no field for).
pub const FIELD: &str = "creature field: ";

/// Convert one creature. A creature that cannot be read is reported as skipped here.
pub fn convert(sh: &Shared, st: &mut State, ent: &Ent) -> Conv {
    match stat_block_in(&ent.raw, ent.ruleset, &sh.legendary_groups) {
        Err(reason) => {
            st.report
                .skipped(&ent.file, KIND, &ent.label(), &format!("{SKIP}{reason}"));
            Conv::default()
        }
        Ok((data, lost)) => {
            let partial = lost
                .into_iter()
                .map(|l| {
                    if l.starts_with(SECTION) {
                        l
                    } else {
                        format!("{FIELD}{l}")
                    }
                })
                .collect();
            let id = st.ids.named(IdKind::Monster, &ent.source, &ent.name);
            let mut el = data.to_element(id, &ent.name, sh.source_display(&ent.source));
            el.maps_to = srd_equivalent(sh, ent);
            Conv {
                elements: vec![el],
                partial,
            }
        }
    }
}

/// Books whose creatures are the SRD's creatures under the same name: the 2014 Monster Manual
/// (SRD 5.1) and the 2025 Monster Manual (SRD 5.2.1).
pub const SRD_MONSTER_BOOKS: &[&str] = &["MM", "XMM"];

/// `maps_to` kind `equivalent` from a Monster Manual creature to the bundled SRD stat block of
/// the same name and edition, so the two are one creature. Other books get no link, nor does
/// the other edition (its stat block differs; the SRD packs map those pairs `approximate`).
fn srd_equivalent(sh: &Shared, ent: &Ent) -> Option<borealis_content::MapsTo> {
    if !SRD_MONSTER_BOOKS
        .iter()
        .any(|b| ent.source.eq_ignore_ascii_case(b))
    {
        return None;
    }
    sh.maps_to(SrdKind::Monster, ent.ruleset, &ent.name, &[], &|_| None)
}

/// The typed stat block of a resolved 5etools creature and what it loses (with no legendary
/// groups: a creature's lair is a partial note).
///
/// # Errors
/// Why the creature cannot be imported.
pub fn stat_block(raw: &Value, ruleset: Ruleset) -> Result<(MonsterData, Vec<String>), String> {
    stat_block_in(raw, ruleset, &LegendaryGroups::default())
}

/// [`stat_block`] with the legendary groups that hold the creatures' lair actions and
/// regional effects.
///
/// # Errors
/// Why the creature cannot be imported.
pub fn stat_block_in(
    raw: &Value,
    ruleset: Ruleset,
    groups: &LegendaryGroups,
) -> Result<(MonsterData, Vec<String>), String> {
    let mut lost = Vec::new();
    let (creature_type, tags, swarm_of) = creature_type(raw)?;
    let abilities = abilities(raw)?;
    let challenge = challenge(raw, &mut lost)?;
    let cr = challenge.map(|(cr, _, _)| cr);
    let mut skill_notes = Vec::new();
    let skills = bonuses::<Skill>(raw, "skill", &mut lost, &mut skill_notes);
    let passive = passive(raw, &abilities, &skills);
    let mut traits = features(raw.get("trait"), "trait", &mut lost);
    let mut actions = features(raw.get("action"), "action", &mut lost);
    let mut bonus_actions = features(raw.get("bonus"), "bonus action", &mut lost);
    let mut reactions = features(raw.get("reaction"), "reaction", &mut lost);
    for sc in raw
        .get("spellcasting")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(f) = casting_feature(sc) else {
            lost.push("a spellcasting entry without text not imported".into());
            continue;
        };
        match sc.get("displayAs").and_then(Value::as_str) {
            Some("action" | "legendary") => actions.push(f),
            Some("bonus") => bonus_actions.push(f),
            Some("reaction") => reactions.push(f),
            _ => traits.push(f),
        }
    }
    let lair = super::monster_lair::lair(raw, groups, &mut lost);
    for (key, what) in [
        ("mythic", "mythic actions"),
        ("variant", "variant sidebars"),
        ("actionHeader", "the actions' introduction"),
        ("bonusHeader", "the bonus actions' introduction"),
        ("reactionHeader", "the reactions' introduction"),
    ] {
        if raw.get(key).is_some() {
            lost.push(format!("{SECTION}{what} not imported"));
        }
    }
    let size = sizes(raw)?;
    let (ac, ac_note) = match armor(raw, &mut lost) {
        Ok(ac) => (ac, None),
        Err(nan) => (Vec::new(), Some(formula_note(nan, cr)?)),
    };
    let (hp, hp_note) = match hit_points(raw, &mut lost) {
        Ok(hp) => (Some(hp), None),
        Err(nan) => (None, Some(formula_note(nan, cr)?)),
    };
    let data = MonsterData {
        ruleset,
        size,
        swarm_of,
        creature_type,
        tags,
        alignment: alignment(raw),
        ac,
        ac_note,
        hp,
        hp_note,
        speed: speeds(raw, &mut lost)?,
        initiative: initiative(raw, ruleset, &abilities, cr),
        saves: bonuses::<Ability>(raw, "save", &mut lost, &mut Vec::new()),
        skills,
        skill_notes,
        damage_vulnerabilities: damage(raw, "vulnerable"),
        damage_resistances: damage(raw, "resist"),
        damage_immunities: damage(raw, "immune"),
        condition_immunities: condition_immunities(raw),
        gear: gear(raw),
        senses: senses(raw, passive),
        languages: texts(raw.get("languages")),
        telepathy: telepathy(raw),
        cr,
        cr_note: cr.is_none().then(|| challenge_note(raw)).flatten(),
        xp: challenge.map(|(_, xp, _)| xp),
        xp_lair: challenge.and_then(|(_, _, lair)| lair),
        proficiency_bonus: cr.map(Cr::proficiency_bonus),
        traits,
        actions,
        bonus_actions,
        reactions,
        legendary: legendary(raw, ruleset, &mut lost),
        lair_actions: lair.actions,
        regional_effects: lair.regional_effects,
        description: None,
        abilities,
    };
    if let Some(issue) = data.validate().first() {
        return Err(format!("stat block not valid: {issue}"));
    }
    Ok((data, lost))
}

fn parse_as<T: DeserializeOwned>(text: &str) -> Option<T> {
    serde_json::from_value(Value::String(text.to_lowercase())).ok()
}

fn size_of(code: &str) -> Option<CreatureSize> {
    Some(match code {
        "T" => CreatureSize::Tiny,
        "S" => CreatureSize::Small,
        "M" => CreatureSize::Medium,
        "L" => CreatureSize::Large,
        "H" => CreatureSize::Huge,
        "G" => CreatureSize::Gargantuan,
        _ => return None,
    })
}

fn sizes(raw: &Value) -> Result<Vec<CreatureSize>, String> {
    let codes: Vec<&str> = match raw.get("size") {
        Some(Value::Array(a)) => a.iter().filter_map(Value::as_str).collect(),
        Some(Value::String(s)) => vec![s.as_str()],
        _ => return Err("no size".into()),
    };
    codes
        .iter()
        .map(|c| size_of(c).ok_or_else(|| format!("size {c:?} is not a creature size")))
        .collect()
}

type TypeParts = (CreatureType, Vec<String>, Option<CreatureSize>);
/// Challenge rating, XP and lair XP.
type Challenge = (Cr, u32, Option<u32>);

fn creature_type(raw: &Value) -> Result<TypeParts, String> {
    let ty = raw.get("type").ok_or("no creature type")?;
    let (name, obj) = match ty {
        Value::String(s) => (s.clone(), None),
        Value::Object(o) => match o.get("type") {
            Some(Value::String(s)) => (s.clone(), Some(o)),
            Some(other) => {
                return Err(format!(
                    "the creature type is a choice ({})",
                    strip_tags(&other.to_string())
                ))
            }
            None => return Err("no creature type".into()),
        },
        _ => return Err("no creature type".into()),
    };
    let parsed: CreatureType =
        parse_as(&name).ok_or_else(|| format!("creature type {name:?} is not known"))?;
    let tags = obj
        .and_then(|o| o.get("tags"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|t| match t {
            Value::String(s) => Some(strip_tags(s)),
            other => {
                let tag = other.get("tag")?.as_str()?;
                Some(match other.get("prefix").and_then(Value::as_str) {
                    Some(p) => format!("{p} {tag}"),
                    None => tag.to_string(),
                })
            }
        })
        .collect();
    let swarm = obj
        .and_then(|o| o.get("swarmSize"))
        .and_then(Value::as_str)
        .and_then(size_of);
    Ok((parsed, tags, swarm))
}

fn abilities(raw: &Value) -> Result<Abilities, String> {
    let score = |k: &str| {
        raw.get(k)
            .and_then(Value::as_u64)
            .and_then(|n| u8::try_from(n).ok())
            .ok_or_else(|| format!("the {k} score is not a number"))
    };
    Ok(Abilities {
        str: score("str")?,
        dex: score("dex")?,
        con: score("con")?,
        int: score("int")?,
        wis: score("wis")?,
        cha: score("cha")?,
    })
}

fn cr_of(text: &str) -> Result<Cr, String> {
    Cr::parse(text.trim()).ok_or_else(|| format!("challenge rating {text:?} is not a number"))
}

/// Challenge rating, XP and lair XP. `None` for a creature printed without a challenge rating
/// (decision IM12): it imports with no CR and no XP.
fn challenge(raw: &Value, lost: &mut Vec<String>) -> Result<Option<Challenge>, String> {
    let as_u32 = |v: Option<&Value>| {
        v.and_then(Value::as_u64)
            .and_then(|n| u32::try_from(n).ok())
    };
    match raw.get("cr") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => {
            let cr = cr_of(s)?;
            Ok(Some((cr, cr.standard_xp(), None)))
        }
        Some(Value::Object(o)) => {
            let Some(text) = o.get("cr").and_then(Value::as_str) else {
                return Ok(None);
            };
            let cr = cr_of(text)?;
            let xp = as_u32(o.get("xp")).unwrap_or_else(|| cr.standard_xp());
            let lair = match (
                as_u32(o.get("xpLair")),
                o.get("lair").and_then(Value::as_str),
            ) {
                (Some(x), _) => Some(x),
                (None, Some(l)) => Some(cr_of(l)?.standard_xp()),
                _ => None,
            };
            if o.contains_key("coven") {
                lost.push("the challenge rating in a coven not imported".into());
            }
            Ok(Some((cr, xp, lair)))
        }
        Some(_) => Err("the challenge rating is neither text nor an object".into()),
    }
}

/// What stands in for a missing challenge rating, from the fields 5etools prints instead: the
/// spell or class whose level the creature scales with, or its own level (level-scaled NPCs).
fn challenge_note(raw: &Value) -> Option<String> {
    let name_of = |key: &str| {
        raw.get(key)
            .and_then(Value::as_str)
            .and_then(|r| r.split('|').next())
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .map(str::to_string)
    };
    if let Some(spell) = name_of("summonedBySpell") {
        return Some(format!("Scales with spell level ({spell})"));
    }
    if raw.get("summonedBySpellLevel").is_some() {
        return Some("Scales with spell level".into());
    }
    if let Some(class) = name_of("summonedByClass") {
        return Some(format!("Scales with the summoner's level ({class})"));
    }
    raw.get("level")
        .and_then(Value::as_u64)
        .map(|level| format!("Level {level}"))
}

/// Why an armor class or hit points are not a number: the skip reason, and the printed text
/// when there is some (a formula of the spell or summoner level).
struct NotANumber {
    reason: String,
    text: Option<String>,
}

impl NotANumber {
    fn plain(reason: &str) -> Self {
        Self {
            reason: reason.to_string(),
            text: None,
        }
    }
}

/// Decision IM12a: a creature without a challenge rating (a summon whose numbers follow the
/// spell or the summoner's level) keeps an armor class or hit points printed as a formula as
/// display text. With a challenge rating, or with no text at all, it stays a skip.
fn formula_note(nan: NotANumber, cr: Option<Cr>) -> Result<String, String> {
    match nan.text {
        Some(text) if cr.is_none() => Ok(text),
        _ => Err(nan.reason),
    }
}

/// The printed text of a `special` entry, tags stripped; `None` when empty.
fn special_text(v: &Value) -> Option<String> {
    let text = strip_tags(v.get("special")?.as_str()?);
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

fn hit_points(raw: &Value, lost: &mut Vec<String>) -> Result<HitPoints, NotANumber> {
    let hp = raw
        .get("hp")
        .ok_or_else(|| NotANumber::plain("no hit points"))?;
    // a `special` that is only a number is the hit points (AWM prints them so)
    let plain_special = hp
        .get("special")
        .and_then(Value::as_str)
        .and_then(|s| s.trim().parse::<u64>().ok());
    let Some(average) = hp
        .get("average")
        .and_then(Value::as_u64)
        .or(plain_special)
        .and_then(|n| u32::try_from(n).ok())
    else {
        let special = hp.get("special").and_then(Value::as_str).unwrap_or("?");
        return Err(NotANumber {
            reason: format!("hit points are not a number ({})", strip_tags(special)),
            text: special_text(hp),
        });
    };
    let formula = hp.get("formula").and_then(Value::as_str).and_then(|f| {
        let flat: String = f
            .chars()
            .filter(|c| !c.is_whitespace())
            .map(|c| if c == '\u{2212}' { '-' } else { c })
            .collect();
        if is_dice(&flat) {
            Some(flat)
        } else {
            lost.push(format!("hit dice {f:?} not imported"));
            None
        }
    });
    Ok(HitPoints { average, formula })
}

/// `NdS`, `NdS+M` or `NdS-M`.
fn is_dice(s: &str) -> bool {
    let Some((n, rest)) = s.split_once('d') else {
        return false;
    };
    let (sides, modifier) = match rest.find(['+', '-']) {
        Some(i) => (&rest[..i], Some(&rest[i + 1..])),
        None => (rest, None),
    };
    let digits = |t: &str| !t.is_empty() && t.chars().all(|c| c.is_ascii_digit());
    // a one-sided die (`50d1`, an object's fixed hit points) is no dice formula
    digits(n) && digits(sides) && sides != "1" && modifier.map_or(true, digits)
}

fn armor(raw: &Value, lost: &mut Vec<String>) -> Result<Vec<ArmorClass>, NotANumber> {
    let mut out = Vec::new();
    let mut not_numbers = Vec::new();
    let entries = raw.get("ac").and_then(Value::as_array);
    for a in entries.into_iter().flatten() {
        let value = a.as_u64().or_else(|| a.get("ac").and_then(Value::as_u64));
        let Some(value) = value.and_then(|v| u32::try_from(v).ok()) else {
            not_numbers.push(format!(
                "armor class {} not imported",
                strip_tags(&a.to_string())
            ));
            continue;
        };
        let source = a.get("from").and_then(Value::as_array).map(|f| {
            f.iter()
                .filter_map(Value::as_str)
                .map(strip_tags)
                .collect::<Vec<_>>()
                .join(", ")
        });
        let condition = a.get("condition").and_then(Value::as_str).map(strip_tags);
        out.push(ArmorClass {
            value,
            source: source.filter(|s| !s.is_empty()),
            condition,
        });
    }
    if out.is_empty() {
        // every entry is text: the formula when each one is a `special` line
        let texts: Option<Vec<String>> = entries
            .filter(|list| !list.is_empty())
            .map(|list| list.iter().map(special_text).collect())
            .unwrap_or_default();
        return Err(NotANumber {
            reason: "armor class is not a number".into(),
            text: texts.map(|t| t.join(", ")),
        });
    }
    lost.extend(not_numbers);
    Ok(out)
}

fn speeds(raw: &Value, lost: &mut Vec<String>) -> Result<Speeds, String> {
    let sp = raw
        .get("speed")
        .and_then(Value::as_object)
        .ok_or("no speed")?;
    let mut out = Speeds {
        hover: sp.get("canHover").and_then(Value::as_bool).unwrap_or(false),
        ..Speeds::default()
    };
    let mut notes = Vec::new();
    for (key, slot) in [
        ("walk", &mut out.walk),
        ("burrow", &mut out.burrow),
        ("climb", &mut out.climb),
        ("fly", &mut out.fly),
        ("swim", &mut out.swim),
    ] {
        let Some(v) = sp.get(key) else { continue };
        let number = v
            .as_u64()
            .or_else(|| v.get("number").and_then(Value::as_u64));
        *slot = number.and_then(|n| u32::try_from(n).ok());
        if let Some(cond) = v.get("condition").and_then(Value::as_str).map(strip_tags) {
            if cond.contains("hover") {
                out.hover = true;
            } else {
                notes.push(format!("{key} {} ft. {cond}", number.unwrap_or(0)));
            }
        }
        if v.is_boolean() {
            notes.push(format!("{key} equal to its walking speed"));
        }
    }
    for key in ["alternate", "choose"] {
        if let Some(v) = sp.get(key) {
            lost.push(format!("{key} speeds not imported"));
            notes.push(strip_tags(&v.to_string()));
        }
    }
    out.note = (!notes.is_empty()).then(|| notes.join("; "));
    Ok(out)
}

fn ability_modifier(score: u8) -> i32 {
    (i32::from(score) - 10).div_euclid(2)
}

/// The initiative; `None` when it adds a proficiency bonus the creature does not have (no
/// challenge rating: a summoned creature uses its summoner's).
fn initiative(raw: &Value, ruleset: Ruleset, ab: &Abilities, cr: Option<Cr>) -> Option<Initiative> {
    let field = raw.get("initiative");
    if field.is_none() && ruleset != Ruleset::Dnd2024 {
        return None;
    }
    let dex = ability_modifier(ab.dex);
    let modifier = match field {
        Some(Value::Number(n)) => i32::try_from(n.as_i64()?).ok()?,
        Some(o) => {
            let mult = o.get("proficiency").and_then(Value::as_i64).unwrap_or(0);
            let pb = match cr {
                Some(cr) => i32::from(cr.proficiency_bonus()),
                None if mult == 0 => 0,
                None => return None,
            };
            dex + i32::try_from(mult).ok()? * pb
        }
        None => dex,
    };
    Some(Initiative {
        modifier,
        score: u8::try_from(10 + modifier).ok()?,
    })
}

/// A `{"dex": "+6"}` map of bonuses keyed by ability or skill. A value with a remark after
/// its number (`+4 (+6 while in snake form)`) keeps the number; the printed text goes to
/// `remarks`.
fn bonuses<K: DeserializeOwned + Ord>(
    raw: &Value,
    field: &str,
    lost: &mut Vec<String>,
    remarks: &mut Vec<String>,
) -> BTreeMap<K, i32> {
    let mut out = BTreeMap::new();
    for (k, v) in raw
        .get(field)
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
    {
        let text = v.as_str().map(|s| strip_tags(s).trim().to_string());
        let n = text
            .as_deref()
            .and_then(|s| s.split_whitespace().next())
            .and_then(|s| s.trim_start_matches('+').parse::<i32>().ok());
        match (parse_as::<K>(k), n) {
            (Some(key), Some(n)) => {
                out.insert(key, n);
                if let Some(t) = text.filter(|t| t.split_whitespace().nth(1).is_some()) {
                    remarks.push(format!("{} {t}", crate::util::title_case(k)));
                }
            }
            _ => lost.push(format!("{field} {k} not imported")),
        }
    }
    out
}

fn passive(raw: &Value, ab: &Abilities, skills: &BTreeMap<Skill, i32>) -> u32 {
    let printed = match raw.get("passive") {
        Some(Value::Number(n)) => n.as_u64(),
        Some(Value::String(s)) => strip_tags(s)
            .split(|c: char| !c.is_ascii_digit())
            .find(|t| !t.is_empty())
            .and_then(|t| t.parse().ok()),
        _ => None,
    };
    printed
        .and_then(|n| u32::try_from(n).ok())
        .unwrap_or_else(|| {
            let bonus = skills
                .get(&Skill::Perception)
                .copied()
                .unwrap_or_else(|| ability_modifier(ab.wis));
            u32::try_from(10 + bonus).unwrap_or(0)
        })
}

fn texts(v: Option<&Value>) -> Vec<String> {
    v.and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(strip_tags)
        .filter(|s| !s.trim().is_empty())
        .collect()
}

fn telepathy(raw: &Value) -> Option<u32> {
    texts(raw.get("languages")).iter().find_map(|l| {
        let lower = l.to_lowercase();
        let rest = &lower[lower.find("telepathy ")? + "telepathy ".len()..];
        rest.split_whitespace().next()?.parse().ok()
    })
}

/// Damage clauses: the plain types in one clause, each conditional group in its own.
fn damage(raw: &Value, key: &str) -> Vec<DamageClause> {
    fn note_of(o: &Value) -> Option<String> {
        let parts: Vec<String> = ["preNote", "note"]
            .iter()
            .filter_map(|k| o.get(*k).and_then(Value::as_str).map(strip_tags))
            .collect();
        (!parts.is_empty()).then(|| parts.join(" "))
    }
    fn group(list: &[Value], key: &str, text: Option<String>) -> Vec<DamageClause> {
        let mut types = Vec::new();
        let mut words = Vec::new();
        let mut out = Vec::new();
        for item in list {
            match item {
                Value::String(s) => match parse_as::<DamageType>(s) {
                    Some(t) => types.push(t),
                    None => words.push(strip_tags(s)),
                },
                Value::Object(_) => {
                    if let Some(s) = item.get("special").and_then(Value::as_str) {
                        out.push(DamageClause {
                            types: Vec::new(),
                            text: Some(strip_tags(s)),
                        });
                    } else if let Some(Value::Array(inner)) = item.get(key) {
                        out.extend(group(inner, key, note_of(item)));
                    }
                }
                _ => {}
            }
        }
        let text = match (words.is_empty(), text) {
            (true, t) => t,
            (false, Some(t)) => Some(format!("{} {t}", words.join(", "))),
            (false, None) => Some(words.join(", ")),
        };
        if !types.is_empty() || text.is_some() {
            out.insert(0, DamageClause { types, text });
        }
        out
    }
    match raw.get(key) {
        Some(Value::Array(list)) => group(list, key, None),
        _ => Vec::new(),
    }
}

fn condition_immunities(raw: &Value) -> Vec<String> {
    let mut out = Vec::new();
    for c in raw
        .get("conditionImmune")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        match c {
            Value::String(s) => out.push(strip_tags(s).to_lowercase()),
            other => {
                let note = ["preNote", "note"]
                    .iter()
                    .filter_map(|k| other.get(*k).and_then(Value::as_str).map(strip_tags))
                    .collect::<Vec<_>>()
                    .join(" ");
                for name in texts(other.get("conditionImmune")) {
                    let name = name.to_lowercase();
                    out.push(if note.is_empty() {
                        name
                    } else {
                        format!("{name} ({note})")
                    });
                }
            }
        }
    }
    out
}

fn gear(raw: &Value) -> Vec<String> {
    raw.get("gear")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|g| match g {
            Value::String(s) => Some(strip_tags(s)),
            other => {
                let item = strip_tags(other.get("item")?.as_str()?);
                Some(match other.get("quantity").and_then(Value::as_u64) {
                    Some(q) if q > 1 => format!("{item} ({q})"),
                    _ => item,
                })
            }
        })
        .filter(|s| !s.trim().is_empty())
        .collect()
}

fn feature(name: &str, text: String) -> Feature {
    let (name, usage, cost) = name_usage(name);
    Feature {
        name,
        text,
        usage,
        cost,
        condition: None,
        attack: None,
        save: None,
        spellcasting: None,
    }
}

fn features(list: Option<&Value>, what: &str, lost: &mut Vec<String>) -> Vec<Feature> {
    let mut out = Vec::new();
    for f in list.and_then(Value::as_array).into_iter().flatten() {
        let name = f.get("name").and_then(Value::as_str).unwrap_or("");
        let text = f
            .get("entries")
            .and_then(Value::as_array)
            .map(|e| Renderer::plain(e))
            .unwrap_or_default();
        if strip_tags(name).trim().is_empty() || text.trim().is_empty() {
            lost.push(format!("a {what} without a name or text not imported"));
            continue;
        }
        out.push(feature(name, text));
    }
    out
}

fn casting_feature(sc: &Value) -> Option<Feature> {
    let text = spellcasting(sc);
    let name = sc
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("Spellcasting");
    (!text.trim().is_empty()).then(|| feature(name, text))
}

fn legendary(raw: &Value, ruleset: Ruleset, lost: &mut Vec<String>) -> Option<LegendaryActions> {
    let actions = features(raw.get("legendary"), "legendary action", lost);
    if actions.is_empty() {
        return None;
    }
    let count = |k: &str| {
        raw.get(k)
            .and_then(Value::as_u64)
            .and_then(|n| u8::try_from(n).ok())
    };
    let per_round = count("legendaryActions").unwrap_or(3);
    let lair_uses = count("legendaryActionsLair");
    let header = raw
        .get("legendaryHeader")
        .and_then(Value::as_array)
        .map(|h| Renderer::plain(h))
        .filter(|h| !h.trim().is_empty());
    let preamble = header.unwrap_or_else(|| default_preamble(raw, ruleset, per_round, lair_uses));
    Some(LegendaryActions {
        per_round: Some(per_round),
        lair_uses,
        preamble,
        actions,
    })
}

fn default_preamble(raw: &Value, ruleset: Ruleset, n: u8, lair: Option<u8>) -> String {
    let name = raw
        .get("shortName")
        .and_then(Value::as_str)
        .or_else(|| raw.get("name").and_then(Value::as_str))
        .unwrap_or("creature");
    let named = raw
        .get("isNamedCreature")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let who = if named {
        name.to_string()
    } else {
        format!("the {}", name.to_lowercase())
    };
    let mut chars = who.chars();
    let who_cap: String = chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect())
        .unwrap_or_default();
    if ruleset == Ruleset::Dnd2024 {
        let lair = lair.map_or_else(String::new, |l| format!(" ({l} in Lair)"));
        format!(
            "Legendary Action Uses: {n}{lair}. Immediately after another creature's turn, {who} \
             can expend a use to take one of the following actions. {who_cap} regains all \
             expended uses at the start of each of its turns."
        )
    } else {
        format!(
            "{who_cap} can take {n} legendary actions, choosing from the options below. Only one \
             legendary action option can be used at a time and only at the end of another \
             creature's turn. {who_cap} regains spent legendary actions at the start of its turn."
        )
    }
}

#[cfg(test)]
#[path = "monster_tests.rs"]
mod tests;
