//! The description pane's Grants list: what choosing an element gives, in level order, named the
//! way the builder asks for it, with the text of each granted trait (tester video 2026-11-02: A3,
//! A6, B9).

use crate::dto_ext::ElementGrant;
use crate::ids::ui_type;
use crate::sanitize::sanitize_html;
use borealis_content::{ContentStore, CustomRule, Element, Rule};
use serde_json::Value;

/// The level of a rule with no level gate.
const FIRST_LEVEL: u32 = 1;

/// A3: granted elements whose text is what the grant does (a species trait, a class feature, a
/// feat), shown under their name. Other grants (proficiencies, languages, sizes) are one line.
const TEXT_TYPES: [&str; 4] = [
    "racial-trait",
    "class-feature",
    "background-feature",
    "feat",
];

/// One grant before it becomes a line: its name, level gate and granted element.
struct Entry<'a> {
    name: String,
    level: Option<u32>,
    element: Option<&'a Element>,
}

/// What choosing `el` grants, by level (stable within a level). Only the element's own rules: a
/// class's multiclass node (a bare "Multiclass" grant and proficiencies the class already gives)
/// applies only when the class is taken as a multiclass (A6).
pub(crate) fn grants(store: &ContentStore, el: &Element) -> Vec<ElementGrant> {
    let mut out: Vec<ElementGrant> = Vec::new();
    for rule in &el.rules {
        let Some(entry) = entry(store, rule) else {
            continue;
        };
        let level = entry.level.unwrap_or(FIRST_LEVEL).max(FIRST_LEVEL);
        // A feature can come twice under one name at different levels (Improved Brutal Strike).
        if out.iter().any(|g| g.name == entry.name && g.level == level) {
            continue;
        }
        out.push(ElementGrant {
            name: entry.name,
            level,
            description_html: entry.element.map(text_of).unwrap_or_default(),
        });
    }
    out.sort_by_key(|g| g.level);
    out
}

fn entry<'a>(store: &'a ContentStore, rule: &Rule) -> Option<Entry<'a>> {
    match rule {
        Rule::Grant(g) => {
            let element = store.element(&g.id);
            Some(Entry {
                name: element.map_or_else(|| g.id.clone(), |e| e.name.clone()),
                level: g.level,
                element,
            })
        }
        Rule::Select(s) => {
            let what = if s.name.is_empty() { &s.ty } else { &s.name };
            Some(Entry {
                name: format!("Choose {}: {what}", s.number.unwrap_or(1)),
                level: s.level,
                element: None,
            })
        }
        Rule::Custom(c) => custom_entry(store, c),
        Rule::Stat(_) | Rule::Unknown(_) => None,
    }
}

/// The sanitised text of a granted trait, feature or feat; empty for other grants.
fn text_of(el: &Element) -> String {
    if TEXT_TYPES.contains(&ui_type(&el.ty)) {
        sanitize_html(&el.description)
    } else {
        String::new()
    }
}

/// B9: the custom rules that give the character something to have or to choose, named the way
/// the builder asks for them ("Magic Initiate (Cleric)", "Choose: Ability Scores (Acolyte)").
fn custom_entry<'a>(store: &'a ContentStore, c: &CustomRule) -> Option<Entry<'a>> {
    let text = |key: &str| {
        c.fields
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
    };
    let level = c
        .fields
        .get("level")
        .and_then(Value::as_u64)
        .and_then(|l| u32::try_from(l).ok());
    match c.kind.as_str() {
        "grant_feat" => {
            let id = text("feat")?;
            let element = store.element(id);
            let name = element.map_or_else(|| id.to_string(), |e| e.name.clone());
            let params: Vec<&str> = c
                .fields
                .get("params")
                .and_then(Value::as_object)
                .map(|p| p.values().filter_map(Value::as_str).collect())
                .unwrap_or_default();
            let name = if params.is_empty() {
                name
            } else {
                format!("{name} ({})", params.join(", "))
            };
            Some(Entry {
                name,
                level,
                element,
            })
        }
        "ability_distribution" | "equipment_package" => text("name").map(|n| Entry {
            name: format!("Choose: {n}"),
            level,
            element: None,
        }),
        _ => None,
    }
}
