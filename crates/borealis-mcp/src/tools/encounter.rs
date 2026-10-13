//! `encounter_evaluate`.

use borealis_content::MonsterData;
use borealis_encounter::{evaluate_encounter, Cr, Ruleset};
use serde::Deserialize;
use serde_json::{json, Value};

use super::{parse_args, parse_ruleset, Tools};

const MAX_PARTY: usize = 20;
const MAX_GROUPS: usize = 40;
const METHOD: &str = "srd-5.2.1-budget";
const NOTE_2014: &str = "Rated with the SRD 5.2.1 XP-budget method; the classic 2014 DMG method needs user-supplied tables.";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Group {
    id: Option<String>,
    cr: Option<Value>,
    count: Option<u32>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Args {
    party_levels: Vec<u8>,
    monsters: Vec<Group>,
    ruleset: String,
}

fn resolve_cr(tools: &Tools, group: &Group) -> Result<Cr, String> {
    if let Some(id) = &group.id {
        let element = tools
            .store
            .element(id)
            .ok_or_else(|| format!("no monster with id {id:?}"))?;
        let data = MonsterData::from_element(element)
            .map_err(|e| format!("{id} is not a readable monster: {e}"))?;
        let as_json = serde_json::to_value(data.cr).map_err(|e| e.to_string())?;
        return serde_json::from_value(as_json).map_err(|e| format!("{id}: {e}"));
    }
    let cr = group
        .cr
        .clone()
        .ok_or("each monster needs an `id` or a `cr`")?;
    // Accept both "1/4" and plain numbers.
    let cr = match cr {
        Value::Number(n) => Value::String(n.to_string()),
        other => other,
    };
    serde_json::from_value(cr).map_err(|e| format!("invalid cr: {e}"))
}

pub fn run(tools: &Tools, args: &Value) -> Result<Value, String> {
    let a: Args = parse_args(args)?;
    if a.party_levels.is_empty() || a.party_levels.len() > MAX_PARTY {
        return Err(format!("party_levels needs 1 to {MAX_PARTY} entries"));
    }
    if a.monsters.is_empty() || a.monsters.len() > MAX_GROUPS {
        return Err(format!("monsters needs 1 to {MAX_GROUPS} groups"));
    }
    // Both editions use the SRD 5.2.1 XP-budget method by default.
    let (ruleset, note) = match parse_ruleset(&a.ruleset)? {
        borealis_content::Ruleset::Dnd2014 => (Ruleset::Edition2024, Some(NOTE_2014)),
        borealis_content::Ruleset::Dnd2024 => (Ruleset::Edition2024, None),
        borealis_content::Ruleset::Any => return Err("ruleset must be 2014 or 2024".into()),
    };
    let mut groups = Vec::with_capacity(a.monsters.len());
    for g in &a.monsters {
        groups.push((resolve_cr(tools, g)?, g.count.unwrap_or(1)));
    }
    let report =
        evaluate_encounter(ruleset, &a.party_levels, &groups).map_err(|e| e.to_string())?;
    let mut value = serde_json::to_value(&report).map_err(|e| e.to_string())?;
    if let Some(obj) = value.as_object_mut() {
        obj.insert("method".into(), json!(METHOD));
        if let Some(note) = note {
            obj.insert("note".into(), json!(note));
        }
        obj.insert(
            "monsters".into(),
            json!(groups
                .iter()
                .map(|(cr, n)| json!({"cr": cr.to_string(), "count": n}))
                .collect::<Vec<_>>()),
        );
    }
    Ok(value)
}
