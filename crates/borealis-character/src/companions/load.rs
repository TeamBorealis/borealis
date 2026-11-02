//! Lenient loading of the `companions` block (see the parent module): a broken entry is dropped or
//! repaired and never stops the character loading; keys this version does not know are kept.

use super::{
    next_id, AddedCompanion, CompanionEdits, CompanionTrait, Companions, MAX_ADDED_COMPANIONS,
    MAX_COMPANION_BONUS, MAX_COMPANION_NAME, MAX_COMPANION_NOTES, MAX_COMPANION_TRAITS, MAX_ID,
    MAX_TRAIT_TEXT,
};
use serde::{Deserialize, Deserializer};
use serde_json::{Map, Value};
use std::collections::BTreeSet;

fn take_string(map: &mut Map<String, Value>, key: &str, max: usize) -> String {
    match map.remove(key) {
        Some(Value::String(s)) => s.chars().take(max).collect(),
        _ => String::new(),
    }
}

fn take_bonus(map: &mut Map<String, Value>, key: &str) -> i32 {
    map.remove(key).and_then(|v| v.as_i64()).map_or(0, |n| {
        let limit = i64::from(MAX_COMPANION_BONUS);
        i32::try_from(n.clamp(-limit, limit)).unwrap_or(0)
    })
}

fn trait_from(v: Value) -> Option<CompanionTrait> {
    let Value::Object(mut map) = v else {
        return None;
    };
    let name = take_string(&mut map, "name", MAX_COMPANION_NAME);
    let name = name.trim().to_string();
    (!name.is_empty()).then(|| CompanionTrait {
        name,
        text: take_string(&mut map, "text", MAX_TRAIT_TEXT),
    })
}

/// Takes the edit keys out of `map`; whatever is left belongs to the caller.
fn edits_from(map: &mut Map<String, Value>) -> CompanionEdits {
    let traits = match map.remove("traits") {
        Some(Value::Array(list)) => list
            .into_iter()
            .filter_map(trait_from)
            .take(MAX_COMPANION_TRAITS)
            .collect(),
        _ => Vec::new(),
    };
    CompanionEdits {
        notes: take_string(map, "notes", MAX_COMPANION_NOTES),
        traits,
        armor_class_bonus: take_bonus(map, "armor_class_bonus"),
        hit_points_bonus: take_bonus(map, "hit_points_bonus"),
        speed_bonus: take_bonus(map, "speed_bonus"),
    }
}

fn added_from(v: Value) -> Option<AddedCompanion> {
    let Value::Object(mut map) = v else {
        return None;
    };
    let id = take_string(&mut map, "id", MAX_ID + 1);
    let portrait = match map.remove("portrait") {
        Some(Value::String(s)) if s.starts_with("data:image/") => Some(s),
        _ => None,
    };
    Some(AddedCompanion {
        id,
        creature: take_string(&mut map, "creature", MAX_ID * 4),
        name: take_string(&mut map, "name", MAX_COMPANION_NAME),
        portrait,
        edits: edits_from(&mut map),
        extra: map.into_iter().collect(),
    })
}

/// Gives every companion with a missing, overlong or repeated id a new one.
fn with_unique_ids(list: Vec<AddedCompanion>) -> Vec<AddedCompanion> {
    let mut seen = BTreeSet::new();
    let mut out: Vec<AddedCompanion> = Vec::with_capacity(list.len());
    let mut broken = Vec::new();
    for c in list {
        let ok = !c.id.is_empty() && c.id.len() <= MAX_ID && seen.insert(c.id.clone());
        if ok {
            out.push(c);
        } else {
            broken.push((out.len() + broken.len(), c));
        }
    }
    for (at, c) in broken {
        let id = next_id(&out);
        out.insert(at.min(out.len()), AddedCompanion { id, ..c });
    }
    out
}

impl Companions {
    /// Read leniently from a saved value (see the module notes).
    #[must_use]
    pub fn from_value(v: Value) -> Self {
        let Value::Object(mut map) = v else {
            return Self::default();
        };
        let granted = match map.remove("granted") {
            Some(Value::Object(mut g)) => edits_from(&mut g),
            _ => CompanionEdits::default(),
        };
        let added = match map.remove("added") {
            Some(Value::Array(list)) => with_unique_ids(
                list.into_iter()
                    .filter_map(added_from)
                    .take(MAX_ADDED_COMPANIONS)
                    .collect(),
            ),
            _ => Vec::new(),
        };
        Self {
            granted,
            added,
            extra: map.into_iter().collect(),
        }
    }
}

impl<'de> Deserialize<'de> for Companions {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(Self::from_value(Value::deserialize(d)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_hostile_block_loads_repaired() {
        let v = json!({
            "granted": {"notes": 7, "traits": [{"name": ""}, {"name": "Keen"}, 3], "speed_bonus": 5000},
            "added": [
                {"id": "companion-2", "name": "A", "portrait": "javascript:x"},
                {"id": "companion-2", "name": "B"},
                {"name": "C", "hit_points_bonus": "lots"},
                "junk",
                {"id": "companion-7", "future": {"x": 1}}
            ],
            "later": true
        });
        let c = Companions::from_value(v);
        assert_eq!(c.granted.notes, "");
        assert_eq!(c.granted.traits.len(), 1);
        assert_eq!(c.granted.speed_bonus, MAX_COMPANION_BONUS);
        let ids: Vec<_> = c.added.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(ids.len(), 4);
        assert_eq!(ids.iter().collect::<BTreeSet<_>>().len(), 4, "{ids:?}");
        assert_eq!(c.added[0].portrait, None);
        assert_eq!(c.added[3].extra["future"], json!({"x": 1}));
        assert_eq!(c.extra["later"], json!(true));
        assert_eq!(Companions::from_value(json!(12)), Companions::default());
    }
}
