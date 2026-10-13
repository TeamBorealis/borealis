//! `list_core_ids`.

use serde::Deserialize;
use serde_json::{json, Value};

use borealis_content::Element;

use super::{parse_args, parse_ruleset, Tools};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Args {
    category: String,
    ruleset: Option<String>,
}

fn matches(category: &str, e: &Element) -> bool {
    match category {
        "skills" => {
            e.ty == "Proficiency"
                && e.supports.iter().any(|s| s == "Skill")
                && e.id.starts_with("ID_PROFICIENCY_SKILL_")
        }
        "abilities" => e.ty == "Ability Score Improvement" && e.id.starts_with("ID_INTERNAL_ASI_"),
        "proficiencies" => e.ty == "Proficiency",
        "languages" => e.ty == "Language",
        "damage_types" => e.ty == "Damage Type",
        "conditions" => e.ty == "Condition" && !e.id.starts_with("ID_INTERNAL"),
        "sizes" => e.ty == "Size",
        _ => false,
    }
}

pub fn run(tools: &Tools, args: &Value) -> Result<Value, String> {
    let a: Args = parse_args(args)?;
    if !matches!(
        a.category.as_str(),
        "skills"
            | "abilities"
            | "proficiencies"
            | "languages"
            | "damage_types"
            | "conditions"
            | "sizes"
    ) {
        return Err(format!("unknown category {:?}", a.category));
    }
    let ruleset = a.ruleset.as_deref().map(parse_ruleset).transpose()?;
    let mut rows: Vec<Value> = tools
        .store
        .iter()
        .map(|s| &s.element)
        .filter(|e| matches(&a.category, e))
        .filter(|e| match (ruleset, e.ruleset) {
            (Some(want), Some(have)) => have.compatible_with(want),
            _ => true,
        })
        .map(|e| json!([e.id, e.name]))
        .collect();
    rows.sort_by_key(Value::to_string);
    Ok(json!({"category": a.category, "count": rows.len(), "ids": rows}))
}
