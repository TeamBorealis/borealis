//! Deterministic mechanics mapping of an imported class (invented content, scripted fake
//! provider, no CLI): a points caster whose spell list is made of existing SRD spells, a new
//! weapon group the class grants, a table column no rule reads, a resource formula written with
//! a bare ability, and proficiency/cantrip choices from the class text.

mod import_fake;

use borealis_assist::import::anything::ElementStatus;
use borealis_assist::import::anything::ImportSource;
use borealis_assist::{
    finalize_draft, import_anything, revise_draft, Edition, ImportDraft, ImportStep,
};
use borealis_content::file::PackOrigin;
use borealis_engine::commands::{self, slot_key};
use borealis_engine::{CharacterSnapshot, CharacterState, Choice, Engine};
use import_fake::*;
use serde_json::{json, Value};

const SOURCE: &str = "# The Cinderwright\n\nCrafters of small fires. Hit die d8.\n\n\
Weapons: Simple weapons, firearms\nSkills: Choose two from Acrobatics, Athletics, and Insight\n\n\
Level 1: Spellcasting\nSpellcasting. You cast from a pool of cinder points.\n\n\
Level 2: Ember Tokens\nEmber Tokens. You hold tokens as the table shows.\n\n\
Level 3: Kindling Lore\nKindling Lore. You learn one cantrip of your choice from the Wizard spell list.\n\n\
Cinderwright spell list: Fire Bolt, Light, Burning Hands, Magic Missile, Shield, Ashen Veil\n\n\
## Sparkcaster\nA firearm. 1d8 piercing, range 30/90.\n";

const LIST: [&str; 6] = [
    "Fire Bolt",
    "Light",
    "Burning Hands",
    "Magic Missile",
    "Shield",
    "Ashen Veil",
];

/// The SRD 2024 Fighter class element as an invented class: own text, wrong skill modelling
/// (one grant plus a choice of one), a counter column no rule reads.
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
        .replace("Fighter", "Cinderwright")
        .replace("FIGHTER", "CINDERWRIGHT");
    let mut el: Value = serde_json::from_str(&fighter).unwrap();
    el["description"] = json!(
        "<p><strong>Weapons:</strong> Simple weapons, firearms</p>\
<p><strong>Skills:</strong> Choose two from Acrobatics, Athletics, and Insight</p>"
    );
    let data = el["data"].as_object_mut().unwrap();
    data.remove("core_traits");
    let pb = data["progression"]["proficiency_bonus"].clone();
    let mut tokens: Vec<Value> = (1..=20).map(|l| json!(1 + l / 5)).collect();
    tokens[0] = json!("1");
    data.insert(
        "progression".into(),
        json!({"proficiency_bonus": pb, "ember_tokens": tokens}),
    );
    let req = "!ID_2024_MULTICLASS_CINDERWRIGHT";
    let rules = el["rules"].as_array_mut().unwrap();
    rules.retain(|r| {
        r["kind"] != "equipment_package"
            && r["type"] != "Proficiency"
            && !r["id"]
                .as_str()
                .is_some_and(|i| i.contains("SKILL") || i.contains("MARTIAL"))
    });
    rules.extend([
        json!({"kind": "grant", "type": "Proficiency", "id": "ID_PROFICIENCY_WEAPON_PROFICIENCY_SIMPLE_WEAPONS", "requirements": req}),
        json!({"kind": "grant", "type": "Proficiency", "id": "ID_PROFICIENCY_SKILL_ACROBATICS", "requirements": req}),
        json!({"kind": "select", "type": "Proficiency", "name": "Skill (Cinderwright)", "supports": "ID_PROFICIENCY_SKILL_ATHLETICS|ID_PROFICIENCY_SKILL_INSIGHT", "requirements": req}),
    ]);
    json!({"element": el, "notes": []}).to_string()
}

fn features_reply() -> String {
    let lore = feature(
        "Kindling Lore",
        json!({"type": "Class Feature",
            "description": "<p>You learn one cantrip of your choice from the Wizard spell list.</p>",
            "rules": [{"kind": "resource", "id": "kindling", "name": "Kindling",
                "max": {"type": "formula", "expr": "1 + wisdom"}, "recharge": ["long_rest"]}]}),
    );
    batch_reply(&[
        el_reply("Spellcasting", "Class Feature"),
        el_reply("Ember Tokens", "Class Feature"),
        lore,
    ])
}

fn fake() -> Fake {
    fake_with(&LIST)
}

fn fake_with(spells: &[&str]) -> Fake {
    let detect = json!({"kind": "mixed", "edition": "2024", "confidence": 0.9, "items": [
        {"kind": "class", "name": "Cinderwright"},
        {"kind": "item", "name": "Sparkcaster", "item_type": "weapon"}]});
    let class = serde_json::from_str::<Value>(&outline(
        "Cinderwright",
        json!({"hit_die": 8, "subclass_level": 3, "spellcasting": {"kind": "points",
            "ability": "intelligence", "list": "Cinderwright", "prepared": true, "spells": spells}}),
        &[(1, "Spellcasting"), (2, "Ember Tokens"), (3, "Kindling Lore")],
    ))
    .unwrap()["outline"]
        .clone();
    let gun = json!({"element": {"id": "X", "name": "Sparkcaster", "type": "Weapon",
        "description": "<p>A firearm.</p>", "data": {"weapon_category": "Firearm", "melee": false,
        "damage": {"dice": "1d8", "type": "piercing"}, "range": "30/90",
        "properties": ["Ammunition"], "cost": {"value": 90, "currency": "gp"}, "weight": 3}},
        "notes": []})
    .to_string();
    Fake::default()
        .on("import_detect", "Classify", &[detect.to_string()])
        .on(
            "import_outline",
            "Cinderwright",
            &[json!({"outline": class, "outlines": [class]}).to_string()],
        )
        .on(
            "import_generate",
            "of type Class Feature (",
            &[features_reply()],
        )
        .on("import_generate", "(type Class,", &[class_reply()])
        .on(
            "import_generate",
            "of type Weapon (",
            &[batch_reply(std::slice::from_ref(&gun))],
        )
        .on("import_generate", "(type Weapon,", &[gun])
}

async fn draft() -> ImportDraft {
    draft_with(&fake()).await
}

async fn draft_with(fake: &Fake) -> ImportDraft {
    let req = request(ImportSource::Text(SOURCE.into()), "CW", Edition::Y2024);
    match import_anything(fake, &base(), &req, &opts(), None)
        .await
        .unwrap()
    {
        ImportStep::Draft(d) => *d,
        ImportStep::NeedsChoice(d) => panic!("stopped at a choice: {:?}", d.plan.open_choices()),
    }
}

fn level(engine: &Engine, class_id: &str, to: u32) -> CharacterState {
    let mut state = commands::new_character(engine, borealis_content::Ruleset::Dnd2024);
    let snap = engine.derive(&state);
    let key = slot_key(&snap, "ID_LEVEL_1", "Class", 1).expect("class slot");
    state = commands::select(engine, &state, &key, Choice::Element(class_id.into()))
        .unwrap()
        .state;
    state = commands::set_base_scores(engine, &state, [10, 16, 14, 12, 14, 8])
        .unwrap()
        .state;
    for _ in 1..to {
        state = commands::level_up_main(engine, &state, None).unwrap().state;
    }
    state
}

fn offers(snap: &CharacterSnapshot, name_part: &str, id: &str) -> bool {
    snap.slots
        .iter()
        .any(|s| s.name.contains(name_part) && s.options.iter().any(|o| o.id == id))
}

#[tokio::test]
async fn a_new_points_caster_class_gets_its_list_choices_counters_and_weapon_group() {
    let d = draft().await;
    let failed: Vec<_> = d
        .elements
        .iter()
        .filter(|e| e.status == ElementStatus::Failed)
        .map(|e| (&e.name, &e.issues))
        .collect();
    assert!(failed.is_empty(), "{failed:?} / {:?}", d.sweep_issues);
    let lore = d
        .elements
        .iter()
        .find(|e| e.name == "Kindling Lore")
        .unwrap();
    let kindling = &lore.json["rules"].as_array().unwrap()[0];
    assert_eq!(kindling["max"]["expr"], "1 + wisdom:modifier");
    assert!(
        lore.normalised.iter().any(|n| n.contains("wisdom")),
        "{:?}",
        lore.normalised
    );

    let fin = finalize_draft(&base(), &d).expect("a points caster with an SRD spell list commits");
    let pack = fin.pack;
    // Five existing spells join the new list by appends; the unknown one is reported.
    let appended: Vec<&str> = pack.content.appends.iter().map(|a| a.id.as_str()).collect();
    assert_eq!(appended.len(), 5, "{appended:?}");
    assert!(appended.contains(&"ID_2024_SPELL_FIRE_BOLT"));
    let report = pack.report.clone().unwrap();
    let says = |part: &str| report.normalised.iter().any(|n| n.message.contains(part));
    assert!(says("Ashen Veil"), "{:?}", report.normalised);
    assert!(says("pool of points"));
    assert!(says("Weapon Proficiency (Firearms)"));
    assert!(says("ember_tokens"));
    assert!(says("skill proficiencies modelled as one choice of 2"));
    assert!(says("cantrip choice"));

    let class = pack
        .content
        .elements
        .iter()
        .find(|e| e.name == "Cinderwright" && e.ty == "Class")
        .unwrap();
    let class_id = class.id.clone();
    let store = base()
        .add_file("imported", PackOrigin::Custom, pack.content.clone())
        .build();
    let engine = Engine::new(&store);

    let mut state = level(&engine, &class_id, 3);
    let gun = pack
        .content
        .elements
        .iter()
        .find(|e| e.name == "Sparkcaster")
        .unwrap()
        .id
        .clone();
    state = commands::add_item(&engine, &state, &gun, 1).unwrap().state;
    let uid = state.inventory.items.last().unwrap().uid.clone();
    state = commands::equip(&engine, &state, &uid, None).unwrap().state;
    let snap = engine.derive(&state);

    assert!(
        offers(&snap, "Cantrip", "ID_2024_SPELL_FIRE_BOLT"),
        "class cantrips"
    );
    assert!(
        offers(&snap, "Wizard Cantrip", "ID_2024_SPELL_FIRE_BOLT"),
        "the feature's Wizard cantrip"
    );
    let slots: i32 = snap.spellcasting.iter().flat_map(|s| s.slots.iter()).sum();
    assert_eq!(slots, 0, "a points caster gets no slots");
    let skill_slots = snap
        .slots
        .iter()
        .filter(|s| s.name == "Skill Proficiency (Cinderwright)")
        .count();
    assert_eq!(skill_slots, 2);
    let atk = snap
        .attacks
        .iter()
        .find(|a| a.name == "Sparkcaster")
        .unwrap();
    assert!(atk.proficient, "the class grants the new firearms group");
    let res = |id: &str| snap.resources.iter().find(|r| r.id == id).map(|r| r.max);
    assert_eq!(res("ember tokens"), Some(1));
    assert_eq!(res("kindling"), Some(3), "1 + Wis 14 modifier");
}

#[tokio::test]
async fn a_list_with_no_known_spell_never_blocks_the_commit_even_after_a_revise() {
    let fake = fake_with(&["Ashen Veil"]).on(
        "import_revise",
        "cinder",
        &[el_reply("Spellcasting", "Class Feature")],
    );
    let d = draft_with(&fake).await;
    assert!(d.elements.iter().all(|e| e.status != ElementStatus::Failed));
    let target = d
        .elements
        .iter()
        .find(|e| e.name == "Spellcasting")
        .unwrap();
    // The builder re-adds its spell choices on every normalisation; the derived pass leaves them
    // out of what the engine loads while the list is empty, so a revise cannot re-break the class.
    let next = revise_draft(
        &fake,
        &base(),
        &d,
        "spells cost cinder points",
        Some(&target.id),
        &opts(),
        None,
    )
    .await
    .unwrap();
    assert!(next.sweep_issues.is_empty(), "{:?}", next.sweep_issues);
    assert!(next
        .elements
        .iter()
        .all(|e| e.status != ElementStatus::Failed));
    let pack = finalize_draft(&base(), &next).expect("commits").pack;
    assert!(pack.content.appends.is_empty());
    let report = pack.report.unwrap();
    assert!(
        report.normalised.iter().any(|n| n
            .message
            .contains("no spell is on the Cinderwright spell list")),
        "{:?}",
        report.normalised
    );
    let caster = pack
        .content
        .elements
        .iter()
        .find(|e| e.name == "Spellcasting")
        .unwrap();
    let json = serde_json::to_string(&caster.rules).unwrap();
    assert!(!json.contains("$(spellcasting:list)"), "{json}");
}
