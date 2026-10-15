//! Opt-in corrections of schema/id slips in a frozen `expected.json`.
//!
//! A case author can freeze an `expected.json` that is right about the source but wrong about the
//! engine's vocabulary: a class id that does not exist, or an empty `"type"` meaning "any type".
//! The frozen file stays untouched; a sibling `expected.corrections.json` lists, row by row, the
//! slip and the reason. The harness applies it only when `BOREALIS_HOMEBREW_CORRECTIONS=1`, so the
//! unmodified file always has an official score and the corrected one is a second number.
//!
//! Only two kinds of slip can be corrected, and each is checked against what the frozen file
//! says, so a correction can never change what the source is expected to contain:
//! * `engine_id`: `class_id` of a character, `ID_CLASS_<X>` -> `ID_WOTC_PHB_CLASS_<X>`.
//! * `empty_type`: `"type": ""` on an element row -> no type constraint (the key is removed).
//!
//! ```json
//! {"schema": 1, "corrections": [
//!   {"id": "<row or character id>", "kind": "empty_type", "reason": "..."},
//!   {"id": "<character id>", "kind": "engine_id", "to": "ID_WOTC_PHB_CLASS_X", "reason": "..."}
//! ]}
//! ```

use std::path::Path;

use serde::Deserialize;
use serde_json::Value;

/// Env var that switches corrections on (`1`).
pub const CORRECTIONS_ENV: &str = "BOREALIS_HOMEBREW_CORRECTIONS";
/// File next to `expected.json`.
pub const CORRECTIONS_FILE: &str = "expected.corrections.json";

const BARE_CLASS_PREFIX: &str = "ID_CLASS_";
const ENGINE_CLASS_PREFIX: &str = "ID_WOTC_PHB_CLASS_";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    schema: u32,
    corrections: Vec<Correction>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Correction {
    id: String,
    kind: Kind,
    /// New value (`engine_id` only).
    #[serde(default)]
    to: Option<String>,
    reason: String,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Kind {
    EngineId,
    EmptyType,
}

/// Is the opt-in switched on?
pub fn enabled() -> bool {
    std::env::var(CORRECTIONS_ENV).is_ok_and(|v| v == "1")
}

/// Apply `expected.corrections.json` (when present and enabled) to the raw `expected.json`.
/// Returns one line per applied correction.
pub fn apply_for(case_file: &Path, raw: &mut Value) -> Result<Vec<String>, String> {
    if !enabled() {
        return Ok(Vec::new());
    }
    let path = case_file.with_file_name(CORRECTIONS_FILE);
    if !path.is_file() {
        return Ok(Vec::new());
    }
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    apply(raw, &text).map_err(|e| format!("{CORRECTIONS_FILE}: {e}"))
}

/// Apply the corrections in `text` to `raw`.
pub fn apply(raw: &mut Value, text: &str) -> Result<Vec<String>, String> {
    let file: File = serde_json::from_str(text).map_err(|e| e.to_string())?;
    if file.schema != 1 {
        return Err(format!("schema {}, this harness reads 1", file.schema));
    }
    let mut applied = Vec::new();
    for c in &file.corrections {
        if c.reason.trim().is_empty() {
            return Err(format!("{}: a correction needs a reason", c.id));
        }
        apply_one(raw, c)?;
        applied.push(format!("{} ({:?}): {}", c.id, c.kind, c.reason));
    }
    Ok(applied)
}

fn apply_one(raw: &mut Value, c: &Correction) -> Result<(), String> {
    let section = match c.kind {
        Kind::EngineId => "characters",
        Kind::EmptyType => "elements",
    };
    let mut hits = raw
        .get_mut(section)
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
        .filter(|r| r.get("id").and_then(Value::as_str) == Some(c.id.as_str()));
    let (Some(row), None) = (hits.next(), hits.next()) else {
        return Err(format!("{}: not exactly one row in {section}", c.id));
    };
    match c.kind {
        Kind::EmptyType => {
            if row.get("type").and_then(Value::as_str) != Some("") {
                return Err(format!("{}: its type is not empty", c.id));
            }
            row.as_object_mut().map(|o| o.remove("type"));
        }
        Kind::EngineId => {
            let old = row.get("class_id").and_then(Value::as_str).unwrap_or("");
            let to = c.to.as_deref().unwrap_or("");
            let bare = old.strip_prefix(BARE_CLASS_PREFIX);
            if bare.is_none() || to.strip_prefix(ENGINE_CLASS_PREFIX) != bare {
                return Err(format!(
                    "{}: {old:?} -> {to:?} is not a {BARE_CLASS_PREFIX}<X> -> \
                     {ENGINE_CLASS_PREFIX}<X> fix",
                    c.id
                ));
            }
            row["class_id"] = Value::String(to.to_string());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn raw() -> Value {
        json!({
            "elements": [{"id": "e1", "type": "", "name": "X"}, {"id": "e2", "type": "Spell", "name": "Y"}],
            "characters": [{"id": "c1", "class_id": "ID_CLASS_WIZARD", "level": 1}]
        })
    }

    #[test]
    fn an_empty_type_becomes_no_type_and_an_engine_id_is_fixed() {
        let mut r = raw();
        let text = r#"{"schema":1,"corrections":[
            {"id":"e1","kind":"empty_type","reason":"empty means any"},
            {"id":"c1","kind":"engine_id","to":"ID_WOTC_PHB_CLASS_WIZARD","reason":"bundled id"}]}"#;
        let done = apply(&mut r, text).expect("applies");
        assert_eq!(done.len(), 2);
        assert!(r["elements"][0].get("type").is_none());
        assert_eq!(r["elements"][1]["type"], "Spell");
        assert_eq!(r["characters"][0]["class_id"], "ID_WOTC_PHB_CLASS_WIZARD");
    }

    #[test]
    fn a_correction_that_would_change_the_expectation_is_refused() {
        let cases = [
            // a typed row is not a slip
            r#"{"id":"e2","kind":"empty_type","reason":"r"}"#,
            // a different class is not the same class
            r#"{"id":"c1","kind":"engine_id","to":"ID_WOTC_PHB_CLASS_ROGUE","reason":"r"}"#,
            // not an id fix at all
            r#"{"id":"c1","kind":"engine_id","to":"ID_ANYTHING","reason":"r"}"#,
            // unknown row
            r#"{"id":"nope","kind":"empty_type","reason":"r"}"#,
            // no reason
            r#"{"id":"e1","kind":"empty_type","reason":" "}"#,
            // unknown key
            r#"{"id":"e1","kind":"empty_type","reason":"r","level":"should"}"#,
        ];
        for c in cases {
            let mut r = raw();
            let text = format!(r#"{{"schema":1,"corrections":[{c}]}}"#);
            assert!(apply(&mut r, &text).is_err(), "{c}");
            assert_eq!(r, raw(), "{c}: nothing changed");
        }
    }
}
