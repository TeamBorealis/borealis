//! A weapon group and weapon properties no loaded pack has (invented content, scripted fake
//! provider, no CLI): the source's class grants its own group, its weapons are tagged and
//! proficient through it, and its new properties become elements with the source's text.

mod import_fake;

use borealis_assist::import::anything::{ElementStatus, ImportSource};
use borealis_assist::{finalize_draft, import_anything, Edition, ImportDraft, ImportStep};
use borealis_content::file::PackOrigin;
use borealis_engine::commands::{self, slot_key};
use borealis_engine::{CharacterSnapshot, CharacterState, Choice, Engine};
use import_fake::*;
use serde_json::{json, Value};

const SOURCE: &str = "# The Linkwarden\n\nWardens of hooked chains. Hit die d10.\n\n\
Weapons: Simple weapons, chain weapons\n\n\
Level 1: Bind\nBind. You bind a foe with your chain.\n\n\
## Chain Weapons\nChain weapons are a weapon category of their own.\n\n\
## Hookchain\nChain weapon. 1d8 slashing, Reach, Snaring, Rattling.\n\
Rattling. The din of the links gives the target disadvantage on its next Stealth check.\n\n\
## Flail Chain\nMartial melee, chain weapons group. 1d10 bludgeoning, Heavy.\n\n\
## Snaring\nSnaring. A creature hit by the weapon has its speed reduced by 10 feet until your next turn.\n";

/// The SRD 2024 Fighter class element as an invented class proficient with simple weapons and
/// its own chain weapons only.
fn class_reply() -> String {
    let text = std::fs::read_to_string(root().join("srd-5.2.1/classes/fighter.json")).unwrap();
    let file: Value = serde_json::from_str(&text).unwrap();
    let fighter = file["elements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == "ID_2024_CLASS_FIGHTER")
        .unwrap()
        .to_string()
        .replace("Fighter", "Linkwarden")
        .replace("FIGHTER", "LINKWARDEN");
    let mut el: Value = serde_json::from_str(&fighter).unwrap();
    el["description"] = json!("<p><strong>Weapons:</strong> Simple weapons, chain weapons</p>");
    el["data"].as_object_mut().unwrap().remove("core_traits");
    let req = "!ID_2024_MULTICLASS_LINKWARDEN";
    let rules = el["rules"].as_array_mut().unwrap();
    rules.retain(|r| {
        r["kind"] != "equipment_package"
            && !r["id"]
                .as_str()
                .is_some_and(|i| i.contains("WEAPON") || i.contains("MARTIAL"))
    });
    rules.push(json!({"kind": "grant", "type": "Proficiency",
        "id": "ID_PROFICIENCY_WEAPON_PROFICIENCY_SIMPLE_WEAPONS", "requirements": req}));
    json!({"element": el, "notes": []}).to_string()
}

fn weapon(name: &str, description: &str, data: Value) -> String {
    json!({"element": {"id": "X", "name": name, "type": "Weapon",
        "description": description, "data": data}, "notes": []})
    .to_string()
}

fn fake() -> Fake {
    let detect = json!({"kind": "mixed", "edition": "2024", "confidence": 0.9, "items": [
        {"kind": "class", "name": "Linkwarden"},
        {"kind": "item", "name": "Hookchain", "item_type": "weapon"},
        {"kind": "item", "name": "Flail Chain", "item_type": "weapon"},
        {"kind": "weapon_property", "name": "Snaring"}]});
    let class = serde_json::from_str::<Value>(&outline(
        "Linkwarden",
        json!({"hit_die": 10, "subclass_level": 3}),
        &[(1, "Bind")],
    ))
    .unwrap()["outline"]
        .clone();
    let hook = weapon(
        "Hookchain",
        "<p>Chain weapon.</p><p>Rattling. The din of the links gives the target disadvantage on its next Stealth check.</p>",
        json!({"weapon_category": "Chain Weapon", "melee": true,
            "damage": {"dice": "1d8", "type": "slashing"},
            "properties": ["Reach", "Snaring", "Rattling"], "cost": "15 gp", "weight": 6}),
    );
    let flail = weapon(
        "Flail Chain",
        "<p>A heavy chain.</p>",
        json!({"weapon_category": "Martial Melee", "weapon_group": "Chain Weapons",
            "melee": true, "damage": {"dice": "1d10", "type": "bludgeoning"},
            "properties": ["Heavy"], "cost": "20 gp", "weight": 8}),
    );
    let bind = el_reply("Bind", "Class Feature");
    Fake::default()
        .on("import_detect", "Classify", &[detect.to_string()])
        .on(
            "import_outline",
            "Linkwarden",
            &[json!({"outline": class, "outlines": [class]}).to_string()],
        )
        .on(
            "import_generate",
            "of type Class Feature (",
            &[batch_reply(std::slice::from_ref(&bind))],
        )
        .on("import_generate", "(type Class Feature,", &[bind])
        .on("import_generate", "(type Class,", &[class_reply()])
        .on(
            "import_generate",
            "of type Weapon (",
            &[batch_reply(&[hook.clone(), flail.clone()])],
        )
        .on("import_generate", "(type Weapon, name Hookchain", &[hook])
        .on("import_generate", "(type Weapon, name Flail", &[flail])
        .on(
            "import_generate",
            "(type Weapon Property,",
            &[el_reply("Snaring", "Weapon Property")],
        )
}

async fn draft(fake: &Fake) -> ImportDraft {
    let req = request(ImportSource::Text(SOURCE.into()), "LW", Edition::Y2024);
    match import_anything(fake, &base(), &req, &opts(), None)
        .await
        .unwrap()
    {
        ImportStep::Draft(d) => *d,
        ImportStep::NeedsChoice(d) => panic!("stopped at a choice: {:?}", d.plan.open_choices()),
    }
}

/// `(attack name, proficient)` of a level 1 character of `class_id` holding each weapon in turn.
fn attacks(engine: &Engine, class_id: &str, weapons: &[(String, String)]) -> Vec<(String, bool)> {
    let mut out = Vec::new();
    for (w, name) in weapons {
        let mut state: CharacterState =
            commands::new_character(engine, borealis_content::Ruleset::Dnd2024);
        let snap = engine.derive(&state);
        let key = slot_key(&snap, "ID_LEVEL_1", "Class", 1).expect("class slot");
        state = commands::select(engine, &state, &key, Choice::Element(class_id.into()))
            .unwrap()
            .state;
        state = commands::set_base_scores(engine, &state, [16, 12, 14, 10, 12, 8])
            .unwrap()
            .state;
        state = commands::add_item(engine, &state, w, 1).unwrap().state;
        let uid = state.inventory.items.last().unwrap().uid.clone();
        state = commands::equip(engine, &state, &uid, None).unwrap().state;
        let snap: CharacterSnapshot = engine.derive(&state);
        let a = snap
            .attacks
            .iter()
            .find(|a| &a.name == name)
            .expect("the weapon is an attack");
        out.push((a.name.clone(), a.proficient));
    }
    out
}

#[tokio::test]
async fn a_source_weapon_group_granted_by_its_class_makes_its_weapons_proficient() {
    let fake = fake();
    let d = draft(&fake).await;
    let failed: Vec<_> = d
        .elements
        .iter()
        .filter(|e| e.status == ElementStatus::Failed)
        .map(|e| (&e.name, &e.issues))
        .collect();
    assert!(failed.is_empty(), "{failed:?} / {:?}", d.sweep_issues);

    let pack = finalize_draft(&base(), &d).expect("commits").pack;
    let el = |name: &str| {
        pack.content
            .elements
            .iter()
            .find(|e| e.name == name)
            .unwrap_or_else(|| panic!("{name} missing"))
            .clone()
    };
    // The group, its category tag and the new properties are named from the source, ids of the
    // import's scheme.
    let group = el("Weapon Proficiency (Chain Weapons)");
    assert_eq!(
        group.id,
        "ID_LW_PROFICIENCY_WEAPON_PROFICIENCY_CHAIN_WEAPONS"
    );
    let category = el("Chain Weapon");
    assert_eq!(category.ty, "Weapon Category");
    assert_eq!(category.id, "ID_LW_WEAPON_CATEGORY_CHAIN_WEAPON");
    let rattling = el("Rattling");
    assert_eq!(rattling.ty, "Weapon Property");
    assert!(
        rattling.description.contains("Stealth"),
        "the source's text: {}",
        rattling.description
    );
    let snaring = el("Snaring");
    let hook = el("Hookchain");
    let flail = el("Flail Chain");
    for (w, tags) in [
        (
            &hook,
            vec![
                category.id.as_str(),
                rattling.id.as_str(),
                snaring.id.as_str(),
            ],
        ),
        (
            &flail,
            vec![
                category.id.as_str(),
                "ID_INTERNAL_WEAPON_CATEGORY_MARTIAL_MELEE",
            ],
        ),
    ] {
        for t in tags {
            assert!(
                w.supports.iter().any(|s| s == t),
                "{} lacks {t}: {:?}",
                w.name,
                w.supports
            );
        }
    }
    // A martial weapon of the group is still covered by martial proficiency (an append).
    assert!(pack
        .content
        .appends
        .iter()
        .any(|a| a.id == "ID_PROFICIENCY_WEAPON_PROFICIENCY_MARTIAL_MELEE_WEAPONS"));
    let report = pack.report.clone().unwrap();
    let says = |part: &str| report.normalised.iter().any(|n| n.message.contains(part));
    assert!(
        says("Weapon Proficiency (Chain Weapons)"),
        "{:?}",
        report.normalised
    );
    assert!(says("\"Rattling\""));

    let class_id = pack
        .content
        .elements
        .iter()
        .find(|e| e.ty == "Class")
        .unwrap()
        .id
        .clone();
    let store = base()
        .add_file("imported", PackOrigin::Custom, pack.content.clone())
        .build();
    let engine = Engine::new(&store);
    let weapons = [
        (hook.id.clone(), hook.name.clone()),
        (flail.id.clone(), flail.name.clone()),
    ];
    let warden = attacks(&engine, &class_id, &weapons);
    assert_eq!(
        warden,
        vec![("Hookchain".into(), true), ("Flail Chain".into(), true)],
        "the class grants its chain weapons"
    );
    let fighter = attacks(&engine, "ID_2024_CLASS_FIGHTER", &weapons);
    assert_eq!(
        fighter,
        vec![("Hookchain".into(), false), ("Flail Chain".into(), true)],
        "no chain weapon proficiency; martial still covers the martial one"
    );
}
