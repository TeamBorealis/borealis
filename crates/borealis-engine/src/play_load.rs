//! Lenient loading of `build.play` (GitHub #2 review L1/L2/M2). A wrong value in one entry, such
//! as a negative count, an unknown spell level or a broken tracker, drops or repairs that entry
//! with a warning; it never stops the character loading. Keys this version does not know are kept
//! in `extra`, so a newer version's data survives a save here.

use crate::commands::{MAX_TRACKERS, MAX_TRACKER_MAX, MAX_TRACKER_NAME};
use crate::state::{CustomTracker, PlayState, TrackerReset};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};

/// Prefix of the ids the tracker commands give.
pub const TRACKER_ID_PREFIX: &str = "tracker-";
/// Longest tracker id kept as written; a longer one gets a new id.
const MAX_TRACKER_ID: usize = 64;
/// Name of a tracker whose saved name was empty.
const FALLBACK_NAME: &str = "Tracker";

impl TrackerReset {
    /// The saved spelling.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::ShortRest => "short_rest",
            Self::LongRest => "long_rest",
            Self::Never => "never",
            Self::Other(s) => s,
        }
    }
}

impl Serialize for TrackerReset {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for TrackerReset {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(match String::deserialize(d)?.as_str() {
            "short_rest" => Self::ShortRest,
            "long_rest" => Self::LongRest,
            "never" => Self::Never,
            other => Self::Other(other.to_string()),
        })
    }
}

/// Bidirectional formatting and zero-width characters, which could make a name display reversed
/// or hide text (review L4).
fn is_hidden_format(c: char) -> bool {
    matches!(c, '\u{061C}' | '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2060}'..='\u{2069}' | '\u{FEFF}')
}

/// A tracker name without bidi or zero-width characters, trimmed.
#[must_use]
pub fn clean_tracker_name(name: &str) -> String {
    name.chars()
        .filter(|c| !is_hidden_format(*c))
        .collect::<String>()
        .trim()
        .to_string()
}

/// A new `tracker-<n>` id not used by `trackers`: one above the highest number in use, or the
/// lowest free number when that would overflow (a hand-edited file).
#[must_use]
pub fn next_tracker_id(trackers: &[CustomTracker]) -> String {
    let used: BTreeSet<u32> = trackers
        .iter()
        .filter_map(|t| t.id.strip_prefix(TRACKER_ID_PREFIX)?.parse::<u32>().ok())
        .collect();
    let n = match used.last() {
        None => 1,
        Some(top) => top
            .checked_add(1)
            .unwrap_or_else(|| (1..).find(|n| !used.contains(n)).unwrap_or(1)),
    };
    let id = format!("{TRACKER_ID_PREFIX}{n}");
    debug_assert!(!trackers.iter().any(|t| t.id == id));
    id
}

fn typed<T: DeserializeOwned + Default>(key: &str, v: Value) -> T {
    serde_json::from_value(v).unwrap_or_else(|e| {
        tracing::warn!("character play state: dropped `{key}`: {e}");
        T::default()
    })
}

/// `spell_slots_used`: spell levels 1..=9 with a count above 0; anything else is dropped.
fn slot_counts(v: Value) -> BTreeMap<u32, u32> {
    let Value::Object(map) = v else {
        tracing::warn!("character play state: dropped `spell_slots_used` (not a map)");
        return BTreeMap::new();
    };
    let mut out = BTreeMap::new();
    for (k, n) in map {
        let level = k.parse::<u32>().ok().filter(|l| (1..=9).contains(l));
        let count = n.as_u64().and_then(|n| u32::try_from(n).ok());
        match (level, count) {
            (Some(l), Some(c)) if c > 0 => {
                out.insert(l, c);
            }
            (Some(_), Some(_)) => {}
            _ => tracing::warn!("character play state: dropped spell slot entry `{k}`"),
        }
    }
    out
}

/// One saved tracker, repaired: a name without hidden characters (at most
/// [`MAX_TRACKER_NAME`] characters), a maximum in 1..=[`MAX_TRACKER_MAX`] and a used count at
/// most the maximum. `None` when it is not a tracker at all.
fn tracker(v: Value) -> Option<CustomTracker> {
    let mut t: CustomTracker = match serde_json::from_value(v) {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!("character play state: dropped a tracker: {e}");
            return None;
        }
    };
    let name: String = clean_tracker_name(&t.name)
        .chars()
        .filter(|c| !c.is_control())
        .take(MAX_TRACKER_NAME)
        .collect();
    t.name = if name.trim().is_empty() {
        FALLBACK_NAME.to_string()
    } else {
        name.trim().to_string()
    };
    t.max = t.max.clamp(1, MAX_TRACKER_MAX);
    t.used = t.used.min(t.max);
    Some(t)
}

/// `trackers`: broken entries dropped, the rest repaired; empty, overlong or repeated ids get new
/// ones; at most [`MAX_TRACKERS`] are kept.
fn trackers(v: Value) -> Vec<CustomTracker> {
    let Value::Array(items) = v else {
        tracing::warn!("character play state: dropped `trackers` (not a list)");
        return Vec::new();
    };
    if items.len() > MAX_TRACKERS {
        tracing::warn!(
            "character play state: kept the first {MAX_TRACKERS} of {} trackers",
            items.len()
        );
    }
    let mut out: Vec<CustomTracker> = Vec::new();
    for t in items.into_iter().take(MAX_TRACKERS).filter_map(tracker) {
        let bad_id = t.id.is_empty()
            || t.id.chars().count() > MAX_TRACKER_ID
            || out.iter().any(|o| o.id == t.id);
        let t = if bad_id {
            tracing::warn!("character play state: gave a tracker a new id");
            CustomTracker {
                id: next_tracker_id(&out),
                ..t
            }
        } else {
            t
        };
        out.push(t);
    }
    out
}

/// The play state in `v`, read leniently (see the module docs).
#[must_use]
pub fn play_from_value(v: Value) -> PlayState {
    let map = match v {
        Value::Object(m) => m,
        Value::Null => Map::new(),
        _ => {
            tracing::warn!("character play state: not a map, ignored");
            Map::new()
        }
    };
    let mut p = PlayState::default();
    for (k, v) in map {
        match k.as_str() {
            "resources_used" => p.resources_used = typed(&k, v),
            "toggles" => p.toggles = typed(&k, v),
            "exhaustion" => p.exhaustion = typed(&k, v),
            "heroic_inspiration" => p.heroic_inspiration = typed(&k, v),
            "wild_shape_forms" => p.wild_shape_forms = typed(&k, v),
            "equipment_choices" => p.equipment_choices = typed(&k, v),
            "spell_slots_used" => p.spell_slots_used = slot_counts(v),
            "pact_slots_used" => p.pact_slots_used = typed(&k, v),
            "concentration" => p.concentration = typed(&k, v),
            "trackers" => p.trackers = trackers(v),
            _ => {
                p.extra.insert(k, v);
            }
        }
    }
    p
}
