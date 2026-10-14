//! The resource pools of the whole pack, for every generation prompt: a feature that spends a
//! pool defined on another element (a sibling written in an earlier batch, or a class-table
//! counter added when the pack is assembled) can only reference it if it knows the id.

use serde_json::Value;

use super::types::{ImportDraft, ImportKind, PlannedElement};

/// Class-table columns that are never a pool of their own.
const NOT_POOLS: [&str; 9] = [
    "level",
    "proficiency",
    "features",
    "cantrips",
    "spells known",
    "spell slots",
    "slot level",
    "prepared",
    "spells",
];

fn words(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// A column holding a number per level, not a slot/known/feature column.
fn is_pool_column(name: &str, values: &[Value]) -> bool {
    let n = words(name);
    let numeric = values.iter().any(|v| {
        v.as_i64().is_some_and(|x| x > 0)
            || v.as_str()
                .and_then(|s| s.trim().trim_start_matches('+').parse::<i64>().ok())
                .is_some_and(|x| x > 0)
    });
    numeric
        && !n.is_empty()
        && !n.chars().next().is_some_and(|c| c.is_ascii_digit())
        && !NOT_POOLS.iter().any(|w| n.contains(w))
}

/// `(resource id, what defines it)` of every pool the pack has or will have, outside `batch`.
pub(crate) fn known_pools(draft: &ImportDraft, batch: &[PlannedElement]) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut add = |id: String, what: String| {
        if !out.iter().any(|(i, _)| *i == id) {
            out.push((id, what));
        }
    };
    for e in draft
        .elements
        .iter()
        .filter(|e| !e.json.is_null() && !batch.iter().any(|p| p.id == e.id))
    {
        for (id, name) in super::rules::resources_of(&e.json) {
            add(id, format!("\"{name}\" on \"{}\"", e.name));
        }
    }
    for g in draft
        .plan
        .groups
        .iter()
        .filter(|g| g.kind == ImportKind::Class)
    {
        let Some(o) = g.outline.as_ref() else {
            continue;
        };
        for c in o
            .columns
            .iter()
            .filter(|c| is_pool_column(&c.name, &c.values))
        {
            add(
                super::rules::resource_id(&words(&c.name)),
                format!("the \"{}\" column of the {} class table", c.name, o.name),
            );
        }
    }
    out
}

/// The prompt paragraph listing [`known_pools`] (empty when there are none), with the
/// convention for a pool defined by an element not written yet.
pub(crate) fn pools_note(draft: &ImportDraft, batch: &[PlannedElement]) -> String {
    let pools = known_pools(draft, batch);
    let convention = "A feature that spends or regains uses of a pool defined by ANOTHER element of this pack \
writes `resource_cost` / `resource_regain` with that pool's id; a pool not listed here (its element is not written \
yet) is referenced by its name in lowercase (e.g. \"ember points\"); it is linked when the pack is assembled. \
Never leave a cost out because the pool lives elsewhere, and never define another element's pool again.";
    if pools.is_empty() {
        return format!("\n\n{convention}");
    }
    let lines: Vec<String> = pools
        .iter()
        .map(|(id, what)| format!("- \"{id}\": {what}"))
        .collect();
    format!(
        "\n\nResource pools already in this pack (use these ids):\n{}\n{convention}",
        lines.join("\n")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn pools_of_written_siblings_and_class_columns_are_listed() {
        let mut draft: ImportDraft = serde_json::from_value(json!({"edition": "2014",
            "id_prefix": "t", "source_name": "T", "pack_id": "t", "pack_name": "T",
            "source_kind": "text", "source_text": "",
            "detection": {"kind": "class", "edition": "2014", "confidence": 1.0, "items": []},
            "plan": {"edition": "2014", "elements": [], "notes": [], "groups": [
                {"kind": "class", "name": "Drifter", "outline": {"name": "Drifter", "features": [],
                 "columns": [
                    {"name": "Proficiency Bonus", "values": ["+2", "+2"]},
                    {"name": "Spell Slots", "values": [1, 2]},
                    {"name": "Dust Points", "values": [0, 2]},
                    {"name": "Features", "values": ["Dust", "-"]}]}}]},
            "elements": [], "sweep_issues": []}))
        .unwrap();
        let written: super::super::types::DraftElement = serde_json::from_value(json!({
            "id": "ID_T_PART_SCROLLS", "name": "Scroll Satchel", "ty": "Class Feature",
            "role": "part", "level": null, "status": "ok", "issues": [], "excerpt": null,
            "balance": null, "repair_rounds": 0, "revisions": 0,
            "json": {"rules": [{"kind": "resource", "id": "scroll satchel", "name": "Scroll Satchel"}]}}))
        .unwrap();
        draft.elements.push(written);
        let pools = known_pools(&draft, &[]);
        let ids: Vec<&str> = pools.iter().map(|(i, _)| i.as_str()).collect();
        assert_eq!(ids, vec!["scroll satchel", "dust points"]);
        let note = pools_note(&draft, &[]);
        assert!(note.contains("\"scroll satchel\": \"Scroll Satchel\" on \"Scroll Satchel\""));
        assert!(note.contains("referenced by its name in lowercase"));
    }
}
