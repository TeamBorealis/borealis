//! The creature form's fields (spec 4.11): the stat block in the element's `data` (`MonsterData`, `monster.rs`),
//! read and written through the typed model. Each field holds one part of it, so every key a stat block has is a
//! field's and none is kept as a part from the original.
//!
//! A field written unchanged puts its stored bytes back. An edited one is decoded into the model's type (a value the
//! model can't hold is a problem at the field) and written in the model's shape. Two parts are worked out at write:
//! the stat block edition follows the element's edition, and an unset passive Perception, XP or proficiency bonus is
//! the usual one ([`super::creature_values`]). The stat block's consistency is checked by the content validator
//! (`monster_validate.rs`), whose problems point at these fields.

use super::creature_values::{
    clean_feature, clean_lair, clean_legendary, dice_text, some_text, text_of, texts,
    usual_passive, ArmorClassValue, ChallengeValue, HitPointsValue, SensesValue,
};
use super::{decode, to_json, Reader};
use crate::form::write::Out;
use crate::form::{FieldMemo, FormProblem, Slots};
use borealis_content::monster::{
    dice_average, parse_dice, Abilities, Ability, CreatureSize, CreatureType, DamageClause,
    Feature, HitPoints, Initiative, LairSection, LegendaryActions, Skill, Speeds,
};
use borealis_content::{Cr, Ruleset};
use serde::Serialize;
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

/// A creature field (spec 4.11): one part of the stat block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreatureField {
    /// The stat block's edition (`data.ruleset`): the element's edition, shown to experts.
    StatBlock,
    /// Sizes (`data.size`).
    Size,
    /// A swarm's member size (`data.swarm_of`).
    SwarmOf,
    /// Creature type (`data.creature_type`).
    Type,
    /// The type's tags (`data.tags`: goblinoid, shapechanger).
    Tags,
    /// Alignment as printed (`data.alignment`).
    Alignment,
    /// Armor class entries or a note (`data.ac`, `data.ac_note`).
    ArmorClass,
    /// Hit points or a note (`data.hp`, `data.hp_note`).
    HitPoints,
    /// Speeds (`data.speed`).
    Speed,
    /// Initiative modifier and score (`data.initiative`, 2024).
    Initiative,
    /// The six scores (`data.abilities`).
    Abilities,
    /// Saving throw bonuses (`data.saves`).
    Saves,
    /// Skill bonuses (`data.skills`).
    Skills,
    /// Remarks on skills (`data.skill_notes`).
    SkillNotes,
    /// Damage vulnerabilities (`data.damage_vulnerabilities`).
    Vulnerabilities,
    /// Damage resistances (`data.damage_resistances`).
    Resistances,
    /// Damage immunities (`data.damage_immunities`).
    Immunities,
    /// Condition immunities (`data.condition_immunities`).
    ConditionImmunities,
    /// Gear (`data.gear`, 2024).
    Gear,
    /// Senses and passive Perception (`data.senses`).
    Senses,
    /// Languages (`data.languages`).
    Languages,
    /// Telepathy range (`data.telepathy`).
    Telepathy,
    /// CR or a note, XP, XP in its lair, proficiency bonus.
    Challenge,
    /// Traits (`data.traits`).
    Traits,
    /// Actions (`data.actions`).
    Actions,
    /// Bonus actions (`data.bonus_actions`).
    BonusActions,
    /// Reactions (`data.reactions`).
    Reactions,
    /// Legendary actions (`data.legendary`).
    Legendary,
    /// Lair actions (`data.lair_actions`).
    LairActions,
    /// Regional effects (`data.regional_effects`).
    RegionalEffects,
    /// Flavour text printed with the stat block (`data.description`).
    Flavour,
}

impl CreatureField {
    /// The `data` keys the field holds.
    pub(crate) fn data_keys(self) -> Vec<&'static str> {
        match self {
            Self::StatBlock => vec!["ruleset"],
            Self::Size => vec!["size"],
            Self::SwarmOf => vec!["swarm_of"],
            Self::Type => vec!["creature_type"],
            Self::Tags => vec!["tags"],
            Self::Alignment => vec!["alignment"],
            Self::ArmorClass => vec!["ac", "ac_note"],
            Self::HitPoints => vec!["hp", "hp_note"],
            Self::Speed => vec!["speed"],
            Self::Initiative => vec!["initiative"],
            Self::Abilities => vec!["abilities"],
            Self::Saves => vec!["saves"],
            Self::Skills => vec!["skills"],
            Self::SkillNotes => vec!["skill_notes"],
            Self::Vulnerabilities => vec!["damage_vulnerabilities"],
            Self::Resistances => vec!["damage_resistances"],
            Self::Immunities => vec!["damage_immunities"],
            Self::ConditionImmunities => vec!["condition_immunities"],
            Self::Gear => vec!["gear"],
            Self::Senses => vec!["senses"],
            Self::Languages => vec!["languages"],
            Self::Telepathy => vec!["telepathy"],
            Self::Challenge => vec!["cr", "cr_note", "xp", "xp_lair", "proficiency_bonus"],
            Self::Traits => vec!["traits"],
            Self::Actions => vec!["actions"],
            Self::BonusActions => vec!["bonus_actions"],
            Self::Reactions => vec!["reactions"],
            Self::Legendary => vec!["legendary"],
            Self::LairActions => vec!["lair_actions"],
            Self::RegionalEffects => vec!["regional_effects"],
            Self::Flavour => vec!["description"],
        }
    }

    pub(super) fn bindings(self) -> Vec<String> {
        self.data_keys()
            .into_iter()
            .map(|k| format!("data.{k}"))
            .collect()
    }

    /// A list of entries (traits, actions...): its problems point at the entry (`fields.actions.2`).
    pub(crate) fn is_entry_list(self) -> bool {
        matches!(
            self,
            Self::Traits | Self::Actions | Self::BonusActions | Self::Reactions
        )
    }

    /// Worked out from other parts when written, so it is never put back from its stored bytes.
    fn follows_others(self) -> bool {
        matches!(self, Self::StatBlock | Self::Senses)
    }
}

/// Read one field: its `data` keys, the value and the stored bytes (`{key: value}` of the keys the element has).
pub(super) fn read(field: CreatureField, r: &mut Reader<'_>) -> FieldMemo {
    let mut raw = Map::new();
    for k in field.data_keys() {
        if let Some(v) = r.take_data(k) {
            raw.insert(k.to_string(), v.clone());
        }
    }
    let data = &r.e.data;
    let get = |k: &str| raw.get(k).cloned();
    let or = |k: &str, none: Value| get(k).unwrap_or(none);
    let value = match field {
        CreatureField::StatBlock
        | CreatureField::SwarmOf
        | CreatureField::Type
        | CreatureField::Alignment
        | CreatureField::Flavour => json!(text_of(raw.get(field.data_keys()[0]))),
        CreatureField::Size
        | CreatureField::Tags
        | CreatureField::SkillNotes
        | CreatureField::Vulnerabilities
        | CreatureField::Resistances
        | CreatureField::Immunities
        | CreatureField::ConditionImmunities
        | CreatureField::Gear
        | CreatureField::Languages
        | CreatureField::Traits
        | CreatureField::Actions
        | CreatureField::BonusActions
        | CreatureField::Reactions => or(field.data_keys()[0], json!([])),
        CreatureField::Speed | CreatureField::Saves | CreatureField::Skills => {
            or(field.data_keys()[0], json!({}))
        }
        CreatureField::Initiative
        | CreatureField::Abilities
        | CreatureField::Telepathy
        | CreatureField::Legendary
        | CreatureField::LairActions
        | CreatureField::RegionalEffects => or(field.data_keys()[0], Value::Null),
        CreatureField::ArmorClass => json!({
            "entries": or("ac", json!([])),
            "note": text_of(raw.get("ac_note")),
        }),
        CreatureField::HitPoints => json!({
            "average": get("hp").and_then(|h| h.get("average").cloned()).unwrap_or(Value::Null),
            "formula": text_of(get("hp").as_ref().and_then(|h| h.get("formula"))),
            "note": text_of(raw.get("hp_note")),
        }),
        CreatureField::Senses => {
            let mut v = or("senses", json!({}));
            if let Some(m) = v.as_object_mut() {
                let usual = json!(usual_passive(data));
                let passive = m.entry("passive_perception").or_insert(Value::Null);
                if *passive == usual {
                    *passive = Value::Null;
                }
            }
            v
        }
        CreatureField::Challenge => {
            to_json(&ChallengeValue::read(&raw.clone().into_iter().collect()))
        }
    };
    FieldMemo {
        value,
        raw: Value::Object(raw),
        slots: Slots::default(),
    }
}

/// Write one field into `out` (fields write in form order: the scores and skills come before the senses).
pub(crate) fn write(
    field: CreatureField,
    value: &Value,
    memo: Option<&FieldMemo>,
    path: &str,
    out: &mut Out,
) -> Result<(), FormProblem> {
    if let Some(m) = memo.filter(|m| m.value == *value && !field.follows_others()) {
        if let Some(raw) = m.raw.as_object() {
            for (k, v) in raw {
                out.element.data.insert(k.clone(), v.clone());
            }
        }
        return Ok(());
    }
    let key = field.data_keys()[0];
    match field {
        CreatureField::StatBlock => {
            let ruleset = match out.element.ruleset {
                Some(r @ (Ruleset::Dnd2014 | Ruleset::Dnd2024)) => r,
                _ => match decode::<String>(value, path)?.as_str() {
                    "dnd2014" => Ruleset::Dnd2014,
                    _ => Ruleset::Dnd2024,
                },
            };
            put(out, key, &ruleset);
        }
        CreatureField::Size => {
            let sizes: Vec<CreatureSize> = decode(value, path)?;
            put(out, key, &sizes);
        }
        CreatureField::SwarmOf => {
            if let Some(t) = some_text(Some(decode(value, path)?)) {
                let size: CreatureSize = decode(&json!(t), path)?;
                put(out, key, &size);
            }
        }
        CreatureField::Type => match some_text(Some(decode(value, path)?)) {
            Some(t) => put(out, key, &decode::<CreatureType>(&json!(t), path)?),
            None => return Err(FormProblem::new(path, "Choose the creature type.")),
        },
        CreatureField::Alignment => {
            let t: String = decode(value, path)?;
            put(out, key, &t.trim());
        }
        CreatureField::Flavour => {
            let t: String = decode(value, path)?;
            if !t.trim().is_empty() {
                put(out, key, &t.trim_end());
            }
        }
        CreatureField::Tags
        | CreatureField::SkillNotes
        | CreatureField::ConditionImmunities
        | CreatureField::Gear
        | CreatureField::Languages => {
            put_list(out, key, &texts(&decode::<Vec<String>>(value, path)?))
        }
        CreatureField::Vulnerabilities | CreatureField::Resistances | CreatureField::Immunities => {
            let clauses: Vec<DamageClause> = decode(value, path)?;
            let clauses: Vec<DamageClause> = clauses
                .into_iter()
                .map(|c| DamageClause {
                    text: some_text(c.text),
                    ..c
                })
                .filter(|c| !c.types.is_empty() || c.text.is_some())
                .collect();
            put_list(out, key, &clauses);
        }
        CreatureField::Traits
        | CreatureField::Actions
        | CreatureField::BonusActions
        | CreatureField::Reactions => {
            let entries: Vec<Feature> = decode(value, path)?;
            let entries: Vec<Feature> = entries.into_iter().map(clean_feature).collect();
            put_list(out, key, &entries);
        }
        CreatureField::Speed => {
            let mut s: Speeds = decode(value, path)?;
            s.note = some_text(s.note);
            put(out, key, &s);
        }
        CreatureField::Initiative => {
            if let Some(i) = decode::<Option<Initiative>>(value, path)? {
                put(out, key, &i);
            }
        }
        CreatureField::Abilities => {
            let a: Abilities = decode(value, path)?;
            put(out, key, &a);
        }
        CreatureField::Saves => {
            let m: BTreeMap<Ability, i32> = decode(value, path)?;
            put_map(out, key, &m);
        }
        CreatureField::Skills => {
            let m: BTreeMap<Skill, i32> = decode(value, path)?;
            put_map(out, key, &m);
        }
        CreatureField::Telepathy => {
            if let Some(feet) = decode::<Option<u32>>(value, path)?.filter(|f| *f > 0) {
                put(out, key, &feet);
            }
        }
        CreatureField::Legendary => {
            if let Some(l) = decode::<Option<LegendaryActions>>(value, path)? {
                put(out, key, &clean_legendary(l));
            }
        }
        CreatureField::LairActions | CreatureField::RegionalEffects => {
            if let Some(s) = decode::<Option<LairSection>>(value, path)? {
                put(out, key, &clean_lair(s));
            }
        }
        CreatureField::ArmorClass => write_ac(&decode(value, path)?, out),
        CreatureField::HitPoints => write_hp(decode(value, path)?, path, out)?,
        CreatureField::Senses => {
            let s: SensesValue = decode(value, path)?;
            let usual = usual_passive(&out.element.data);
            put(out, key, &s.to_senses(usual));
        }
        CreatureField::Challenge => write_challenge(&decode(value, path)?, path, out)?,
    }
    Ok(())
}

fn put<T: Serialize + ?Sized>(out: &mut Out, key: &str, v: &T) {
    let v = serde_json::to_value(v).unwrap_or(Value::Null);
    out.element.data.insert(key.to_string(), v);
}

/// A list, left out when empty (the stat block skips empty lists).
fn put_list<T: Serialize>(out: &mut Out, key: &str, items: &[T]) {
    if !items.is_empty() {
        put(out, key, items);
    }
}

fn put_map<K: Serialize + Ord, V: Serialize>(out: &mut Out, key: &str, m: &BTreeMap<K, V>) {
    if !m.is_empty() {
        put(out, key, m);
    }
}

fn write_ac(v: &ArmorClassValue, out: &mut Out) {
    let entries: Vec<_> = v
        .entries
        .iter()
        .cloned()
        .map(|mut a| {
            a.source = some_text(a.source);
            a.condition = some_text(a.condition);
            a
        })
        .collect();
    put_list(out, "ac", &entries);
    if let Some(note) = some_text(Some(v.note.clone())) {
        put(out, "ac_note", &note);
    }
}

fn write_hp(v: HitPointsValue, path: &str, out: &mut Out) -> Result<(), FormProblem> {
    let formula = dice_text(&v.formula);
    if !formula.is_empty() && parse_dice(&formula).is_none() {
        return Err(FormProblem::new(
            path,
            "Write the hit dice as dice, like 2d8+2.",
        ));
    }
    let worked_out = (!formula.is_empty())
        .then(|| dice_average(&formula))
        .flatten()
        .and_then(|a| u32::try_from(a).ok());
    if let Some(average) = v.average.or(worked_out) {
        let hp = HitPoints {
            average,
            formula: (!formula.is_empty()).then_some(formula),
        };
        put(out, "hp", &hp);
    }
    if let Some(note) = some_text(Some(v.note)) {
        put(out, "hp_note", &note);
    }
    Ok(())
}

fn write_challenge(v: &ChallengeValue, path: &str, out: &mut Out) -> Result<(), FormProblem> {
    let text = v.cr.trim();
    let cr =
        if text.is_empty() {
            None
        } else {
            Some(Cr::parse(text).ok_or_else(|| {
                FormProblem::new(path, "Choose a challenge rating from the list.")
            })?)
        };
    match cr {
        Some(cr) => {
            put(out, "cr", &cr);
            put(out, "xp", &v.xp.unwrap_or_else(|| cr.standard_xp()));
            if let Some(lair) = v.xp_lair {
                put(out, "xp_lair", &lair);
            }
            let pb = v
                .proficiency_bonus
                .unwrap_or_else(|| cr.proficiency_bonus());
            put(out, "proficiency_bonus", &pb);
        }
        None => {
            if let Some(note) = some_text(Some(v.note.clone())) {
                put(out, "cr_note", &note);
            }
            if let Some(pb) = v.proficiency_bonus {
                put(out, "proficiency_bonus", &pb);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "creature_tests.rs"]
mod tests;
