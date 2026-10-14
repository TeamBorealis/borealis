//! Named sub-items are elements of their own, pickable on a character: species traits and
//! subraces, background features, a pick-N option list of a class feature, the named
//! sub-features of a subclass feature, and rules or lore kept as text. Scripted fake provider,
//! invented content, no CLI.

mod import_fake;

use borealis_assist::import::anything::{ImportSource, Role};
use borealis_assist::{finalize_draft, import_anything, Edition, ImportDraft, ImportStep};
use borealis_content::file::PackOrigin;
use borealis_content::Ruleset;
use borealis_engine::commands::{self, slot_key};
use borealis_engine::{CharacterSnapshot, CharacterState, Choice, Engine};
use import_fake::*;
use serde_json::{json, Value};

const IMPORT_TASKS: [&str; 4] = [
    "import_detect",
    "import_outline",
    "import_generate",
    "import_repair",
];

fn total_calls(fake: &Fake) -> usize {
    IMPORT_TASKS.iter().map(|t| fake.calls(t)).sum()
}

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

fn part(name: &str) -> Value {
    json!({"name": name, "summary": format!("{name} effect"), "quote": format!("{name}.")})
}

/// An outline whose features are `(level, name, extra fields)`.
fn outline_with(name: &str, extra: Value, features: &[(u32, &str, Value)]) -> Value {
    let fs: Vec<Value> = features
        .iter()
        .map(|(l, n, x)| {
            let mut f = json!({"level": l, "name": n, "summary": format!("{n} effect"),
                "quote": format!("{n}.")});
            for (k, v) in x.as_object().cloned().unwrap_or_default() {
                f[k] = v;
            }
            f
        })
        .collect();
    let mut o = json!({"name": name, "summary": format!("{name} summary"), "features": fs,
        "quote": name});
    for (k, v) in extra.as_object().cloned().unwrap_or_default() {
        o[k] = v;
    }
    o
}

fn traits(name: &str, names: &[&str]) -> Value {
    let fs: Vec<(u32, &str, Value)> = names.iter().map(|n| (1, *n, json!({}))).collect();
    outline_with(name, json!({}), &fs)
}

fn engine_store(d: &ImportDraft) -> borealis_content::ContentStore {
    let pack = finalize_draft(&base(), d).unwrap().pack;
    base()
        .add_file("imported", PackOrigin::Custom, pack.content)
        .build()
}

fn by_name<'a>(
    d: &'a ImportDraft,
    name: &str,
) -> &'a borealis_assist::import::anything::DraftElement {
    d.elements
        .iter()
        .find(|e| e.name == name)
        .unwrap_or_else(|| panic!("no element {name}"))
}

/// Open slots offering `id`.
fn offering<'a>(
    snap: &'a CharacterSnapshot,
    id: &str,
) -> Vec<&'a borealis_engine::snapshot::SlotSnapshot> {
    snap.slots
        .iter()
        .filter(|s| s.chosen.is_none() && s.options.iter().any(|o| o.id == id))
        .collect()
}

fn pick(engine: &Engine, state: &CharacterState, id: &str) -> CharacterState {
    let snap = engine.derive(state);
    let key = offering(&snap, id)
        .first()
        .unwrap_or_else(|| panic!("no open slot offers {id}"))
        .key
        .clone();
    commands::select(engine, state, &key, Choice::Element(id.into()))
        .unwrap()
        .state
}

fn names(v: &[String]) -> Vec<&str> {
    v.iter().map(String::as_str).collect()
}

fn has_feature(snap: &CharacterSnapshot, name: &str) -> bool {
    snap.features.iter().any(|f| f.name == name)
}

// ---- a species with subraces and traits, a background with a feature, a lore section ------

const MARSH_SOURCE: &str = "# Mirefolk\n\nMirefolk wander the reed marshes.\n\n\
Ability Score Increase. Your Constitution score increases by 2.\n\n\
Reed Breath. You can hold your breath for an hour.\n\n\
Murk Sight. You see through fog and murk.\n\n\
## Fen Mirefolk\n\nFen Lore. You always know the safe path through a fen.\n\n\
## Tide Mirefolk\n\nTide Grip. You have a swimming speed equal to your walking speed.\n\n\
# Lamplighter\n\nYou kept the marsh lamps lit.\n\n\
Lamp Route. You know every lamp post and the folk who tend them.\n\n\
# The Drowned Bells\n\nLong ago the marsh swallowed a bell tower.\n\
On still nights the bells still ring under the water.\n\n\
Those who hear them dream of the old town.\n";

const MIRE_TRAITS: [&str; 2] = ["Reed Breath", "Murk Sight"];

fn marsh_fake() -> Fake {
    let detect = json!({"kind": "mixed", "edition": "2014", "confidence": 0.9, "items": [
        {"kind": "species", "name": "Mirefolk"},
        {"kind": "subrace", "name": "Fen Mirefolk", "parent_hint": "Mirefolk"},
        {"kind": "subrace", "name": "Tide Mirefolk", "parent_hint": "Mirefolk"},
        {"kind": "background", "name": "Lamplighter"}],
        "other": [{"name": "The Drowned Bells", "what": "lore"}]});
    let outlines = json!({"outlines": [traits("Mirefolk", &MIRE_TRAITS),
        traits("Fen Mirefolk", &["Fen Lore"]), traits("Tide Mirefolk", &["Tide Grip"]),
        traits("Lamplighter", &["Lamp Route"])]});
    Fake::default()
        .on("import_detect", "Classify", &[detect.to_string()])
        .on("import_outline", "in ONE answer", &[outlines.to_string()])
        .on(
            "import_generate",
            "of type Racial Trait (",
            &[batch_of(
                &["Reed Breath", "Murk Sight", "Fen Lore", "Tide Grip"],
                "Racial Trait",
            )],
        )
        .on(
            "import_generate",
            "of type Background Feature (",
            &[batch_of(&["Lamp Route"], "Background Feature")],
        )
        .on(
            "import_generate",
            "of type Sub Race (",
            &[batch_of(&["Fen Mirefolk", "Tide Mirefolk"], "Sub Race")],
        )
        .on(
            "import_generate",
            "(type Race,",
            &[el_reply("Mirefolk", "Race")],
        )
        .on(
            "import_generate",
            "(type Background,",
            &[el_reply("Lamplighter", "Background")],
        )
}

#[tokio::test]
async fn species_traits_subraces_background_features_and_lore_are_their_own_elements() {
    let fake = marsh_fake();
    let d = draft_for(&fake, MARSH_SOURCE, "mf", Edition::Y2014).await;
    let bad: Vec<_> = d
        .elements
        .iter()
        .filter(|e| e.json.is_null() || !e.issues.is_empty())
        .map(|e| (&e.name, &e.issues))
        .collect();
    assert!(bad.is_empty(), "{bad:?}");
    assert!(d.sweep_issues.is_empty(), "{:?}", d.sweep_issues);
    // detect + one outline call + traits + background features + subraces + species + background.
    assert_eq!(total_calls(&fake), 7);
    for name in ["Reed Breath", "Fen Lore", "Lamp Route"] {
        assert_eq!(by_name(&d, name).role, Role::Part);
    }
    assert_eq!(by_name(&d, "Fen Lore").ty, "Racial Trait");
    assert_eq!(by_name(&d, "Lamp Route").ty, "Background Feature");
    assert_eq!(by_name(&d, "Fen Mirefolk").ty, "Sub Race");
    // The umbrella keeps its text; its prompt says the traits are written separately.
    let race_prompt = fake
        .prompts("import_generate")
        .into_iter()
        .find(|p| p.contains("(type Race,"))
        .unwrap();
    assert!(race_prompt.contains("separate elements"));

    // Lore: a text-only element with the whole section, built without the model.
    let lore = by_name(&d, "The Drowned Bells");
    assert_eq!(lore.role, Role::Text);
    let text = lore.json["description"].as_str().unwrap();
    for line in [
        "swallowed a bell tower",
        "ring under the water",
        "dream of the old town",
    ] {
        assert!(text.contains(line), "{line} missing from {text}");
    }
    assert!(!text.contains("Mirefolk"), "{text}");

    let pack = finalize_draft(&base(), &d).unwrap().pack;
    let report = pack.report.as_ref().unwrap();
    let entry = report
        .entries
        .iter()
        .find(|e| e.source == "The Drowned Bells")
        .expect("lore reported");
    assert_eq!(entry.outcome, "text_only");
    assert!(entry.note.contains("not mechanised"), "{}", entry.note);

    // A character picks the species, then the subrace it offers, then the background.
    let store = engine_store(&d);
    let engine = Engine::new(&store);
    let mut state = commands::new_character(&engine, Ruleset::Dnd2014);
    let snap = engine.derive(&state);
    let key = slot_key(&snap, "ID_LEVEL_1", "Race", 1).expect("race slot");
    let mire = by_name(&d, "Mirefolk").id.clone();
    state = commands::select(&engine, &state, &key, Choice::Element(mire))
        .unwrap()
        .state;
    let fen = by_name(&d, "Fen Mirefolk").id.clone();
    let tide = by_name(&d, "Tide Mirefolk").id.clone();
    let snap = engine.derive(&state);
    let slots = offering(&snap, &fen);
    assert_eq!(slots.len(), 1, "one subrace choice");
    assert!(slots[0].options.iter().any(|o| o.id == tide));
    assert!(!slots[0].options.iter().any(|o| o.id.contains("HILL")));
    state = pick(&engine, &state, &fen);
    let key = slot_key(&engine.derive(&state), "ID_LEVEL_1", "Background", 1).expect("bg slot");
    let lamp = by_name(&d, "Lamplighter").id.clone();
    state = commands::select(&engine, &state, &key, Choice::Element(lamp))
        .unwrap()
        .state;
    let snap = engine.derive(&state);
    for name in ["Reed Breath", "Murk Sight", "Fen Lore", "Lamp Route"] {
        assert!(has_feature(&snap, name), "{name} missing");
    }
    assert!(!has_feature(&snap, "Tide Grip"));
}

// ---- a class feature with a pick-2 list, a subclass feature with three named parts ---------

const HEDGE_SOURCE: &str = "# The Hedgewarden\n\nWardens of the green hedges. Hit die d10.\n\n\
Hedge Sense. You feel every root around you.\n\n\
Thorn Charms. At 2nd level you learn two charms of your choice, and one more at 6th level.\n\n\
Burr Charm. Burrs cling to a foe you hit.\n\n\
Sap Charm. Sap closes a wound.\n\n\
Nettle Charm. Nettles sting a foe.\n\n\
Holly Charm. Prerequisite: 5th level. Holly leaves shield you.\n\n\
Hedgewarden Subclass. At 3rd level you choose a circle.\n\n\
## Circle of Brambles\n\n\
Bramble Heart. At 3rd level you gain three gifts.\n\n\
Thorn Skin. Your skin hardens like bark.\n\n\
Root Step. You step from root to root.\n\n\
Briar Voice. The briars whisper to you.\n";

const CHARMS: [&str; 4] = ["Burr Charm", "Sap Charm", "Nettle Charm", "Holly Charm"];
const GIFTS: [&str; 3] = ["Thorn Skin", "Root Step", "Briar Voice"];

/// The SRD 2024 Fighter class element renamed to `name` (a known engine-valid class shape).
fn class_reply(name: &str) -> String {
    let text = std::fs::read_to_string(root().join("srd-5.2.1/classes/fighter.json")).unwrap();
    let file: Value = serde_json::from_str(&text).unwrap();
    let fighter = file["elements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == "ID_2024_CLASS_FIGHTER")
        .unwrap()
        .to_string()
        .replace("Fighter", name)
        .replace("FIGHTER", &name.to_uppercase());
    let mut el: Value = serde_json::from_str(&fighter).unwrap();
    el["rules"]
        .as_array_mut()
        .unwrap()
        .retain(|r| r["kind"] != "equipment_package");
    json!({"element": el, "notes": []}).to_string()
}

fn hedge_fake() -> Fake {
    let detect = json!({"kind": "mixed", "edition": "2024", "confidence": 0.9, "items": [
        {"kind": "class", "name": "Hedgewarden"},
        {"kind": "subclass", "name": "Circle of Brambles", "parent_hint": "Hedgewarden"}]});
    let mut options: Vec<Value> = CHARMS.iter().map(|n| part(n)).collect();
    options[3]["level"] = json!(5);
    let class = outline_with(
        "Hedgewarden",
        json!({"hit_die": 10, "subclass_level": 3}),
        &[
            (1, "Hedge Sense", json!({})),
            (
                2,
                "Thorn Charms",
                json!({"options": options, "choose": [{"level": 2, "count": 2}, {"level": 6, "count": 1}]}),
            ),
            // The model also listed an option as a feature row: the same thing, not a feature.
            (2, "Sap Charm", json!({})),
            (3, "Hedgewarden Subclass", json!({})),
        ],
    );
    let gifts: Vec<Value> = GIFTS.iter().map(|n| part(n)).collect();
    let circle = outline_with(
        "Circle of Brambles",
        json!({}),
        &[(3, "Bramble Heart", json!({"parts": gifts}))],
    );
    let class_features: Vec<&str> = CHARMS
        .iter()
        .chain(&["Hedge Sense", "Thorn Charms"])
        .copied()
        .collect();
    let sub_features: Vec<&str> = GIFTS.iter().chain(&["Bramble Heart"]).copied().collect();
    Fake::default()
        .on("import_detect", "Classify", &[detect.to_string()])
        .on(
            "import_outline",
            "in ONE answer",
            &[json!({"outlines": [class, circle]}).to_string()],
        )
        .on(
            "import_generate",
            "of type Class Feature (",
            &[batch_of(&class_features, "Class Feature")],
        )
        .on(
            "import_generate",
            "of type Archetype Feature (",
            &[batch_of(&sub_features, "Archetype Feature")],
        )
        .on(
            "import_generate",
            "(type Archetype,",
            &[el_reply("Circle of Brambles", "Archetype")],
        )
        .on(
            "import_generate",
            "(type Class,",
            &[class_reply("Hedgewarden")],
        )
}

#[tokio::test]
async fn option_lists_and_named_sub_features_are_pickable_elements() {
    let fake = hedge_fake();
    let d = draft_for(&fake, HEDGE_SOURCE, "hw", Edition::Y2024).await;
    assert!(d.sweep_issues.is_empty(), "{:?}", d.sweep_issues);
    // detect + outlines + options + parts + class features + subclass features + root + class.
    assert_eq!(total_calls(&fake), 8);
    assert_eq!(
        d.elements.iter().filter(|e| e.name == "Sap Charm").count(),
        1
    );
    for r in CHARMS {
        assert_eq!(by_name(&d, r).role, Role::Option);
    }
    for g in GIFTS {
        assert_eq!(by_name(&d, g).role, Role::Part);
        assert_eq!(by_name(&d, g).ty, "Archetype Feature");
    }
    let hollies = &by_name(&d, "Holly Charm").json;
    assert_eq!(hollies["requirements"], "[level:5]");
    let rites = &by_name(&d, "Thorn Charms").json;
    let selects: Vec<&Value> = rites["rules"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["kind"] == "select")
        .collect();
    assert_eq!(selects.len(), 2, "{selects:?}");
    assert_eq!(selects[0]["number"], 2);
    assert_eq!(selects[1]["level"], 6);

    let store = engine_store(&d);
    let engine = Engine::new(&store);
    let class_id = d.plan.groups[0].root_id.clone().unwrap();
    let circle = d.plan.groups[1].root_id.clone().unwrap();
    let mut state = commands::new_character(&engine, Ruleset::Dnd2024);
    let key = slot_key(&engine.derive(&state), "ID_LEVEL_1", "Class", 1).expect("class slot");
    state = commands::select(&engine, &state, &key, Choice::Element(class_id))
        .unwrap()
        .state;
    let burrs = by_name(&d, "Burr Charm").id.clone();
    assert!(
        offering(&engine.derive(&state), &burrs).is_empty(),
        "no charms at level 1"
    );
    state = commands::level_up_main(&engine, &state, None)
        .unwrap()
        .state;
    let snap = engine.derive(&state);
    let open = offering(&snap, &burrs);
    assert_eq!(open.len(), 2, "two picks at level 2");
    let hollies_id = by_name(&d, "Holly Charm").id.clone();
    assert!(
        open.iter()
            .all(|s| s.options.iter().all(|o| o.id != hollies_id)),
        "the level 5 charm is not offered at level 2"
    );
    state = pick(&engine, &state, &burrs);
    state = pick(&engine, &state, &by_name(&d, "Sap Charm").id);
    state = commands::level_up_main(&engine, &state, None)
        .unwrap()
        .state;
    state = pick(&engine, &state, &circle);
    let snap = engine.derive(&state);
    for name in ["Burr Charm", "Sap Charm", "Bramble Heart"]
        .into_iter()
        .chain(GIFTS)
    {
        assert!(has_feature(&snap, name), "{name} missing at level 3");
    }
    assert!(!has_feature(&snap, "Nettle Charm"));
}

// ---- call count of a species-heavy source --------------------------------------------------

const KIN: [&str; 6] = [
    "Ashkin", "Brookkin", "Cindrel", "Dunmoth", "Emberlin", "Fogwight",
];

#[tokio::test]
async fn species_heavy_source_stays_within_a_small_number_of_calls() {
    // 6 species (3 traits each) with one subrace each (1 trait), 2 backgrounds (1 feature), 3
    // lore sections.
    let mut items = Vec::new();
    let mut outlines = Vec::new();
    let mut source = String::new();
    let (mut trait_names, mut subs) = (Vec::new(), Vec::new());
    for k in KIN {
        let sub = format!("Deep {k}");
        items.push(json!({"kind": "species", "name": k}));
        items.push(json!({"kind": "subrace", "name": sub, "parent_hint": k}));
        let ts: Vec<String> = (1..=3).map(|i| format!("{k} Gift {i}")).collect();
        let st = format!("{sub} Gift");
        outlines.push(traits(
            k,
            &ts.iter().map(String::as_str).collect::<Vec<_>>(),
        ));
        outlines.push(traits(&sub, &[&st]));
        source.push_str(&format!("# {k}\n\n{k} folk.\n\n"));
        for t in &ts {
            source.push_str(&format!("{t}. It helps.\n\n"));
        }
        source.push_str(&format!("## {sub}\n\n{st}. It helps too.\n\n"));
        trait_names.extend(ts);
        trait_names.push(st);
        subs.push(sub);
    }
    for b in ["Rope Maker", "Tin Smith"] {
        items.push(json!({"kind": "background", "name": b}));
        outlines.push(traits(b, &[&format!("{b} Ties")]));
        source.push_str(&format!("# {b}\n\n{b} Ties. Friends in town.\n\n"));
    }
    let lore = ["Old Roads", "The Salt War", "Moon Fairs"];
    for l in lore {
        source.push_str(&format!("# {l}\n\nA tale of {l}.\n\n"));
    }
    let other: Vec<Value> = lore
        .iter()
        .map(|l| json!({"name": l, "what": "lore"}))
        .collect();
    let detect = json!({"kind": "mixed", "edition": "2014", "confidence": 0.9, "items": items,
        "other": other});
    let (first, rest) = outlines.split_at(12);
    let fake = Fake::default()
        .on("import_detect", "Classify", &[detect.to_string()])
        .on(
            "import_outline",
            "in ONE answer",
            &[
                json!({"outlines": first}).to_string(),
                json!({"outlines": rest}).to_string(),
            ],
        )
        .on(
            "import_generate",
            "of type Racial Trait (",
            &[batch_of(&names(&trait_names), "Racial Trait")],
        )
        .on(
            "import_generate",
            "of type Background Feature (",
            &[batch_of(
                &["Rope Maker Ties", "Tin Smith Ties"],
                "Background Feature",
            )],
        )
        .on(
            "import_generate",
            "of type Sub Race (",
            &[batch_of(&names(&subs), "Sub Race")],
        )
        .on(
            "import_generate",
            "of type Race (",
            &[batch_of(&KIN, "Race")],
        )
        .on(
            "import_generate",
            "of type Background (",
            &[batch_of(&["Rope Maker", "Tin Smith"], "Background")],
        );
    let d = draft_for(&fake, &source, "kin", Edition::Y2014).await;
    assert!(d.sweep_issues.is_empty(), "{:?}", d.sweep_issues);
    assert_eq!(d.elements.len(), 6 + 6 + 24 + 2 + 2 + 3);
    assert_eq!(
        d.elements.iter().filter(|e| e.role == Role::Text).count(),
        3
    );
    // detect 1 + outlines 2 (12 per call) + traits 2 (16 per call) + background features 1 +
    // subraces 1 + species 1 + backgrounds 1; the lore sections cost no call.
    assert_eq!(total_calls(&fake), 9);
}

// ---- a lineage of a library species ---------------------------------------------------------

#[tokio::test]
async fn a_lineage_of_a_library_species_joins_its_choice() {
    let source = "# Dusk Elf\n\nA lineage of elves.\n\nDusk Step. You fade into twilight.\n";
    let detect = json!({"kind": "subrace", "edition": "2024", "confidence": 0.9, "items": [
        {"kind": "subrace", "name": "Dusk Elf", "parent_hint": "Elf"}]});
    let fake = Fake::default()
        .on("import_detect", "Classify", &[detect.to_string()])
        .on(
            "import_outline",
            "Outline",
            &[json!({"outline": traits("Dusk Elf", &["Dusk Step"])}).to_string()],
        )
        .on(
            "import_generate",
            "of type Racial Trait (",
            &[batch_of(&["Dusk Step"], "Racial Trait")],
        )
        .on(
            "import_generate",
            "(type Species Trait Option,",
            &[el_reply("Dusk Elf", "Species Trait Option")],
        );
    let d = draft_for(&fake, source, "de", Edition::Y2024).await;
    assert!(d.sweep_issues.is_empty(), "{:?}", d.sweep_issues);
    let dusk = by_name(&d, "Dusk Elf");
    assert_eq!(dusk.ty, "Species Trait Option");
    assert_eq!(dusk.json["supports"], json!(["Elven Lineage"]));

    let store = engine_store(&d);
    let engine = Engine::new(&store);
    let mut state = commands::new_character(&engine, Ruleset::Dnd2024);
    let key = slot_key(&engine.derive(&state), "ID_LEVEL_1", "Race", 1).expect("species slot");
    state = commands::select(
        &engine,
        &state,
        &key,
        Choice::Element("ID_2024_SPECIES_ELF".into()),
    )
    .unwrap()
    .state;
    let snap = engine.derive(&state);
    let slots = offering(&snap, &dusk.id);
    assert_eq!(slots.len(), 1, "offered by the elf's lineage choice");
    assert!(slots[0]
        .options
        .iter()
        .any(|o| o.id == "ID_2024_OPTION_ELVEN_LINEAGE_DROW"));
    state = pick(&engine, &state, &dusk.id);
    assert!(has_feature(&engine.derive(&state), "Dusk Step"));
}
