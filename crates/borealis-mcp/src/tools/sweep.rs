//! `class_sweep`: level a test character 1 to 20 with the engine.

use borealis_content::{ContentStore, Ruleset};
use borealis_engine::commands::{self, slot_key};
use borealis_engine::{CharacterState, Choice, Engine, Severity};
use serde::Deserialize;
use serde_json::{json, Value};

use super::overlay::{build, new_issues, normalize, summarize, Budget};
use super::{parse_args, parse_ruleset, Tools};

const MAX_LEVEL: u32 = 20;
const MAX_ERRORS_PER_LEVEL: usize = 3;
const LEVEL_ONE_OWNER: &str = "ID_LEVEL_1";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Args {
    class_json_bundle: Value,
    edition: String,
    subclass: Option<String>,
}

fn bundle_class_id(doc: &Value) -> Option<String> {
    doc["elements"]
        .as_array()?
        .iter()
        .find(|e| e["type"] == "Class")
        .and_then(|e| e["id"].as_str())
        .map(String::from)
}

fn resolve_subclass(
    store: &ContentStore,
    wanted: &str,
    ruleset: Ruleset,
) -> Result<String, String> {
    let lower = wanted.to_lowercase();
    store
        .by_type("Archetype")
        .map(|s| &s.element)
        .filter(|e| e.ruleset.map_or(true, |r| r.compatible_with(ruleset)))
        .find(|e| e.id == wanted || e.name.to_lowercase() == lower)
        .map(|e| e.id.clone())
        .ok_or_else(|| format!("no subclass (Archetype) {wanted:?} in the loaded content"))
}

/// Pick the subclass the first time a slot offers it.
fn pick_subclass(engine: &Engine, state: CharacterState, subclass: Option<&str>) -> CharacterState {
    let Some(id) = subclass else { return state };
    let snap = engine.derive(&state);
    let Some(slot) = snap
        .slots
        .iter()
        .find(|s| s.chosen.is_none() && s.options.iter().any(|o| o.id == id))
    else {
        return state;
    };
    commands::select(engine, &state, &slot.key, Choice::Element(id.to_string()))
        .map_or(state, |o| o.state)
}

pub fn run(tools: &Tools, args: &Value) -> Result<Value, String> {
    let budget = Budget::start();
    let a: Args = parse_args(args)?;
    let ruleset = parse_ruleset(&a.edition)?;
    if ruleset == Ruleset::Any {
        return Err("edition must be 2014 or 2024".into());
    }
    let doc = normalize(&a.class_json_bundle)?;
    let class_id = bundle_class_id(&doc).ok_or("the bundle contains no element of type Class")?;
    let store = build(tools, &doc)?;
    budget.check()?;
    let subclass = a
        .subclass
        .as_deref()
        .map(|s| resolve_subclass(&store, s, ruleset))
        .transpose()?;
    let load_report = summarize(new_issues(tools, &store));

    let engine = Engine::new(&store);
    let mut state = commands::new_character(&engine, ruleset);
    let snap = engine.derive(&state);
    let key = slot_key(&snap, LEVEL_ONE_OWNER, "Class", 1)
        .ok_or("the engine offers no level 1 class slot for this edition")?;
    state = commands::select(&engine, &state, &key, Choice::Element(class_id.clone()))
        .map_err(|e| format!("cannot choose {class_id} at level 1: {e}"))?
        .state;

    let mut levels = Vec::new();
    let mut failures = 0usize;
    for level in 1..=MAX_LEVEL {
        budget.check()?;
        if level > 1 {
            match commands::level_up_main(&engine, &state, None) {
                Ok(o) => state = o.state,
                Err(e) => {
                    levels
                        .push(json!({"level": level, "errors": [format!("level up failed: {e}")]}));
                    failures += 1;
                    break;
                }
            }
        }
        state = pick_subclass(&engine, state, subclass.as_deref());
        let snap = engine.derive(&state);
        let errors: Vec<String> = snap
            .diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .map(|d| format!("{}: {}", d.code, d.message))
            .collect();
        failures += errors.len();
        let open = snap
            .slots
            .iter()
            .filter(|s| {
                s.requires_selection && !s.locked && s.chosen.is_none() && s.list_choice.is_none()
            })
            .count();
        let mut row = json!({
            "level": snap.level, "hp": snap.hit_points, "prof": snap.proficiency_bonus,
            "ac": snap.armor_class, "features": snap.elements.len(), "open_choices": open,
        });
        if !errors.is_empty() {
            row["errors"] = json!(errors.iter().take(MAX_ERRORS_PER_LEVEL).collect::<Vec<_>>());
            row["error_count"] = json!(errors.len());
        }
        levels.push(row);
    }
    let reached = levels
        .iter()
        .filter(|r| r.get("errors").is_none() || r["hp"].is_number())
        .count();
    let passed = failures == 0 && reached as u32 == MAX_LEVEL && load_report["errors"] == 0;
    Ok(json!({
        "passed": passed, "class": class_id, "subclass": subclass,
        "edition": ruleset.as_str(), "levels_reached": reached,
        "engine_errors": failures, "load": load_report, "levels": levels,
    }))
}
