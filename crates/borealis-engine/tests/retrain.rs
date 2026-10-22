//! Spell retraining : Replace spell, per-slot "gained on level" and the
//! "replaced" history, for a 2014 and a 2024 caster on the bundled SRD packs.

use borealis_content::{ContentStoreBuilder, Ruleset};
use borealis_engine::commands::{self, slot_key};
use borealis_engine::snapshot::{CharacterSnapshot, SlotSnapshot};
use borealis_engine::state::{CharacterState, Choice, SlotKey};
use borealis_engine::Engine;
use std::path::Path;
use std::sync::OnceLock;

fn engine(pack: &str) -> &'static Engine {
    static E14: OnceLock<Engine> = OnceLock::new();
    static E24: OnceLock<Engine> = OnceLock::new();
    let cell = if pack == "srd-5.1" { &E14 } else { &E24 };
    cell.get_or_init(|| {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
        let store = ContentStoreBuilder::new()
            .add_pack_dir(&root.join("core"))
            .and_then(|b| b.add_pack_dir(&root.join(pack)))
            .expect("packs load")
            .build();
        Engine::new(&store)
    })
}

/// A character of `class` at `level`, every open spell slot of the class filled.
fn caster(e: &Engine, ruleset: Ruleset, class: &str, level: u32) -> CharacterState {
    let mut s = commands::new_character(e, ruleset);
    let key = slot_key(&e.derive(&s), "ID_LEVEL_1", "Class", 1).expect("class slot");
    s = commands::select(e, &s, &key, Choice::Element(class.into()))
        .expect("class")
        .state;
    for _ in 1..level {
        s = commands::level_up_main(e, &s, None)
            .expect("level up")
            .state;
    }
    for _ in 0..40 {
        let snap = e.derive(&s);
        let open = snap.slots.iter().find(|x| {
            x.spellcasting.is_some()
                && x.chosen.is_none()
                && x.rejected.is_none()
                && x.options.iter().any(|o| !taken(&snap, &o.id))
        });
        let Some(slot) = open else { break };
        let id = slot
            .options
            .iter()
            .find(|o| !taken(&snap, &o.id))
            .unwrap()
            .id
            .clone();
        s = commands::select(e, &s, &slot.key.clone(), Choice::Element(id))
            .expect("spell pick")
            .state;
    }
    s
}

fn taken(snap: &CharacterSnapshot, id: &str) -> bool {
    snap.slots
        .iter()
        .any(|s| s.chosen.as_ref().is_some_and(|c| c.id == id))
}

fn slot<'a>(snap: &'a CharacterSnapshot, name: &str, level: u32) -> &'a SlotSnapshot {
    snap.slots
        .iter()
        .find(|s| s.name == name && s.level == level && s.chosen.is_some())
        .unwrap_or_else(|| panic!("no filled slot {name} at {level}"))
}

fn by_key<'a>(snap: &'a CharacterSnapshot, key: &SlotKey) -> &'a SlotSnapshot {
    snap.slots.iter().find(|s| &s.key == key).expect("slot")
}

#[test]
fn sorcerer_2014_replaces_a_known_spell_and_keeps_history() {
    let e = engine("srd-5.1");
    let s = caster(e, Ruleset::Dnd2014, "ID_WOTC_PHB_CLASS_SORCERER", 5);
    let snap = e.derive(&s);
    let spell = slot(&snap, "Spell (Sorcerer)", 2);
    assert!(spell.replaceable, "known casters swap spells");
    assert_eq!(spell.gained_level, 2);
    assert!(spell.replaced.is_empty());
    let cantrip = slot(&snap, "Cantrip (Sorcerer)", 1);
    assert!(!cantrip.replaceable, "2014: no cantrip swap");
    let (key, old) = (spell.key.clone(), spell.chosen.clone().unwrap());

    let s = commands::replace_spell(e, &s, &key).expect("replace").state;
    let snap = e.derive(&s);
    let reopened = by_key(&snap, &key);
    assert!(reopened.chosen.is_none(), "old pick is unregistered");
    assert_eq!(
        reopened.gained_level, 5,
        "slot reopens at the current class level"
    );
    assert_eq!(reopened.replaced.len(), 1);
    assert_eq!(reopened.replaced[0].pick.id, old.id);
    assert_eq!(reopened.replaced[0].pick.name, old.name);
    assert_eq!(reopened.replaced[0].level, 5);
    let max_option = reopened.options.iter().filter_map(|o| o.level).max();
    assert_eq!(
        max_option,
        Some(3),
        "level-5 sorcerer may learn 3rd-level spells"
    );

    let new = reopened
        .options
        .iter()
        .find(|o| o.id != old.id && !taken(&snap, &o.id))
        .unwrap()
        .id
        .clone();
    let s = commands::select(e, &s, &key, Choice::Element(new.clone()))
        .expect("pick again")
        .state;
    let snap = e.derive(&s);
    let filled = by_key(&snap, &key);
    assert_eq!(
        filled.chosen.as_ref().map(|c| c.id.as_str()),
        Some(new.as_str())
    );
    assert_eq!(
        filled.gained_level, 5,
        "new pick gained at the reopen level"
    );
    assert_eq!(filled.replaced.len(), 1, "history survives the new pick");
    assert!(
        commands::replace_spell(e, &s, &cantrip_key(&snap)).is_err(),
        "cantrips are refused"
    );
}

fn cantrip_key(snap: &CharacterSnapshot) -> SlotKey {
    slot(snap, "Cantrip (Sorcerer)", 1).key.clone()
}

#[test]
fn wizard_2014_spellbook_slots_are_not_replaceable() {
    let e = engine("srd-5.1");
    let s = caster(e, Ruleset::Dnd2014, "ID_WOTC_PHB_CLASS_WIZARD", 3);
    let snap = e.derive(&s);
    let book = slot(&snap, "Spellbook (Wizard)", 2);
    assert!(!book.replaceable);
    let err = commands::replace_spell(e, &s, &book.key).unwrap_err();
    assert!(err.to_string().contains("cannot be replaced"), "{err}");
    let empty = SlotKey::new("ID_NOPE", 0, 1);
    assert!(matches!(
        commands::replace_spell(e, &s, &empty),
        Err(commands::CommandError::UnknownSlot)
    ));
}

#[test]
fn wizard_2024_replaces_a_cantrip_on_level_gain() {
    let e = engine("srd-5.2.1");
    let s = caster(e, Ruleset::Dnd2024, "ID_2024_CLASS_WIZARD", 4);
    let snap = e.derive(&s);
    let cantrip = slot(&snap, "Cantrip (Wizard 1)", 1);
    assert!(
        cantrip.replaceable,
        "2024: cantrip swap on a class level gain"
    );
    let key = cantrip.key.clone();
    let s = commands::replace_spell(e, &s, &key).expect("replace").state;
    let reopened = by_key(&e.derive(&s), &key).clone();
    assert!(reopened.chosen.is_none());
    assert_eq!(reopened.gained_level, 4);
    assert_eq!(reopened.replaced.len(), 1);
}

#[test]
fn level_down_drops_history_of_vanished_slots() {
    let e = engine("srd-5.1");
    let s = caster(e, Ruleset::Dnd2014, "ID_WOTC_PHB_CLASS_SORCERER", 3);
    let key = slot(&e.derive(&s), "Spell (Sorcerer)", 3).key.clone();
    let s = commands::replace_spell(e, &s, &key).expect("replace").state;
    assert_eq!(s.slot_history.len(), 1);
    let s = commands::level_down(e, &s).expect("level down").state;
    let s = commands::prune(e, &s);
    assert!(s.slot_history.is_empty());
}

#[test]
fn only_one_spell_is_replaced_per_level_gained() {
    let e = engine("srd-5.1");
    let s = caster(e, Ruleset::Dnd2014, "ID_WOTC_PHB_CLASS_SORCERER", 5);
    let snap = e.derive(&s);
    let first = slot(&snap, "Spell (Sorcerer)", 2).key.clone();
    let second = slot(&snap, "Spell (Sorcerer)", 3).key.clone();
    assert!(by_key(&snap, &second).replaceable);
    let s = commands::replace_spell(e, &s, &first)
        .expect("first swap")
        .state;
    let snap = e.derive(&s);
    assert!(
        !by_key(&snap, &second).replaceable,
        "the level's swap is spent"
    );
    let err = commands::replace_spell(e, &s, &second).unwrap_err();
    assert!(err.to_string().contains("Only one spell"), "{err}");
    // a new level gained restores the swap
    let s = commands::level_up_main(e, &s, None)
        .expect("level up")
        .state;
    assert!(by_key(&e.derive(&s), &second).replaceable);
}

#[test]
fn wizard_2024_swaps_one_cantrip_per_level() {
    let e = engine("srd-5.2.1");
    let s = caster(e, Ruleset::Dnd2024, "ID_2024_CLASS_WIZARD", 4);
    let snap = e.derive(&s);
    let cantrip = slot(&snap, "Cantrip (Wizard 1)", 1).key.clone();
    let s = commands::replace_spell(e, &s, &cantrip)
        .expect("cantrip")
        .state;
    let snap = e.derive(&s);
    let others: Vec<_> = snap
        .slots
        .iter()
        .filter(|x| x.name.starts_with("Cantrip") && x.chosen.is_some())
        .collect();
    assert!(!others.is_empty());
    assert!(
        others.iter().all(|c| !c.replaceable),
        "one cantrip swap per level"
    );
}
