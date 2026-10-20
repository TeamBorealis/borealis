//! Lair actions and regional effects of creatures. 5etools keeps them apart from the stat
//! block, in legendary groups (`bestiary/legendarygroups.json`, property `legendaryGroup`); a
//! creature names its group (`legendaryGroup: {name, source}`). A group may `_copy` another
//! one, and a `_versions` entry may apply legendary group templates (`bestiary/template.json`,
//! `legendaryGroupTemplate`). Deterministic; no AI is involved.

use crate::copy;
use crate::text::{strip_tags, Renderer};
use borealis_content::monster::{LairOption, LairSection};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// Property of the legendary groups.
pub const PROP: &str = "legendaryGroup";
/// Property of the legendary group templates.
pub const TEMPLATE_PROP: &str = "legendaryGroupTemplate";
/// The `_versions` key that names templates to apply.
const TEMPLATES: &str = "_templates";

/// The resolved legendary groups of a run, by `name|source`.
#[derive(Debug, Default, Clone)]
pub struct LegendaryGroups {
    by_key: BTreeMap<String, Value>,
}

impl LegendaryGroups {
    /// Resolve the groups (`_copy`, `_versions` and the templates the versions name). What
    /// cannot be resolved is left out and described in `notes`.
    #[must_use]
    pub fn new(groups: &[Value], templates: &[Value], notes: &mut Vec<String>) -> Self {
        let resolved = copy::resolve_array(PROP, groups);
        notes.extend(resolved.notes);
        for (name, reason) in resolved.failed {
            notes.push(format!("{name}: cannot resolve _copy: {reason}"));
        }
        let templates: HashMap<String, Value> = templates
            .iter()
            .map(|t| (crate::copy_monster::template_key(t), t.clone()))
            .collect();
        let mut by_key = BTreeMap::new();
        for mut group in resolved.entries {
            if let Some(Value::Array(refs)) =
                group.as_object_mut().and_then(|m| m.remove(TEMPLATES))
            {
                let mut failed = Vec::new();
                crate::copy_monster::apply_templates(
                    &mut group,
                    &BTreeSet::new(),
                    &refs,
                    &templates,
                    &mut failed,
                );
                if !failed.is_empty() {
                    notes.push(format!(
                        "{}: {}",
                        copy::display_name(&group),
                        failed.join("; ")
                    ));
                    continue;
                }
            }
            by_key
                .entry(copy::identity_key(PROP, &group))
                .or_insert(group);
        }
        Self { by_key }
    }

    /// The group a creature's `legendaryGroup` reference names.
    #[must_use]
    pub fn get(&self, reference: &Value) -> Option<&Value> {
        self.by_key.get(&copy::identity_key(PROP, reference))
    }

    /// Number of groups.
    #[must_use]
    pub fn len(&self) -> usize {
        self.by_key.len()
    }

    /// True when there are no groups.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_key.is_empty()
    }
}

/// Lair actions and regional effects of a creature, from its legendary group.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Lair {
    /// Lair actions.
    pub actions: Option<LairSection>,
    /// Regional effects.
    pub regional_effects: Option<LairSection>,
}

/// The lair of the creature `raw`. A group that is not found, and the parts of a group the
/// model has no field for (mythic encounters), are described in `lost`.
pub fn lair(raw: &Value, groups: &LegendaryGroups, lost: &mut Vec<String>) -> Lair {
    let Some(reference) = raw.get("legendaryGroup") else {
        return Lair::default();
    };
    let Some(group) = groups.get(reference) else {
        lost.push(format!(
            "{}lair actions and regional effects (legendary group) not imported",
            super::monster::SECTION
        ));
        return Lair::default();
    };
    if group.get("mythicEncounter").is_some() {
        lost.push(format!(
            "{}the mythic encounter (legendary group) not imported",
            super::monster::SECTION
        ));
    }
    let of = |key: &str| {
        group
            .get(key)
            .and_then(Value::as_array)
            .and_then(|e| section(e))
    };
    Lair {
        actions: of("lairActions"),
        regional_effects: of("regionalEffects"),
    }
}

fn is_list(v: &Value) -> bool {
    v.get("type").and_then(Value::as_str) == Some("list")
}

/// One section: the entries before the first list are the preamble, that list's items the
/// options, the rest the closing text. `None` when nothing has text.
#[must_use]
pub fn section(entries: &[Value]) -> Option<LairSection> {
    let (before, list, after) = match entries.iter().position(is_list) {
        Some(i) => (&entries[..i], Some(&entries[i]), &entries[i + 1..]),
        None => (entries, None, &[][..]),
    };
    let options: Vec<LairOption> = list
        .and_then(|l| l.get("items"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(option)
        .collect();
    let preamble = Renderer::plain(before).trim().to_string();
    let closing = Renderer::plain(after).trim().to_string();
    if preamble.is_empty() && options.is_empty() && closing.is_empty() {
        return None;
    }
    Some(LairSection {
        preamble,
        options,
        closing,
    })
}

/// A list item: plain text, or a named item (`{name, entry}` / `{name, entries}`).
fn option(item: &Value) -> Option<LairOption> {
    let (name, text) = match item {
        Value::String(s) => (None, strip_tags(s)),
        Value::Object(_) => {
            let name = item
                .get("name")
                .and_then(Value::as_str)
                .map(|n| {
                    strip_tags(n)
                        .trim()
                        .trim_end_matches('.')
                        .trim()
                        .to_string()
                })
                .filter(|n| !n.is_empty());
            let body: Vec<Value> = match (item.get("entry"), item.get("entries")) {
                (Some(e), _) => vec![e.clone()],
                (None, Some(Value::Array(list))) => list.clone(),
                _ => Vec::new(),
            };
            (name, Renderer::plain(&body))
        }
        _ => return None,
    };
    let text = text.trim().to_string();
    (!text.is_empty()).then_some(LairOption { name, text })
}

#[cfg(test)]
#[path = "monster_lair_tests.rs"]
mod tests;
