//! What the spellcasting section's fields share when they write (spec 4.6), and the two places the section reaches
//! outside its own element:
//!
//! * [`CasterCx`]: the section as a whole (on or off, the spellcasting name its stats are counted under, the
//!   edition, the slot table), made from the form's values before its fields write ([`context`]).
//! * [`follow_parent`]: a class feature read with its class whose spellcasting name is the class's name follows the
//!   class (renaming the class renames the section's stats and choices).
//! * [`sync_class`]: a 2024 class keeps the spellcasting columns of its class table (`data.progression`: cantrips,
//!   prepared, slots, pact slots; the engine reads `prepared` and the pact columns) and its `data.spellcasting`
//!   summary in step with its Spellcasting feature, whenever that feature's section was edited.

use super::super::child_refs::{self, ParentInfo};
use super::super::FormState;
use crate::caster::section::{self, SlotGrid, SlotTable};
use crate::Edition;
use borealis_content::{Element, Ruleset};
use serde_json::{json, Map, Value};

/// The section's field keys (spec 4.6), in form order.
pub(crate) const SECTION_FIELDS: [&str; 14] = [
    "casts",
    "casterName",
    "ability",
    "preparation",
    "spellList",
    "swap",
    "casterType",
    "slotTable",
    "multiclassSlots",
    "prepared",
    "prepareFormula",
    "cantrips",
    "known",
    "preparedChange",
];

/// The spellcasting section as its fields write it.
#[derive(Debug, Clone)]
pub(crate) struct CasterCx {
    /// The element gives spellcasting.
    pub(crate) on: bool,
    /// The name its spells, slots and choices are counted under (the class's, unless given).
    pub(crate) name: String,
    pub(crate) edition: Edition,
    /// The slot table (`None`: no slots, or a custom table that can't be read).
    pub(crate) table: Option<SlotTable>,
    /// The caster type is a custom table.
    pub(crate) custom: bool,
    /// The spellcasting ability (`charisma`), lower case.
    pub(crate) ability: String,
}

impl CasterCx {
    /// Slots per level (none without a table).
    pub(crate) fn grid(&self) -> SlotGrid {
        self.table.as_ref().map_or([[0; 9]; 20], SlotTable::grid)
    }
}

/// A form value, as edited or as read.
pub(crate) fn value<'f>(form: &'f FormState, key: &str) -> Option<&'f Value> {
    form.fields
        .get(key)
        .or_else(|| form.memo.fields.get(key).map(|m| &m.value))
}

fn text<'f>(form: &'f FormState, key: &str) -> &'f str {
    value(form, key).and_then(Value::as_str).unwrap_or_default()
}

/// A custom slot table from its form value (20 rows of 9 whole numbers); `None` when it isn't one.
pub(crate) fn grid_of(v: &Value) -> Option<SlotGrid> {
    let rows = v.as_array().filter(|r| r.len() == 20)?;
    let mut grid = [[0u32; 9]; 20];
    for (row, cells) in grid.iter_mut().zip(rows) {
        let cells = cells.as_array().filter(|c| c.len() == 9)?;
        for (slot, cell) in row.iter_mut().zip(cells) {
            *slot = u32::try_from(cell.as_u64()?).ok().filter(|n| *n <= 9)?;
        }
    }
    Some(grid)
}

/// The edition a section writes in: its class's, else its own (none counts as 2014, as the SRD stores 2014 content).
pub(crate) fn edition_of(form: &FormState, parent: Option<&ParentInfo>) -> Edition {
    let own = match text(form, "edition") {
        "dnd2024" => Some(Ruleset::Dnd2024),
        "dnd2014" => Some(Ruleset::Dnd2014),
        _ => None,
    };
    match parent.and_then(|p| p.edition).or(own) {
        Some(Ruleset::Dnd2024) => Edition::Y2024,
        _ => Edition::Y2014,
    }
}

/// The section of a form (a form of a kind without one is off).
pub(crate) fn context(form: &FormState, parent: Option<&ParentInfo>) -> CasterCx {
    let given = text(form, "casterName").trim();
    let name = if given.is_empty() {
        parent
            .map(|p| p.name.trim().to_string())
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| borealis_content::rich::normalize_plain_text(text(form, "name")))
    } else {
        given.to_string()
    };
    let grid = value(form, "slotTable").and_then(grid_of);
    CasterCx {
        on: value(form, "casts").and_then(Value::as_bool) == Some(true),
        name,
        edition: edition_of(form, parent),
        table: SlotTable::from_key(text(form, "casterType"), grid),
        custom: text(form, "casterType") == "custom",
        ability: text(form, "ability").trim().to_lowercase(),
    }
}

/// Children of a class read with it: a section whose spellcasting name is the class's name follows the class (its
/// name field is left empty, as a new section's is).
pub(crate) fn follow_parent(form: &mut FormState) {
    let parent = borealis_content::rich::normalize_plain_text(text(form, "name"));
    if parent.is_empty() {
        return;
    }
    for c in &mut form.children {
        if c.fields.get("casterName").and_then(Value::as_str) != Some(parent.as_str()) {
            continue;
        }
        c.fields.insert("casterName".into(), json!(""));
        if let Some(m) = c.memo.fields.get_mut("casterName") {
            m.value = json!("");
        }
    }
}

/// A section field was edited (or is new) since it was read.
fn edited(form: &FormState) -> bool {
    SECTION_FIELDS
        .iter()
        .any(|k| match form.memo.fields.get(*k) {
            Some(m) => form.fields.get(*k).is_some_and(|v| *v != m.value),
            None => form.fields.contains_key(*k),
        })
}

fn had_section(form: &FormState) -> bool {
    value(form, "casts").and_then(Value::as_bool) == Some(true)
        || form.memo.fields.get("casts").map(|m| &m.value) == Some(&json!(true))
}

fn levels(v: Option<&Value>) -> Option<[u32; 20]> {
    let v = v?;
    let list = v.get("levels").unwrap_or(v).as_array()?;
    let mut out = [0u32; 20];
    for (slot, n) in out.iter_mut().zip(list) {
        *slot = u32::try_from(n.as_u64()?).ok()?;
    }
    (list.len() == 20).then_some(out)
}

/// The 2024 class's spellcasting columns and summary, from its Spellcasting feature when that feature's section was
/// edited: the columns [`section::progression_columns`] writes replace the old ones (the class's own counters stay),
/// and `data.spellcasting` (kept only where the class has it, as every 2024 mirror) is rewritten.
pub(crate) fn sync_class(element: &mut Element, class: &FormState) {
    let Some(child) = class.children.iter().find(|c| had_section(c)) else {
        return;
    };
    if !edited(child) {
        return;
    }
    let info = child_refs::parent_info(class, child);
    let cx = context(child, Some(&info));
    if cx.edition != Edition::Y2024 {
        return;
    }
    let prepares = text(child, "preparation") != "known";
    let cantrips = levels(value(child, "cantrips")).unwrap_or([0; 20]);
    let prepared = levels(value(child, "prepared")).filter(|_| prepares);
    let columns = if cx.on {
        section::progression_columns(cx.table.as_ref(), &cantrips, prepared.as_ref())
    } else {
        Vec::new()
    };
    let mut table: Map<String, Value> = element
        .data
        .get("progression")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    table.retain(|k, _| !section::is_progression_column(k));
    table.extend(columns);
    if table.is_empty() {
        element.data.remove("progression");
    } else {
        element
            .data
            .insert("progression".into(), Value::Object(table));
    }
    if let Some(Value::Object(summary)) = element.data.get_mut("spellcasting") {
        write_summary(summary, &cx, child, cantrips.iter().any(|c| *c > 0));
    }
}

fn write_summary(
    summary: &mut Map<String, Value>,
    cx: &CasterCx,
    child: &FormState,
    has_cantrips: bool,
) {
    let list = match text(child, "spellList").trim() {
        "" => cx.name.clone(),
        l => l.to_string(),
    };
    let model = match (cx.table.as_ref(), text(child, "preparation")) {
        (Some(SlotTable::Pact), _) => "pact",
        (_, "spellbook") => "spellbook",
        (_, "known") => "known",
        _ => "prepared",
    };
    summary.insert("ability".into(), json!(cx.ability));
    summary.insert("list".into(), json!(list));
    summary.insert("model".into(), json!(model));
    match text(child, "preparedChange") {
        "levelOne" => summary.insert("replace".into(), json!("level")),
        "" => summary.remove("replace"),
        _ => summary.insert("replace".into(), json!("long_rest")),
    };
    match super::caster_read::multiclass_suffix(text(child, "multiclassSlots"), cx.table.as_ref()) {
        Some(s) => summary.insert(
            "multiclass_slots_grant".into(),
            json!(format!("{}{s}", section::MULTICLASS_GRANT_PREFIX)),
        ),
        None => summary.remove("multiclass_slots_grant"),
    };
    summary.insert("has_cantrips".into(), json!(has_cantrips));
}
