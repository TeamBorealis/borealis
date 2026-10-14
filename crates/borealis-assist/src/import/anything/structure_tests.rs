//! Plan structure end to end on invented outlines (no model): tiered class-table features,
//! the printed prepared formula, a class's spellcasting sub-headings, choices among existing
//! elements, headed trait sub-items and per-type item variants.

use std::path::Path;

use borealis_content::{ContentStore, ContentStoreBuilder};
use serde_json::{json, Value};

use super::types::{
    DetectedItem, Edition, ImportKind, ImportPlan, ItemType, Outline, PlannedElement, Role,
};

fn library() -> ContentStore {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let mut b = ContentStoreBuilder::new();
    for pack in ["core", "srd-5.1"] {
        b = b.add_pack_dir(&root.join(pack)).expect("pack loads");
    }
    b.build()
}

fn outline(v: Value) -> Outline {
    serde_json::from_value(v).expect("outline parses")
}

const SOURCE: &str = "# Warden\n\n## Grove Magic\n\nYou cast from a pool.\n\n#### Cantrips\n\nYou know one cantrip.\n\n\
## Deep Lore\n\nAt 11th level you learn a secret.\n\n# Fenfolk\n\n### Bog Gift\n\nYou gain a gift.\n\n\
#### Mire Spells\n\nYou know two spells.\n\n### Fen Skill\n\nOne of Acrobatics, Stealth or Perception.\n\n\
### Footnote\n\nOther text.\n";

fn plan() -> ImportPlan {
    let class = outline(json!({
    "name": "Warden", "summary": "A warden.", "quote": "",
    "spellcasting": {"ability": "Wisdom", "progression": "points", "list": "Warden",
        "prepared": true, "prepared_formula": "Wisdom modifier (minimum of one spell)"},
    "features": [
        {"level": 1, "name": "Grove Magic", "summary": "Casting.", "quote": "",
         "parts": [{"name": "Cantrips", "summary": "One cantrip."},
                   {"name": "Grove Points", "summary": "A pool."}]},
        {"level": 11, "name": "Deep Lore (6th level)", "summary": "A 6th-level spell."},
        {"level": 13, "name": "Deep Lore (7th level)", "summary": ""},
        {"level": 15, "name": "Deep Lore (8th level)", "summary": ""}
    ]}));
    let species = outline(json!({
    "name": "Fenfolk", "summary": "Bog people.", "quote": "",
    "features": [
        {"level": 1, "name": "Bog Gift", "summary": "A gift.",
         "parts": [{"name": "Mire Spells", "summary": "Two spells."},
                   {"name": "Damp Skin", "summary": "No heading."}]},
        {"level": 1, "name": "Fen Skill", "summary": "One skill.",
         "options": [{"name": "Acrobatics"}, {"name": "Stealth"}, {"name": "Perception"}],
         "choose": [{"level": 1, "count": 1}]}
    ]}));
    let mut g_class = super::plan::standalone(ImportKind::Class, "Warden", None);
    g_class.outline = Some(class);
    let mut g_species = super::plan::standalone(ImportKind::Species, "Fenfolk", None);
    g_species.outline = Some(species);
    let items = super::variants::with_bases(
        ["Mitts of the Marsh, Heron", "Mitts of the Marsh, Newt"]
            .iter()
            .map(|n| DetectedItem {
                kind: ImportKind::Item,
                name: (*n).into(),
                parent_hint: None,
                item_type: Some(ItemType::MagicItem),
            })
            .collect(),
    );
    let mut groups = vec![g_class, g_species];
    groups.extend(
        items
            .iter()
            .map(|i| super::plan::standalone(i.kind, &i.name, i.item_type)),
    );
    let mut plan = ImportPlan {
        edition: Edition::Y2014,
        groups,
        elements: Vec::new(),
        notes: Vec::new(),
    };
    super::plan::rebuild(&mut plan, &library(), "ID_T", SOURCE);
    plan
}

fn named<'a>(plan: &'a ImportPlan, name: &str) -> &'a PlannedElement {
    plan.elements
        .iter()
        .find(|e| e.name == name)
        .unwrap_or_else(|| panic!("no planned {name}"))
}

#[test]
fn tier_rows_become_one_feature_and_the_printed_formula_is_read() {
    let plan = plan();
    let lore = named(&plan, "Deep Lore");
    assert_eq!((lore.role, lore.level), (Role::ClassFeature, Some(11)));
    assert!(lore.brief.contains("(11, 13, 15)"));
    assert!(!plan.elements.iter().any(|e| e.name.contains("(6th level)")));
    let formula = plan.groups[0]
        .outline
        .as_ref()
        .and_then(|o| o.spellcasting.as_ref())
        .and_then(|s| s.prepared_formula.clone());
    assert_eq!(formula.as_deref(), Some("mod"));
}

#[test]
fn a_spellcasting_sub_heading_keeps_no_model_spell_choices() {
    let plan = plan();
    let cantrips = named(&plan, "Cantrips");
    let mut el = json!({"rules": [
        {"kind": "select", "type": "Spell", "name": "Cantrip", "supports": "Warden,0"},
        {"kind": "stat", "name": "speed", "value": 5}]});
    super::parts::apply(&mut el, cantrips, &plan);
    assert_eq!(el["rules"].as_array().map(Vec::len), Some(1));
    // Another part of the section keeps its rules.
    let points = named(&plan, "Grove Points");
    let mut pool = json!({"rules": [{"kind": "select", "type": "Spell", "supports": "Warden,1"}]});
    super::parts::apply(&mut pool, points, &plan);
    assert_eq!(pool["rules"].as_array().map(Vec::len), Some(1));
}

#[test]
fn a_choice_among_skills_is_one_select_over_their_ids() {
    let plan = plan();
    assert!(!plan.elements.iter().any(|e| e.name == "Stealth"));
    let skill = named(&plan, "Fen Skill");
    let mut el = json!({"rules": [
        {"kind": "grant", "type": "Proficiency", "id": "ID_PROFICIENCY_SKILL_STEALTH"}]});
    super::parts::apply(&mut el, skill, &plan);
    let rules = el["rules"].as_array().cloned().unwrap_or_default();
    assert_eq!(rules.len(), 1, "{rules:?}");
    assert_eq!(rules[0]["type"], "Proficiency");
    let ids = rules[0]["supports"].as_str().unwrap_or_default();
    for skill in ["ACROBATICS", "STEALTH", "PERCEPTION"] {
        assert!(
            ids.contains(&format!("ID_PROFICIENCY_SKILL_{skill}")),
            "{ids}"
        );
    }
}

#[test]
fn a_headed_trait_sub_item_is_its_own_granted_trait() {
    let plan = plan();
    let spells = named(&plan, "Mire Spells");
    let gift = named(&plan, "Bog Gift");
    assert_eq!(spells.parent.as_deref(), Some(gift.id.as_str()));
    assert!(!plan.elements.iter().any(|e| e.name == "Damp Skin"));
    let mut el = json!({});
    super::parts::apply(&mut el, gift, &plan);
    assert_eq!(el["rules"][0]["id"], spells.id.as_str());
    assert!(!super::parts::headed_within(
        SOURCE,
        "Fen Skill",
        "Mire Spells"
    ));
}

#[test]
fn per_type_variants_have_a_base_item() {
    let plan = plan();
    let base = named(&plan, "Mitts of the Marsh");
    assert_eq!(base.ty, "Magic Item");
    assert!(base.brief.contains("Mitts of the Marsh, Heron"));
    assert!(named(&plan, "Mitts of the Marsh, Newt")
        .brief
        .contains("One version of Mitts of the Marsh"));
}

/// Offline replay of a stored draft (`BOREALIS_PLAN_REPLAY=<draft.json>`): the plan rebuilt
/// from its stored outlines and detection, and the structure step re-applied to its stored
/// elements. Prints counts before (stored) and after (rebuilt); never calls a model.
#[test]
#[ignore = "needs a stored draft"]
fn replay_a_stored_draft() {
    let Ok(path) = std::env::var("BOREALIS_PLAN_REPLAY") else {
        return;
    };
    let text = std::fs::read_to_string(path).expect("draft reads");
    let draft: super::types::ImportDraft = serde_json::from_str(&text).expect("draft parses");
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let mut b = ContentStoreBuilder::new();
    for pack in ["core", "srd-5.1", "srd-5.2.1"] {
        b = b.add_pack_dir(&root.join(pack)).expect("pack loads");
    }
    let store = b.build();
    let mut plan = draft.plan.clone();
    let items = super::variants::with_bases(draft.detection.items.clone());
    for i in &items {
        if !plan.groups.iter().any(|g| g.name == i.name) {
            plan.groups
                .push(super::plan::standalone(i.kind, &i.name, i.item_type));
        }
    }
    super::plan::rebuild(&mut plan, &store, &draft.id_prefix, &draft.source_text);
    let summary = |p: &ImportPlan| {
        let target = p
            .elements
            .iter()
            .filter(|e| e.picks.as_ref().is_some_and(|k| k.key.starts_with("ID_")))
            .count();
        format!(
            "elements {} options {} parts {} tiered-names {} target-selects {}",
            p.elements.len(),
            p.elements.iter().filter(|e| e.role == Role::Option).count(),
            p.elements.iter().filter(|e| e.role == Role::Part).count(),
            p.elements.iter().filter(|e| e.name.ends_with(')')).count(),
            target
        )
    };
    println!("before: {}", summary(&draft.plan));
    println!("after:  {}", summary(&plan));
    let old: Vec<&str> = draft
        .plan
        .elements
        .iter()
        .map(|e| e.name.as_str())
        .collect();
    let new: Vec<&str> = plan.elements.iter().map(|e| e.name.as_str()).collect();
    println!(
        "added: {:?}",
        new.iter().filter(|n| !old.contains(n)).collect::<Vec<_>>()
    );
    println!(
        "gone: {:?}",
        old.iter().filter(|n| !new.contains(n)).collect::<Vec<_>>()
    );
    let (mut req, mut req_after, mut spell_sel, mut spell_sel_after) = (0, 0, 0, 0);
    for d in &draft.elements {
        let Some(planned) = plan.elements.iter().find(|e| e.id == d.id) else {
            continue;
        };
        let count = |v: &Value| {
            v["rules"].as_array().map_or(0, |r| {
                r.iter()
                    .filter(|x| x["kind"] == "select" && x["type"] == "Spell")
                    .count()
            })
        };
        let mut el = d.json.clone();
        super::parts::apply(&mut el, planned, &plan);
        let terms = |v: &Value| {
            v["requirements"]
                .as_str()
                .map_or(0, |s| s.matches('[').count())
        };
        if terms(&d.json) != terms(&el) {
            println!(
                "requirements: {} {} -> {}",
                d.name, d.json["requirements"], el["requirements"]
            );
        }
        req += terms(&d.json);
        req_after += terms(&el);
        spell_sel += count(&d.json);
        spell_sel_after += count(&el);
    }
    println!(
        "requirement terms {req} -> {req_after}; spell selects {spell_sel} -> {spell_sel_after}"
    );
    for e in &plan.elements {
        if let Some(k) = e.picks.as_ref().filter(|k| k.key.starts_with("ID_")) {
            println!("select: {} {} {}", e.name, k.ty, k.key);
        }
    }
    for n in &plan.notes {
        println!("note: {n}");
    }
}
