use serde_json::{json, Value};

use super::super::{groups::weapon_groups, str_at, Derived};
use super::*;
use crate::import::anything::types::{Edition, ImportPlan, PlannedElement, Role, SourceExcerpt};

const SOURCE: &str = "\
#### Valley Arms

The Valley Arms table lists the firearms of the valley.

#### Firearm Properties

Thunder. Firing this weapon wakes everyone within 100 feet.

Ammunition. Every shot uses up one cartridge.

#### Sparkfoot

A feat about something else entirely.

Name | Cost | Damage | Properties
Martial Ranged Weapons
Thunder Tube | 90 gp | 1d10 piercing | Ammunition (range 80/240), thunder
Smoke Lance | 60 gp | 1d8 piercing | Ammunition (range 30/90)

#### Hunting Gear

Name | Cost | Damage | Properties
Simple Ranged Weapons
Reed Sling | 1 sp | 1d4 bludgeoning | Ammunition (range 30/120)
";

fn bundled() -> ContentStore {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    borealis_content::ContentStoreBuilder::new()
        .add_pack_dir(&root.join("core"))
        .and_then(|b| b.add_pack_dir(&root.join("srd-5.1")))
        .expect("bundled packs")
        .build()
}

fn planned(id: &str, ty: &str, name: &str, line: usize) -> PlannedElement {
    PlannedElement {
        id: id.into(),
        ty: ty.into(),
        name: name.into(),
        role: Role::Standalone,
        level: None,
        owner: None,
        brief: String::new(),
        excerpt: Some(SourceExcerpt {
            start_line: line,
            end_line: line,
            snippet: String::new(),
        }),
        parent: None,
        picks: None,
        prerequisite_level: None,
        spells: Vec::new(),
    }
}

fn weapon(id: &str, name: &str, category: &str, props: &[&str]) -> Value {
    json!({"id": id, "name": name, "type": "Weapon", "source": "Test",
        "data": {"weapon_category": category, "melee": false, "properties": props},
        "setters": [{"name": "proficiency",
            "value": "ID_PROFICIENCY_WEAPON_PROFICIENCY_MARTIAL_RANGED_WEAPONS"}]})
}

fn line_of(text: &str) -> usize {
    SOURCE
        .lines()
        .position(|l| l.starts_with(text))
        .expect("line")
        + 1
}

fn supports(doc: &Value) -> Vec<&str> {
    doc["supports"]
        .as_array()
        .map(|a| a.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default()
}

/// The class names "firearms" while the source files its guns under a standard category: the
/// weapons whose table or property definitions sit under a heading naming firearms join the
/// known Firearm category and its group proficiency; a weapon of another table stays out.
#[test]
fn a_known_group_named_only_on_the_class_line_takes_the_weapons_printed_under_it() {
    let store = bundled();
    let plan = ImportPlan {
        edition: Edition::Y2014,
        groups: Vec::new(),
        elements: vec![
            planned(
                "ID_T_WEAPON_THUNDER_TUBE",
                "Weapon",
                "Thunder Tube",
                line_of("Thunder Tube"),
            ),
            planned(
                "ID_T_WEAPON_SMOKE_LANCE",
                "Weapon",
                "Smoke Lance",
                line_of("Smoke Lance"),
            ),
            planned(
                "ID_T_WEAPON_REED_SLING",
                "Weapon",
                "Reed Sling",
                line_of("Reed Sling"),
            ),
        ],
        notes: Vec::new(),
    };
    let mut d = Derived {
        docs: vec![
            weapon(
                "ID_T_WEAPON_THUNDER_TUBE",
                "Thunder Tube",
                "Martial Ranged",
                &["Ammunition", "Thunder"],
            ),
            weapon(
                "ID_T_WEAPON_SMOKE_LANCE",
                "Smoke Lance",
                "Martial Ranged",
                &["Ammunition"],
            ),
            weapon(
                "ID_T_WEAPON_REED_SLING",
                "Reed Sling",
                "Simple Ranged",
                &["Ammunition"],
            ),
            json!({"id": "ID_T_CLASS_DRIFTER", "name": "Drifter", "type": "Class",
                "description": "<p><strong>Weapons:</strong> Simple weapons, firearms</p>",
                "rules": [{"kind": "grant", "type": "Proficiency",
                    "id": "ID_PROFICIENCY_WEAPON_PROFICIENCY_SIMPLE_WEAPONS"}]}),
        ],
        ..Derived::default()
    };
    weapon_groups(&plan, SOURCE, &store, &mut d);
    let firearm = "ID_INTERNAL_WEAPON_CATEGORY_FIREARM";
    // Thunder Tube: by its "Thunder." definition under "Firearm Properties". Smoke Lance: its
    // "Ammunition" is a core property (its definition under the firearms heading does not count),
    // and its table's heading names no group.
    assert!(!supports(&d.docs[1]).contains(&firearm), "{}", d.docs[1]);
    assert!(supports(&d.docs[0]).contains(&firearm), "{}", d.docs[0]);
    assert!(!supports(&d.docs[2]).contains(&firearm), "{}", d.docs[2]);
    let group = d
        .extra
        .iter()
        .find(|e| str_at(e, "id") == "ID_PROFICIENCY_WEAPON_PROFICIENCY_FIREARMS")
        .expect("the known firearms group proficiency");
    assert_eq!(str_at(group, "name"), "Weapon Proficiency (Firearms)");
    let class_rules = d.docs[3]["rules"].as_array().expect("rules");
    assert!(class_rules
        .iter()
        .any(|r| str_at(r, "id") == "ID_PROFICIENCY_WEAPON_PROFICIENCY_FIREARMS"));
}

#[test]
fn a_weapons_context_holds_its_table_caption_category_and_property_headings() {
    let lines: Vec<&str> = SOURCE.lines().collect();
    let row = line_of("Reed Sling") - 1;
    let ctx = weapon_context(&lines, Some(row), &[]);
    assert!(ctx.contains(&"Hunting Gear".to_string()), "{ctx:?}");
    assert!(
        ctx.contains(&"Simple Ranged Weapons".to_string()),
        "{ctx:?}"
    );
    assert!(!names_group(&ctx, "firearm"));
    let ctx = weapon_context(&lines, None, &["Thunder (loud)".to_string()]);
    assert!(names_group(&ctx, "firearm"), "{ctx:?}");
    assert_eq!(key_words("Firearm Properties"), " firearm property ");
}

/// A proficiency filter that finds no weapon (here a property nothing carries) is reported; one
/// that finds a library weapon is not.
#[test]
fn a_proficiency_filter_matching_no_weapon_is_reported() {
    let store = bundled();
    let mut d = Derived {
        docs: vec![
            weapon(
                "ID_T_WEAPON_THUNDER_TUBE",
                "Thunder Tube",
                "Martial Ranged",
                &["Ammunition"],
            ),
            json!({"id": "ID_T_CLASS_DRIFTER", "name": "Drifter", "type": "Class",
                "rules": [{"kind": "proficiency_filter", "name": "Hand Cannons",
                    "properties_any": ["hand cannon"]}],
                "multiclass": {"rules": [{"kind": "proficiency_filter", "name": "Bows",
                    "properties_any": ["ammunition"]}]}}),
        ],
        ..Derived::default()
    };
    unmatched_filters(&store, &mut d);
    let notes: Vec<&str> = d.notes.iter().map(|(_, m)| m.as_str()).collect();
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert!(
        notes[0].contains("\"Hand Cannons\" matches no weapon"),
        "{notes:?}"
    );
    assert_eq!(d.notes[0].0, "ID_T_CLASS_DRIFTER");
}
