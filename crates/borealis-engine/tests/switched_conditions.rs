//! HC1 (WB-19 Open, item HtCaM): conditions a character can have switched on. The snapshot lists the conditions the
//! loaded content defines (the SRD's, imported ones, homebrew ones; never the engine's helper conditions such as
//! Resistance (Fire), never Exhaustion, which has its own levels), each off until the user switches it on. While a
//! condition is on, its rules apply like any element the character has (its statistics, its grants, rules elsewhere
//! that require it), and it is saved with the character. Off by default: nothing changes for existing characters.

use borealis_content::{ContentFile, ContentStoreBuilder, PackOrigin, Ruleset};
use borealis_engine::commands::{self, CommandError};
use borealis_engine::{CharacterSnapshot, CharacterState, Engine};
use serde_json::{json, Value};
use std::path::Path;

const EMBER: &str = "ID_EI_CONDITION_EMBER_BURNT";
const PRONE: &str = "ID_2024_CONDITION_PRONE";
const WARDED: &str = "ID_T_FEAT_EMBER_WARD";

fn engine(pack: &str, ruleset: &str, extra: Vec<Value>) -> Engine {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let file: ContentFile = serde_json::from_value(
        json!({"format": "borealis-content/1", "ruleset": ruleset, "elements": extra}),
    )
    .expect("synthetic content");
    let store = ContentStoreBuilder::new()
        .add_pack_dir(&root.join("core"))
        .and_then(|b| b.add_pack_dir(&root.join(pack)))
        .expect("bundled packs")
        .add_file("homebrew.json", PackOrigin::Core, file)
        .build();
    Engine::new(&store)
}

/// A homebrew 2024 condition as the creator's Condition form writes it, with rule blocks; and a feat whose rule
/// applies only while the character has the condition.
fn homebrew() -> Vec<Value> {
    vec![
        json!({"id": EMBER, "name": "Ember-Burnt", "type": "Condition", "source": "Ember Isles", "ruleset": "dnd2024",
            "supports": ["Condition"], "data": {"tag": "Condition"},
            "description": "<p>Smouldering embers cling to you.</p>",
            "rules": [{"kind": "stat", "name": "speed", "value": "-10"},
                      {"kind": "stat", "name": "ability checks", "value": "-1"}]}),
        json!({"id": WARDED, "name": "Ember Ward", "type": "Feat", "source": "Ember Isles", "ruleset": "dnd2024",
            "rules": [{"kind": "stat", "name": "ac:misc", "value": "2", "requirements": EMBER},
                      {"kind": "stat", "name": "innate speed", "value": "30"}]}),
    ]
}

fn names(snap: &CharacterSnapshot) -> Vec<&str> {
    snap.conditions.iter().map(|c| c.name.as_str()).collect()
}

fn switch(e: &Engine, s: &CharacterState, id: &str, on: bool) -> CharacterState {
    commands::set_condition(e, s, id, on)
        .unwrap_or_else(|err| panic!("{id}: {err}"))
        .state
}

const SRD: [&str; 14] = [
    "Blinded",
    "Charmed",
    "Deafened",
    "Frightened",
    "Grappled",
    "Incapacitated",
    "Invisible",
    "Paralyzed",
    "Petrified",
    "Poisoned",
    "Prone",
    "Restrained",
    "Stunned",
    "Unconscious",
];

#[test]
fn the_snapshot_lists_the_loaded_conditions_all_off() {
    let e = engine("srd-5.2.1", "dnd2024", homebrew());
    let snap = e.derive(&commands::new_character(&e, Ruleset::Dnd2024));
    let mut want: Vec<&str> = SRD.to_vec();
    want.push("Ember-Burnt");
    want.sort_unstable();
    assert_eq!(names(&snap), want, "by name; no Exhaustion, no helper");
    assert!(snap.conditions.iter().all(|c| !c.on));
    let ember = snap.conditions.iter().find(|c| c.id == EMBER).unwrap();
    assert_eq!(ember.source, "Ember Isles");
    assert!(ember.description.contains("Smouldering embers"));
    assert_eq!(ember.effects, ["Speed -10", "Ability checks -1"]);

    let e14 = engine("srd-5.1", "dnd2014", Vec::new());
    let snap = e14.derive(&commands::new_character(&e14, Ruleset::Dnd2014));
    assert_eq!(
        names(&snap),
        SRD,
        "the 2014 character lists the SRD 5.1 conditions"
    );
    assert!(snap
        .conditions
        .iter()
        .all(|c| c.id.starts_with("ID_SRD51_")));
}

#[test]
fn a_condition_switched_on_applies_its_rules_until_it_is_switched_off() {
    let e = engine("srd-5.2.1", "dnd2024", homebrew());
    let mut s = commands::new_character(&e, Ruleset::Dnd2024);
    s.options.push(WARDED.into());
    let before = e.derive(&s);
    let on = switch(&e, &s, EMBER, true);
    assert!(on.play.conditions.contains(EMBER));
    let snap = e.derive(&on);
    assert!(snap.conditions.iter().find(|c| c.id == EMBER).unwrap().on);
    assert_eq!(snap.speed.walk, before.speed.walk - 10);
    let stealth = |x: &CharacterSnapshot| x.skill("stealth").unwrap().total;
    assert_eq!(stealth(&snap), stealth(&before) - 1);
    assert_eq!(
        snap.armor_class,
        before.armor_class + 2,
        "a rule that requires it"
    );
    let off = switch(&e, &on, EMBER, false);
    assert!(off.play.conditions.is_empty());
    assert_eq!(e.derive(&off), before);
}

#[test]
fn an_srd_condition_switched_on_is_listed_on_and_changes_no_number() {
    let e = engine("srd-5.2.1", "dnd2024", Vec::new());
    let s = commands::new_character(&e, Ruleset::Dnd2024);
    let before = e.derive(&s);
    let snap = e.derive(&switch(&e, &s, PRONE, true));
    let prone = snap.conditions.iter().find(|c| c.id == PRONE).unwrap();
    assert!(prone.on && prone.effects.is_empty());
    assert_eq!(
        (snap.speed.walk, snap.armor_class, snap.initiative),
        (before.speed.walk, before.armor_class, before.initiative)
    );
    assert_eq!(snap.skills, before.skills);
}

#[test]
fn only_a_listed_condition_can_be_switched_on_and_any_can_be_switched_off() {
    let e = engine("srd-5.2.1", "dnd2024", homebrew());
    let s = commands::new_character(&e, Ruleset::Dnd2024);
    for id in [
        "ID_2024_CONDITION_EXHAUSTION",
        "ID_INTERNAL_CONDITION_DAMAGE_RESISTANCE_FIRE",
        "ID_2024_FEAT_ALERT",
        "ID_NOT_LOADED",
    ] {
        assert!(
            matches!(
                commands::set_condition(&e, &s, id, true),
                Err(CommandError::UnknownElement(_))
            ),
            "{id}"
        );
    }
    // a condition of a pack that is gone: it stays off the list and can be switched off
    let mut gone = s.clone();
    gone.play.conditions.insert("ID_GONE_CONDITION".into());
    let snap = e.derive(&gone);
    assert!(!snap.conditions.iter().any(|c| c.id == "ID_GONE_CONDITION"));
    assert!(switch(&e, &gone, "ID_GONE_CONDITION", false)
        .play
        .conditions
        .is_empty());
}

/// IpY2E: a condition the creator wrote with "Listed as a condition" off (`data.listed: false`) is no condition a
/// character switches on, in a 2014 pack too, where the listed ones carry no tag either.
#[test]
fn a_condition_marked_unlisted_is_not_offered() {
    const WARD: &str = "ID_EI_CONDITION_EMBER_WARD";
    let ward = |ruleset: &str| {
        json!({"id": WARD, "name": "Ember Ward", "type": "Condition", "source": "Ember Isles", "ruleset": ruleset,
            "data": {"listed": false}, "rules": [{"kind": "stat", "name": "ac:misc", "value": "1"}]})
    };
    for (pack, ruleset, edition) in [
        ("srd-5.1", "dnd2014", Ruleset::Dnd2014),
        ("srd-5.2.1", "dnd2024", Ruleset::Dnd2024),
    ] {
        let e = engine(pack, ruleset, vec![ward(ruleset)]);
        let s = commands::new_character(&e, edition);
        let snap = e.derive(&s);
        assert!(!snap.conditions.iter().any(|c| c.id == WARD), "{ruleset}");
        assert_eq!(names(&snap), SRD, "{ruleset}: the SRD's stay listed");
        assert!(
            matches!(
                commands::set_condition(&e, &s, WARD, true),
                Err(CommandError::UnknownElement(_))
            ),
            "{ruleset}"
        );
    }
}

#[test]
fn switched_conditions_are_saved_and_last_through_rests() {
    let e = engine("srd-5.2.1", "dnd2024", homebrew());
    let s = commands::new_character(&e, Ruleset::Dnd2024);
    let none = serde_json::to_value(&s.play).unwrap();
    assert!(
        none.get("conditions").is_none(),
        "nothing written while none is on"
    );
    let on = switch(&e, &switch(&e, &s, EMBER, true), PRONE, true);
    let saved = serde_json::to_value(&on.play).unwrap();
    assert_eq!(saved["conditions"], json!([PRONE, EMBER]));
    let back: borealis_engine::state::PlayState = serde_json::from_value(saved).unwrap();
    assert_eq!(back, on.play);
    // an older file's play state has no list: nothing is on
    let old: borealis_engine::state::PlayState =
        serde_json::from_value(json!({"toggles": [], "exhaustion": 1})).unwrap();
    assert!(old.conditions.is_empty() && old.extra.is_empty());
    // a broken list is dropped, not the whole play state
    let bad: borealis_engine::state::PlayState =
        serde_json::from_value(json!({"conditions": 3, "exhaustion": 2})).unwrap();
    assert!(bad.conditions.is_empty() && bad.exhaustion == 2);
    let rested = commands::long_rest(&e, &on).unwrap().state;
    assert_eq!(rested.play.conditions, on.play.conditions);
    let rested = commands::short_rest(&e, &on).unwrap().state;
    assert_eq!(rested.play.conditions, on.play.conditions);
}
