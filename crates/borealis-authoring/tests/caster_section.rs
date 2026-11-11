//! Spec 4.6 and 18.1 (WB-17): the spellcasting section against every bundled SRD caster class (2014: SRD 5.1,
//! 2024: SRD 5.2.1).
//!
//! * **Builder output == SRD stored rules**: for each class, the section builder's slot rules for its caster type
//!   (with 2024 pact magic's `pact_magic` rule) and its multiclass slot grant are exactly the stored ones.
//! * **Read**: the class's Spellcasting feature opens with the right caster type, ability, preparation, list, swap,
//!   multiclass slots and counts; every stored rule of the section is held by a field, and what stays a rule block is
//!   only what the spec leaves to blocks (spellcasting focus, ritual casting, the spellbook and its choices).
//! * **Rename**: the class renamed, the section writes its stats and choices under the new name.

use borealis_authoring::caster::section::{self, SlotTable};
use borealis_authoring::expr_build::condition::ConditionContext;
use borealis_authoring::form::{
    element_group_to_form, element_to_form, form_to_element, form_to_elements, CatalogBlockCodec,
    FormContext, NoLookup,
};
use borealis_authoring::pack::granted_children;
use borealis_authoring::Edition;
use borealis_content::{ContentFile, Element};
use serde_json::{json, Value};
use std::path::Path;

fn classes(pack: &str) -> Vec<Element> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../content")
        .join(pack)
        .join("classes");
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .expect("classes")
        .map(|e| e.expect("entry").path())
        .collect();
    files.sort();
    files
        .iter()
        .flat_map(|f| {
            ContentFile::from_json_str(&std::fs::read_to_string(f).expect("file"))
                .expect("content")
                .elements
        })
        .collect()
}

/// (pack, class, caster type, multiclass grant, ability, preparation, swap).
const CASTERS: &[(&str, &str, &str, &str, &str, &str, bool)] = &[
    ("srd-5.1", "Bard", "full", "FULL", "Charisma", "known", true),
    (
        "srd-5.1", "Cleric", "full", "FULL", "Wisdom", "prepared", false,
    ),
    (
        "srd-5.1", "Druid", "full", "FULL", "Wisdom", "prepared", false,
    ),
    (
        "srd-5.1", "Paladin", "halfDown", "HALF", "Charisma", "prepared", false,
    ),
    (
        "srd-5.1", "Ranger", "halfDown", "HALF", "Wisdom", "known", true,
    ),
    (
        "srd-5.1", "Sorcerer", "full", "FULL", "Charisma", "known", true,
    ),
    (
        "srd-5.1", "Warlock", "pact", "SOLO", "Charisma", "known", true,
    ),
    (
        "srd-5.1",
        "Wizard",
        "full",
        "FULL",
        "Intelligence",
        "spellbook",
        false,
    ),
    (
        "srd-5.2.1",
        "Bard",
        "full",
        "FULL",
        "Charisma",
        "prepared",
        true,
    ),
    (
        "srd-5.2.1",
        "Cleric",
        "full",
        "FULL",
        "Wisdom",
        "prepared",
        true,
    ),
    (
        "srd-5.2.1",
        "Druid",
        "full",
        "FULL",
        "Wisdom",
        "prepared",
        true,
    ),
    (
        "srd-5.2.1",
        "Paladin",
        "halfUp",
        "HALF_UP",
        "Charisma",
        "prepared",
        true,
    ),
    (
        "srd-5.2.1",
        "Ranger",
        "halfUp",
        "HALF_UP",
        "Wisdom",
        "prepared",
        true,
    ),
    (
        "srd-5.2.1",
        "Sorcerer",
        "full",
        "FULL",
        "Charisma",
        "prepared",
        true,
    ),
    (
        "srd-5.2.1",
        "Warlock",
        "pact",
        "SOLO",
        "Charisma",
        "prepared",
        true,
    ),
    (
        "srd-5.2.1",
        "Wizard",
        "full",
        "FULL",
        "Intelligence",
        "spellbook",
        true,
    ),
];

fn feature<'e>(all: &'e [Element], class: &str) -> &'e Element {
    all.iter()
        .find(|e| {
            e.ty == "Class Feature"
                && e.spellcasting.as_ref().and_then(|s| s.name.as_deref()) == Some(class)
        })
        .unwrap_or_else(|| panic!("{class}'s spellcasting feature"))
}

fn edition(pack: &str) -> Edition {
    if pack == "srd-5.2.1" {
        Edition::Y2024
    } else {
        Edition::Y2014
    }
}

fn rules_json(e: &Element) -> Vec<Value> {
    e.rules
        .iter()
        .map(|r| serde_json::to_value(r).expect("rule"))
        .collect()
}

fn sorted(mut v: Vec<Value>) -> Vec<String> {
    let mut out: Vec<String> = v.drain(..).map(|x| x.to_string()).collect();
    out.sort();
    out
}

/// What the spec leaves to rule blocks: spellcasting focus, ritual casting, the spellbook and its choices.
fn is_block_part(r: &Value) -> bool {
    let id = r["id"].as_str().unwrap_or("");
    id.starts_with("ID_INTERNAL_PROFICIENCY_SPELLFOCUS_")
        || id == "ID_INTERNAL_RITUAL_CASTING"
        || r["kind"] == "spellbook"
        || r["name"]
            .as_str()
            .is_some_and(|n| n.starts_with("Spellbook ("))
}

fn cx<'a>(codec: &'a CatalogBlockCodec) -> FormContext<'a> {
    FormContext {
        conditions: ConditionContext::new([
            "Bard", "Cleric", "Druid", "Paladin", "Ranger", "Sorcerer", "Warlock", "Wizard",
        ]),
        lookup: &NoLookup,
        blocks: codec,
    }
}

#[test]
fn the_builder_writes_the_srd_classes_slot_rules_and_multiclass_grant() {
    for (pack, class, ty, mc, ..) in CASTERS {
        let all = classes(pack);
        let stored = rules_json(feature(&all, class));
        let table = SlotTable::from_key(ty, None).expect("a standard type");
        let mut built = section::slot_rules(&table, class, edition(pack));
        built.push(section::multiclass_rule(mc));
        let lc = class.to_lowercase();
        let mine: Vec<Value> = stored
            .iter()
            .filter(|r| {
                r["name"]
                    .as_str()
                    .is_some_and(|n| n.starts_with(&format!("{lc}:spellcasting:slot")))
                    || r["kind"] == "pact_magic"
                    || r["id"]
                        .as_str()
                        .is_some_and(|i| i.starts_with(section::MULTICLASS_GRANT_PREFIX))
            })
            .cloned()
            .collect();
        assert_eq!(sorted(built), sorted(mine), "{pack} {class}");
        assert_eq!(
            table.multiclass(),
            *mc,
            "{pack} {class}: the caster type's own grant"
        );
    }
}

#[test]
fn every_srd_caster_opens_with_its_section_read() {
    let codec = CatalogBlockCodec;
    let cx = cx(&codec);
    for (pack, class, ty, _, ability, preparation, swap) in CASTERS {
        let all = classes(pack);
        let e = feature(&all, class);
        let form = element_to_form(e, &cx);
        let f = |k: &str| form.fields.get(k).cloned().unwrap_or(Value::Null);
        let what = format!("{pack} {class}");
        assert_eq!(f("casts"), json!(true), "{what}");
        assert_eq!(f("casterType"), json!(ty), "{what}");
        assert_eq!(f("multiclassSlots"), json!("auto"), "{what}");
        assert_eq!(f("ability"), json!(ability), "{what}");
        assert_eq!(f("preparation"), json!(preparation), "{what}");
        assert_eq!(f("spellList"), json!(class), "{what}");
        assert_eq!(f("swap"), json!(swap), "{what}");
        assert_eq!(
            f("casterName"),
            json!(class),
            "{what}: read alone it keeps its name"
        );
        assert_eq!(f("slotTable"), json!([]), "{what}");
        assert!(
            !form.preserved.iter().any(|p| p.key == "spellcasting"),
            "{what}: the node is the section's"
        );
        // without its blocks, the form writes exactly the stored rules the spec gives the section
        let mut fields_only = form.clone();
        fields_only.blocks.clear();
        let held = rules_json(&form_to_element(&fields_only, &cx).expect("writes"));
        let section_rules: Vec<Value> = rules_json(e)
            .into_iter()
            .filter(|r| !is_block_part(r))
            .collect();
        assert_eq!(
            sorted(held),
            sorted(section_rules),
            "{what}: only focus, rituals and the spellbook stay blocks"
        );
        // counts: cantrips and prepared or known spells are held
        let cantrips = f("cantrips")["levels"].clone();
        let has_cantrips = !matches!(*class, "Paladin" | "Ranger");
        assert_eq!(
            cantrips.as_array().map(|a| a.iter().any(|n| n != 0)),
            Some(has_cantrips),
            "{what}"
        );
        if edition(pack) == Edition::Y2024 {
            assert_eq!(f("prepared").as_array().map(Vec::len), Some(20), "{what}");
            assert_ne!(f("preparedChange"), json!(""), "{what}");
        } else if *preparation == "known" {
            assert!(
                f("known")["levels"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|n| n != 0),
                "{what}"
            );
        } else {
            let half = *class == "Paladin";
            assert_eq!(
                f("prepareFormula"),
                json!(if half { "half" } else { "level" }),
                "{what}"
            );
        }
    }
}

#[test]
fn the_2014_spell_choices_keep_their_spelling() {
    let codec = CatalogBlockCodec;
    let cx = cx(&codec);
    let all = classes("srd-5.1");
    let known = |class: &str| element_to_form(feature(&all, class), &cx).fields["known"].clone();
    assert_eq!(
        known("Bard")["statistic"],
        false,
        "the Bard has no spells known stat"
    );
    assert_eq!(known("Sorcerer")["statistic"], true);
    assert_eq!(known("Sorcerer")["replace"], true);
    assert_eq!(known("Warlock")["levels"][19], 15);
    let ranger = known("Ranger");
    assert_eq!(
        ranger["levels"][0], 0,
        "the 2014 Ranger learns spells from level 2"
    );
    assert_eq!(ranger["levels"][1], 2);
}

#[test]
fn a_renamed_class_writes_its_section_under_the_new_name() {
    let codec = CatalogBlockCodec;
    let cx = cx(&codec);
    for pack in ["srd-5.1", "srd-5.2.1"] {
        let all = classes(pack);
        let class = all
            .iter()
            .find(|e| e.ty == "Class" && e.name == "Paladin")
            .expect("the Paladin");
        let kids = granted_children(class, &all);
        let mut form = element_group_to_form(class, &kids, &cx);
        let sc = form
            .children
            .iter()
            .position(|c| c.fields.get("casts") == Some(&json!(true)))
            .expect("its section");
        assert_eq!(
            form.children[sc].fields["casterName"],
            json!(""),
            "{pack}: it follows its class"
        );
        form.fields.insert("name".into(), json!("Warden"));
        let written = form_to_elements(&form, &cx).expect("writes");
        let feature = &written[sc + 1];
        let text = serde_json::to_string(&feature.rules).expect("rules");
        assert!(!text.contains("paladin:"), "{pack}: {text}");
        assert!(
            text.contains("warden:spellcasting:slots:1"),
            "{pack}: {text}"
        );
        assert_eq!(
            feature
                .spellcasting
                .as_ref()
                .and_then(|s| s.name.as_deref()),
            Some("Warden")
        );
        assert_eq!(
            feature
                .spellcasting
                .as_ref()
                .and_then(|s| s.list.as_ref())
                .map(|l| l.supports.as_str()),
            Some("Paladin"),
            "{pack}: the spell list stays the one it names"
        );
        // the class table is not touched by a rename
        assert_eq!(
            written[0].data.get("progression"),
            class.data.get("progression")
        );
    }
}
