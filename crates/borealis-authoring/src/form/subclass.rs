//! Subclasses (spec 4.7): what a subclass takes from its class.
//!
//! A class offers its subclasses with a `select` of type `Archetype` (on the class, or on the class feature it grants
//! at the subclass level: the SRD's "Fighter Subclass", "Martial Archetype"). The select's tag is what a subclass
//! carries in `supports` ("Fighter Subclass" in 2024, the label "Martial Archetype" in 2014), and its level is the
//! subclass's `data.level_gained`.
//!
//! **Subclass feature levels have no stored location** (hc-wb16 Open 3): they are derived from the class every time
//! a subclass is read or refreshed ([`ClassChoice::levels`]): the choice's level, the levels the class's
//! `data.features_by_level` names "Subclass feature", the levels of the class features it grants named like "<label>
//! Feature" or "Subclass Feature" (the Aurora shape), and the levels its loaded subclasses of the same edition grant
//! features at. The subclass form shows them on its LEVELS grid; nothing writes them.
//!
//! Deterministic, no AI (G7).

use super::value::RefValue;
use super::{FormContext, FormState, Lookup};
use borealis_content::expr::SupportsExpr;
use borealis_content::{Element, Rule, Ruleset};
use serde_json::{json, Value};

/// The subclass form's id.
pub const SUBCLASS_KIND: &str = "subclass";
/// The subclass feature form's id.
pub const SUBCLASS_FEATURE_KIND: &str = "subclassFeature";

/// The field keys the subclass form derives from its class.
pub(crate) const PARENT: &str = "parentClass";
pub(crate) const TAG: &str = "subclassTag";
pub(crate) const LEVEL_GAINED: &str = "levelGained";
pub(crate) const LEVELS: &str = "subclassLevels";

/// A class's subclass choice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ClassChoice {
    /// The class's id.
    pub(crate) class_id: String,
    /// The class's name.
    pub(crate) class_name: String,
    /// The class's own edition.
    pub(crate) edition: Option<Ruleset>,
    /// The tag a subclass carries to be offered there.
    pub(crate) tag: String,
    /// The level the subclass is chosen at.
    pub(crate) level: u32,
    /// The levels the class gives subclass features at, the choice's level first (sorted, no repeats).
    pub(crate) levels: Vec<u32>,
}

/// The first tag a `select`'s supports text names (`"Fighter Subclass"`, `"Martial Archetype"`).
fn first_tag(supports: &str) -> Option<String> {
    let t = supports.trim();
    if t.is_empty() {
        return None;
    }
    match SupportsExpr::parse(t) {
        Ok(Some(e)) => e.tags().first().map(|s| (*s).to_string()),
        _ => Some(t.to_string()),
    }
}

fn level_of(n: Option<u32>) -> Option<u32> {
    n.filter(|l| (1..=20).contains(l))
}

fn data_level(e: &Element) -> Option<u32> {
    e.data
        .get("level")
        .and_then(Value::as_u64)
        .and_then(|n| u32::try_from(n).ok())
}

/// The `select Archetype` of `e`: (tag, label, its own level).
fn archetype_select(e: &Element) -> Option<(String, String, Option<u32>)> {
    e.rules.iter().find_map(|r| match r {
        Rule::Select(s) if s.ty == "Archetype" => {
            let tag = first_tag(s.supports.as_deref().unwrap_or_default())?;
            Some((tag, s.name.clone(), s.level))
        }
        _ => None,
    })
}

/// `class`'s subclass choice, with its features looked up in `lookup`; `None` when the class offers no subclass.
pub(crate) fn class_choice(class: &Element, lookup: &dyn Lookup) -> Option<ClassChoice> {
    let mut found = archetype_select(class)
        .map(|(tag, label, level)| (tag, label, level_of(level).unwrap_or(1)));
    let mut feature_grants: Vec<(String, u32)> = Vec::new();
    for r in &class.rules {
        let Rule::Grant(g) = r else { continue };
        if g.ty.as_deref() != Some("Class Feature") {
            continue;
        }
        let level = level_of(g.level).unwrap_or(1);
        if let Some(name) = lookup.find(&g.id).map(|i| i.name) {
            feature_grants.push((name, level));
        }
        if found.is_none() {
            if let Some(f) = lookup.loaded(&g.id) {
                found = archetype_select(f).map(|(tag, label, own)| {
                    let at = level_of(own)
                        .or_else(|| level_of(g.level))
                        .or_else(|| level_of(data_level(f)))
                        .unwrap_or(1);
                    (tag, label, at)
                });
            }
        }
    }
    let (tag, label, level) = found?;
    let mut levels = vec![level];
    // the 2024 class table names its subclass levels
    if let Some(rows) = class
        .data
        .get("features_by_level")
        .and_then(Value::as_array)
    {
        for (i, row) in rows.iter().enumerate() {
            let named = row
                .as_str()
                .is_some_and(|t| t.to_lowercase().contains("subclass feature"));
            if named {
                levels.extend(u32::try_from(i + 1).ok());
            }
        }
    }
    // a placeholder feature per subclass level ("Martial Archetype Feature")
    let marks = [
        format!("{} feature", label.to_lowercase()),
        format!("{} feature", tag.to_lowercase()),
        "subclass feature".to_string(),
    ];
    for (name, level) in &feature_grants {
        let lower = name.to_lowercase();
        if marks.iter().any(|m| lower.contains(m.as_str())) {
            levels.push(*level);
        }
    }
    // the levels its loaded subclasses give features at
    for sub in lookup.loaded_of_type("Archetype") {
        if sub.ruleset != class.ruleset || !sub.supports.contains(&tag) {
            continue;
        }
        for r in &sub.rules {
            if let Rule::Grant(g) = r {
                if g.ty.as_deref() == Some("Archetype Feature") {
                    levels.extend(level_of(g.level));
                }
            }
        }
    }
    levels.sort_unstable();
    levels.dedup();
    Some(ClassChoice {
        class_id: class.id.clone(),
        class_name: class.name.clone(),
        edition: class.ruleset,
        tag,
        level,
        levels,
    })
}

/// The loaded class whose subclass choice offers `sub` (one of its tags), the subclass's edition first.
pub(crate) fn class_offering(sub: &Element, lookup: &dyn Lookup) -> Option<ClassChoice> {
    let mut found: Vec<(Option<Ruleset>, ClassChoice)> = lookup
        .loaded_of_type("Class")
        .into_iter()
        .filter_map(|c| class_choice(c, lookup).map(|ch| (c.ruleset, ch)))
        .filter(|(_, ch)| sub.supports.contains(&ch.tag))
        .collect();
    let at = found
        .iter()
        .position(|(r, _)| *r == sub.ruleset)
        .unwrap_or(0);
    (!found.is_empty()).then(|| found.swap_remove(at).1)
}

/// The class a stored subclass belongs to: the one `data.class` names, else the one whose choice offers it.
pub(crate) fn stored_class(sub: &Element, lookup: &dyn Lookup) -> Option<ClassChoice> {
    let named = sub
        .data
        .get("class")
        .and_then(Value::as_str)
        .and_then(|id| lookup.loaded(id))
        .and_then(|c| class_choice(c, lookup));
    named.or_else(|| class_offering(sub, lookup))
}

/// The class picked in a subclass form, as it is now.
fn picked(form: &FormState) -> Option<RefValue> {
    form.fields
        .get(PARENT)
        .or_else(|| form.memo.fields.get(PARENT).map(|m| &m.value))
        .and_then(|v| serde_json::from_value::<Option<RefValue>>(v.clone()).ok())
        .flatten()
        .filter(|r| !r.id.trim().is_empty())
}

/// A subclass form with what it takes from its class written from the class picked now (call it on every edit,
/// after the UI's form is merged): the subclass feature levels always; the tag the class's choice offers and the
/// level it is chosen at (and the class's edition) when the class was picked since the form was read (or the subclass
/// is new). A class that is not loaded changes nothing. Other forms are returned as they are.
#[must_use]
pub fn refresh_subclass(form: &FormState, cx: &FormContext<'_>) -> FormState {
    if form.kind != SUBCLASS_KIND {
        return form.clone();
    }
    let Some(class) = picked(form) else {
        return form.clone();
    };
    let Some(choice) = cx
        .lookup
        .loaded(class.id.trim())
        .filter(|c| c.ty == "Class")
        .and_then(|c| class_choice(c, cx.lookup))
    else {
        return form.clone();
    };
    let mut next = form.clone();
    next.fields.insert(LEVELS.into(), json!(choice.levels));
    let read_as = form.memo.fields.get(PARENT).map(|m| &m.value);
    let now = form.fields.get(PARENT);
    let picked_now = read_as.is_none() || now.is_some_and(|v| Some(v) != read_as);
    if picked_now {
        next.fields.insert(TAG.into(), json!(choice.tag));
        next.fields.insert(LEVEL_GAINED.into(), json!(choice.level));
        // a subclass is of its class's edition
        if let Some(r @ (Ruleset::Dnd2014 | Ruleset::Dnd2024)) = choice.edition {
            next.fields.insert("edition".into(), json!(r.as_str()));
        }
        // the class's name as loaded (its features write it)
        next.fields.insert(
            PARENT.into(),
            json!(RefValue::resolve(&choice.class_id, cx.lookup)),
        );
    }
    next
}

#[cfg(test)]
#[path = "subclass_tests.rs"]
mod tests;
