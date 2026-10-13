//! `get_element`.

use serde::Deserialize;
use serde_json::{json, Value};

use super::{parse_args, plain_text, Tools};

const DESCRIPTION_CHARS: usize = 4_000;
const SUGGESTIONS: usize = 5;
/// Fields dropped from the model-facing view (presentation only).
const TRIMMED: &[&str] = &["sheet", "compendium", "source_ref", "extra"];

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Args {
    id: String,
}

pub fn run(tools: &Tools, args: &Value) -> Result<Value, String> {
    let a: Args = parse_args(args)?;
    let Some(element) = tools.store.element(&a.id) else {
        let needle = a.id.to_uppercase();
        let close: Vec<&str> = tools
            .store
            .iter()
            .map(|s| s.element.id.as_str())
            .filter(|id| id.contains(&needle) || needle.contains(id))
            .take(SUGGESTIONS)
            .collect();
        return Err(format!(
            "no element with id {:?}; similar ids: {close:?}",
            a.id
        ));
    };
    let mut value = serde_json::to_value(element).map_err(|e| e.to_string())?;
    if let Some(obj) = value.as_object_mut() {
        for key in TRIMMED {
            obj.remove(*key);
        }
        obj.insert(
            "description".into(),
            json!(plain_text(&element.description, DESCRIPTION_CHARS)),
        );
    }
    Ok(value)
}
