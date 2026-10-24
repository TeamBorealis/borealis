//! Opt-in corrections of schema/id slips in a frozen `expected.json`.
//!
//! A case author can freeze an `expected.json` that is right about the source but wrong about the
//! engine's vocabulary: a class id that does not exist, or an empty `"type"` meaning "any type".
//! The frozen file stays untouched; a sibling `expected.corrections.json` lists, row by row, the
//! slip and the reason. The harness applies it only when `BOREALIS_HOMEBREW_CORRECTIONS=1`, so the
//! unmodified file always has an official score and the corrected one is a second number.
//!
//! Only four kinds of slip can be corrected, and each is checked against what the frozen file
//! says, so a correction can never change what the source is expected to contain:
//! * `engine_id`: `class_id` of a character, `ID_CLASS_<X>` -> `ID_WOTC_PHB_CLASS_<X>`.
//! * `empty_type`: `"type": ""` on an element row -> no type constraint (the key is removed).
//! * `author_missed_printed`: the author missed content that is printed (stat blocks set as
//!   images). It may only relax a `counts` row that has a `max` (`"max"` in the correction = the
//!   new, larger limit) or drop an `absent` row, and needs an `evidence` string (page + what was
//!   seen).
//! * `text_only_ok_accepts_text`: an `elements` row already marked `text_only_ok` also accepts
//!   the text-entry (glossary) element type. Nothing else about the row changes.
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

/// Element type of the importer's text-only entries (accepted by `text_only_ok_accepts_text`).
const TEXT_ENTRY_TYPE: &str = "Rules Glossary Entry";
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
    /// New, larger limit (`author_missed_printed` on a count row).
    #[serde(default)]
    max: Option<usize>,
    /// Page and what was seen (`author_missed_printed` only, mandatory there).
    #[serde(default)]
    evidence: Option<String>,
    reason: String,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Kind {
    EngineId,
    EmptyType,
    AuthorMissedPrinted,
    TextOnlyOkAcceptsText,
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
    if c.kind == Kind::AuthorMissedPrinted && c.evidence.as_deref().is_none_or(str_blank) {
        return Err(format!("{}: author_missed_printed needs evidence", c.id));
    }
    let section = match c.kind {
        Kind::EngineId => "characters",
        Kind::EmptyType | Kind::TextOnlyOkAcceptsText => "elements",
        Kind::AuthorMissedPrinted if has_row(raw, "counts", &c.id) => "counts",
        Kind::AuthorMissedPrinted => "absent",
    };
    if c.kind == Kind::AuthorMissedPrinted && section == "absent" {
        return drop_absent_row(raw, c);
    }
    let Some(row) = one_row(raw, section, &c.id) else {
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
        Kind::AuthorMissedPrinted => relax_max(row, c)?,
        Kind::TextOnlyOkAcceptsText => accept_text_entry(row, &c.id)?,
    }
    Ok(())
}

fn str_blank(s: &str) -> bool {
    s.trim().is_empty()
}

fn has_row(raw: &Value, section: &str, id: &str) -> bool {
    raw.get(section)
        .and_then(Value::as_array)
        .is_some_and(|rows| {
            rows.iter()
                .any(|r| r.get("id").and_then(Value::as_str) == Some(id))
        })
}

fn one_row<'a>(raw: &'a mut Value, section: &str, id: &str) -> Option<&'a mut Value> {
    let mut hits = raw
        .get_mut(section)
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
        .filter(|r| r.get("id").and_then(Value::as_str) == Some(id));
    match (hits.next(), hits.next()) {
        (Some(row), None) => Some(row),
        _ => None,
    }
}

/// `author_missed_printed` on a count row: only a larger `max`, no `equals` / `min` involved.
fn relax_max(row: &mut Value, c: &Correction) -> Result<(), String> {
    let old = row.get("max").and_then(Value::as_u64);
    let (Some(old), Some(new)) = (old, c.max) else {
        return Err(format!(
            "{}: needs a count row with a max and a new max",
            c.id
        ));
    };
    if row.get("equals").is_some() || (new as u64) <= old {
        return Err(format!("{}: {new} does not relax max {old}", c.id));
    }
    row["max"] = Value::from(new);
    Ok(())
}

/// `author_missed_printed` on an absent row: the row is dropped (one exact row required).
fn drop_absent_row(raw: &mut Value, c: &Correction) -> Result<(), String> {
    let rows = raw.get_mut("absent").and_then(Value::as_array_mut);
    let Some(rows) = rows else {
        return Err(format!("{}: not exactly one row in absent", c.id));
    };
    let hits = |r: &Value| r.get("id").and_then(Value::as_str) == Some(c.id.as_str());
    if rows.iter().filter(|r| hits(r)).count() != 1 {
        return Err(format!("{}: not exactly one row in absent", c.id));
    }
    rows.retain(|r| !hits(r));
    Ok(())
}

/// `text_only_ok_accepts_text`: add the text-entry type to a typed `text_only_ok` row.
fn accept_text_entry(row: &mut Value, id: &str) -> Result<(), String> {
    if row.get("class").and_then(Value::as_str) != Some("text_only_ok") {
        return Err(format!("{id}: not a text_only_ok row"));
    }
    let Some(ty) = row
        .get("type")
        .and_then(Value::as_str)
        .filter(|t| !t.is_empty())
    else {
        return Err(format!("{id}: the row has no type list to extend"));
    };
    if ty.split('|').any(|t| t == TEXT_ENTRY_TYPE) {
        return Err(format!("{id}: already accepts {TEXT_ENTRY_TYPE}"));
    }
    row["type"] = Value::String(format!("{ty}|{TEXT_ENTRY_TYPE}"));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn raw() -> Value {
        json!({
            "elements": [{"id": "e1", "type": "", "name": "X"}, {"id": "e2", "type": "Spell", "name": "Y"},
                {"id": "e3", "type": "Option|Rule", "class": "text_only_ok", "name": "W"},
                {"id": "e4", "type": "Option", "class": "mechanised", "name": "V"}],
            "characters": [{"id": "c1", "class_id": "ID_CLASS_WIZARD", "level": 1}],
            "counts": [{"id": "n1", "type": "Monster", "max": 1}, {"id": "n2", "type": "Spell", "equals": 0}],
            "absent": [{"id": "a1", "type": "Monster", "name": "Z"}],
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
            // printed-content corrections without evidence, or that tighten / change a count
            r#"{"id":"n1","kind":"author_missed_printed","max":4,"reason":"r"}"#,
            r#"{"id":"n1","kind":"author_missed_printed","max":4,"evidence":" ","reason":"r"}"#,
            r#"{"id":"n1","kind":"author_missed_printed","max":1,"evidence":"p1","reason":"r"}"#,
            r#"{"id":"n1","kind":"author_missed_printed","evidence":"p1","reason":"r"}"#,
            r#"{"id":"n2","kind":"author_missed_printed","max":3,"evidence":"p1","reason":"r"}"#,
            r#"{"id":"e1","kind":"author_missed_printed","evidence":"p1","reason":"r"}"#,
            // text entries only for rows marked text_only_ok
            r#"{"id":"e4","kind":"text_only_ok_accepts_text","reason":"r"}"#,
            r#"{"id":"e2","kind":"text_only_ok_accepts_text","reason":"r"}"#,
            r#"{"id":"e3","kind":"text_only_ok_accepts_text","to":"x","extra":1,"reason":"r"}"#,
            // unknown kind
            r#"{"id":"e3","kind":"anything_goes","reason":"r"}"#,
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

    #[test]
    fn printed_content_and_text_entry_corrections_apply_and_only_relax() {
        let mut r = raw();
        let text = r#"{"schema":1,"corrections":[
            {"id":"n1","kind":"author_missed_printed","max":4,"evidence":"p2: stat block image","reason":"r"},
            {"id":"a1","kind":"author_missed_printed","evidence":"p3: stat block image","reason":"r"},
            {"id":"e3","kind":"text_only_ok_accepts_text","reason":"r"}]}"#;
        assert_eq!(apply(&mut r, text).expect("applies").len(), 3);
        assert_eq!(r["counts"][0]["max"], 4);
        assert_eq!(r["counts"][1]["equals"], 0);
        assert!(r["absent"].as_array().expect("array").is_empty());
        assert_eq!(r["elements"][2]["type"], "Option|Rule|Rules Glossary Entry");
        assert_eq!(r["elements"][2]["class"], "text_only_ok");
        assert_eq!(r["elements"][3]["type"], "Option");
    }
}
