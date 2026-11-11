//! Child elements (spec 3.1): features, traits and sub-features are separate elements of the pack, edited inside
//! their parent's form and listed under it. A child is an element of one of the parent type's child types that the
//! parent grants (a class's starting-equipment package is its `equipment_package` choice's).

use borealis_content::{Element, Rule};

/// The child types of an element type (the ones its form edits inside it).
#[must_use]
pub fn child_types(ty: &str) -> &'static [&'static str] {
    match ty {
        "Class" => &["Class Feature", "Equipment Package"],
        "Archetype" => &["Archetype Feature"],
        "Feat" => &["Feat Feature"],
        "Background" => &[
            "Background Feature",
            "Background Variant",
            "Equipment Package",
        ],
        "Race" | "Sub Race" | "Species" => &["Racial Trait"],
        _ => &[],
    }
}

/// The elements a parent gives: its grants, the package of its starting-equipment choice, and the elements a choice
/// of one of its child types offers by id (a background's variants: `ID_A|ID_B`).
fn granted_ids(parent: &Element) -> impl Iterator<Item = &str> {
    let types = child_types(&parent.ty);
    parent.rules.iter().flat_map(move |r| -> Vec<&str> {
        match r {
            Rule::Grant(g) => vec![g.id.as_str()],
            Rule::Custom(c) if c.kind == "equipment_package" => c
                .fields
                .get("ref")
                .and_then(serde_json::Value::as_str)
                .into_iter()
                .collect(),
            Rule::Select(s) if types.contains(&s.ty.as_str()) => {
                let ids: Vec<&str> = s
                    .supports
                    .as_deref()
                    .unwrap_or_default()
                    .split('|')
                    .map(str::trim)
                    .collect();
                if ids.iter().all(|id| id.starts_with("ID_")) {
                    ids
                } else {
                    Vec::new()
                }
            }
            _ => Vec::new(),
        }
    })
}

/// The children of `parent` among `pack` (the elements of its pack): granted elements of its child types, in rule
/// order, each once.
#[must_use]
pub fn granted_children<'a>(parent: &Element, pack: &'a [Element]) -> Vec<&'a Element> {
    let types = child_types(&parent.ty);
    let mut out: Vec<&Element> = Vec::new();
    for id in granted_ids(parent) {
        if let Some(child) = pack
            .iter()
            .find(|e| e.id == id && types.contains(&e.ty.as_str()))
        {
            if !out.iter().any(|k| k.id == child.id) {
                out.push(child);
            }
        }
    }
    out
}

/// The parent of `child` among `pack`: the first element whose children include it.
#[must_use]
pub fn parent_of<'a>(child: &Element, pack: &'a [Element]) -> Option<&'a Element> {
    pack.iter().find(|p| {
        p.id != child.id
            && child_types(&p.ty).contains(&child.ty.as_str())
            && granted_ids(p).any(|id| id == child.id)
    })
}
