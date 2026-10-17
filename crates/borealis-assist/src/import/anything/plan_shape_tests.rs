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

const LANTERN: &str = "# Wonders\n\n### Lantern of Tides\n\nWondrous item, rare\n\n\
This lantern of green brass shows the sea's mood. Roll on the table each dusk.\n\n\
#### Tide Moods\n\nd4 | Mood\n1 | **Ebb.** The flame shrinks.\n2 | **Surge.** The flame doubles.\n\
3 | **Slack.** Nothing changes.\n\n### Kettle of Ash\n\nWondrous item, common\n\n\
This kettle boils water with no fire under it.\n\n## Tidal Lore\n\nThe tides answer the moon.\n";

#[test]
fn entries_planned_on_an_items_own_table_are_folded_into_the_item() {
    let item = |name: &str| {
        super::plan::standalone(
            ImportKind::Item,
            name,
            Some(super::types::ItemType::MagicItem),
        )
    };
    let text = |name: &str, what: &str| super::plan::text_group(name, what, "");
    let plan = rebuilt(
        vec![
            item("Lantern of Tides"),
            item("Kettle of Ash"),
            text("Tide Moods", "table"),
            text("Surge", "table row"),
            text("Tidal Lore", "lore"),
        ],
        LANTERN,
    );
    let names: Vec<&str> = plan.elements.iter().map(|e| e.name.as_str()).collect();
    assert!(!names.contains(&"Tide Moods"), "{names:?}");
    assert!(!names.contains(&"Surge"), "{names:?}");
    // A section of its own after the items stays an entry.
    assert!(names.contains(&"Tidal Lore"), "{names:?}");
    // The caption over the item's table is no entry of its own either.
    assert!(!plan.groups.iter().any(|g| g.name == "Tide Moods"));
    assert!(
        plan.notes
            .iter()
            .any(|n| n.starts_with("Tide Moods: a table or row of Lantern of Tides")),
        "{:?}",
        plan.notes
    );
}

#[test]
fn narrative_sections_are_plan_notes_that_outlive_a_rebuild() {
    let other = |name: &str, what: &str| super::types::OtherItem {
        name: name.into(),
        what: what.into(),
        note: String::new(),
    };
    let notes = super::plan::narrative_notes(&[
        other("The Drowned Mill", "narrative (keyed area)"),
        other("Ferry Rules", "optional rule"),
    ]);
    assert_eq!(notes.len(), 1);
    let mut plan = ImportPlan {
        edition: Edition::Y2014,
        groups: vec![super::plan::text_group("Ferry Rules", "optional rule", "")],
        elements: Vec::new(),
        notes,
    };
    let src = "## Ferry Rules\n\nThe ferry costs a copper.\n\n## The Drowned Mill\n\nFog.\n";
    super::plan::rebuild(&mut plan, &library(), "ID_T", src);
    let names: Vec<&str> = super::plan::narrative_sections(&plan).collect();
    assert_eq!(names, ["The Drowned Mill"]);
}

#[test]
fn a_contents_page_is_a_boundary_and_never_an_entry_listed_or_not() {
    let src = "## On the Cover\n\nThe cover shows a tome.\n\n## Contents List\n\n\
Foreword..........pg.2\nSpells.........pg.5\nAppendix.........pg.9\n\n\
Dear reader, this letter was written long ago.\n\n## Spells\n\nFirst spell.\n";
    let mut plan = ImportPlan {
        edition: Edition::Y2014,
        groups: vec![
            super::plan::text_group("On the Cover", "lore section", ""),
            super::plan::text_group("Contents List", "table", ""),
        ],
        elements: Vec::new(),
        notes: Vec::new(),
    };
    super::plan::rebuild(&mut plan, &library(), "ID_T", src);
    let names: Vec<&str> = plan.elements.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, ["On the Cover"]);
    let bounds: Vec<&str> = super::plan::narrative_sections(&plan).collect();
    assert_eq!(bounds, ["Contents List"]);
    // A second rebuild keeps one note.
    super::plan::rebuild(&mut plan, &library(), "ID_T", src);
    assert_eq!(super::plan::narrative_sections(&plan).count(), 1);
}

const REEDS: &str = "## Circle of Reeds\n\nDruids of the marsh.\n\n### Reed Cantrips\n\n\
When you join this circle at 2nd level, you learn the druidcraft,\nguidance, and mending \
cantrips.\n\n### Reed Spells\n\nDruid Level | Reed Spells\n3rd | fog cloud, misty step\n\
5th | sleet storm, water\nbreathing\n\n### Reed Walk\n\nYou walk on reeds.\n";

#[test]
fn spells_and_cantrips_a_feature_prints_are_granted_when_the_outline_drops_one() {
    let store = library();
    let mut sub = group(
        ImportKind::Subclass,
        json!({"name": "Circle of Reeds", "summary": "", "quote": "Druids of the marsh.",
            "features": [
                {"level": 2, "name": "Reed Cantrips", "summary": "You learn three cantrips.",
                    "quote": "you learn the druidcraft", "parts": []},
                {"level": 3, "name": "Reed Spells", "summary": "Circle spells.", "parts": [],
                    "spells": [{"level": 3, "spells": ["Fog Cloud", "Misty Step"]},
                        {"level": 5, "spells": ["Sleet Storm"]}]},
                {"level": 6, "name": "Reed Walk", "summary": "Walk.", "parts": []}]}),
    );
    sub.parent = super::plan::link_for(&store, Edition::Y2014, "ID_WOTC_PHB_CLASS_DRUID");
    assert!(sub.parent.is_some(), "the library druid offers circles");
    let mut plan = ImportPlan {
        edition: Edition::Y2014,
        groups: vec![sub],
        elements: Vec::new(),
        notes: Vec::new(),
    };
    super::plan::rebuild(&mut plan, &store, "ID_T", REEDS);
    let spells = |name: &str| -> Vec<(String, u32, bool)> {
        plan.elements
            .iter()
            .find(|e| e.name == name)
            .expect("feature")
            .spells
            .iter()
            .map(|g| (g.name.clone(), g.level, g.id.is_some()))
            .collect()
    };
    let table = spells("Reed Spells");
    assert!(
        table.contains(&("Water Breathing".to_string(), 5, true)),
        "{table:?}"
    );
    assert_eq!(table.len(), 4, "{table:?}");
    let cantrips = spells("Reed Cantrips");
    let names: Vec<&str> = cantrips.iter().map(|(n, ..)| n.as_str()).collect();
    assert_eq!(names, ["Druidcraft", "Guidance", "Mending"]);
    assert!(cantrips.iter().all(|(_, level, id)| *level == 0 && *id));
    assert!(spells("Reed Walk").is_empty());
}

/// Offline: `BOREALIS_REPLAN_REPLAY=<draft.json> BOREALIS_REPLAN_OUT=<dir>` rebuilds a stored
/// draft's plan with the current planner and, with `BOREALIS_REPLAN_MINE=1`, applies what it
/// changes in owner-table folds and feature spell grants; both runs then fold, reprint the
/// text-only entries, re-apply the plan's structure and finalize into `<dir>` (as
/// `passage_tests::reprint_a_stored_draft` does), so the two packs differ only by those changes.
#[test]
#[ignore = "needs a local draft.json"]
fn replan_a_stored_draft() {
    use super::types::ImportDraft;
    let Ok(path) = std::env::var("BOREALIS_REPLAN_REPLAY") else {
        return;
    };
    let mut d: ImportDraft =
        serde_json::from_str(&std::fs::read_to_string(path).expect("reads")).expect("parses");
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let mut lib = ContentStoreBuilder::new();
    for pack in ["core", "srd-5.1", "srd-5.2.1"] {
        lib = lib.add_pack_dir(&root.join(pack)).expect("pack loads");
    }
    let store = lib.clone().build();
    let mine = std::env::var("BOREALIS_REPLAN_MINE").is_ok();
    let mut replanned = d.plan.clone();
    super::plan::rebuild(&mut replanned, &store, &d.id_prefix, &d.source_text);
    let owned = super::passage::tables::owner_table_entries(&d.plan, &d.source_text);
    for (_, n) in &owned {
        println!("owner table: {n}");
    }
    let mut gone: Vec<String> = owned.into_iter().map(|(id, _)| id).collect();
    gone.extend(
        super::plan::folded_entries(&d.plan.elements, d.plan.edition, &d.source_text)
            .into_iter()
            .map(|(id, _)| id),
    );
    for p in d.plan.elements.iter_mut() {
        let Some(new) = replanned.elements.iter().find(|n| n.id == p.id) else {
            continue;
        };
        let added: Vec<String> = new
            .spells
            .iter()
            .filter(|g| !p.spells.iter().any(|o| o.name == g.name && o.id == g.id))
            .map(|g| format!("{} (level {}, {})", g.name, g.level, g.id.is_some()))
            .collect();
        if !added.is_empty() {
            println!("spells of {}: + {}", p.name, added.join("; "));
            if mine {
                p.spells.clone_from(&new.spells);
            }
        }
    }
    if !mine {
        // Before: the folds the planner made without the owner tables.
        gone = super::plan::sections::fold_table_headings(&d.plan.elements, &d.source_text)
            .into_iter()
            .chain(super::plan::sections::fold_nested_lore(
                &d.plan.elements,
                d.plan.edition,
                &d.source_text,
            ))
            .map(|(id, _)| id)
            .collect();
    }
    d.plan.elements.retain(|e| !gone.contains(&e.id));
    d.plan
        .groups
        .retain(|g| !g.root_id.as_ref().is_some_and(|r| gone.contains(r)));
    d.elements.retain(|e| !gone.contains(&e.id));
    let plan = d.plan.clone();
    for e in d.elements.iter_mut().filter(|e| e.role == Role::Text) {
        if let Some(p) = plan
            .elements
            .iter()
            .find(|p| p.id == e.id && p.excerpt.is_some())
        {
            let el = super::parts::text_element(p, &plan, &d.source_text);
            e.json["description"] = el["description"].clone();
        }
    }
    for e in d.elements.iter_mut().filter(|e| !e.json.is_null()) {
        if let Some(p) = plan.elements.iter().find(|p| p.id == e.id) {
            super::parts::apply(&mut e.json, p, &plan);
        }
    }
    super::generate::keep_printed(&mut d);
    let done = super::finalize::finalize_draft(&lib, &d).expect("finalizes");
    if let Ok(out) = std::env::var("BOREALIS_REPLAN_OUT") {
        done.pack
            .write_to_dir(Path::new(&out))
            .expect("pack writes");
    }
}
