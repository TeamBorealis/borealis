//! Copies (spec 11): a shallow copy drops what describes only the original and keeps the rest; a deep copy of a 2024
//! class copies its features, package and subclasses, and once written every grant names a copy; a species copies its
//! traits and lineage options, offered by id; ids inside expressions and id lists are re-pointed as whole tokens; the
//! edition checks. The bundled classes are in `src-tauri/tests/homebrew_copy.rs`.

use super::*;
use crate::expr_build::condition::ConditionContext;
use crate::form::{CatalogBlockCodec, NoLookup};
use crate::ids::IdGenerator;
use borealis_content::{Element, Rule};
use serde_json::{json, Value};

fn el(v: Value) -> Element {
    serde_json::from_value(v).expect("element")
}

fn with<T>(elements: &[Element], f: impl FnOnce(&FormContext<'_>) -> T) -> T {
    let codec = CatalogBlockCodec;
    let lookup = WithElements {
        base: &NoLookup,
        extra: elements,
    };
    let cx = FormContext {
        conditions: ConditionContext::new(["Knight"]),
        lookup: &lookup,
        blocks: &codec,
    };
    f(&cx)
}

fn ids(loaded: &[Element]) -> IdGenerator {
    IdGenerator::new(
        "HB",
        loaded.iter().map(|e| e.id.as_str()),
        Vec::<String>::new(),
    )
    .expect("generator")
}

/// A 2024 class with a feature at 1, its subclass choice at 3 and a package; one subclass with a feature; a 2014
/// subclass of another class with the same name (never copied).
fn knight() -> Vec<Element> {
    vec![
        el(
            json!({"id": "ID_X_CLASS_KNIGHT", "name": "Knight", "type": "Class", "source": "X", "ruleset": "dnd2024",
            "setters": [{"name": "hd", "value": "d10"}],
            "rules": [
                {"kind": "equipment_package", "ref": "ID_X_EQUIPMENT_PACKAGE_KNIGHT", "name": "Starting Equipment (Knight)"},
                {"kind": "grant", "id": "ID_X_CLASS_FEATURE_KNIGHT_VIGIL", "type": "Class Feature"},
                {"kind": "grant", "id": "ID_X_CLASS_FEATURE_KNIGHT_SUBCLASS", "type": "Class Feature", "level": 3}],
            "data": {"hit_die": 10, "features_by_level": ["Vigil"]},
            "renamed_from": "ID_X_CLASS_OLD_KNIGHT"}),
        ),
        el(
            json!({"id": "ID_X_EQUIPMENT_PACKAGE_KNIGHT", "name": "Starting Equipment (Knight)", "type": "Equipment Package",
            "source": "X", "ruleset": "dnd2024", "data": {"owner": "ID_X_CLASS_KNIGHT",
            "options": [{"label": "A", "items": [], "coins": {"gp": 10}}]}}),
        ),
        el(
            json!({"id": "ID_X_CLASS_FEATURE_KNIGHT_VIGIL", "name": "Vigil", "type": "Class Feature", "source": "X",
            "ruleset": "dnd2024", "requirements": "!ID_X_CLASS_FEATURE_KNIGHT_SUBCLASS",
            "data": {"class": "Knight", "level": 1, "core_text": "stale"}}),
        ),
        el(
            json!({"id": "ID_X_CLASS_FEATURE_KNIGHT_SUBCLASS", "name": "Knight Subclass", "type": "Class Feature",
            "source": "X", "ruleset": "dnd2024", "data": {"class": "Knight", "level": 3},
            "rules": [{"kind": "select", "type": "Archetype", "name": "Knightly Order", "supports": "Knight Subclass"}]}),
        ),
        el(
            json!({"id": "ID_X_SUBCLASS_KNIGHT_ROSE", "name": "Order of the Rose", "type": "Archetype", "source": "X",
            "ruleset": "dnd2024", "supports": ["Knight Subclass"],
            "data": {"class": "ID_X_CLASS_KNIGHT", "class_name": "Knight", "level_gained": 3},
            "rules": [{"kind": "grant", "id": "ID_X_SUBCLASS_FEATURE_KNIGHT_ROSE_THORN", "type": "Archetype Feature", "level": 3}]}),
        ),
        el(
            json!({"id": "ID_X_SUBCLASS_FEATURE_KNIGHT_ROSE_THORN", "name": "Thorn", "type": "Archetype Feature",
            "source": "X", "ruleset": "dnd2024",
            "data": {"class": "Knight", "subclass": "Order of the Rose", "level": 3}}),
        ),
        el(
            json!({"id": "ID_X_SUBCLASS_OTHER", "name": "Order of the Lily", "type": "Archetype", "source": "X",
            "ruleset": "dnd2014", "supports": ["Knight Subclass"]}),
        ),
    ]
}

fn written<'w>(w: &'w [Written], id: &str) -> &'w Element {
    &w.iter()
        .find(|x| x.element.id == id)
        .unwrap_or_else(|| {
            panic!(
                "{id} in {:?}",
                w.iter().map(|x| &x.element.id).collect::<Vec<_>>()
            )
        })
        .element
}

#[test]
fn a_shallow_copy_drops_what_describes_the_original() {
    let content = knight();
    let feature = &content[2];
    with(&content, |cx| {
        let copy = copy_element(feature, cx, false);
        let f = &copy.form;
        assert_eq!(f.id(), None);
        assert_eq!(f.fields["name"], json!("Vigil (copy)"));
        assert_eq!(
            f.fields["edition"],
            json!("dnd2024"),
            "it keeps its edition"
        );
        let kept: Vec<&str> = f.preserved.iter().map(|p| p.key.as_str()).collect();
        assert_eq!(
            kept,
            ["data.class", "data.level"],
            "the mirror key is dropped"
        );
        assert_eq!(
            copy.extras.links,
            [CopyLink {
                key: "e".into(),
                original: feature.id.clone(),
                from: None
            }]
        );
        let shallow = copy_element(&content[0], cx, false);
        assert!(shallow.form.children.is_empty() && shallow.extras.companions.is_empty());
        assert!(
            !shallow
                .form
                .preserved
                .iter()
                .any(|p| p.key == "renamed_from"),
            "what it replaces describes the original"
        );
        let mut g = ids(&content);
        let form = assign_ids(&shallow.form, &mut g);
        let out = write_copy(&form, &shallow.extras, cx, &mut g).expect("writes");
        let class = &out[0].element;
        assert_eq!(class.id, "ID_HB_CLASS_KNIGHT_COPY");
        assert!(
            class
                .rules
                .iter()
                .any(|r| matches!(r, Rule::Grant(g) if g.id == "ID_X_CLASS_FEATURE_KNIGHT_VIGIL")),
            "without deep, the copy grants the original features"
        );
    });
}

#[test]
fn a_deep_class_copy_copies_its_features_package_and_subclasses() {
    let content = knight();
    with(&content, |cx| {
        let copy = copy_element(&content[0], cx, true);
        let names: Vec<&str> = copy
            .form
            .children
            .iter()
            .map(|c| c.fields["name"].as_str().unwrap_or_default())
            .collect();
        assert_eq!(
            names,
            ["Starting Equipment (Knight)", "Vigil", "Knight Subclass"]
        );
        assert!(copy.form.flatten().iter().all(|f| f.id().is_none()));
        assert_eq!(
            copy.extras.companions.len(),
            1,
            "the 2014 one is not offered"
        );
        let sub = &copy.extras.companions[0];
        assert_eq!(sub.key, "s1");
        assert_eq!(sub.children[0].key, "s1/0");
        assert_eq!(copy.extras.links.len(), 6);

        let mut g = ids(&content);
        let form = assign_ids(&copy.form, &mut g);
        let out = write_copy(&form, &copy.extras, cx, &mut g).expect("writes");
        let all: Vec<&str> = out.iter().map(|w| w.element.id.as_str()).collect();
        assert_eq!(
            all,
            [
                "ID_HB_CLASS_KNIGHT_COPY",
                "ID_HB_EQUIPMENT_PACKAGE_KNIGHT_COPY_STARTING_EQUIPMENT_KNIGHT",
                "ID_HB_CLASS_FEATURE_KNIGHT_COPY_VIGIL",
                "ID_HB_CLASS_FEATURE_KNIGHT_COPY_KNIGHT_SUBCLASS",
                "ID_HB_SUBCLASS_ORDER_OF_THE_ROSE",
                "ID_HB_SUBCLASS_FEATURE_ORDER_OF_THE_ROSE_THORN",
            ]
        );
        let text = serde_json::to_string(&out.iter().map(|w| &w.element).collect::<Vec<_>>())
            .expect("json");
        assert!(!text.contains("ID_X_"), "no original is named: {text}");
        let class = written(&out, "ID_HB_CLASS_KNIGHT_COPY");
        assert!(class.renamed_from.is_none());
        assert!(!class.data.contains_key("features_by_level"));
        let vigil = written(&out, "ID_HB_CLASS_FEATURE_KNIGHT_COPY_VIGIL");
        assert_eq!(vigil.data["class"], json!("Knight (copy)"));
        assert!(!vigil.data.contains_key("core_text"));
        assert_eq!(
            vigil.requirements,
            "!ID_HB_CLASS_FEATURE_KNIGHT_COPY_KNIGHT_SUBCLASS"
        );
        let choice = written(&out, "ID_HB_CLASS_FEATURE_KNIGHT_COPY_KNIGHT_SUBCLASS");
        assert!(
            serde_json::to_string(&choice.rules)
                .unwrap()
                .contains("Knight copy Subclass"),
            "{:?}",
            choice.rules
        );
        let package = written(
            &out,
            "ID_HB_EQUIPMENT_PACKAGE_KNIGHT_COPY_STARTING_EQUIPMENT_KNIGHT",
        );
        assert_eq!(package.data["owner"], json!("ID_HB_CLASS_KNIGHT_COPY"));
        let sub = written(&out, "ID_HB_SUBCLASS_ORDER_OF_THE_ROSE");
        assert_eq!(sub.supports, ["Knight copy Subclass"]);
        assert_eq!(sub.data["class"], json!("ID_HB_CLASS_KNIGHT_COPY"));
        let thorn = written(&out, "ID_HB_SUBCLASS_FEATURE_ORDER_OF_THE_ROSE_THORN");
        assert_eq!(thorn.data["class"], json!("Knight (copy)"));
    });
}

/// A 2014 class whose subclass choice is a feature at level 2 offering its label ("Arcane Tradition", the 2014 SRD
/// shape); two subclasses of it (one naming the class, one only carrying the label, as the 2014 SRD's do) and one of
/// another class under the same label.
fn mage() -> Vec<Element> {
    vec![
        el(
            json!({"id": "ID_Y_CLASS_MAGE", "name": "Mage", "type": "Class", "source": "Y", "ruleset": "dnd2014",
            "setters": [{"name": "hd", "value": "d6"}],
            "rules": [{"kind": "grant", "id": "ID_Y_CLASS_FEATURE_MAGE_TRADITION", "type": "Class Feature", "level": 2}]}),
        ),
        el(
            json!({"id": "ID_Y_CLASS_FEATURE_MAGE_TRADITION", "name": "Arcane Tradition", "type": "Class Feature",
            "source": "Y", "ruleset": "dnd2014",
            "rules": [{"kind": "select", "type": "Archetype", "name": "Arcane Tradition", "level": 2,
                       "supports": "Arcane Tradition"}]}),
        ),
        el(
            json!({"id": "ID_Y_ARCHETYPE_EMBERS", "name": "School of Embers", "type": "Archetype", "source": "Y",
            "ruleset": "dnd2014", "supports": ["Arcane Tradition"], "data": {"class": "ID_Y_CLASS_MAGE"},
            "rules": [{"kind": "grant", "id": "ID_Y_ARCHETYPE_FEATURE_EMBERS_SPARK", "type": "Archetype Feature", "level": 2}]}),
        ),
        el(
            json!({"id": "ID_Y_ARCHETYPE_FEATURE_EMBERS_SPARK", "name": "Spark", "type": "Archetype Feature", "source": "Y",
            "ruleset": "dnd2014"}),
        ),
        el(
            json!({"id": "ID_Y_ARCHETYPE_FROST", "name": "School of Frost", "type": "Archetype", "source": "Y",
            "ruleset": "dnd2014", "supports": ["Arcane Tradition"]}),
        ),
        el(
            json!({"id": "ID_Y_ARCHETYPE_MIST", "name": "Path of Mist", "type": "Archetype", "source": "Y",
            "ruleset": "dnd2014", "supports": ["Arcane Tradition"], "data": {"class": "ID_Y_CLASS_SEER"}}),
        ),
    ]
}

/// Hy6pc (WB-22 difference 1): a 2014 class's deep copy offers its subclasses by a tag of its own (written from its
/// name, as a 2024 copy's) at the same level and under the same label, and copies its subclasses with that tag, so the
/// original class never offers the copies and the copy never offers the originals.
#[test]
fn a_deep_copy_of_a_2014_class_copies_its_subclasses_under_its_own_tag() {
    let content = mage();
    with(&content, |cx| {
        // Arrange
        let copy = copy_element(&content[0], cx, true);
        let names: Vec<&str> = copy
            .extras
            .companions
            .iter()
            .map(|c| c.fields["name"].as_str().unwrap_or_default())
            .collect();
        assert_eq!(
            names,
            ["School of Embers", "School of Frost"],
            "not another class's"
        );

        // Act
        let mut g = ids(&content);
        let form = assign_ids(&copy.form, &mut g);
        let out = write_copy(&form, &copy.extras, cx, &mut g).expect("writes");

        // Assert
        let choice = written(&out, "ID_HB_CLASS_FEATURE_MAGE_COPY_ARCANE_TRADITION");
        let expected: Rule = serde_json::from_value(json!({"kind": "select", "type": "Archetype",
            "name": "Arcane Tradition", "level": 2, "supports": "Mage copy Subclass"}))
        .unwrap();
        assert_eq!(choice.rules, [expected]);
        for id in [
            "ID_HB_SUBCLASS_SCHOOL_OF_EMBERS",
            "ID_HB_SUBCLASS_SCHOOL_OF_FROST",
        ] {
            let sub = written(&out, id);
            assert_eq!(sub.supports, ["Mage copy Subclass"], "{id}");
        }
        let embers = written(&out, "ID_HB_SUBCLASS_SCHOOL_OF_EMBERS");
        assert_eq!(embers.data["class"], json!("ID_HB_CLASS_MAGE_COPY"));
        let text =
            serde_json::to_string(&out.iter().map(|w| &w.element).collect::<Vec<_>>()).unwrap();
        assert!(!text.contains("ID_Y_"), "no original is named: {text}");

        // the copied choice reads back as the subclass choice and writes the same rule (the field keeps its shape)
        let read = read_one(choice, "e".to_string(), cx);
        assert_eq!(read.fields["subclassLabel"], json!("Arcane Tradition"));
        assert!(read.blocks.is_empty(), "taken by the field, not a block");
        let again = form_to_elements(&read, cx).expect("writes");
        assert_eq!(again[0].rules, choice.rules);
    });
}

/// A 2024 species with a darkvision trait and a lineage trait whose choice offers two options by tag.
fn elf() -> Vec<Element> {
    vec![
        el(
            json!({"id": "ID_X_SPECIES_ELF", "name": "Elf", "type": "Species", "source": "X", "ruleset": "dnd2024",
            "rules": [{"kind": "grant", "id": "ID_X_TRAIT_ELF_SIGHT", "type": "Racial Trait"},
                {"kind": "grant", "id": "ID_X_TRAIT_ELF_LINEAGE", "type": "Racial Trait"}],
            "data": {"traits": ["Sight", "Lineage"], "creature_type": "Humanoid"}}),
        ),
        el(
            json!({"id": "ID_X_TRAIT_ELF_SIGHT", "name": "Sight", "type": "Racial Trait", "source": "X", "ruleset": "dnd2024"}),
        ),
        el(
            json!({"id": "ID_X_TRAIT_ELF_LINEAGE", "name": "Lineage", "type": "Racial Trait", "source": "X", "ruleset": "dnd2024",
            "rules": [{"kind": "select", "type": "Species Trait Option", "name": "Elven Lineage", "supports": "Elven Lineage"}]}),
        ),
        el(
            json!({"id": "ID_X_OPTION_HIGH", "name": "High Elf", "type": "Species Trait Option", "source": "X",
            "ruleset": "dnd2024", "supports": ["Elven Lineage"], "data": {"lineage": "Elven"}}),
        ),
        el(
            json!({"id": "ID_X_OPTION_WOOD", "name": "Wood Elf", "type": "Species Trait Option", "source": "X",
            "ruleset": "dnd2024", "supports": ["Elven Lineage"]}),
        ),
    ]
}

#[test]
fn a_species_copies_its_traits_and_lineage_options_offered_by_id() {
    let content = elf();
    with(&content, |cx| {
        let copy = copy_element(&content[0], cx, true);
        assert_eq!(copy.form.children.len(), 2);
        assert_eq!(copy.form.children[1].children.len(), 2, "the options");
        assert_eq!(copy.extras.tags.len(), 1);
        let mut g = ids(&content);
        let form = assign_ids(&copy.form, &mut g);
        let out = write_copy(&form, &copy.extras, cx, &mut g).expect("writes");
        let species = &out[0].element;
        let granted: Vec<&str> = species
            .rules
            .iter()
            .filter_map(|r| match r {
                Rule::Grant(g) => Some(g.id.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(
            granted,
            ["ID_HB_TRAIT_ELF_COPY_SIGHT", "ID_HB_TRAIT_ELF_COPY_LINEAGE"]
        );
        let lineage = written(&out, "ID_HB_TRAIT_ELF_COPY_LINEAGE");
        let Rule::Select(s) = &lineage.rules[0] else {
            panic!("{:?}", lineage.rules)
        };
        assert_eq!(
            s.supports.as_deref(),
            Some("ID_HB_TRAIT_OPTION_LINEAGE_HIGH_ELF|ID_HB_TRAIT_OPTION_LINEAGE_WOOD_ELF")
        );
        let high = written(&out, "ID_HB_TRAIT_OPTION_LINEAGE_HIGH_ELF");
        assert!(
            high.supports.is_empty(),
            "the original's choice never offers the copy"
        );
        assert!(!high.data.contains_key("lineage"), "a mirror key");
    });
}

#[test]
fn ids_are_replaced_as_whole_tokens() {
    let map = vec![("ID_A".to_string(), "ID_N".to_string())];
    assert_eq!(
        replace_tokens("ID_A|ID_AB|[ID_A] && !ID_A", &map).as_deref(),
        Some("ID_N|ID_AB|[ID_N] && !ID_N")
    );
    assert_eq!(replace_tokens("ID_AB", &map), None);
    assert_eq!(replace_tokens("no ids", &map), None);
}

#[test]
fn a_class_keeps_its_edition_and_other_kinds_are_adapted() {
    let content = knight();
    with(&content, |cx| {
        let copy = copy_element(&content[0], cx, false).form;
        assert!(edition_problems(&copy, Ruleset::Dnd2024).is_empty());
        let changed = copy.with_field("edition", json!("dnd2014"));
        let ps = edition_problems(&changed, Ruleset::Dnd2024);
        assert_eq!(ps.len(), 1);
        assert!(ps[0].is_error());
        assert_eq!(
            ps[0].message,
            "A class keeps its edition (2024). Make a new class for the other edition."
        );
        let feature = copy_element(&content[2], cx, false).form;
        let ps = edition_problems(&feature, Ruleset::Dnd2014);
        assert_eq!(ps.len(), 1);
        assert!(!ps[0].is_error());
        assert_eq!(
            ps[0].message,
            "2024 content in a 2014 pack is adapted when a character takes it."
        );
        assert!(edition_problems(&feature, Ruleset::Any).is_empty());
        assert!(edition_problems(&feature, Ruleset::Dnd2024).is_empty());
    });
}

#[test]
fn an_override_form_keeps_every_id_and_name_and_writes_the_originals_back() {
    let content = knight();
    with(&content, |cx| {
        let form = override_form(&content[0], cx);
        assert_eq!(form.id(), Some("ID_X_CLASS_KNIGHT"));
        assert_eq!(form.fields["name"], json!("Knight"), "no (copy)");
        assert!(
            form.preserved.iter().any(|p| p.key == "renamed_from"),
            "an override keeps what describes the original"
        );
        let children: Vec<Option<&str>> = form.children.iter().map(FormState::id).collect();
        assert!(
            children.contains(&Some("ID_X_CLASS_FEATURE_KNIGHT_VIGIL")),
            "{children:?}"
        );
        let written = form_to_elements(&form, cx).expect("writes");
        assert_eq!(written[0].id, "ID_X_CLASS_KNIGHT");
        let vigil = written
            .iter()
            .find(|e| e.id == "ID_X_CLASS_FEATURE_KNIGHT_VIGIL")
            .expect("the feature keeps its id");
        assert!(
            content[2].equals_with_ruleset(vigil, content[2].ruleset),
            "an unchanged child writes back as its original"
        );
    });
}
