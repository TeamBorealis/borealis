//! Languages, conditions, source descriptors and entries covered by the core pack.

use super::{drain_renderer, new_element, Conv, Ent, Shared, State};
use crate::core_ids;
use crate::ids::IdKind;
use crate::model::{self, Language, Reference};
use crate::source::default_source_name;
use crate::text::{strip_tags, RefKind, Renderer};
use borealis_content::{Element, Ruleset, Setter};
use serde_json::Value;

/// The `Source` element describing a pack's book.
#[must_use]
pub fn source_element(sh: &Shared, st: &mut State, abbr: &str, ruleset: Ruleset) -> Element {
    let id = st
        .ids
        .id(IdKind::Source, abbr, &[], &format!("source|{abbr}"));
    let name = sh.source_display(abbr);
    let mut e = Element::new(id, name, "Source", "Internal");
    e.ruleset = Some(ruleset);
    let known = default_source_name(abbr) != abbr;
    let playtest = abbr.to_uppercase().starts_with("UA");
    e.setters = vec![
        Setter::new("url", ""),
        Setter::new("abbreviation", abbr),
        Setter::new(
            if known && !playtest {
                "official"
            } else {
                "third-party"
            },
            "true",
        ),
    ];
    if playtest {
        e.setters.push(Setter::new("playtest", "true"));
    }
    e.setters.push(Setter::new(
        "information",
        "Imported from the user's local 5etools data",
    ));
    e.description =
        format!("<p>Imported from a local 5etools data folder ({abbr}). Not redistributable.</p>");
    e
}

/// Convert a language. Languages the core pack already defines are skipped.
pub fn convert_language(sh: &Shared, st: &mut State, ent: &Ent) -> Conv {
    let lang: Language = match model::parse(&ent.raw) {
        Ok(l) => l,
        Err(e) => {
            st.report.skipped(
                &ent.file,
                "language",
                &ent.label(),
                &format!("invalid shape: {e}"),
            );
            return Conv::default();
        }
    };
    if core_ids::core_language_id(&lang.name).is_some() {
        st.report.skipped(
            &ent.file,
            "language",
            &ent.label(),
            "defined by the core pack",
        );
        return Conv::default();
    }
    let resolver = |k: RefKind, n: &str, s: &str| sh.link(k, n, s);
    let mut renderer = Renderer::new(&resolver);
    let id = st.ids.id(IdKind::Language, "", &[&lang.name], &lang.name);
    let mut el = new_element(
        &id,
        &lang.name,
        "Language",
        &sh.source_display(&ent.source),
        ent.ruleset,
    );
    let kind_setter = match lang.language_type.as_deref() {
        Some("standard") => "standard",
        Some("exotic" | "rare") => "exotic",
        Some("secret") => "secret",
        _ => "monster",
    };
    el.setters.push(Setter::new(kind_setter, "true"));
    let speakers = ent
        .raw
        .get("typicalSpeakers")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(strip_tags)
                .collect::<Vec<_>>()
                .join(", ")
        })
        .or_else(|| {
            ent.raw
                .get("origin")
                .and_then(Value::as_str)
                .map(str::to_string)
        });
    if let Some(s) = speakers {
        el.setters.push(Setter::new("speakers", s));
    }
    if let Some(s) = &lang.script {
        el.setters.push(Setter::new("script", s.clone()));
    }
    el.description = renderer.entries(&lang.entries);
    drain_renderer(st, &renderer, &ent.file, &lang.name);
    Conv {
        elements: vec![el],
        partial: Vec::new(),
    }
}

/// Convert a condition, disease or status.
pub fn convert_condition(sh: &Shared, st: &mut State, ent: &Ent, tag: &str) -> Conv {
    let r: Reference = match model::parse(&ent.raw) {
        Ok(r) => r,
        Err(e) => {
            st.report
                .skipped(&ent.file, tag, &ent.label(), &format!("invalid shape: {e}"));
            return Conv::default();
        }
    };
    let resolver = |k: RefKind, n: &str, s: &str| sh.link(k, n, s);
    let mut renderer = Renderer::new(&resolver);
    let id = st.ids.id(
        IdKind::Condition,
        &ent.source,
        &[&r.name],
        &format!("{tag}|{}", r.name),
    );
    let mut el = new_element(
        &id,
        &r.name,
        "Condition",
        &sh.source_display(&ent.source),
        ent.ruleset,
    );
    el.supports = vec![crate::util::title_case(tag)];
    el.description = renderer.entries(&r.entries);
    drain_renderer(st, &renderer, &ent.file, &r.name);
    Conv {
        elements: vec![el],
        partial: Vec::new(),
    }
}

/// A weapon property a book defines (`Evo|AU` Evolving Item, `Vst|EGW` Vestige of
/// Divergence): a `Weapon Property` element the weapons with that code support. Core codes
/// are covered by the core pack.
pub fn convert_item_property(sh: &Shared, st: &mut State, ent: &Ent) -> Conv {
    let r: Reference = match model::parse(&ent.raw) {
        Ok(r) => r,
        Err(e) => {
            st.report.skipped(
                &ent.file,
                "itemProperty",
                &ent.label(),
                &format!("invalid shape: {e}"),
            );
            return Conv::default();
        }
    };
    let code = r.abbreviation.clone().unwrap_or_else(|| ent.name.clone());
    // the printed name and text sit in the first entries block
    let block = r.entries.first().and_then(Value::as_object);
    let name = block
        .and_then(|b| b.get("name"))
        .and_then(Value::as_str)
        .map_or_else(|| code.clone(), str::to_string);
    let body: Vec<Value> = block
        .and_then(|b| b.get("entries"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_else(|| r.entries.clone());
    let resolver = |k: RefKind, n: &str, s: &str| sh.link(k, n, s);
    let mut renderer = Renderer::new(&resolver);
    let id = crate::ids::source_property_id(&code, &ent.source);
    let mut el = new_element(
        &id,
        &name,
        "Weapon Property",
        &sh.source_display(&ent.source),
        ent.ruleset,
    );
    el.description = renderer.entries(&body);
    drain_renderer(st, &renderer, &ent.file, &name);
    Conv {
        elements: vec![el],
        partial: Vec::new(),
    }
}

/// Label of a 5etools character creation option type.
fn char_option_label(code: &str) -> &'static str {
    match code {
        "CS" => "Character Secret",
        "SG" => "Supernatural Gift",
        "DG" => "Dark Gift",
        "RF:B" => "Background Feature Option",
        _ => "Character Option",
    }
}

/// A character creation option (`charoption`): an IDRotF character secret, a MOT/EGW
/// supernatural gift, a VRGR dark gift, a replacement background feature. Kept as rules text
/// (`Rules Glossary Entry`, tagged with its kind); a secret is text by nature, the others
/// have rules no builder choice offers yet, and say so.
pub fn convert_char_option(sh: &Shared, st: &mut State, ent: &Ent) -> Conv {
    let r: Reference = match model::parse(&ent.raw) {
        Ok(r) => r,
        Err(e) => {
            st.report.skipped(
                &ent.file,
                "charoption",
                &ent.label(),
                &format!("invalid shape: {e}"),
            );
            return Conv::default();
        }
    };
    let resolver = |k: RefKind, n: &str, s: &str| sh.link(k, n, s);
    let mut renderer = Renderer::new(&resolver);
    let id = st.ids.named(IdKind::CharOption, &ent.source, &r.name);
    let mut el = new_element(
        &id,
        &r.name,
        "Rules Glossary Entry",
        &sh.source_display(&ent.source),
        ent.ruleset,
    );
    let codes: Vec<&str> = r
        .extra
        .get("optionType")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let labels: Vec<&str> = codes.iter().map(|c| char_option_label(c)).collect();
    el.supports = std::iter::once("Character Option")
        .chain(labels.iter().copied())
        .map(str::to_string)
        .collect();
    el.description = renderer.entries(&r.entries);
    drain_renderer(st, &renderer, &ent.file, &r.name);
    let partial = labels
        .iter()
        .filter(|l| **l != "Character Secret")
        .map(|l| format!("character option ({l}) kept as text: no builder choice offers it"))
        .take(1)
        .collect();
    Conv {
        elements: vec![el],
        partial,
    }
}

/// Catalog entries the core pack already covers are reported as skipped.
pub fn skip_catalog(st: &mut State, ent: &Ent, kind: &str) {
    st.report.skipped(
        &ent.file,
        kind,
        &ent.label(),
        "descriptive catalog entry covered by the core pack",
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ent(raw: Value) -> Ent {
        Ent {
            name: raw
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            source: raw["source"].as_str().unwrap().to_string(),
            ruleset: crate::source::ruleset_of_entry(&raw),
            is_srd: true,
            file: "languages.json".into(),
            raw,
        }
    }

    #[test]
    fn a_book_weapon_property_is_an_element() {
        let sh = Shared::default();
        let mut st = State::default();
        let evo = ent(json!({"abbreviation": "Evo", "source": "AU", "entries": [
            {"type": "entries", "name": "Evolving Item", "entries": ["It grows with you."]}]}));
        let conv = convert_item_property(&sh, &mut st, &evo);
        let el = &conv.elements[0];
        assert_eq!(el.id, "ID_5ET_AU_ITEMPROP_EVO");
        assert_eq!(
            (el.ty.as_str(), el.name.as_str()),
            ("Weapon Property", "Evolving Item")
        );
        assert!(conv.partial.is_empty());
    }

    #[test]
    fn character_options_are_kept_as_tagged_text() {
        let sh = Shared::default();
        let mut st = State::default();
        let gift = ent(
            json!({"name": "Lucky Star", "source": "ZZZ", "optionType": ["SG"],
            "entries": ["You were born under a lucky star."]}),
        );
        let conv = convert_char_option(&sh, &mut st, &gift);
        let el = &conv.elements[0];
        assert_eq!(el.id, "ID_5ET_ZZZ_CHAROPTION_LUCKY_STAR");
        assert_eq!(el.ty, "Rules Glossary Entry");
        assert_eq!(el.supports, ["Character Option", "Supernatural Gift"]);
        assert_eq!(
            conv.partial,
            ["character option (Supernatural Gift) kept as text: no builder choice offers it"]
        );
        assert_eq!(
            crate::report::reason_code(&conv.partial[0]),
            crate::report::code::CHARACTER_OPTION
        );
        // a secret is only text: nothing is lost
        let secret = ent(
            json!({"name": "Hidden Heir", "source": "ZZZ", "optionType": ["CS"],
            "entries": ["I am the heir."]}),
        );
        assert!(convert_char_option(&sh, &mut st, &secret)
            .partial
            .is_empty());
    }

    #[test]
    fn source_elements() {
        let sh = Shared::default();
        let mut st = State::default();
        let e = source_element(&sh, &mut st, "PHB", Ruleset::Dnd2014);
        assert_eq!(e.ty, "Source");
        assert_eq!(e.id, "ID_5ET_PHB_SOURCE");
        assert_eq!(e.name, "Player's Handbook (2014)");
        assert_eq!(e.setter_value("abbreviation"), Some("PHB"));
        assert!(e.setter("official").is_some());
        for r in borealis_content::validate::required_setters("Source") {
            assert!(e.setter(r).is_some());
        }
        let ua = source_element(&sh, &mut st, "UAFoo", Ruleset::Dnd2014);
        assert!(ua.setter("third-party").is_some());
        assert!(ua.setter("playtest").is_some());
        let homebrew = source_element(&sh, &mut st, "MYBREW", Ruleset::Dnd2014);
        assert_eq!(homebrew.name, "MYBREW");
        assert!(homebrew.setter("third-party").is_some());
    }

    #[test]
    fn languages_conditions_and_skips() {
        let sh = Shared::default();
        let mut st = State::default();
        let core = ent(json!({"name": "Elvish", "source": "PHB", "type": "standard"}));
        assert!(convert_language(&sh, &mut st, &core).elements.is_empty());
        let gith = ent(
            json!({"name": "Gith", "source": "MTF", "type": "exotic", "typicalSpeakers": ["{@creature githyanki|MM}"], "script": "Gith", "entries": ["Sharp."]}),
        );
        let el = &convert_language(&sh, &mut st, &gith).elements[0];
        assert_eq!(el.id, "ID_5ET_LANGUAGE_GITH");
        assert_eq!(el.setter_value("exotic"), Some("true"));
        assert_eq!(el.setter_value("speakers"), Some("githyanki"));
        assert_eq!(el.setter_value("script"), Some("Gith"));
        let rare =
            ent(json!({"name": "Gnoll", "source": "XPHB", "type": "rare", "origin": "Gnolls"}));
        let el = &convert_language(&sh, &mut st, &rare).elements[0];
        assert_eq!(el.setter_value("exotic"), Some("true"));
        assert_eq!(el.setter_value("speakers"), Some("Gnolls"));
        let odd = ent(json!({"name": "Beast Cry", "source": "MM", "type": "monstrous"}));
        assert_eq!(
            convert_language(&sh, &mut st, &odd).elements[0].setter_value("monster"),
            Some("true")
        );
        let cond = ent(
            json!({"name": "Blinded", "source": "PHB", "entries": [{"type": "list", "items": ["Can't see."]}]}),
        );
        let el = &convert_condition(&sh, &mut st, &cond, "condition").elements[0];
        assert_eq!(el.ty, "Condition");
        assert_eq!(el.supports, vec!["Condition"]);
        assert!(el.description.contains("<li>Can't see.</li>"));
        skip_catalog(&mut st, &cond, "skill");
        let bad = ent(json!({"name": "Bad", "source": "X", "entries": "no"}));
        assert!(convert_language(&sh, &mut st, &bad).elements.is_empty());
        assert!(convert_condition(&sh, &mut st, &bad, "disease")
            .elements
            .is_empty());
        assert_eq!(st.report.build().skipped.len(), 4);
    }
}
