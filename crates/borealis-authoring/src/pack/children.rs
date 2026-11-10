//! Child elements (spec 3.1): features, traits and sub-features are separate elements of the pack, edited inside
//! their parent's form and listed under it. A child is an element of one of the parent type's child types that the
//! parent grants.

use borealis_content::{Element, Rule};

/// The child types of an element type (the ones its form edits inside it).
#[must_use]
pub fn child_types(ty: &str) -> &'static [&'static str] {
    match ty {
        "Class" => &["Class Feature"],
        "Archetype" => &["Archetype Feature"],
        "Feat" => &["Feat Feature"],
        "Background" => &["Background Feature"],
        "Race" | "Sub Race" | "Species" => &["Racial Trait"],
        _ => &[],
    }
}

fn granted_ids(parent: &Element) -> impl Iterator<Item = &str> {
    parent.rules.iter().filter_map(|r| match r {
        Rule::Grant(g) => Some(g.id.as_str()),
        _ => None,
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
