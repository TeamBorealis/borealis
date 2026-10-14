//! Extraction semantics mapped deterministically (invented content, scripted fake provider,
//! no CLI): a subclass spell table becomes always-prepared spell grants, extra class table
//! columns become progression columns and counters, printed class spell lists and "Classes:"
//! lines become list membership of existing classes, folded +1/+2/+3 items become one item per
//! variant, 2014 subraces carry their race's name as tag, and feat prerequisites become
//! requirements.

mod import_fake;

use borealis_assist::import::anything::ImportSource;
use borealis_assist::{finalize_draft, import_anything, Edition, ImportDraft, ImportStep};
use borealis_content::file::PackOrigin;
use borealis_engine::commands::{self, slot_key};
use borealis_engine::{Choice, Engine};
use import_fake::*;
use serde_json::{json, Value};

async fn draft_for(fake: &Fake, source: &str, prefix: &str, edition: Edition) -> ImportDraft {
    let req = request(ImportSource::Text(source.into()), prefix, edition);
    match import_anything(fake, &base(), &req, &opts(), None)
        .await
        .unwrap()
    {
        ImportStep::Draft(d) => *d,
        ImportStep::NeedsChoice(d) => panic!("stopped at a choice: {:?}", d.plan.open_choices()),
    }
}

fn batch_of(names: &[&str], ty: &str) -> String {
    batch_reply(&names.iter().map(|n| el_reply(n, ty)).collect::<Vec<_>>())
}

fn element<'a>(d: &'a ImportDraft, name: &str) -> &'a Value {
    &d.elements
        .iter()
        .find(|e| e.name == name)
        .unwrap_or_else(|| panic!("no element {name}"))
        .json
}

// ---- a subclass spell table ------------------------------------------------------------------

const HUSH_SOURCE: &str = "## Domain of Hush\n\nClerics of quiet places.\n\n\
Hush Spells. You always have these spells prepared.\n\
Cleric Level | Spells\n3 | Bless, Cure Wounds\n5 | Aid, Quietude Hymn\n\n\
Soft Step. You move without a sound.\n";

#[tokio::test]
async fn a_subclass_spell_table_becomes_always_prepared_spells() {
    let detect = detect("subclass", "Domain of Hush", Some("Cleric"), "2024");
    let outline = json!({"outline": {"name": "Domain of Hush", "summary": "x", "quote": "Domain of Hush",
        "features": [
            {"level": 3, "name": "Hush Spells", "summary": "spells", "quote": "Hush Spells.",
             "spells": [{"level": 3, "spells": ["Bless", "Cure Wounds"]},
                        {"level": 5, "spells": ["Aid", "Quietude Hymn"]}]},
            {"level": 3, "name": "Soft Step", "summary": "quiet", "quote": "Soft Step."}]}});
    let fake = Fake::default()
        .on("import_detect", "Classify", &[detect])
        .on("import_outline", "Domain of Hush", &[outline.to_string()])
        .on(
            "import_generate",
            "of type Archetype Feature (",
            &[batch_of(&["Hush Spells", "Soft Step"], "Archetype Feature")],
        )
        .on(
            "import_generate",
            "Archetype",
            &[el_reply("Domain of Hush", "Archetype")],
        );
    let d = draft_for(&fake, HUSH_SOURCE, "hs", Edition::Y2024).await;
    // The feature prompt no longer tells the model that spell tables are prose only.
    let prompts = fake.prompts("import_generate").join("\n");
    assert!(!prompts.contains("progression tables are generated for you"));

    let rules = element(&d, "Hush Spells")["rules"]
        .as_array()
        .unwrap()
        .clone();
    let grant = |id: &str| {
        rules
            .iter()
            .find(|r| r["kind"] == "grant_spell" && r["spell"] == id)
            .unwrap_or_else(|| panic!("no grant of {id}: {rules:?}"))
    };
    assert_eq!(grant("ID_2024_SPELL_BLESS")["level"], 3);
    assert_eq!(grant("ID_2024_SPELL_AID")["level"], 5);
    assert_eq!(grant("ID_2024_SPELL_CURE_WOUNDS")["spell_list"], "Cleric");
    assert_eq!(grant("ID_2024_SPELL_AID")["mode"], "always_prepared");
    assert!(
        d.plan.notes.iter().any(|n| n.contains("Quietude Hymn")),
        "{:?}",
        d.plan.notes
    );

    let pack = finalize_draft(&base(), &d).expect("commits").pack;
    let store = base()
        .add_file("imported", PackOrigin::Custom, pack.content.clone())
        .build();
    let engine = Engine::new(&store);
    let sub = d.plan.groups[0].root_id.clone().unwrap();
    let mut state = commands::new_character(&engine, borealis_content::Ruleset::Dnd2024);
    let key = slot_key(&engine.derive(&state), "ID_LEVEL_1", "Class", 1).unwrap();
    state = commands::select(
        &engine,
        &state,
        &key,
        Choice::Element("ID_2024_CLASS_CLERIC".into()),
    )
    .unwrap()
    .state;
    for _ in 1..3 {
        state = commands::level_up_main(&engine, &state, None)
            .unwrap()
            .state;
    }
    let snap = engine.derive(&state);
    let slot = snap
        .slots
        .iter()
        .find(|s| s.chosen.is_none() && s.options.iter().any(|o| o.id == sub))
        .expect("the domain is offered at level 3");
    let key = slot.key.clone();
    state = commands::select(&engine, &state, &key, Choice::Element(sub))
        .unwrap()
        .state;
    let snap = engine.derive(&state);
    let spell = |name: &str| {
        snap.spellcasting
            .iter()
            .flat_map(|s| s.spells.iter())
            .find(|s| s.name == name)
            .map(|s| s.always_prepared)
    };
    assert_eq!(spell("Bless"), Some(true));
    assert_ne!(
        spell("Aid"),
        Some(true),
        "a level-5 row is not granted at level 3"
    );
}

// ---- extra class table columns ------------------------------------------------------------

const BRAMBLE_SOURCE: &str = "# The Thornwarden\n\nKeepers of hedges. Hit die d10.\n\n\
Level | Proficiency Bonus | Bramble Points | Features\n\n\
Level 1: Bramble Ward\nBramble Ward. You spend bramble points to raise thorns.\n";

fn thornwarden_reply() -> String {
    let text = std::fs::read_to_string(root().join("srd-5.2.1/classes/fighter.json")).unwrap();
    let file: Value = serde_json::from_str(&text).unwrap();
    let fighter = file["elements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == "ID_2024_CLASS_FIGHTER")
        .unwrap()
        .to_string()
        .replace("Fighter", "Thornwarden")
        .replace("FIGHTER", "THORNWARDEN");
    let mut el: Value = serde_json::from_str(&fighter).unwrap();
    el["rules"]
        .as_array_mut()
        .unwrap()
        .retain(|r| r["kind"] != "equipment_package");
    json!({"element": el, "notes": []}).to_string()
}

#[tokio::test]
async fn class_table_columns_the_model_left_out_become_progression_and_counters() {
    let detect = detect("class", "Thornwarden", None, "2024");
    let mut points: Vec<Value> = vec![json!("—")];
    points.extend((2..=20).map(|l| json!(format!("{}", l / 2 + 1))));
    let outline = json!({"outline": {"name": "Thornwarden", "summary": "x", "quote": "The Thornwarden",
        "hit_die": 10, "subclass_level": 3,
        "columns": [{"name": "Bramble Points", "values": points},
                    {"name": "Proficiency Bonus", "values": [2]}],
        "features": [{"level": 1, "name": "Bramble Ward", "summary": "thorns", "quote": "Bramble Ward."}]}});
    let fake = Fake::default()
        .on("import_detect", "Classify", &[detect])
        .on("import_outline", "Thornwarden", &[outline.to_string()])
        .on(
            "import_generate",
            "of type Class Feature (",
            &[batch_of(&["Bramble Ward"], "Class Feature")],
        )
        .on("import_generate", "(type Class,", &[thornwarden_reply()]);
    let d = draft_for(&fake, BRAMBLE_SOURCE, "tw", Edition::Y2024).await;
    let prompts = fake.prompts("import_outline").join("\n");
    assert!(
        prompts.contains("`columns`"),
        "the outline asks for every column"
    );
    let col = element(&d, "Thornwarden")["data"]["progression"]["bramble_points"].clone();
    let col = col.as_array().expect("column written").clone();
    assert_eq!(col.len(), 20);
    assert_eq!((col[0].clone(), col[1].clone()), (json!(0), json!(2)));

    let pack = finalize_draft(&base(), &d).expect("commits").pack;
    let report = pack.report.unwrap();
    assert!(
        report
            .normalised
            .iter()
            .any(|n| n.message.contains("bramble_points")),
        "{:?}",
        report.normalised
    );
    let ward = pack
        .content
        .elements
        .iter()
        .find(|e| e.name == "Bramble Ward")
        .unwrap();
    let json = serde_json::to_string(&ward.rules).unwrap();
    assert!(
        json.contains("bramble_points"),
        "counter on its feature: {json}"
    );
}

// ---- a spell compendium with class spell lists -------------------------------------------

const GRIMOIRE_SOURCE: &str = "# Spells\n\n## Ember Veil\n1st-level evocation\n\
Classes: Sorcerer\nA veil of embers.\n\n\
## Saltwhisper\nCantrip divination\nClasses: Wizard, Druid\nYou taste the air.\n\n\
## Class Spell Lists\n\n### Wizard Spells\n\n- Ember Veil\n- Cure Wounds\n\n\
### Sorcerer Spells\n\n- Ember Veil\n";

fn spell_reply(name: &str, classes: &[&str]) -> String {
    json!({"element": {"id": "X", "name": name, "type": "Spell",
        "description": format!("<p>{name}.</p>"), "supports": classes,
        "setters": [{"name": "level", "value": "1"}, {"name": "school", "value": "Evocation"},
            {"name": "time", "value": "1 action"}, {"name": "range", "value": "30 feet"},
            {"name": "duration", "value": "Instantaneous"},
            {"name": "hasVerbalComponent", "value": "true"},
            {"name": "hasSomaticComponent", "value": "true"},
            {"name": "hasMaterialComponent", "value": "false"},
            {"name": "isConcentration", "value": "false"}, {"name": "isRitual", "value": "false"}]},
        "notes": []})
    .to_string()
}

#[tokio::test]
async fn printed_class_spell_lists_join_existing_classes_by_union() {
    let detect = json!({"kind": "spell", "edition": "2014", "confidence": 0.9, "items": [
        {"kind": "spell", "name": "Ember Veil"}, {"kind": "spell", "name": "Saltwhisper"}],
        "other": [{"name": "Wizard Spells", "what": "class spell list"},
                  {"name": "Sorcerer Spells", "what": "class spell list"}]});
    let spells = batch_reply(&[
        spell_reply("Ember Veil", &["Sorcerer"]),
        spell_reply("Saltwhisper", &["Wizard", "Druid"]),
    ]);
    let fake = Fake::default()
        .on("import_detect", "Classify", &[detect.to_string()])
        .on("import_generate", "of type Spell (", &[spells]);
    let d = draft_for(&fake, GRIMOIRE_SOURCE, "gr", Edition::Y2014).await;
    let prompts = fake.prompts("import_detect").join("\n");
    assert!(prompts.contains("class spell list"));

    let fin = finalize_draft(&base(), &d).expect("commits");
    let pack = fin.pack;
    let supports = |name: &str| {
        pack.content
            .elements
            .iter()
            .find(|e| e.name == name)
            .unwrap()
            .supports
            .clone()
    };
    // Union: the Wizard list section adds Wizard to Ember Veil; Saltwhisper keeps its own line.
    assert_eq!(supports("Ember Veil"), ["Sorcerer", "Wizard"]);
    assert_eq!(supports("Saltwhisper"), ["Wizard", "Druid"]);
    // An SRD spell printed in the Wizard list joins the bundled Wizard list by an append.
    let cure = pack
        .content
        .appends
        .iter()
        .find(|a| a.id == "ID_PHB_SPELL_CURE_WOUNDS")
        .expect("append for the existing spell");
    assert_eq!(
        serde_json::to_value(cure).unwrap()["supports"],
        json!(["Wizard"])
    );
    let report = pack.report.unwrap();
    let says = |part: &str| report.normalised.iter().any(|n| n.message.contains(part));
    assert!(says("Classes line omits Wizard"), "{:?}", report.normalised);
    assert!(says("missing from its spell list section"));
    assert!(says("Cure Wounds"));
}

// ---- numbered item variants, 2014 subrace tags, feat prerequisites ------------------------

const MIXED_SOURCE: &str = "## Lantern of Dusk, +1, +2, or +3\nWondrous item. A lantern \
that grants a bonus to saving throws.\n\n## Burrowfolk\nSmall diggers.\n\n## Deepdelver\n\
A burrowfolk subrace.\n\n## Gloomstep\nYou step through shadows.\n\n## Ashen Grip\n\
Prerequisite: Gloomstep feat, Strength 13 or higher\nYour grip burns.\n";

#[tokio::test]
async fn variants_split_subraces_carry_the_race_name_and_prerequisites_gate_feats() {
    let detect = json!({"kind": "mixed", "edition": "2014", "confidence": 0.9, "items": [
        {"kind": "item", "name": "Lantern of Dusk +1/+2/+3", "item_type": "magic_item"},
        {"kind": "species", "name": "Burrowfolk"},
        {"kind": "subrace", "name": "Deepdelver", "parent_hint": "Burrowfolk"},
        {"kind": "feat", "name": "Gloomstep"}, {"kind": "feat", "name": "Ashen Grip"}]});
    let lantern = |n: &str| {
        json!({"element": {"id": "X", "name": n, "type": "Magic Item",
            "description": "<p>Worth 250gp, weighs 2lbs.</p>",
            "setters": [{"name": "rarity", "value": "Uncommon"}]}, "notes": []})
        .to_string()
    };
    let lanterns: Vec<String> = ["+1", "+2", "+3"]
        .iter()
        .map(|b| lantern(&format!("Lantern of Dusk, {b}")))
        .collect();
    let outlines = json!({"outlines": [
        {"name": "Burrowfolk", "summary": "x", "features": []},
        {"name": "Deepdelver", "summary": "x", "features": []}]});
    let grip = json!({"element": {"id": "X", "name": "Ashen Grip", "type": "Feat",
        "description": "<p>Your grip burns.</p>",
        "prerequisite": "Gloomstep feat, Strength 13 or higher"}, "notes": []})
    .to_string();
    let fake = Fake::default()
        .on("import_detect", "Classify", &[detect.to_string()])
        .on("import_outline", "Burrowfolk", &[outlines.to_string()])
        .on(
            "import_generate",
            "of type Magic Item (",
            &[batch_reply(&lanterns)],
        )
        .on(
            "import_generate",
            "(type Magic Item,",
            &[lanterns[0].clone()],
        )
        .on(
            "import_generate",
            "of type Feat (",
            &[batch_reply(&[el_reply("Gloomstep", "Feat"), grip.clone()])],
        )
        .on("import_generate", "(type Feat,", &[grip])
        .on(
            "import_generate",
            "of type Race (",
            &[batch_of(&["Burrowfolk"], "Race")],
        )
        .on(
            "import_generate",
            "(type Race,",
            &[el_reply("Burrowfolk", "Race")],
        )
        .on(
            "import_generate",
            "(type Sub Race,",
            &[el_reply("Deepdelver", "Sub Race")],
        );
    let d = draft_for(&fake, MIXED_SOURCE, "mx", Edition::Y2014).await;
    let detect_prompt = fake.prompts("import_detect").join("\n");
    assert!(detect_prompt.contains("once per version"));

    let names: Vec<&str> = d.elements.iter().map(|e| e.name.as_str()).collect();
    for b in ["+1", "+2", "+3"] {
        assert!(
            names.contains(&format!("Lantern of Dusk, {b}").as_str()),
            "{names:?}"
        );
    }
    let lantern = element(&d, "Lantern of Dusk, +2");
    assert_eq!(lantern["description"], "<p>Worth 250 gp, weighs 2 lb.</p>");

    // The subrace is tagged with its race's name, as the bundled 2014 subraces are.
    let deep = d.elements.iter().find(|e| e.name == "Deepdelver").unwrap();
    assert_eq!(deep.ty, "Sub Race", "{:?} {:?}", d.plan.notes, deep.issues);
    assert_eq!(element(&d, "Deepdelver")["supports"], json!(["Burrowfolk"]));
    let race_rules = element(&d, "Burrowfolk")["rules"]
        .as_array()
        .unwrap()
        .clone();
    let select = race_rules
        .iter()
        .find(|r| r["kind"] == "select" && r["type"] == "Sub Race")
        .expect("subrace choice");
    assert_eq!(select["supports"], "Burrowfolk");
    assert_eq!(select["name"], "Burrowfolk Subrace");

    assert_eq!(
        element(&d, "Ashen Grip")["requirements"],
        "[feature:Gloomstep]&&[str:13]"
    );
    finalize_draft(&base(), &d).expect("commits");
}
