//! Class-table data the outline extracts and the plan maps deterministically, the way the
//! bundled packs model it:
//!
//! * a feature's spell table (subclass spells and the like): one grant per spell at its class
//!   level, always prepared (2014 `grant` of type Spell with `spellcasting` and `prepared`, 2024
//!   `grant_spell` on the class's list), resolved against this import and the library;
//! * the class table's other columns (a points pool, uses, a die size): written into
//!   `data.progression`, where the derived pass turns numeric ones into counters.
//!
//! The model's spell table is checked against the printed one (see `feature_spells_text.rs`):
//! a member it dropped or misspelled, and a cantrip the feature names, are granted too.

use std::collections::HashMap;

use borealis_content::ContentStore;
use serde_json::{json, Value};

use super::fewshot::in_edition;
use super::types::{
    Edition, ImportPlan, OutlineFeature, PlannedElement, Role, SpellGrant, TableColumn,
};

#[path = "feature_spells_text.rs"]
mod text;
pub(crate) use text::SpellBook;
use text::{key, level_rows, members, named_cantrips, own_lines};

/// `SpellGrant::level` of a cantrip the feature names: known from the feature on, no class
/// level of its own.
const CANTRIP: u32 = 0;
/// A printed table lists at least 1 in this many of the outline's spells to be that feature's
/// table (half of them).
const SAME_TABLE_SHARE: usize = 2;

/// Levels of a class table.
const TABLE_ROWS: usize = 20;
/// Column headings (normalised words) the caster builder or the plan already covers.
const COVERED: [&str; 8] = [
    "level",
    "proficiency",
    "features",
    "cantrip",
    "spells known",
    "prepared",
    "slot",
    "spell level",
];

fn norm(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn push_grant(out: &mut Vec<SpellGrant>, level: u32, name: &str, class: &str) {
    if out.iter().any(|g| key(&g.name) == key(name)) {
        return;
    }
    out.push(SpellGrant {
        level,
        name: name.to_string(),
        id: None,
        class: class.to_string(),
    });
}

/// The grants of feature `f`'s spell table, ids not yet resolved. A model-written name that is
/// no spell as a whole but a list of them ("a and b") or one with a mark is read as the spells
/// it names; any other name is kept as written (resolving notes it).
pub(crate) fn unresolved(f: &OutlineFeature, class: &str, book: &SpellBook) -> Vec<SpellGrant> {
    let mut out: Vec<SpellGrant> = Vec::new();
    for row in &f.spells {
        let level = row.level.max(f.level).max(1);
        for name in row
            .spells
            .iter()
            .map(|n| n.trim())
            .filter(|n| !n.is_empty())
        {
            let found = match book.find(name) {
                Some((n, _)) => vec![n.clone()],
                None => members(name, book).into_iter().map(|(n, _)| n).collect(),
            };
            if found.is_empty() {
                push_grant(&mut out, level, name, class);
            }
            for n in found {
                push_grant(&mut out, level, &n, class);
            }
        }
    }
    out
}

/// The grants feature `p` (outlined as `f`) prints that `have` lacks: the members of its spell
/// table's rows (when the outline lists a table and the printed one shares at least half of its
/// spells, see [`SAME_TABLE_SHARE`]: the same table), and the cantrips it says you gain (when
/// the outline speaks of a cantrip). Only names of spells the import or the library has.
pub(crate) fn printed_grants(
    source: &str,
    p: &PlannedElement,
    f: &OutlineFeature,
    class: &str,
    book: &SpellBook,
    have: &[SpellGrant],
) -> Vec<SpellGrant> {
    let lines = own_lines(source, p);
    let mut out: Vec<SpellGrant> = Vec::new();
    let known =
        |n: &str, out: &[SpellGrant]| have.iter().chain(out).any(|g| key(&g.name) == key(n));
    if !have.is_empty() {
        let rows: Vec<(u32, Vec<String>)> = level_rows(&lines)
            .into_iter()
            .map(|(l, cells)| {
                (
                    l,
                    members(&cells, book).into_iter().map(|(n, _)| n).collect(),
                )
            })
            .collect();
        let printed: Vec<&String> = rows.iter().flat_map(|(_, n)| n).collect();
        let shared = have
            .iter()
            .filter(|g| printed.iter().any(|n| key(n) == key(&g.name)))
            .count();
        let same = shared > 0 && shared * SAME_TABLE_SHARE >= have.len();
        if same {
            for (level, names) in &rows {
                for n in names {
                    if !known(n, &out) {
                        push_grant(&mut out, (*level).max(f.level).max(1), n, class);
                    }
                }
            }
        }
    }
    let speaks = |t: &str| t.to_lowercase().contains("cantrip");
    if speaks(&f.name) || speaks(&f.summary) || speaks(&f.quote) {
        for list in named_cantrips(&lines) {
            for (n, level) in members(&list, book) {
                if matches!(level, Some(0) | None) && !known(&n, &out) {
                    push_grant(&mut out, CANTRIP, &n, class);
                }
            }
        }
    }
    out
}

/// Resolve every planned spell grant to a spell of this import or of the library (same
/// edition). Returns plan notes for the spells neither has.
pub(crate) fn resolve(
    out: &mut [PlannedElement],
    store: &ContentStore,
    edition: Edition,
) -> Vec<String> {
    if out.iter().all(|e| e.spells.is_empty()) {
        return Vec::new();
    }
    let mut ids: HashMap<String, String> = store
        .by_type("Spell")
        .map(|s| &s.element)
        .filter(|e| in_edition(e, edition))
        .map(|e| (key(&e.name), e.id.clone()))
        .collect();
    // The import's own spells win over a library spell of the same name.
    ids.extend(
        out.iter()
            .filter(|e| e.ty == "Spell")
            .map(|e| (key(&e.name), e.id.clone())),
    );
    let mut notes = Vec::new();
    for e in out.iter_mut().filter(|e| !e.spells.is_empty()) {
        let mut missing = Vec::new();
        for g in &mut e.spells {
            g.id = ids.get(&key(&g.name)).cloned();
            if g.id.is_none() {
                missing.push(g.name.clone());
            }
        }
        if !missing.is_empty() {
            notes.push(format!(
                "{}: {} spell(s) of its spell table are neither in the library nor in this import, so they are not granted: {}",
                e.name,
                missing.len(),
                missing.join(", ")
            ));
        }
    }
    notes
}

fn is_spell_grant_of(r: &Value, ids: &[&str]) -> bool {
    let kind = r.get("kind").and_then(Value::as_str);
    let id = match kind {
        Some("grant") if r.get("type").and_then(Value::as_str) == Some("Spell") => r.get("id"),
        Some("grant_spell") => r.get("spell"),
        _ => None,
    };
    id.and_then(Value::as_str).is_some_and(|i| ids.contains(&i))
}

/// Write the planned spell grants into `el` (replacing the model's grants of the same spells).
pub(crate) fn grant_spells(el: &mut Value, planned: &PlannedElement, edition: Edition) {
    let grants: Vec<&SpellGrant> = planned.spells.iter().filter(|g| g.id.is_some()).collect();
    if grants.is_empty() {
        return;
    }
    let ids: Vec<&str> = grants.iter().filter_map(|g| g.id.as_deref()).collect();
    let Some(o) = el.as_object_mut() else { return };
    let rules = o.entry("rules").or_insert_with(|| json!([]));
    if !rules.is_array() {
        *rules = json!([]);
    }
    let Some(rules) = rules.as_array_mut() else {
        return;
    };
    rules.retain(|r| !is_spell_grant_of(r, &ids));
    rules.extend(grants.iter().map(|g| match (edition, g.level == CANTRIP) {
        // A named cantrip: known from the feature on, as the bundled species grant theirs.
        (Edition::Y2014, true) => {
            json!({"kind": "grant", "id": g.id, "type": "Spell", "spellcasting": g.class})
        }
        (Edition::Y2024, true) => json!({"kind": "grant_spell", "spell": g.id,
            "mode": "known_cantrip", "spell_list": g.class}),
        (Edition::Y2014, false) => json!({"kind": "grant", "id": g.id, "type": "Spell",
            "level": g.level, "spellcasting": g.class, "prepared": true}),
        (Edition::Y2024, false) => json!({"kind": "grant_spell", "level": g.level,
            "spell": g.id, "mode": "always_prepared", "counts_against_prepared": false,
            "spell_list": g.class}),
    }));
}

/// `data.progression` key of a column heading.
fn column_key(name: &str) -> String {
    norm(name).replace(' ', "_")
}

fn is_covered(name: &str) -> bool {
    let n = norm(name);
    n.is_empty()
        || n.chars().next().is_some_and(|c| c.is_ascii_digit())
        || COVERED.iter().any(|w| n == *w || n.contains(w))
}

/// A table cell as the bundled packs store it: integers as numbers, a dash as 0, anything
/// else (a die, "+10 ft.") as the printed string.
fn cell(v: &Value) -> Value {
    match v {
        Value::Number(_) => v.clone(),
        Value::String(s) => {
            let t = s.trim();
            if t.is_empty() || t.chars().all(|c| matches!(c, '-' | '—' | '–')) {
                json!(0)
            } else if let Ok(n) = t.replace(',', "").parse::<i64>() {
                json!(n)
            } else {
                json!(t)
            }
        }
        Value::Null => json!(0),
        other => json!(other.to_string()),
    }
}

/// The outline's class-table columns, as `(key, 20 values)`, without the covered ones.
pub(crate) fn columns(cols: &[TableColumn]) -> Vec<(String, Vec<Value>)> {
    cols.iter()
        .filter(|c| !is_covered(&c.name) && c.values.len() == TABLE_ROWS)
        .map(|c| (column_key(&c.name), c.values.iter().map(cell).collect()))
        .collect()
}

/// A class root gets the table columns of its outline the model left out.
pub(crate) fn table_columns(el: &mut Value, planned: &PlannedElement, plan: &ImportPlan) {
    if planned.role != Role::Class {
        return;
    }
    let Some(cols) = plan
        .groups
        .iter()
        .find(|g| g.root_id.as_deref() == Some(planned.id.as_str()))
        .and_then(|g| g.outline.as_ref())
        .map(|o| columns(&o.columns))
        .filter(|c| !c.is_empty())
    else {
        return;
    };
    let Some(o) = el.as_object_mut() else { return };
    let data = o.entry("data").or_insert_with(|| json!({}));
    if !data.is_object() {
        *data = json!({});
    }
    let prog = &mut data["progression"];
    if !prog.is_object() {
        *prog = json!({});
    }
    let Some(prog) = prog.as_object_mut() else {
        return;
    };
    for (key, values) in cols {
        prog.entry(key).or_insert_with(|| json!(values));
    }
}

#[cfg(test)]
mod tests {
    use super::super::types::LeveledSpells;
    use super::*;

    #[test]
    fn columns_keep_the_bundled_cell_shapes_and_skip_covered_ones() {
        let col = |name: &str, v: Vec<Value>| TableColumn {
            name: name.into(),
            values: v,
        };
        let mut pool: Vec<Value> = vec![json!("—")];
        pool.extend((2..=20).map(|l| json!(l.to_string())));
        let dice: Vec<Value> = (1..=20).map(|_| json!("1d6")).collect();
        let got = columns(&[
            col("Moss Points", pool),
            col("Ward Die", dice.clone()),
            col("Cantrips Known", dice.clone()),
            col("1st", dice.clone()),
            col("Short", vec![json!(1)]),
        ]);
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].0, "moss_points");
        assert_eq!(got[0].1[0], json!(0));
        assert_eq!(got[0].1[19], json!(20));
        assert_eq!(
            (got[1].0.as_str(), &got[1].1[0]),
            ("ward_die", &json!("1d6"))
        );
    }

    #[test]
    fn spell_grants_take_the_bundled_shape_and_replace_the_models() {
        let f = OutlineFeature {
            level: 3,
            name: "Hollow Spells".into(),
            summary: String::new(),
            quote: String::new(),
            parts: Vec::new(),
            options: Vec::new(),
            choose: Vec::new(),
            spells: vec![LeveledSpells {
                level: 5,
                spells: vec!["Mist Step".into(), " ".into()],
            }],
        };
        let mut grants = unresolved(&f, "Hedgekeeper", &SpellBook::of(&[]));
        assert_eq!(grants.len(), 1);
        grants[0].id = Some("ID_X_MIST_STEP".into());
        let planned = PlannedElement {
            id: "ID_F".into(),
            ty: "Archetype Feature".into(),
            name: "Hollow Spells".into(),
            role: Role::SubclassFeature,
            level: Some(3),
            owner: None,
            brief: String::new(),
            excerpt: None,
            parent: None,
            picks: None,
            prerequisite_level: None,
            spells: grants,
        };
        let mut el = json!({"rules": [{"kind": "grant", "type": "Spell", "id": "ID_X_MIST_STEP"}]});
        grant_spells(&mut el, &planned, Edition::Y2014);
        grant_spells(&mut el, &planned, Edition::Y2014);
        let rules = el["rules"].as_array().unwrap();
        assert_eq!(rules.len(), 1, "{rules:?}");
        assert_eq!(rules[0]["level"], 5);
        assert_eq!(rules[0]["spellcasting"], "Hedgekeeper");
        let mut el = json!({});
        grant_spells(&mut el, &planned, Edition::Y2024);
        assert_eq!(el["rules"][0]["kind"], "grant_spell");
        assert_eq!(el["rules"][0]["spell_list"], "Hedgekeeper");
        // A named cantrip is known from the feature on: no class level, not "prepared".
        let mut cantrip = planned;
        cantrip.spells[0].level = CANTRIP;
        let mut el = json!({});
        grant_spells(&mut el, &cantrip, Edition::Y2014);
        assert_eq!(
            el["rules"][0],
            json!({"kind": "grant", "id": "ID_X_MIST_STEP", "type": "Spell", "spellcasting": "Hedgekeeper"})
        );
        let mut el = json!({});
        grant_spells(&mut el, &cantrip, Edition::Y2024);
        assert_eq!(el["rules"][0]["mode"], "known_cantrip");
        assert!(el["rules"][0].get("level").is_none());
    }
}
