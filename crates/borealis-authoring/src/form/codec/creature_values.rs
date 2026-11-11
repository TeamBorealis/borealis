//! The creature form's value shapes (spec 4.11) that are not the stat block's own (`MonsterData`, `monster.rs`): the
//! parts a field binds to more than one `data` key, or works out from other parts.
//!
//! * **Armor class**: the entries (`data.ac`) and the note that stands in for them (`data.ac_note`, IM12a).
//! * **Hit points**: the average and the dice (`data.hp`), or the note that stands in for them (`data.hp_note`).
//! * **Senses**: `data.senses` with the passive Perception left out (`null`) while it is the usual 10 + the Perception
//!   bonus (or the Wisdom modifier).
//! * **Challenge**: the CR (`data.cr`) or the note that stands in for it (`data.cr_note`, IM12), the XP (`data.xp`,
//!   `null` while it is the CR's standard XP), the XP in its lair (`data.xp_lair`) and the proficiency bonus
//!   (`data.proficiency_bonus`, `null` while it follows the CR).
//!
//! Every other creature field holds the stat block's own shape of its key (`size`, `speed`, `traits`...).

use borealis_content::monster::{ArmorClass, Feature, LairSection, LegendaryActions, Senses};
use borealis_content::Cr;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// The armor class field: the entries, or a note that stands in for them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArmorClassValue {
    /// One per form (`13 (natural armor)`).
    #[serde(default)]
    pub entries: Vec<ArmorClass>,
    /// How it is worked out, when it has no number ("11 + the spell's level").
    #[serde(default)]
    pub note: String,
}

/// The hit points field: the average and the dice, or a note that stands in for them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HitPointsValue {
    /// The average (`None`: worked out from the dice, or none).
    #[serde(default)]
    pub average: Option<u32>,
    /// The hit dice (`2d8+2`).
    #[serde(default)]
    pub formula: String,
    /// How they are worked out, when they have no number.
    #[serde(default)]
    pub note: String,
}

/// The senses field: the stat block's senses, the passive Perception `None` while it is the usual one.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SensesValue {
    /// Blindsight range.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blindsight: Option<u32>,
    /// Darkvision range.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub darkvision: Option<u32>,
    /// Tremorsense range.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tremorsense: Option<u32>,
    /// Truesight range.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub truesight: Option<u32>,
    /// Reduced range while deafened.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deafened_range: Option<u32>,
    /// "blind beyond this radius".
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub blind_beyond: bool,
    /// Passive Perception; `None`: 10 + the Perception bonus (or the Wisdom modifier).
    #[serde(default)]
    pub passive_perception: Option<u32>,
    /// Printed remarks.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

impl SensesValue {
    /// The stat block's senses, with `passive` when the field leaves it to the usual one.
    pub(super) fn to_senses(&self, passive: u32) -> Senses {
        Senses {
            blindsight: self.blindsight,
            darkvision: self.darkvision,
            tremorsense: self.tremorsense,
            truesight: self.truesight,
            deafened_range: self.deafened_range,
            blind_beyond: self.blind_beyond,
            passive_perception: self.passive_perception.unwrap_or(passive),
            notes: texts(&self.notes),
        }
    }
}

/// The challenge field: the CR or the note that stands in for it, the XP and the proficiency bonus.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChallengeValue {
    /// `0`, `1/8`, `1/4`, `1/2`, `1` to `30`; empty for none.
    #[serde(default)]
    pub cr: String,
    /// What stands in for a missing CR ("Scales with the spell's level").
    #[serde(default)]
    pub note: String,
    /// The XP when it is not the CR's standard XP.
    #[serde(default)]
    pub xp: Option<u32>,
    /// The XP when fought in its lair.
    #[serde(default)]
    pub xp_lair: Option<u32>,
    /// The proficiency bonus when it does not follow the CR (or with no CR).
    #[serde(default)]
    pub proficiency_bonus: Option<u8>,
}

impl ChallengeValue {
    /// The field's value read from a stat block's `data`.
    pub(super) fn read(data: &BTreeMap<String, Value>) -> Self {
        let cr_value = data.get("cr");
        let cr = cr_value.and_then(|v| serde_json::from_value::<Cr>(v.clone()).ok());
        let number = |k: &str| data.get(k).and_then(Value::as_u64);
        let xp = number("xp").and_then(|x| u32::try_from(x).ok());
        let pb = number("proficiency_bonus").and_then(|x| u8::try_from(x).ok());
        Self {
            cr: match (cr, cr_value) {
                (Some(cr), _) => cr.as_text(),
                (None, Some(Value::String(t))) => t.clone(),
                (None, _) => String::new(),
            },
            note: text_of(data.get("cr_note")),
            xp: xp.filter(|x| cr.is_none_or(|cr| *x != cr.standard_xp())),
            xp_lair: number("xp_lair").and_then(|x| u32::try_from(x).ok()),
            proficiency_bonus: pb.filter(|p| cr.is_none_or(|cr| *p != cr.proficiency_bonus())),
        }
    }
}

/// A stored text, or empty.
pub(super) fn text_of(v: Option<&Value>) -> String {
    v.and_then(Value::as_str).unwrap_or_default().to_string()
}

/// Trimmed texts without the empty ones.
pub(super) fn texts(items: &[String]) -> Vec<String> {
    items
        .iter()
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .collect()
}

/// A trimmed text, `None` when it is empty.
pub(super) fn some_text(t: Option<String>) -> Option<String> {
    t.map(|t| t.trim().to_string()).filter(|t| !t.is_empty())
}

/// Dice as the stat block writes them: no spaces (`2d8 + 2` -> `2d8+2`).
pub(super) fn dice_text(t: &str) -> String {
    t.chars().filter(|c| !c.is_whitespace()).collect()
}

/// An entry with its empty optional texts left out (a blank "only in a form" or spell note).
pub(super) fn clean_feature(mut f: Feature) -> Feature {
    f.condition = some_text(f.condition);
    if let Some(a) = f.attack.as_mut() {
        for d in &mut a.damage {
            d.dice = some_text(d.dice.take()).map(|t| dice_text(&t));
        }
    }
    if let Some(s) = f.save.as_mut() {
        for d in &mut s.damage {
            d.dice = some_text(d.dice.take()).map(|t| dice_text(&t));
        }
    }
    if let Some(sc) = f.spellcasting.as_mut() {
        for list in &mut sc.lists {
            for spell in &mut list.spells {
                spell.id = some_text(spell.id.take());
                spell.note = some_text(spell.note.take());
            }
        }
    }
    f
}

/// Legendary actions with their entries cleaned.
pub(super) fn clean_legendary(mut l: LegendaryActions) -> LegendaryActions {
    l.actions = l.actions.into_iter().map(clean_feature).collect();
    l
}

/// Lair actions or regional effects with blank option names left out.
pub(super) fn clean_lair(mut s: LairSection) -> LairSection {
    for o in &mut s.options {
        o.name = some_text(o.name.take());
    }
    s
}

/// The usual passive Perception of a stat block's `data`: 10 + its Perception bonus, else its Wisdom modifier.
pub(super) fn usual_passive(data: &BTreeMap<String, Value>) -> u32 {
    let wis = data
        .get("abilities")
        .and_then(|a| a.get("wis"))
        .and_then(Value::as_i64)
        .unwrap_or(10);
    let perception = data
        .get("skills")
        .and_then(|s| s.get("perception"))
        .and_then(Value::as_i64)
        .unwrap_or_else(|| (wis - 10).div_euclid(2));
    u32::try_from((10 + perception).max(0)).unwrap_or(0)
}
