//! Spells.

use super::build::setter_with;
use super::spells_extra::school_name;
use super::{append_fluff, drain_renderer, new_element, Conv, Ent, Shared, State};
use crate::ids::IdKind;
use crate::model::{self, Spell};
use crate::srd::SrdKind;
use crate::text::{RefKind, Renderer};
use borealis_content::Setter;
use serde_json::{json, Value};

fn plural(n: i64, unit: &str) -> String {
    if n == 1 {
        format!("{n} {unit}")
    } else {
        format!("{n} {unit}s")
    }
}

/// `1 action`, `1 bonus action`, `1 reaction, which you take when ...`, `10 minutes`.
#[must_use]
pub fn casting_time(times: &[Value]) -> String {
    let parts: Vec<String> = times
        .iter()
        .map(|t| {
            let n = t.get("number").and_then(Value::as_i64).unwrap_or(1);
            let unit = t.get("unit").and_then(Value::as_str).unwrap_or("action");
            let base = match unit {
                "bonus" => plural(n, "bonus action"),
                "special" => "Special".to_string(),
                u => plural(n, u),
            };
            match t.get("condition").and_then(Value::as_str) {
                Some(c) => format!("{base}, {}", crate::text::strip_tags(c)),
                None => base,
            }
        })
        .collect();
    if parts.is_empty() {
        "1 action".to_string()
    } else {
        parts.join(" or ")
    }
}

/// `150 feet`, `Self (15-foot cone)`, `Touch`, `Sight`, `Unlimited`.
#[must_use]
pub fn range_text(range: Option<&Value>) -> String {
    let Some(r) = range else {
        return "Special".into();
    };
    let ty = r.get("type").and_then(Value::as_str).unwrap_or("point");
    let dist = r.get("distance");
    let amount = dist.and_then(|d| d.get("amount")).and_then(Value::as_i64);
    let dtype = dist.and_then(|d| d.get("type")).and_then(Value::as_str);
    match ty {
        "special" => "Special".into(),
        "point" => match (dtype, amount) {
            (Some("self"), _) => "Self".into(),
            (Some("touch"), _) => "Touch".into(),
            (Some("sight"), _) => "Sight".into(),
            (Some("unlimited"), _) => "Unlimited".into(),
            (Some("plane"), _) => "Same plane".into(),
            (Some(u), Some(n)) => {
                let unit = match u {
                    "feet" => "feet".to_string(),
                    "miles" => {
                        if n == 1 {
                            "mile".into()
                        } else {
                            "miles".to_string()
                        }
                    }
                    "yards" => "yards".to_string(),
                    other => other.to_string(),
                };
                format!("{n} {unit}")
            }
            _ => "Special".into(),
        },
        shape => {
            let shape = if shape == "radius" { "radius" } else { shape };
            match amount {
                Some(n) => format!("Self ({n}-foot {shape})"),
                None => "Self".into(),
            }
        }
    }
}

/// `Instantaneous`, `Concentration, up to 10 minutes`, `1 hour`, `Until dispelled`.
#[must_use]
pub fn duration_text(durations: &[Value]) -> (String, bool) {
    let mut concentration = false;
    let parts: Vec<String> = durations
        .iter()
        .map(
            |d| match d.get("type").and_then(Value::as_str).unwrap_or("instant") {
                "instant" => "Instantaneous".to_string(),
                "permanent" => {
                    let ends = d.get("ends").and_then(Value::as_array);
                    let dispel = ends.is_some_and(|e| e.iter().any(|x| x == "dispel"));
                    if dispel {
                        "Until dispelled".to_string()
                    } else {
                        "Permanent".to_string()
                    }
                }
                "special" => "Special".to_string(),
                "timed" => {
                    let dur = d.get("duration");
                    let n = dur
                        .and_then(|x| x.get("amount"))
                        .and_then(Value::as_i64)
                        .unwrap_or(1);
                    let unit = dur
                        .and_then(|x| x.get("type"))
                        .and_then(Value::as_str)
                        .unwrap_or("round");
                    let conc = d.get("concentration") == Some(&Value::Bool(true));
                    concentration |= conc;
                    let span = plural(n, unit);
                    if conc {
                        format!("Concentration, up to {span}")
                    } else {
                        span
                    }
                }
                other => other.to_string(),
            },
        )
        .collect();
    if parts.is_empty() {
        ("Instantaneous".to_string(), false)
    } else {
        (parts.join(" or "), concentration)
    }
}

fn truthy(v: Option<&Value>) -> bool {
    matches!(v, Some(Value::Bool(true))) || matches!(v, Some(Value::Object(_)))
}

fn bool_setter(name: &str, v: bool) -> Setter {
    Setter::new(name, if v { "true" } else { "false" })
}

/// Convert one spell.
pub fn convert(sh: &Shared, st: &mut State, ent: &Ent) -> Conv {
    let spell: Spell = match model::parse(&ent.raw) {
        Ok(s) => s,
        Err(e) => {
            st.report.skipped(
                &ent.file,
                "spell",
                &ent.label(),
                &format!("invalid shape: {e}"),
            );
            return Conv::default();
        }
    };
    let resolver = |k: RefKind, n: &str, s: &str| sh.link(k, n, s);
    let mut renderer = Renderer::new(&resolver);
    let id = st.ids.named(IdKind::Spell, &ent.source, &spell.name);
    let mut el = new_element(
        &id,
        &spell.name,
        "Spell",
        &sh.source_display(&ent.source),
        ent.ruleset,
    );
    let school = school_name(&spell.school);
    let (duration, concentration) = duration_text(&spell.duration);
    let components = spell.components.clone().unwrap_or(Value::Null);
    let material = components.get("m");
    let material_text = match material {
        Some(Value::String(s)) => Some(s.clone()),
        Some(Value::Object(o)) => o.get("text").and_then(Value::as_str).map(str::to_string),
        _ => None,
    };
    let ritual = spell.meta.as_ref().and_then(|m| m.get("ritual")) == Some(&Value::Bool(true));

    let mut keywords: Vec<String> = vec![school.to_lowercase()];
    keywords.extend(spell.damage_inflict.iter().cloned());
    keywords.dedup();
    el.setters = vec![
        Setter::new("keywords", keywords.join(", ")),
        Setter::new("level", spell.level.to_string()),
        Setter::new("school", school),
        Setter::new("time", casting_time(&spell.time)),
        Setter::new("duration", duration),
        Setter::new("range", range_text(spell.range.as_ref())),
        bool_setter(
            "hasVerbalComponent",
            components.get("v") == Some(&Value::Bool(true)),
        ),
        bool_setter(
            "hasSomaticComponent",
            components.get("s") == Some(&Value::Bool(true)),
        ),
        bool_setter("hasMaterialComponent", truthy(material)),
    ];
    if let Some(m) = material_text {
        el.setters.push(Setter::new(
            "materialComponent",
            crate::text::strip_tags(&m),
        ));
    }
    if let Some(Value::Object(o)) = material {
        if let Some(cost) = o.get("cost").and_then(Value::as_f64) {
            el.setters.push(setter_with(
                "materialCost",
                &format!("{}", cost / 100.0),
                &[("currency", "gp")],
            ));
            if o.get("consume").is_some_and(|c| c != &Value::Bool(false)) {
                el.setters.push(Setter::new("materialConsumed", "true"));
            }
        }
    }
    if components.get("r") == Some(&Value::Bool(true)) {
        el.setters.push(bool_setter("hasRoyaltyComponent", true));
    }
    el.setters
        .push(bool_setter("isConcentration", concentration));
    el.setters.push(bool_setter("isRitual", ritual));

    // Class lists become the supports tags (legacy content convention).
    let mut supports = sh.spell_classes.classes(&spell.name, &ent.source);
    supports.extend(sh.spell_classes.subclasses(&spell.name, &ent.source));
    supports.sort();
    supports.dedup();
    el.supports = supports;

    let mut description = renderer.entries(&spell.entries);
    if !spell.entries_higher_level.is_empty() {
        description.push_str(&renderer.entries(&spell.entries_higher_level));
    }
    append_fluff(sh, "spell", ent, &mut description, &mut renderer);
    el.description = description;

    // Mechanical hints for the sheet and for homebrew tooling.
    if !spell.damage_inflict.is_empty() {
        el.data
            .insert("damage_inflict".into(), json!(spell.damage_inflict));
    }
    if !spell.saving_throw.is_empty() {
        el.data
            .insert("saving_throw".into(), json!(spell.saving_throw));
    }
    for key in [
        "scalingLevelDice",
        "spellAttack",
        "areaTags",
        "subschools",
        "affectsCreatureType",
    ] {
        if let Some(v) = spell.extra.get(key) {
            el.data.insert(key.to_string(), v.clone());
        }
    }
    el.maps_to = sh.maps_to(
        SrdKind::Spell,
        ent.ruleset,
        &spell.name,
        &spell.reprinted_as,
        &|uid| sh.reprint(RefKind::Spell, uid),
    );
    drain_renderer(st, &renderer, &ent.file, &spell.name);
    Conv {
        elements: vec![el],
        partial: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spell_lookup::SpellClassLookup;
    use borealis_content::Ruleset;

    fn ent(raw: Value) -> Ent {
        Ent {
            name: raw["name"].as_str().unwrap().to_string(),
            source: raw["source"].as_str().unwrap().to_string(),
            ruleset: crate::source::ruleset_of_entry(&raw),
            is_srd: true,
            file: "spells/spells-zzz.json".into(),
            raw,
        }
    }

    #[test]
    fn time_range_duration_text() {
        assert_eq!(
            casting_time(&[json!({"number": 1, "unit": "bonus"})]),
            "1 bonus action"
        );
        assert_eq!(
            casting_time(&[json!({"number": 10, "unit": "minute"})]),
            "10 minutes"
        );
        assert_eq!(
            casting_time(&[
                json!({"number": 1, "unit": "reaction", "condition": "which you take when {@b hit}"})
            ]),
            "1 reaction, which you take when hit"
        );
        assert_eq!(
            casting_time(&[
                json!({"number": 1, "unit": "action"}),
                json!({"number": 1, "unit": "hour"})
            ]),
            "1 action or 1 hour"
        );
        assert_eq!(casting_time(&[]), "1 action");
        assert_eq!(
            range_text(Some(
                &json!({"type": "point", "distance": {"type": "feet", "amount": 150}})
            )),
            "150 feet"
        );
        assert_eq!(
            range_text(Some(
                &json!({"type": "point", "distance": {"type": "miles", "amount": 1}})
            )),
            "1 mile"
        );
        assert_eq!(
            range_text(Some(
                &json!({"type": "point", "distance": {"type": "self"}})
            )),
            "Self"
        );
        assert_eq!(
            range_text(Some(
                &json!({"type": "point", "distance": {"type": "touch"}})
            )),
            "Touch"
        );
        assert_eq!(
            range_text(Some(
                &json!({"type": "cone", "distance": {"type": "feet", "amount": 15}})
            )),
            "Self (15-foot cone)"
        );
        assert_eq!(range_text(Some(&json!({"type": "special"}))), "Special");
        assert_eq!(
            range_text(Some(
                &json!({"type": "point", "distance": {"type": "unlimited"}})
            )),
            "Unlimited"
        );
        assert_eq!(range_text(None), "Special");
        assert_eq!(
            duration_text(&[json!({"type": "instant"})]),
            ("Instantaneous".to_string(), false)
        );
        assert_eq!(
            duration_text(&[
                json!({"type": "timed", "duration": {"type": "minute", "amount": 10}, "concentration": true})
            ]),
            ("Concentration, up to 10 minutes".to_string(), true)
        );
        assert_eq!(
            duration_text(&[json!({"type": "timed", "duration": {"type": "hour", "amount": 1}})]).0,
            "1 hour"
        );
        assert_eq!(
            duration_text(&[json!({"type": "permanent", "ends": ["dispel"]})]).0,
            "Until dispelled"
        );
        assert_eq!(duration_text(&[json!({"type": "special"})]).0, "Special");
        assert_eq!(duration_text(&[]).0, "Instantaneous");
    }

    #[test]
    fn fireball_like_spell() {
        let sh = Shared {
            spell_classes: SpellClassLookup::new(Some(json!({
                "zzz": {"blast": {"class": {"ZZZ": {"Sorcerer": true, "Wizard": true}},
                                  "subclass": {"ZZZ": {"Fighter": {"ZZZ": {"Eldritch Knight": {"name": "Eldritch Knight"}}}}}}}
            }))),
            ..Shared::default()
        };
        let mut st = State::default();
        let e = ent(json!({
            "name": "Blast", "source": "ZZZ", "level": 3, "school": "V",
            "time": [{"number": 1, "unit": "action"}],
            "range": {"type": "point", "distance": {"type": "feet", "amount": 150}},
            "components": {"v": true, "s": true, "m": {"text": "a pinch of sulfur", "cost": 5000, "consume": true}},
            "duration": [{"type": "instant"}],
            "entries": ["Deals {@damage 8d6} fire."],
            "entriesHigherLevel": [{"type": "entries", "name": "At Higher Levels", "entries": ["More."]}],
            "damageInflict": ["fire"], "savingThrow": ["dexterity"], "areaTags": ["S"],
            "scalingLevelDice": {"label": "x", "scaling": {"1": "1d6"}}
        }));
        let conv = convert(&sh, &mut st, &e);
        assert!(conv.partial.is_empty());
        let el = &conv.elements[0];
        assert_eq!(el.id, "ID_5ET_ZZZ_SPELL_BLAST");
        assert_eq!(el.ty, "Spell");
        assert_eq!(el.ruleset, Some(Ruleset::Dnd2014));
        assert_eq!(el.supports, vec!["Eldritch Knight", "Sorcerer", "Wizard"]);
        let get = |n: &str| el.setter_value(n).unwrap().to_string();
        assert_eq!(get("level"), "3");
        assert_eq!(get("school"), "Evocation");
        assert_eq!(get("time"), "1 action");
        assert_eq!(get("range"), "150 feet");
        assert_eq!(get("duration"), "Instantaneous");
        assert_eq!(get("hasMaterialComponent"), "true");
        assert_eq!(get("materialComponent"), "a pinch of sulfur");
        assert_eq!(get("materialCost"), "50");
        assert_eq!(get("materialConsumed"), "true");
        assert_eq!(get("isConcentration"), "false");
        assert_eq!(get("isRitual"), "false");
        assert_eq!(get("keywords"), "evocation, fire");
        assert!(el.description.contains("Deals 8d6 fire."));
        assert!(el.description.contains("<h4>At Higher Levels</h4>"));
        assert_eq!(el.data["damage_inflict"], json!(["fire"]));
        assert!(el.data.contains_key("scalingLevelDice"));
        for required in borealis_content::validate::required_setters("Spell") {
            assert!(el.setter(required).is_some(), "{required}");
        }
    }

    #[test]
    fn ritual_concentration_and_bad_shape() {
        let sh = Shared::default();
        let mut st = State::default();
        let e = ent(
            json!({"name": "Sense", "source": "ZZZ", "level": 1, "school": "D", "meta": {"ritual": true},
            "time": [{"number": 1, "unit": "action"}], "range": {"type": "point", "distance": {"type": "self"}},
            "components": {"v": true, "s": true, "m": true},
            "duration": [{"type": "timed", "duration": {"type": "minute", "amount": 10}, "concentration": true}], "entries": ["x"]}),
        );
        let el = &convert(&sh, &mut st, &e).elements[0];
        assert_eq!(el.setter_value("isRitual"), Some("true"));
        assert_eq!(el.setter_value("isConcentration"), Some("true"));
        assert_eq!(el.setter_value("hasMaterialComponent"), Some("true"));
        assert!(el.setter("materialComponent").is_none());
        assert!(el.supports.is_empty());
        let bad = ent(json!({"name": "Bad", "source": "ZZZ", "level": "high"}));
        assert!(convert(&sh, &mut st, &bad).elements.is_empty());
        assert_eq!(st.report.build().skipped.len(), 1);
    }
}
