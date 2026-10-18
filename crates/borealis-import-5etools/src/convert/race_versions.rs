//! Races whose 5etools `_versions` are one choice of the race: the XPHB Dragonborn ancestries,
//! Gnome and Elf lineages, Goliath giant ancestries, Tiefling legacies (and the MPMM Aasimar,
//! Kobold, Shifter forms). As the bundled SRD 5.2.1 species, the race is offered once, its
//! replaced trait (`Gnomish Lineage`) selects one option element per version, and each option
//! carries what its version changes. The expanded versions stay elements of their own (the
//! former species), no longer offered, so characters saved with one keep working.

use super::build::select;
use super::race::{convert_plain, race_key, split_traits};
use super::{drain_renderer, new_element, sheet_text, short_text, Conv, Ent, Shared, State};
use crate::copy::{VERSIONS, VERSION_OF, VERSION_TRAIT};
use crate::ids::IdKind;
use crate::text::{RefKind, Renderer};
use borealis_content::{Element, Rule, Ruleset};
use serde_json::{json, Value};

/// The entry without the version markers.
fn unmarked(raw: &Value) -> Value {
    let mut v = raw.clone();
    if let Value::Object(m) = &mut v {
        for k in [VERSIONS, VERSION_TRAIT, VERSION_OF] {
            m.remove(k);
        }
    }
    v
}

fn with_raw(ent: &Ent, raw: Value) -> Ent {
    let name = raw
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or(&ent.name)
        .to_string();
    Ent {
        raw,
        name,
        ..ent.clone()
    }
}

/// Option name of a version: `Gnome; Forest Gnome Lineage` -> `Forest Gnome Lineage`,
/// `Dragonborn (Black)` -> `Black`.
#[must_use]
pub fn version_label(race: &str, version: &str) -> String {
    if let Some(rest) = version.strip_prefix(&format!("{race}; ")) {
        return rest.trim().to_string();
    }
    if let Some(inner) = version
        .strip_prefix(&format!("{race} ("))
        .and_then(|r| r.strip_suffix(')'))
    {
        return inner.trim().to_string();
    }
    version.to_string()
}

fn is_trait_grant(r: &Rule) -> bool {
    matches!(r, Rule::Grant(g) if g.ty.as_deref() == Some("Racial Trait"))
}

/// The race with its versions as options, or `None` for a race without such versions.
pub fn convert(sh: &Shared, st: &mut State, ent: &Ent) -> Option<Conv> {
    let trait_name = ent.raw.get(VERSION_TRAIT)?.as_str()?.to_string();
    let versions: Vec<Value> = ent.raw.get(VERSIONS)?.as_array()?.clone();
    let mut conv = convert_plain(sh, st, &with_raw(ent, unmarked(&ent.raw)), false);
    if conv.elements.is_empty() {
        return Some(conv);
    }
    let mut converted: Vec<(Ent, Element)> = Vec::new();
    for v in &versions {
        let vent = with_raw(ent, unmarked(v));
        let mut vc = convert_plain(sh, st, &vent, false);
        if vc.elements.is_empty() {
            conv.partial
                .push(format!("version {:?} could not be converted", vent.name));
            continue;
        }
        converted.push((vent, vc.elements.swap_remove(0)));
    }
    if converted.is_empty() {
        return Some(conv);
    }
    let own: Vec<Vec<Rule>> = converted
        .iter()
        .map(|(_, e)| {
            e.rules
                .iter()
                .filter(|r| !is_trait_grant(r))
                .cloned()
                .collect()
        })
        .collect();
    let base_rules = conv.elements[0].rules.clone();
    // a rule of the race that no version keeps (Dragonborn's free resistance choice) is the
    // versions' to give
    conv.elements[0]
        .rules
        .retain(|r| is_trait_grant(r) || own.iter().any(|rules| rules.contains(r)));

    let base_id = conv.elements[0].id.clone();
    // the race's own alternative spell groups (XPHB Elf) went with a dropped select: every
    // version states its spells itself
    let keeps_own_options = conv.elements[0]
        .rules
        .iter()
        .any(|r| matches!(r, Rule::Select(s) if s.supports.as_deref() == Some(base_id.as_str())));
    if !keeps_own_options {
        conv.elements.retain(|e| {
            e.id == base_id || !matches!(e.supports.as_slice(), [only] if *only == base_id)
        });
    }
    let base_name = conv.elements[0].name.clone();
    let source_display = conv.elements[0].source.clone();
    let is_2024 = ent.ruleset == Ruleset::Dnd2024;
    let option_ty = if is_2024 {
        "Species Trait Option"
    } else {
        "Racial Trait"
    };
    // the select sits on the trait the versions replace, as the SRD's Gnomish Lineage
    let holder = conv
        .elements
        .iter()
        .position(|e| e.ty == "Racial Trait" && e.name == trait_name)
        .unwrap_or(0);
    let tag = if holder == 0 {
        format!("{base_id}_VERSION")
    } else {
        conv.elements[holder].id.clone()
    };
    conv.elements[holder]
        .rules
        .push(Rule::Select(select(option_ty, &trait_name, Some(&tag), 1)));

    let (_, base_traits) = split_traits(entries_of(&ent.raw));
    let resolver = |k: RefKind, n: &str, s: &str| sh.link(k, n, s);
    let mut renderer = Renderer::new(&resolver);
    let key = race_key(&base_name, &ent.source);
    for ((vent, vel), rules) in converted.iter().zip(own) {
        let label = version_label(&base_name, &vent.name);
        let opt_id = st.ids.id(
            IdKind::SpeciesOption,
            &ent.source,
            &[&base_name, &label],
            &format!("{key}|version|{label}"),
        );
        let mut oe = new_element(&opt_id, &label, option_ty, &source_display, ent.ruleset);
        oe.supports = vec![tag.clone()];
        oe.rules = rules
            .into_iter()
            .filter(|r| !base_rules.contains(r))
            .collect();
        if vel.spellcasting != conv.elements[0].spellcasting {
            oe.spellcasting.clone_from(&vel.spellcasting);
        }
        let (_, traits) = split_traits(entries_of(&vent.raw));
        let changed: Vec<Value> = traits
            .into_iter()
            .filter(|t| !base_traits.contains(t))
            .map(|(name, body)| json!({"type": "entries", "name": name, "entries": body}))
            .collect();
        oe.description = renderer.entries(&changed);
        oe.sheet = Some(sheet_text(&short_text(&Renderer::plain(&changed), 300)));
        oe.data
            .insert("version_of".into(), Value::String(base_id.clone()));
        oe.data
            .insert("former_id".into(), Value::String(vel.id.clone()));
        conv.elements.push(oe);
    }
    drain_renderer(st, &renderer, &ent.file, &base_name);
    Some(conv)
}

fn entries_of(raw: &Value) -> &[Value] {
    raw.get("entries")
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

/// A version of such a race, converted on its own (the former species): kept for characters
/// saved with it, but no longer offered for new picks (it requires the race it is a version
/// of, which a character choosing a species never has; the engine keeps a stored pick).
pub fn keep_former_version(sh: &Shared, ent: &Ent, conv: &mut Conv) {
    let Some(of) = ent.raw.get(VERSION_OF) else {
        return;
    };
    let field = |k: &str| of.get(k).and_then(Value::as_str).unwrap_or_default();
    let Some(base_id) = sh.race_ids.get(&race_key(field("name"), field("source"))) else {
        return;
    };
    if let Some(el) = conv.elements.first_mut() {
        el.requirements.clone_from(base_id);
        el.data
            .insert("version_of".into(), Value::String(base_id.clone()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::copy::resolve_array;

    fn ent(raw: Value) -> Ent {
        Ent {
            name: raw["name"].as_str().unwrap().to_string(),
            source: raw["source"].as_str().unwrap().to_string(),
            ruleset: crate::source::ruleset_of_entry(&raw),
            is_srd: false,
            file: "races.json".into(),
            raw,
        }
    }

    fn dragonborn() -> Value {
        json!({"name": "Dragonborn", "source": "ZZZ", "edition": "one", "size": ["M"], "speed": 30,
            "darkvision": 60, "resist": [{"choose": {"from": ["acid", "fire"]}}],
            "entries": [
                {"type": "entries", "name": "Draconic Ancestry", "entries": ["Choose a dragon."]},
                {"type": "entries", "name": "Damage Resistance", "entries": ["Resistance to your type."]},
                {"type": "entries", "name": "Darkvision", "entries": ["60 feet."]}],
            "_versions": [{"_abstract": {"name": "Dragonborn ({{color}})", "source": "ZZZ",
                "_mod": {"entries": [{"mode": "removeArr", "names": "Draconic Ancestry"},
                    {"mode": "replaceArr", "replace": "Damage Resistance",
                     "items": {"type": "entries", "name": "Damage Resistance", "entries": ["Resistance to {{damageType}}."]}}]}},
                "_implementations": [
                    {"_variables": {"color": "Black", "damageType": "acid"}, "resist": ["acid"]},
                    {"_variables": {"color": "Red", "damageType": "fire"}, "resist": ["fire"]}]}]})
    }

    #[test]
    fn versions_become_options_of_the_replaced_trait() {
        let resolved = resolve_array("race", &[dragonborn()]);
        assert_eq!(resolved.entries.len(), 3, "the race and its two versions");
        assert_eq!(resolved.entries[0][VERSION_TRAIT], "Draconic Ancestry");
        let mut sh = Shared::default();
        let mut st = State::default();
        let conv =
            super::super::race::convert(&sh, &mut st, &ent(resolved.entries[0].clone()), false);
        let base = &conv.elements[0];
        assert_eq!(base.ty, "Species");
        assert!(
            !base
                .rules
                .iter()
                .any(|r| matches!(r, Rule::Select(s) if s.ty == "Condition")),
            "the free resistance choice is the versions' to give"
        );
        let holder = conv
            .elements
            .iter()
            .find(|e| e.name == "Draconic Ancestry")
            .unwrap();
        let Some(Rule::Select(sel)) = holder.rules.last() else {
            panic!("no select")
        };
        assert_eq!(sel.ty, "Species Trait Option");
        assert_eq!(sel.supports.as_deref(), Some(holder.id.as_str()));
        let options: Vec<&Element> = conv
            .elements
            .iter()
            .filter(|e| e.ty == "Species Trait Option")
            .collect();
        assert_eq!(
            options.iter().map(|o| o.name.as_str()).collect::<Vec<_>>(),
            ["Black", "Red"]
        );
        assert_eq!(options[0].supports, std::slice::from_ref(&holder.id));
        assert!(options[0].rules.iter().any(
            |r| matches!(r, Rule::Grant(g) if g.ty.as_deref() == Some("Condition") && g.id.contains("ACID"))
        ));
        assert!(!options[0]
            .rules
            .iter()
            .any(|r| matches!(r, Rule::Grant(g) if g.id.contains("DARKVISION"))));
        assert!(options[0].description.contains("acid"));
        assert_eq!(
            options[0].data["former_id"],
            "ID_5ET_ZZZ_SPECIES_DRAGONBORN_BLACK"
        );

        // the former version species stays for saved characters, no longer offered
        sh.race_ids
            .insert(race_key("Dragonborn", "ZZZ"), base.id.clone());
        let former =
            super::super::race::convert(&sh, &mut st, &ent(resolved.entries[1].clone()), false);
        assert_eq!(former.elements[0].id, "ID_5ET_ZZZ_SPECIES_DRAGONBORN_BLACK");
        assert_eq!(former.elements[0].requirements, base.id);
    }

    #[test]
    fn labels_and_unshared_traits() {
        assert_eq!(
            version_label("Gnome", "Gnome; Forest Gnome Lineage"),
            "Forest Gnome Lineage"
        );
        assert_eq!(version_label("Dragonborn", "Dragonborn (Gold)"), "Gold");
        assert_eq!(version_label("Elf", "Drow"), "Drow");
        // versions naming different traits stay separate species
        let mut raw = dragonborn();
        raw["_versions"] = json!([
            {"name": "Dragonborn; A", "source": "ZZZ", "_mod": {"entries": {"mode": "replaceArr", "replace": "Darkvision", "items": []}}},
            {"name": "Dragonborn; B", "source": "ZZZ", "_mod": {"entries": {"mode": "replaceArr", "replace": "Damage Resistance", "items": []}}}]);
        let resolved = resolve_array("race", &[raw]);
        assert!(resolved.entries[0].get(VERSION_TRAIT).is_none());
        assert!(resolved.entries[1].get(VERSION_OF).is_none());
    }
}
