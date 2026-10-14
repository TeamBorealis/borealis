//! Sources that define the parts they use: a class with its own subclasses, weapons with new
//! properties, items far into a long source. Scripted fake provider, no CLI.

mod import_fake;

use borealis_assist::import::anything::{ImportSource, Role};
use borealis_assist::{finalize_draft, import_anything, Edition, ImportDraft, ImportStep};
use borealis_content::file::PackOrigin;
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

async fn draft_for(fake: &Fake, source: &str, prefix: &str) -> ImportDraft {
    let req = request(ImportSource::Text(source.into()), prefix, Edition::Y2024);
    match import_anything(fake, &base(), &req, &opts(), None)
        .await
        .unwrap()
    {
        ImportStep::Draft(d) => *d,
        ImportStep::NeedsChoice(d) => panic!("stopped at a choice: {:?}", d.plan.open_choices()),
    }
}

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

fn strs(v: &[String]) -> Vec<&str> {
    v.iter().map(String::as_str).collect()
}

fn batch_of(names: &[&str], ty: &str) -> String {
    batch_reply(&names.iter().map(|n| el_reply(n, ty)).collect::<Vec<_>>())
}

// ---- a class with two subclasses of its own ------------------------------------------------

const LANTERN_SOURCE: &str = "# The Lanternwright\n\nKeepers of small flames. Hit die d8.\n\n\
Level 1: Kindle\nKindle. You light a flame in your palm.\n\n\
Level 3: Lanternwright Subclass\nYou choose a lantern order.\n\n\
## Order of the Wick\n\nWick Ward. A ward of light.\n\nSteady Glow. Your light holds.\n\n\
## Glassmoth Circle\n\nMoth Swarm. Moths gather.\n\nMirror Wings. Wings reflect.\n\n\
Bright Exodus. The moths carry you away.\n";

const WICK: [(u32, &str); 2] = [(3, "Wick Ward"), (7, "Steady Glow")];
const MOTH: [(u32, &str); 3] = [
    (3, "Moth Swarm"),
    (7, "Mirror Wings"),
    (14, "Bright Exodus"),
];

fn lantern_fake() -> Fake {
    let detect = json!({"kind": "mixed", "edition": "2024", "confidence": 0.9, "items": [
        {"kind": "class", "name": "Lanternwright"},
        {"kind": "subclass", "name": "Order of the Wick", "parent_hint": "Lanternwright order"},
        {"kind": "subclass", "name": "Glassmoth Circle"}]});
    let class = serde_json::from_str::<Value>(&outline(
        "Lanternwright",
        json!({"hit_die": 8, "subclass_level": 3}),
        &[
            (1, "Kindle"),
            (3, "Lanternwright Subclass"),
            (7, "Subclass Feature"),
            (15, "Subclass Feature"),
        ],
    ))
    .unwrap()["outline"]
        .clone();
    let outlines = json!({"outlines": [class, outline_of("Order of the Wick", &WICK),
        outline_of("Glassmoth Circle", &MOTH)]});
    let sub_features: Vec<&str> = WICK.iter().chain(&MOTH).map(|(_, n)| *n).collect();
    Fake::default()
        .on("import_detect", "Classify", &[detect.to_string()])
        .on("import_outline", "in ONE answer", &[outlines.to_string()])
        .on(
            "import_generate",
            "of type Class Feature (",
            &[batch_of(&["Kindle"], "Class Feature")],
        )
        .on(
            "import_generate",
            "of type Archetype Feature (",
            &[batch_of(&sub_features, "Archetype Feature")],
        )
        .on(
            "import_generate",
            "of type Archetype (",
            &[batch_of(
                &["Order of the Wick", "Glassmoth Circle"],
                "Archetype",
            )],
        )
        .on(
            "import_generate",
            "(type Class,",
            &[class_reply("Lanternwright")],
        )
}

fn engine_with(content: borealis_content::file::ContentFile) -> borealis_content::ContentStore {
    base()
        .add_file("imported", PackOrigin::Custom, content)
        .build()
}

fn character_of(engine: &Engine, class_id: &str) -> CharacterState {
    let state = commands::new_character(engine, borealis_content::Ruleset::Dnd2024);
    let snap = engine.derive(&state);
    let key = slot_key(&snap, "ID_LEVEL_1", "Class", 1).expect("class slot");
    commands::select(engine, &state, &key, Choice::Element(class_id.into()))
        .unwrap()
        .state
}

/// The open slot offering `id`, if any.
fn offering<'a>(
    snap: &'a CharacterSnapshot,
    id: &str,
) -> Option<&'a borealis_engine::snapshot::SlotSnapshot> {
    snap.slots
        .iter()
        .find(|s| s.chosen.is_none() && s.options.iter().any(|o| o.id == id))
}

#[tokio::test]
async fn subclasses_of_a_class_from_the_same_source_link_to_it() {
    let fake = lantern_fake();
    let d = draft_for(&fake, LANTERN_SOURCE, "lw").await;
    let class_id = d.plan.groups[0].root_id.clone().unwrap();
    for g in &d.plan.groups[1..] {
        let p = g.parent.as_ref().expect("linked to the planned class");
        assert!(p.in_source);
        assert_eq!(p.class_id, class_id);
        assert_eq!(p.supports, "Lanternwright Subclass");
        assert_eq!(p.subclass_levels, vec![3, 7, 15]);
    }
    // One gate; the "Subclass Feature" rows only say when subclass features come.
    let gates = d.elements.iter().filter(|e| e.role == Role::SubclassGate);
    assert_eq!(gates.count(), 1);
    assert!(!d.elements.iter().any(|e| e.name == "Subclass Feature"));
    let exodus = d
        .elements
        .iter()
        .find(|e| e.name == "Bright Exodus")
        .unwrap();
    assert_eq!(exodus.level, Some(15), "{:?}", d.plan.notes);
    assert!(d.sweep_issues.is_empty(), "{:?}", d.sweep_issues);
    // detect + outlines + class features + subclass features + roots + class.
    assert_eq!(total_calls(&fake), 6);

    let pack = finalize_draft(&base(), &d).unwrap().pack;
    let store = engine_with(pack.content.clone());
    let engine = Engine::new(&store);
    let moth = d.plan.groups[2].root_id.clone().unwrap();
    let wick = d.plan.groups[1].root_id.clone().unwrap();
    let mut state = character_of(&engine, &class_id);
    for level in 2..=15 {
        state = commands::level_up_main(&engine, &state, None)
            .unwrap()
            .state;
        let snap = engine.derive(&state);
        if level < 3 {
            assert!(offering(&snap, &moth).is_none(), "offered at level {level}");
        }
        if level == 3 {
            let slot = offering(&snap, &moth).expect("subclass choice at level 3");
            assert!(slot.options.iter().any(|o| o.id == wick));
            let key = slot.key.clone();
            state = commands::select(&engine, &state, &key, Choice::Element(moth.clone()))
                .unwrap()
                .state;
        }
    }
    let snap = engine.derive(&state);
    for name in ["Moth Swarm", "Mirror Wings", "Bright Exodus"] {
        assert!(
            snap.features.iter().any(|f| f.name == name),
            "{name} missing at level 15"
        );
    }
}

// ---- equipment: weapons, armor, ammunition, gear, a new weapon property --------------------

const GEAR_SOURCE: &str = "# The Tinker's Rack\n\n\
Brass Spitter. Firearm, 1d10 piercing, range 40/120, Ammunition, Loading, Sparkburst. 300 gp, 4 lb.\n\n\
Hooked Maul. Martial melee, 2d6 bludgeoning, Heavy, Two-Handed, Clatter. 20 gp, 9 lb.\n\n\
Riveted Coat. Medium armor, AC 13 + Dex (max 2). 60 gp, 18 lb.\n\n\
Spitter Shot. Ammunition for the brass spitter, bundle of 10. 2 gp.\n\n\
Powder Horn. Holds powder. 1 gp, 1 lb.\n\n\
Lantern Of Echoes. Wondrous item, uncommon.\n\n\
Sparkburst. When you roll a 1 on the attack, the weapon sparks and jams.\n";

fn item(name: &str, item_type: &str) -> Value {
    json!({"kind": "item", "name": name, "item_type": item_type})
}

fn el_with(name: &str, ty: &str, data: Value) -> String {
    json!({"element": {"id": "X", "name": name, "type": ty,
        "description": format!("<p>{name}.</p>"), "data": data}, "notes": []})
    .to_string()
}

fn gear_fake() -> Fake {
    let detect = json!({"kind": "mixed", "edition": "2024", "confidence": 0.9, "items": [
        item("Brass Spitter", "weapon"), item("Hooked Maul", "weapon"),
        item("Riveted Coat", "armor"), item("Spitter Shot", "ammunition"),
        item("Powder Horn", "gear"), item("Lantern Of Echoes", "magic_item"),
        {"kind": "weapon_property", "name": "Sparkburst"}],
        "other": [{"name": "Sparkburst", "what": "weapon property"}]});
    let spitter = el_with(
        "Brass Spitter",
        "Weapon",
        json!({"weapon_category": "Firearm", "melee": false,
            "damage": {"dice": "1d10", "type": "piercing"}, "range": "40/120",
            "properties": ["Ammunition", "Loading", "Sparkburst"],
            "cost": {"value": 300, "currency": "gp"}, "weight": 4}),
    );
    let maul = el_with(
        "Hooked Maul",
        "Weapon",
        json!({"weapon_category": "Martial Melee", "melee": true,
            "damage": {"dice": "2d6", "type": "bludgeoning"},
            "properties": ["Heavy", "Two-Handed", "Clatter"], "cost": "20 gp", "weight": "9 lb."}),
    );
    let coat = el_with(
        "Riveted Coat",
        "Armor",
        json!({"armor_category": "Medium", "base_ac": 13, "dex_modifier": true, "dex_cap": 2,
            "cost": {"value": 60, "currency": "gp"}, "weight": 18}),
    );
    let shot = el_with(
        "Spitter Shot",
        "Item",
        json!({"amount": 10, "cost": "2 gp"}),
    );
    let horn = el_with("Powder Horn", "Item", json!({"cost": "1 gp", "weight": 1}));
    Fake::default()
        .on("import_detect", "Classify", &[detect.to_string()])
        .on(
            "import_generate",
            "of type Weapon (",
            &[batch_reply(&[spitter, maul])],
        )
        .on(
            "import_generate",
            "of type Item (",
            &[batch_reply(&[shot, horn])],
        )
        .on("import_generate", "(type Armor,", &[coat])
        .on(
            "import_generate",
            "(type Magic Item,",
            &[el_reply("Lantern Of Echoes", "Magic Item")],
        )
        .on(
            "import_generate",
            "(type Weapon Property,",
            &[el_reply("Sparkburst", "Weapon Property")],
        )
}

fn by_name<'a>(
    d: &'a ImportDraft,
    name: &str,
) -> &'a borealis_assist::import::anything::DraftElement {
    d.elements.iter().find(|e| e.name == name).unwrap()
}

#[tokio::test]
async fn equipment_is_typed_like_the_srd_and_attacks_on_a_character() {
    let fake = gear_fake();
    let d = draft_for(&fake, GEAR_SOURCE, "tk").await;
    let types: Vec<(&str, &str)> = d
        .elements
        .iter()
        .map(|e| (e.name.as_str(), e.ty.as_str()))
        .collect();
    for want in [
        ("Brass Spitter", "Weapon"),
        ("Hooked Maul", "Weapon"),
        ("Riveted Coat", "Armor"),
        ("Spitter Shot", "Item"),
        ("Powder Horn", "Item"),
        ("Lantern Of Echoes", "Magic Item"),
        ("Sparkburst", "Weapon Property"),
    ] {
        assert!(types.contains(&want), "{want:?} not in {types:?}");
    }
    let bad: Vec<_> = d
        .elements
        .iter()
        .filter(|e| !e.issues.is_empty())
        .map(|e| (&e.name, &e.issues))
        .collect();
    assert!(bad.is_empty(), "{bad:?}");
    let spark = by_name(&d, "Sparkburst").id.clone();
    let spitter = &by_name(&d, "Brass Spitter").json;
    let tags = spitter["supports"].as_array().unwrap();
    for t in [
        "ID_INTERNAL_WEAPON_CATEGORY_FIREARM",
        "ID_INTERNAL_WEAPON_PROPERTY_AMMUNITION",
        "ID_INTERNAL_WEAPON_PROPERTY_LOADING",
        "ID_INTERNAL_DAMAGE_TYPE_PIERCING",
        spark.as_str(),
    ] {
        assert!(tags.iter().any(|v| v == t), "{t} missing: {tags:?}");
    }
    assert_eq!(
        spitter["data"]["properties"],
        json!(["Ammunition", "Loading", "Sparkburst"])
    );
    assert_eq!(by_name(&d, "Spitter Shot").json["data"]["ammunition"], true);
    // detect + weapons + gear batch + armor + magic item + property; no outline call.
    assert_eq!(total_calls(&fake), 6);

    let pack = finalize_draft(&base(), &d).unwrap().pack;
    let report = pack.report.clone().unwrap();
    let entry = |n: &str| report.entries.iter().find(|e| e.source == n).cloned();
    let s = entry("Sparkburst").expect("new property reported");
    assert_eq!(
        (s.outcome.as_str(), s.elements.clone()),
        ("text_only", vec![spark])
    );
    let c = entry("Clatter").expect("undefined property reported, not dropped");
    assert_eq!(c.elements, vec![by_name(&d, "Hooked Maul").id.clone()]);
    assert_eq!(report.entries.len(), 2, "{:?}", report.entries);

    let store = engine_with(pack.content.clone());
    let engine = Engine::new(&store);
    let mut state = character_of(&engine, "ID_2024_CLASS_FIGHTER");
    state = commands::set_base_scores(&engine, &state, [10, 16, 14, 10, 12, 8])
        .unwrap()
        .state;
    for name in ["Brass Spitter", "Riveted Coat"] {
        state = commands::add_item(&engine, &state, &by_name(&d, name).id, 1)
            .unwrap()
            .state;
        let uid = state.inventory.items.last().unwrap().uid.clone();
        state = commands::equip(&engine, &state, &uid, None).unwrap().state;
    }
    let snap = engine.derive(&state);
    let atk = snap
        .attacks
        .iter()
        .find(|a| a.name == "Brass Spitter")
        .expect("the imported weapon is an attack");
    assert_eq!(atk.damage, "1d10+3 piercing");
    assert_eq!(atk.range, "40/120");
    assert!(atk.proficient, "martial ranged proficiency covers firearms");
    assert_eq!(atk.attack, 3 + 2);
    // The same character in the bundled Scale Mail (14 + Dex, max 2): one point more.
    let mut srd = character_of(&engine, "ID_2024_CLASS_FIGHTER");
    srd = commands::set_base_scores(&engine, &srd, [10, 16, 14, 10, 12, 8])
        .unwrap()
        .state;
    srd = commands::add_item(&engine, &srd, "ID_2024_ARMOR_SCALE_MAIL", 1)
        .unwrap()
        .state;
    let uid = srd.inventory.items.last().unwrap().uid.clone();
    srd = commands::equip(&engine, &srd, &uid, None).unwrap().state;
    assert_eq!(snap.armor_class, engine.derive(&srd).armor_class - 1);
}

// ---- long sources: no silent cut at the start ------------------------------------------------

/// Over 9000 characters of filler, then three feats; "Iron Gut" is printed under another name.
fn long_source() -> String {
    let mut s = String::from("# A Long Almanac\n\n");
    for i in 0..400 {
        s.push_str(&format!("Filler line {i} about weather and roads.\n"));
    }
    s.push_str("\nQuick Draw. You draw faster than most.\n\n");
    s.push_str("Steady Hand. Your aim does not waver.\n\n");
    s.push_str("Gut Of Iron. Poison troubles you less.\n");
    s
}

#[tokio::test]
async fn long_source_batches_give_each_element_its_own_source_and_report_it() {
    let detect = json!({"kind": "feat", "edition": "2024", "confidence": 0.9, "items": [
        {"kind": "feat", "name": "Quick Draw"}, {"kind": "feat", "name": "Steady Hand"},
        {"kind": "feat", "name": "Iron Gut"}]});
    let feats = batch_of(&["Quick Draw", "Steady Hand", "Iron Gut"], "Feat");
    let fake = Fake::default()
        .on("import_detect", "Classify", &[detect.to_string()])
        .on("import_generate", "of type Feat (", &[feats]);
    let source = long_source();
    assert!(source.len() > 9000);
    let d = draft_for(&fake, &source, "lg").await;
    let prompts = fake.prompts("import_generate");
    // One call: every feat brings its own labelled source. The located pair their passages
    // only; the unlocated feat the whole source (its passage is past the old 9000-character cut).
    assert_eq!(prompts.len(), 1, "{}", prompts.len());
    let p = &prompts[0];
    let own = |name: &str| {
        let start = p.find(&format!("=== Source for \"{name}\"")).unwrap();
        let rest = &p[start + 4..];
        rest[..rest.find("=== Source for").unwrap_or(rest.len())].to_string()
    };
    let quick = own("Quick Draw");
    assert!(quick.contains("Quick Draw. You draw") && !quick.contains("Filler line 3 "));
    let whole = own("Iron Gut");
    assert!(
        whole.contains("Filler line 3 ")
            && whole.contains("Gut Of Iron. Poison troubles you less.")
    );
    let report = finalize_draft(&base(), &d).unwrap().pack.report.unwrap();
    assert_eq!(report.warnings.len(), 1, "{:?}", report.warnings);
    assert!(report.warnings[0].message.contains("Iron Gut"));
}

// ---- cost of a large mixed source ------------------------------------------------------------

#[tokio::test]
async fn class_with_five_subclasses_and_26_items_costs_twelve_calls() {
    let subs: Vec<String> = (1..=5).map(|i| format!("Lantern Order {i}")).collect();
    let feats = |i: usize| -> Vec<(u32, String)> {
        [3, 7, 15]
            .iter()
            .map(|l| (*l, format!("Order {i} Gift {l}")))
            .collect()
    };
    let per_type: [(&str, &str, usize); 5] = [
        ("weapon", "Weapon", 10),
        ("ammunition", "Item", 4),
        ("gear", "Item", 2),
        ("armor", "Armor", 2),
        ("magic_item", "Magic Item", 8),
    ];
    let mut items = vec![json!({"kind": "class", "name": "Lanternwright"})];
    items.extend(subs.iter().map(|s| json!({"kind": "subclass", "name": s})));
    let mut by_type: Vec<(&str, Vec<String>)> = Vec::new();
    for (it, ty, n) in per_type {
        let names: Vec<String> = (1..=n).map(|k| format!("Rack {it} {k}")).collect();
        items.extend(names.iter().map(|nm| item(nm, it)));
        match by_type.iter_mut().find(|(t, _)| *t == ty) {
            Some((_, v)) => v.extend(names),
            None => by_type.push((ty, names)),
        }
    }
    let detect = json!({"kind": "mixed", "edition": "2024", "confidence": 0.9, "items": items});
    let class = serde_json::from_str::<Value>(&outline(
        "Lanternwright",
        json!({"hit_die": 8, "subclass_level": 3}),
        &[(1, "Kindle"), (3, "Lanternwright Subclass")],
    ))
    .unwrap()["outline"]
        .clone();
    let mut outlines = vec![class];
    let mut sub_feature_names: Vec<String> = Vec::new();
    for (i, s) in subs.iter().enumerate() {
        let f = feats(i + 1);
        let refs: Vec<(u32, &str)> = f.iter().map(|(l, n)| (*l, n.as_str())).collect();
        outlines.push(outline_of(s, &refs));
        sub_feature_names.extend(f.into_iter().map(|(_, n)| n));
    }
    let sub_refs: Vec<&str> = subs.iter().map(String::as_str).collect();
    let mut fake = Fake::default()
        .on("import_detect", "Classify", &[detect.to_string()])
        .on(
            "import_outline",
            "in ONE answer",
            &[json!({ "outlines": outlines }).to_string()],
        )
        .on(
            "import_generate",
            "of type Class Feature (",
            &[batch_of(&["Kindle"], "Class Feature")],
        )
        .on(
            "import_generate",
            "of type Archetype Feature (",
            &[batch_of(&strs(&sub_feature_names), "Archetype Feature")],
        )
        .on(
            "import_generate",
            "of type Archetype (",
            &[batch_of(&sub_refs, "Archetype")],
        )
        .on(
            "import_generate",
            "(type Class,",
            &[class_reply("Lanternwright")],
        );
    for (ty, list) in &by_type {
        let needle = format!("of type {ty} (");
        fake = fake.on("import_generate", &needle, &[batch_of(&strs(list), ty)]);
    }
    let d = draft_for(&fake, LANTERN_SOURCE, "big").await;
    assert_eq!(
        d.elements
            .iter()
            .filter(|e| e.role == Role::Standalone)
            .count(),
        26
    );
    // detect 1 + outlines 1 + class features 1 + subclass features 2 (15 / 8) + roots 1 +
    // class 1 + weapons 2 + ammunition and gear 1 + armor 1 + magic items 1.
    assert_eq!(
        total_calls(&fake),
        12,
        "{:?}",
        fake.log.lock().unwrap().len()
    );
    assert_eq!(fake.calls("import_repair"), 0);
}
