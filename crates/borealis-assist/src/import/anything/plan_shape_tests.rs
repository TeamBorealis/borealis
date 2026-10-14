//! Plan shapes from invented sources (no model): trait sub-headings the outline skipped,
//! source-defined conditions, and a class whose spellcasting section has another name.

use std::path::Path;

use borealis_content::{ContentStore, ContentStoreBuilder};
use serde_json::{json, Value};

use super::types::{Edition, ImportKind, ImportPlan, Outline, PlanGroup, Role};

fn library() -> ContentStore {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let mut b = ContentStoreBuilder::new();
    for pack in ["core", "srd-5.1"] {
        b = b.add_pack_dir(&root.join(pack)).expect("pack loads");
    }
    b.build()
}

fn group(kind: ImportKind, outline: Value) -> PlanGroup {
    let o: Outline = serde_json::from_value(outline).expect("outline");
    let mut g = super::plan::standalone(kind, &o.name, None);
    g.outline = Some(o);
    g
}

fn rebuilt(groups: Vec<PlanGroup>, source: &str) -> ImportPlan {
    let mut plan = ImportPlan {
        edition: Edition::Y2014,
        groups,
        elements: Vec::new(),
        notes: Vec::new(),
    };
    super::plan::rebuild(&mut plan, &library(), "ID_T", source);
    plan
}

const SPECIES: &str = "# Glasskin\n\n## Traits\n\n### Prism Body\n\nYou are made of glass.\n\n\
Focus Stone\nA focus stone is a gem.\n\n#### Spells\n\nYou know the light cantrip.\n\n\
#### Glare Table\n\n| d6 | Glare |\n| 1 | dim |\n\n#### Marsh Glasskin\n\nA subrace.\n\n\
### Languages\n\nYou speak Common.\n";

#[test]
fn a_trait_sub_heading_the_outline_skipped_is_its_own_trait() {
    let species = group(
        ImportKind::Species,
        json!({"name": "Glasskin", "summary": "", "quote": "",
            "features": [{"level": 1, "name": "Prism Body", "summary": "Glass.", "parts": []}]}),
    );
    let subrace = super::plan::standalone(ImportKind::Subrace, "Marsh Glasskin", None);
    let plan = rebuilt(vec![species, subrace], SPECIES);
    let body = plan
        .elements
        .iter()
        .find(|e| e.name == "Prism Body")
        .expect("trait");
    let children: Vec<&str> = plan
        .elements
        .iter()
        .filter(|e| e.parent.as_deref() == Some(body.id.as_str()) && e.role == Role::Part)
        .map(|e| e.name.as_str())
        .collect();
    // The prose sub-heading only: not the bare table caption, not another group's heading,
    // not an un-headed run-in line.
    assert_eq!(children, vec!["Spells"]);
    assert_eq!(
        super::parts::sub_headings(SPECIES, "Prism Body"),
        vec!["Spells", "Marsh Glasskin"]
    );
}

#[test]
fn a_condition_the_source_defines_is_a_condition_element() {
    let cond = super::plan::text_group("Sizzling", "condition", "");
    assert_eq!(cond.kind, ImportKind::Condition);
    let rule = super::plan::text_group("Hazards", "rules for condition immunity", "");
    assert_eq!(rule.kind, ImportKind::Rules);
    let source = "## Sizzling\n\nA sizzling creature takes 1d4 fire damage at the start of each of its turns.\n";
    let plan = rebuilt(vec![cond], source);
    let p = &plan.elements[0];
    assert_eq!((p.ty.as_str(), p.role), ("Condition", Role::Text));
    let el = super::parts::text_element(p, &plan, source);
    assert_eq!(el["supports"], json!(["Condition"]));
    assert!(el["description"]
        .as_str()
        .unwrap()
        .contains("start of each of its turns"));
}

#[test]
fn a_spellcasting_section_with_another_name_gets_no_generated_twin() {
    let class = |parts: Value| {
        group(
            ImportKind::Class,
            json!({"name": "Dowser", "summary": "", "quote": "",
                "spellcasting": {"ability": "Wisdom", "progression": "half", "list": "Dowser"},
                "features": [{"level": 1, "name": "Water Lore", "summary": "", "parts": parts}]}),
        )
    };
    let twin = |plan: &ImportPlan| {
        plan.elements
            .iter()
            .any(|e| e.name == "Dowser Spellcasting")
    };
    let with = rebuilt(
        vec![class(
            json!([{"name": "Cantrips"}, {"name": "Spell Slots"}]),
        )],
        "",
    );
    assert!(!twin(&with));
    let without = rebuilt(vec![class(json!([{"name": "Rods"}]))], "");
    assert!(twin(&without));
}

#[test]
fn a_species_brief_names_the_traits_granted_for_it() {
    let species = group(
        ImportKind::Species,
        json!({"name": "Glasskin", "summary": "Glass folk.", "quote": "",
            "features": [{"level": 1, "name": "Prism Body", "summary": "Glass.", "parts": []}]}),
    );
    let plan = rebuilt(vec![species], SPECIES);
    let root = plan
        .elements
        .iter()
        .find(|e| e.name == "Glasskin")
        .expect("species");
    assert!(
        root.brief
            .contains("separate elements granted for you: Prism Body."),
        "{}",
        root.brief
    );
    assert!(root.brief.contains("languages"), "{}", root.brief);
}

#[test]
fn spellcasting_headings_printed_flat_after_a_feature_are_its_parts() {
    let source = "#### Dowser\n\nWater finders.\n\n#### Rod Sense\n\nYou feel water.\n\n\
#### Water Magic\n\nYou draw on springs.\n\n#### Spells From Wells\n\nOther spells count.\n\n\
#### Cantrips\n\nYou know two cantrips.\n\n#### Spring Points\n\nYou have a pool.\n\n\
#### Preparing Spells\n\nYou prepare spells.\n\n#### Deep Current\n\nA later feature.\n";
    let class = group(
        ImportKind::Class,
        json!({"name": "Dowser", "summary": "", "quote": "",
            "spellcasting": {"ability": "Wisdom", "progression": "points", "list": "Dowser"},
            "features": [
                {"level": 1, "name": "Rod Sense", "summary": ""},
                {"level": 1, "name": "Water Magic", "summary": ""},
                {"level": 2, "name": "Deep Current", "summary": ""}]}),
    );
    let plan = rebuilt(vec![class], source);
    assert!(!plan
        .elements
        .iter()
        .any(|e| e.name == "Dowser Spellcasting"));
    let magic = plan
        .elements
        .iter()
        .find(|e| e.name == "Water Magic")
        .expect("feature");
    let parts: Vec<&str> = plan
        .elements
        .iter()
        .filter(|e| e.parent.as_deref() == Some(magic.id.as_str()) && e.role == Role::Part)
        .map(|e| e.name.as_str())
        .collect();
    assert_eq!(
        parts,
        [
            "Spells From Wells",
            "Cantrips",
            "Spring Points",
            "Preparing Spells"
        ]
    );
    // A heading planned as its own rules entry inside the run is passed over.
    let class = group(
        ImportKind::Class,
        json!({"name": "Dowser", "summary": "", "quote": "",
            "spellcasting": {"ability": "Wisdom", "progression": "points", "list": "Dowser"},
            "features": [{"level": 1, "name": "Water Magic", "summary": ""},
                {"level": 2, "name": "Deep Current", "summary": ""}]}),
    );
    let rules = super::plan::text_group("Spells From Wells", "rules", "");
    let plan = rebuilt(vec![class, rules], source);
    let parts = plan
        .elements
        .iter()
        .filter(|e| e.role == Role::Part)
        .map(|e| e.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(parts, ["Cantrips", "Spring Points", "Preparing Spells"]);
    // One spellcasting heading alone is not a section.
    let lone = source.replace("#### Preparing Spells\n\nYou prepare spells.\n\n", "");
    let class = group(
        ImportKind::Class,
        json!({"name": "Dowser", "summary": "", "quote": "",
            "spellcasting": {"ability": "Wisdom", "progression": "points", "list": "Dowser"},
            "features": [{"level": 1, "name": "Water Magic", "summary": ""}]}),
    );
    let plan = rebuilt(vec![class], &lone);
    assert!(plan
        .elements
        .iter()
        .any(|e| e.name == "Dowser Spellcasting"));
}
