//! The numbers the Play tab rolls against (GitHub #1, #14): ability check bonuses, attack to-hit,
//! the spell attack roll, the die of a resource and the hit dice. Each carries the 2024 Exhaustion
//! penalty to D20 Tests (`d20 tests`) where the engine has not already put it in, so the UI only
//! adds a d20 to a snapshot number and never applies a rule itself.

use crate::dto::AbilityRecord;
use borealis_engine::snapshot::{CharacterSnapshot as Engine, SpellEntry};
use serde::Serialize;
use serde_json::Value;

/// Faces a resource die may have.
const DIE_SIZES: [i32; 6] = [4, 6, 8, 10, 12, 20];
/// Stat suffixes that name the die of a pool (`bardic inspiration die`, `bardic inspiration:die`,
/// and the Aurora forms such as `song-of-rest:dice:size`).
const DIE_SUFFIXES: [&str; 5] = [" die", ":die", ":dice", ":size", ":dice:size"];
/// Ability index of Constitution.
const CON: usize = 2;

fn stat(s: &Engine, name: &str) -> i32 {
    s.stats.get(name).map_or(0, |g| g.value)
}

/// The modifier to every D20 Test (zero or negative: 2024 Exhaustion).
#[must_use]
pub fn d20_tests(s: &Engine) -> i32 {
    stat(s, borealis_engine::play_rules::D20_GROUP)
}

/// Ability check bonus per ability: the modifier, bonuses to every ability check and the D20 Test
/// modifier (skills, saves and initiative already carry both).
#[must_use]
pub fn ability_checks(s: &Engine) -> AbilityRecord<i32> {
    let extra = stat(s, borealis_engine::stats::ABILITY_CHECKS_GROUP) + d20_tests(s);
    AbilityRecord::from_fn(|i| s.abilities.get(i).map_or(0, |a| a.modifier) + extra)
}

/// The to-hit of a typed attack bonus (`+5`, `5`, `-1`) with the D20 Test modifier; `None` for
/// anything else (`DC 13 DEX`, free text).
#[must_use]
pub fn manual_to_hit(text: &str, d20: i32) -> Option<i32> {
    let t = text.trim();
    let digits = t.strip_prefix('+').unwrap_or(t);
    if digits.is_empty() || digits.starts_with('+') {
        return None;
    }
    digits.parse::<i32>().ok().map(|n| n.saturating_add(d20))
}

fn normalized(name: &str) -> String {
    name.trim().to_lowercase().replace(['-', '_'], " ")
}

/// The die one use of a pool rolls, from a stat named after its id or name (`1d6`).
#[must_use]
pub fn resource_die(s: &Engine, id: &str, name: &str) -> Option<String> {
    let bases = [normalized(id), normalized(name)];
    let faces = s.stats.iter().find_map(|(k, g)| {
        let key = normalized(k);
        let hit = bases
            .iter()
            .any(|b| !b.is_empty() && DIE_SUFFIXES.iter().any(|suf| key == format!("{b}{suf}")));
        (hit && DIE_SIZES.contains(&g.value)).then_some(g.value)
    })?;
    Some(format!("1d{faces}"))
}

/// The engine's resources with a `die` on each (null when the pool rolls none).
#[must_use]
pub fn resources_with_dice(s: &Engine, resources: Value) -> Value {
    let Value::Array(rows) = resources else {
        return resources;
    };
    Value::Array(
        rows.into_iter()
            .map(|row| {
                let Value::Object(mut m) = row else {
                    return row;
                };
                let id = m.get("id").and_then(Value::as_str).unwrap_or("");
                let name = m.get("name").and_then(Value::as_str).unwrap_or("");
                let die = resource_die(s, id, name).map_or(Value::Null, Value::String);
                m.insert("die".into(), die);
                Value::Object(m)
            })
            .collect(),
    )
}

/// The hit dice of one class.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HitDiceDto {
    pub class_name: String,
    /// Faces of the die.
    pub die: u32,
    /// Dice the class gives (its level).
    pub total: u32,
    /// What spending one die rolls: the die plus the Constitution modifier (`1d8+2`).
    pub notation: String,
}

/// Hit dice per class with a hit die.
#[must_use]
pub fn hit_dice(s: &Engine) -> Vec<HitDiceDto> {
    let con = s.abilities.get(CON).map_or(0, |a| a.modifier);
    let bonus = match con {
        0 => String::new(),
        n => format!("{n:+}"),
    };
    s.classes
        .iter()
        .filter_map(|c| {
            let die = c.hit_die.filter(|d| *d > 0)?;
            let class = c.class.as_ref()?;
            (c.level > 0).then(|| HitDiceDto {
                class_name: class.name.clone(),
                die,
                total: c.level,
                notation: format!("1d{die}{bonus}"),
            })
        })
        .collect()
}

/// A spell the character can cast now: cantrips; a known caster's known spells; otherwise the
/// prepared and always-prepared ones (a spellbook holds more than the wizard has ready).
#[must_use]
pub fn spell_ready(model: &str, s: &SpellEntry) -> bool {
    s.level == 0 || s.prepared || s.always_prepared || (model == "known" && s.known)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manual_bonuses_parse_with_their_sign() {
        assert_eq!(manual_to_hit("+5", 0), Some(5));
        assert_eq!(manual_to_hit(" 5 ", -2), Some(3));
        assert_eq!(manual_to_hit("-1", 0), Some(-1));
        assert_eq!(manual_to_hit("DC 13 DEX", 0), None);
        assert_eq!(manual_to_hit("++5", 0), None);
        assert_eq!(manual_to_hit("", 0), None);
        assert_eq!(manual_to_hit("+", 0), None);
    }

    #[test]
    fn names_are_compared_without_case_or_dashes() {
        assert_eq!(normalized("Bardic-Inspiration"), "bardic inspiration");
        assert_eq!(normalized(" Rune_Dice "), "rune dice");
    }
}
