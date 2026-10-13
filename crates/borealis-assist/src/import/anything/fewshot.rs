//! Few-shot exemplars and the core id catalogue, both taken from the enabled packs (the bundled
//! SRD) through the `ContentStore`, matching the edition.

use borealis_content::{ContentStore, Element, Ruleset};
use serde_json::Value;

use super::types::Edition;

const DESCRIPTION_CAP: usize = 280;
const RULES_CAP: usize = 40;
const CATALOGUE_CAP: usize = 220;
const CATALOGUE_TYPES: [&str; 3] = ["Ability", "Skill", "Proficiency"];

/// True when `el` belongs to `edition`.
pub(crate) fn in_edition(el: &Element, edition: Edition) -> bool {
    matches!(
        (edition, el.ruleset),
        (_, Some(Ruleset::Any))
            | (Edition::Y2024, Some(Ruleset::Dnd2024))
            | (Edition::Y2014, None | Some(Ruleset::Dnd2014))
    )
}

fn shorten(v: &mut Value) {
    if let Some(Value::String(d)) = v.get_mut("description") {
        if d.chars().count() > DESCRIPTION_CAP {
            *d = d.chars().take(DESCRIPTION_CAP).collect::<String>() + "...";
        }
    }
}

/// A real SRD element of `ty` in `edition`, preferring a name containing `prefer`.
pub(crate) fn exemplar(
    store: &ContentStore,
    edition: Edition,
    ty: &str,
    prefer: &[&str],
) -> Option<Value> {
    let mut found: Vec<&Element> = store
        .by_type(ty)
        .map(|s| &s.element)
        .filter(|e| in_edition(e, edition) && e.source.to_ascii_uppercase().contains("SRD"))
        .collect();
    found.sort_by(|a, b| a.id.cmp(&b.id));
    let pick = prefer
        .iter()
        .find_map(|p| found.iter().find(|e| e.name.eq_ignore_ascii_case(p)))
        .or_else(|| found.first())?;
    let mut v = serde_json::to_value(pick).ok()?;
    shorten(&mut v);
    Some(v)
}

/// A compact class exemplar: the class plus one of its features (a "real Class/Feature").
pub(crate) fn class_exemplars(store: &ContentStore, edition: Edition, caster: bool) -> Vec<Value> {
    let prefer: &[&str] = if caster {
        &["Wizard", "Cleric"]
    } else {
        &["Fighter", "Barbarian"]
    };
    let Some(class) = exemplar(store, edition, "Class", prefer) else {
        return Vec::new();
    };
    let mut out = vec![class.clone()];
    let granted: Vec<String> = class
        .get("rules")
        .and_then(Value::as_array)
        .map(|rules| {
            rules
                .iter()
                .filter(|r| {
                    r.get("kind") == Some(&Value::from("grant"))
                        && r.get("type") == Some(&Value::from("Class Feature"))
                })
                .filter_map(|r| r.get("id")?.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let wanted = if caster {
        granted.iter().find(|i| i.contains("SPELLCASTING"))
    } else {
        None
    };
    let feature = wanted
        .or_else(|| granted.last())
        .and_then(|id| store.element(id).and_then(|e| serde_json::to_value(e).ok()));
    if let Some(mut f) = feature {
        shorten(&mut f);
        if let Some(r) = f.get_mut("rules").and_then(Value::as_array_mut) {
            r.truncate(RULES_CAP);
        }
        out.push(f);
    }
    out
}

/// Exemplars for `ty` (a subclass comes with one of its features).
pub(crate) fn exemplars_for(
    store: &ContentStore,
    edition: Edition,
    ty: &str,
    caster: bool,
    prefer: &[&str],
) -> Vec<Value> {
    match ty {
        "Class" => class_exemplars(store, edition, caster),
        "Archetype" => {
            let mut out: Vec<Value> = exemplar(store, edition, "Archetype", prefer)
                .into_iter()
                .collect();
            out.extend(exemplar(store, edition, "Archetype Feature", prefer));
            out
        }
        other => exemplar(store, edition, other, prefer)
            .into_iter()
            .collect(),
    }
}

/// The core ids the model may reuse (abilities, skills, proficiencies), as `ID name` lines.
pub(crate) fn core_catalogue(store: &ContentStore) -> String {
    let mut lines: Vec<String> = Vec::new();
    for ty in CATALOGUE_TYPES {
        let mut ids: Vec<(&str, &str)> = store
            .by_type(ty)
            .map(|s| (s.element.id.as_str(), s.element.name.as_str()))
            .collect();
        ids.sort_unstable();
        lines.extend(ids.into_iter().map(|(i, n)| format!("{ty}: {i} ({n})")));
    }
    lines.truncate(CATALOGUE_CAP);
    lines.join("\n")
}
