//! Try it's answers (spec 9 "Output"): the trial character's snapshot (the same `CharacterSnapshot` the app shows,
//! with the draft's own choices locked), what the draft changed, and the 1-20 level ladder.

use crate::dto::CharacterSnapshot;
use crate::homebrew_dto::{HomebrewLadderRow, HomebrewProblem};
use crate::homebrew_trial_build::{Note, Placement, TrialDraft, TEST_NAME};
use crate::settings::AppSettings;
use crate::snapshot_map::{build_snapshot, MapInput};
use crate::state::ContentHub;
use borealis_authoring::check::SweepIssue;
use borealis_character::Character;
use borealis_engine::snapshot::CharacterSnapshot as EngineSnapshot;
use borealis_engine::{CharacterState, Severity};
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};

/// Problem codes of Try it (the editor's own problems keep theirs).
const NOTE_CODE: &str = "trial";
const ENGINE_CODE: &str = "trial.engine";
const SWEEP_CODE: &str = "trial.sweep";
/// Spell slot levels on the ladder.
const SLOT_LEVELS: usize = 9;
const ABILITIES: [&str; 6] = ["str", "dex", "con", "int", "wis", "cha"];
/// Numbers of the snapshot whose change is marked: (JSON pointer, path the UI matches).
const VALUES: [(&str, &str); 6] = [
    ("/stats/armorClass", "stats.armorClass"),
    ("/stats/speed", "stats.speed"),
    ("/stats/hpMax", "stats.hpMax"),
    ("/stats/initiative", "stats.initiative"),
    ("/stats/proficiencyBonus", "stats.proficiencyBonus"),
    ("/stats/passivePerception", "stats.passivePerception"),
];

/// The trial character's snapshot. A choice that holds the draft is locked: SETUP names the draft, and the Build
/// pickers hide locked slots (E31 SO-3).
pub(crate) fn snapshot(
    hub: &ContentHub,
    settings: &AppSettings,
    trial_id: &str,
    state: &CharacterState,
    draft: &TrialDraft,
) -> CharacterSnapshot {
    let mut character = Character::with_ruleset(TEST_NAME, state.ruleset);
    character.build = state.clone();
    let mut snap = build_snapshot(&MapInput {
        id: trial_id,
        character: &character,
        dirty: false,
        display_path: String::new(),
        hub,
        settings,
        rolls: &[],
        warnings: &[],
    });
    for slot in &mut snap.slots {
        if slot.chosen.as_ref().is_some_and(|c| draft.is_draft(&c.id)) {
            slot.locked = true;
        }
    }
    snap
}

/// The entries of a list in the snapshot JSON, by name (a text entry is its own name).
fn entries<'a>(items: impl Iterator<Item = &'a Value>) -> BTreeMap<String, &'a Value> {
    items
        .filter_map(|v| {
            let name = v
                .as_str()
                .or_else(|| v.get("name").and_then(Value::as_str))?;
            Some((name.to_string(), v))
        })
        .collect()
}

fn array<'a>(v: &'a Value, pointer: &str) -> impl Iterator<Item = &'a Value> {
    v.pointer(pointer)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
}

/// Every list whose entries are compared: (path the UI matches, entries).
fn lists(v: &Value) -> Vec<(&'static str, BTreeMap<String, &Value>)> {
    let proficiencies = array(v, "/stats/proficiencies").flat_map(|g| array(g, "/values"));
    let spells = array(v, "/spellcasting/classes").flat_map(|c| array(c, "/spells"));
    vec![
        ("stats.skills", entries(array(v, "/stats/skills"))),
        ("stats.proficiencies", entries(proficiencies)),
        ("stats.languages", entries(array(v, "/stats/languages"))),
        ("play.features", entries(array(v, "/play/features"))),
        ("spellcasting.spells", entries(spells)),
        ("attacks", entries(array(v, "/attacks"))),
        ("play.resources", entries(array(v, "/play/resources"))),
        ("companions", entries(array(v, "/companions"))),
    ]
}

fn slot_totals(v: &Value) -> BTreeMap<u64, &Value> {
    array(v, "/spellcasting/slots")
        .filter_map(|r| Some((r.get("level")?.as_u64()?, r.get("total")?)))
        .collect()
}

/// What the draft changed (`HomebrewTrialView.changed`): the trial's snapshot compared with the same character
/// derived without the draft. A number is its path (`stats.speed`, `abilities.final.dex`, `spellcasting.slots.3`); a
/// list entry the draft adds or changes is `<list>:<name>` (`play.features:Lucky`).
pub(crate) fn changed(trial: &CharacterSnapshot, without: &CharacterSnapshot) -> Vec<String> {
    match (serde_json::to_value(trial), serde_json::to_value(without)) {
        (Ok(a), Ok(b)) => changed_values(&a, &b),
        _ => Vec::new(),
    }
}

/// [`changed`] on the snapshots' JSON.
fn changed_values(a: &Value, b: &Value) -> Vec<String> {
    let mut out: Vec<String> = VALUES
        .iter()
        .filter(|(pointer, _)| a.pointer(pointer) != b.pointer(pointer))
        .map(|(_, path)| (*path).to_string())
        .collect();
    for group in ["final", "save"] {
        for k in ABILITIES {
            let pointer = format!("/abilities/{group}/{k}");
            if a.pointer(&pointer) != b.pointer(&pointer) {
                out.push(format!("abilities.{group}.{k}"));
            }
        }
    }
    let (slots_a, slots_b) = (slot_totals(a), slot_totals(b));
    let levels: std::collections::BTreeSet<u64> =
        slots_a.keys().chain(slots_b.keys()).copied().collect();
    out.extend(
        levels
            .into_iter()
            .filter(|l| slots_a.get(l) != slots_b.get(l))
            .map(|l| format!("spellcasting.slots.{l}")),
    );
    for ((list, mine), (_, theirs)) in lists(a).into_iter().zip(lists(b)) {
        out.extend(
            mine.iter()
                .filter(|(name, v)| theirs.get(*name) != Some(*v))
                .map(|(name, _)| format!("{list}:{name}")),
        );
    }
    out
}

/// Spell slots per slot level 1-9 (the pooled multiclass table when there is one).
fn spell_slots(snap: &EngineSnapshot) -> Vec<u32> {
    let mut sum = [0_i32; SLOT_LEVELS];
    match &snap.multiclass_slots {
        Some(m) => sum = m.slots,
        None => {
            for c in &snap.spellcasting {
                for (t, n) in sum.iter_mut().zip(c.slots) {
                    *t += n;
                }
            }
        }
    }
    sum.iter().map(|n| u32::try_from(*n).unwrap_or(0)).collect()
}

fn problem(
    severity: &str,
    message: impl Into<String>,
    code: &str,
    element: Option<&str>,
) -> HomebrewProblem {
    HomebrewProblem {
        severity: severity.to_string(),
        message: message.into(),
        path: None,
        element_id: element.map(str::to_string),
        code: code.to_string(),
    }
}

fn severity_word(s: Severity) -> Option<&'static str> {
    match s {
        Severity::Error => Some("error"),
        Severity::Warning => Some("warning"),
        Severity::Info => None,
    }
}

/// Is the draft on this level's character?
fn has_draft(state: &CharacterState, snap: &EngineSnapshot, draft: &TrialDraft) -> bool {
    if draft.placement == Placement::Item {
        return state
            .inventory
            .items
            .iter()
            .any(|i| i.item == draft.root.id);
    }
    snap.elements.iter().any(|e| draft.is_draft(&e.element.id))
}

/// The ladder (spec 9): per level the features gained, the new choices, the spell slots and the problems that show
/// first at that level (the build's notes, the sweep's issues, the engine's errors and warnings about the draft), and
/// the first level the draft is on the character.
pub(crate) fn ladder(
    hub: &ContentHub,
    states: &[CharacterState],
    draft: &TrialDraft,
    notes: &[Note],
    sweep: &[SweepIssue],
) -> (Vec<HomebrewLadderRow>, Option<u32>) {
    let snaps: Vec<EngineSnapshot> = states
        .iter()
        .map(|s| hub.engine.derive_with(s, false))
        .collect();
    let root = Some(draft.root.id.as_str());
    let mut seen: HashSet<(Option<String>, String)> = HashSet::new();
    let mut placed_at = None;
    let mut rows = Vec::with_capacity(snaps.len());
    for (i, snap) in snaps.iter().enumerate() {
        let level = u32::try_from(i + 1).unwrap_or(u32::MAX);
        let prev = i.checked_sub(1).map(|j| &snaps[j]);
        let had: HashSet<&str> = prev
            .map(|p| p.features.iter().map(|f| f.id.as_str()).collect())
            .unwrap_or_default();
        let mut features: Vec<String> = snap
            .features
            .iter()
            .filter(|f| !had.contains(f.id.as_str()))
            .map(|f| f.name.clone())
            .collect();
        if placed_at.is_none() && has_draft(&states[i], snap, draft) {
            placed_at = Some(level);
            if !features.iter().any(|f| f == draft.name()) {
                features.push(draft.name().to_string());
            }
        }
        let keys: HashSet<_> = prev
            .map(|p| p.slots.iter().map(|s| s.key.clone()).collect())
            .unwrap_or_default();
        let mut choices: Vec<String> = Vec::new();
        for s in snap
            .slots
            .iter()
            .filter(|s| !s.locked && !keys.contains(&s.key))
        {
            if !choices.contains(&s.name) {
                choices.push(s.name.clone());
            }
        }
        let mut problems: Vec<HomebrewProblem> = notes
            .iter()
            .filter(|n| n.level == level)
            .map(|n| problem(n.severity, n.message.clone(), NOTE_CODE, root))
            .collect();
        problems.extend(sweep.iter().filter(|s| s.level.max(1) == level).map(|s| {
            problem(
                "error",
                s.message.clone(),
                SWEEP_CODE,
                s.element_id.as_deref(),
            )
        }));
        for d in &snap.diagnostics {
            let (Some(word), Some(el)) = (severity_word(d.severity), d.element.as_deref()) else {
                continue;
            };
            if draft.is_draft(el) && seen.insert((d.element.clone(), d.message.clone())) {
                problems.push(problem(word, d.message.clone(), ENGINE_CODE, Some(el)));
            }
        }
        rows.push(HomebrewLadderRow {
            level,
            features,
            choices,
            slots: spell_slots(snap),
            problems,
        });
    }
    (rows, placed_at)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sheet(speed: i32, dex: i32, features: &[&str], slots: &[(u32, i32)]) -> Value {
        json!({
            "stats": {"armorClass": 12, "speed": speed, "hpMax": 10, "initiative": 1, "proficiencyBonus": 2,
                      "passivePerception": 10, "skills": [{"name": "Stealth", "bonus": dex}],
                      "languages": ["Common"], "proficiencies": [{"label": "Armor", "values": ["Light Armor"]}]},
            "abilities": {"final": {"str": 10, "dex": dex, "con": 10, "int": 10, "wis": 10, "cha": 10},
                          "save": {"str": {"bonus": 0}, "dex": {"bonus": 0}, "con": {"bonus": 0},
                                   "int": {"bonus": 0}, "wis": {"bonus": 0}, "cha": {"bonus": 0}}},
            "play": {"features": features.iter().map(|f| json!({"name": f, "level": 1})).collect::<Vec<_>>(),
                     "resources": []},
            "spellcasting": {"classes": [], "slots": slots.iter().map(|(l, t)| json!({"level": l, "total": t})).collect::<Vec<_>>()},
            "attacks": [], "companions": []
        })
    }

    #[test]
    fn changed_names_numbers_and_list_entries_the_draft_moved() {
        let without = sheet(30, 12, &["Second Wind"], &[(1, 2)]);
        let same = changed_values(&without, &without);
        assert!(same.is_empty(), "{same:?}");
        let with = sheet(35, 14, &["Second Wind", "Fleet Step"], &[(1, 2), (2, 1)]);
        let got = changed_values(&with, &without);
        assert_eq!(
            got,
            vec![
                "stats.speed",
                "abilities.final.dex",
                "spellcasting.slots.2",
                "stats.skills:Stealth",
                "play.features:Fleet Step",
            ]
        );
    }
}
