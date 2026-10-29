//! Play tab spell tracker (GitHub #2, decision U13) on the bundled core + SRD 5.1 packs: spell
//! slot uses per level (the class table, or the multiclass table), Pact Magic uses, the spell
//! being concentrated on, and the user's own trackers. Normal slots come back on a long rest,
//! pact slots on a short or long rest; a lower maximum clamps the used count.

mod common;

use borealis_engine::commands::{self, CmdResult, CommandError, SlotPool, TrackerInput};
use borealis_engine::spell_tracker::{normalized, slot_pools, tracker_view, TrackerView};
use borealis_engine::state::{CharacterState, TrackerReset};
use borealis_engine::Engine;
use common::{class, Hero, Variant, ACOLYTE, RACE_HUMAN};

const BLESS: &str = "ID_PHB_SPELL_BLESS";
const SHIELD_OF_FAITH: &str = "ID_PHB_SPELL_SHIELD_OF_FAITH";
const CURE_WOUNDS: &str = "ID_PHB_SPELL_CURE_WOUNDS";

fn engine() -> Option<&'static Engine> {
    common::engine(Variant::Bundled)
}

fn hero(e: &Engine, class_name: &str, level: u32) -> CharacterState {
    let mut h = Hero::new(e, true);
    h.base([10, 14, 14, 15, 15, 15])
        .race_class(RACE_HUMAN, &class(class_name))
        .background(ACOLYTE)
        .level_to(level);
    h.s
}

fn view(e: &Engine, s: &CharacterState) -> TrackerView {
    tracker_view(e, s, &e.derive_with(s, false))
}

/// (level, max, used) of every spell level with slots.
fn rows(v: &TrackerView) -> Vec<(u32, u32, u32)> {
    v.spell_slots
        .iter()
        .map(|r| (r.level, r.max, r.used))
        .collect()
}

fn spend(e: &Engine, s: &CharacterState, pool: SlotPool, n: u32) -> CmdResult {
    commands::spend_spell_slots(&e.derive_with(s, false), s, pool, n)
}

fn restore(e: &Engine, s: &CharacterState, pool: SlotPool, n: u32) -> CharacterState {
    commands::restore_spell_slots(&e.derive_with(s, false), s, pool, n)
        .expect("restore")
        .state
}

fn use_slots(e: &Engine, s: &CharacterState, pool: SlotPool, n: u32) -> CharacterState {
    spend(e, s, pool, n).expect("spend slots").state
}

fn conc(e: &Engine, s: &CharacterState, spell: Option<&str>) -> CmdResult {
    commands::set_concentration(e, s, &e.derive_with(s, false), spell)
}

fn short_rest(e: &Engine, s: &CharacterState) -> CharacterState {
    commands::short_rest(e, s).expect("short rest").state
}

fn long_rest(e: &Engine, s: &CharacterState) -> CharacterState {
    commands::long_rest(e, s).expect("long rest").state
}

#[test]
fn wizard_slots_follow_the_class_table_and_come_back_on_a_long_rest() {
    let Some(e) = engine() else { return };
    let s = hero(e, "Wizard", 3);
    let v = view(e, &s);
    assert_eq!(rows(&v), vec![(1, 4, 0), (2, 2, 0)]);
    assert!(v.pact_slots.is_none());

    let s = use_slots(e, &s, SlotPool::Level(1), 3);
    let s = use_slots(e, &s, SlotPool::Level(2), 1);
    assert_eq!(rows(&view(e, &s)), vec![(1, 4, 3), (2, 2, 1)]);
    assert!(
        spend(e, &s, SlotPool::Level(1), 5).is_err(),
        "more than the maximum"
    );
    assert!(
        spend(e, &s, SlotPool::Level(3), 1).is_err(),
        "no 3rd level slots"
    );
    assert!(spend(e, &s, SlotPool::Level(0), 0).is_err());
    assert!(spend(e, &s, SlotPool::Level(10), 0).is_err());
    assert!(spend(e, &s, SlotPool::Pact, 0).is_err());

    let rested = short_rest(e, &s);
    assert_eq!(
        rows(&view(e, &rested)),
        vec![(1, 4, 3), (2, 2, 1)],
        "a short rest gives no normal slots back"
    );
    let rested = long_rest(e, &rested);
    assert_eq!(rows(&view(e, &rested)), vec![(1, 4, 0), (2, 2, 0)]);
    assert!(rested.play.spell_slots_used.is_empty());
}

#[test]
fn pact_slots_are_their_own_pool_and_come_back_on_a_short_rest() {
    let Some(e) = engine() else { return };
    let s = hero(e, "Warlock", 3);
    let pools = slot_pools(&e.derive_with(&s, false));
    assert_eq!(
        pools.levels, [0; 9],
        "pact slots never join the normal pool"
    );
    let v = view(e, &s);
    assert!(v.spell_slots.is_empty());
    let pact = v.pact_slots.expect("pact slots");
    assert_eq!((pact.level, pact.max, pact.used), (2, 2, 0));

    let s = use_slots(e, &s, SlotPool::Pact, 2);
    assert_eq!(view(e, &s).pact_slots.map(|p| p.used), Some(2));
    assert!(spend(e, &s, SlotPool::Pact, 3).is_err());
    assert!(spend(e, &s, SlotPool::Level(1), 1).is_err());
    let rested = short_rest(e, &s);
    assert_eq!(view(e, &rested).pact_slots.map(|p| p.used), Some(0));
    let s = use_slots(e, &rested, SlotPool::Pact, 1);
    let rested = long_rest(e, &s);
    assert_eq!(view(e, &rested).pact_slots.map(|p| p.used), Some(0));
    assert_eq!(rested.play.pact_slots_used, 0);
}

#[test]
fn a_multiclass_caster_uses_the_pooled_table() {
    let Some(e) = engine() else { return };
    let mut h = Hero::new(e, true);
    h.base([10, 14, 14, 15, 15, 10])
        .race_class(RACE_HUMAN, &class("Cleric"))
        .background(ACOLYTE)
        .level_to(3);
    h.s = commands::level_up_new_multiclass(h.e, &h.s, None)
        .expect("new multiclass level")
        .state;
    h.pick(
        "ID_LEVEL_4",
        "Multiclass (Level 4)",
        1,
        "ID_WOTC_PHB_MULTICLASS_WIZARD",
    );
    h.s = commands::level_up_multiclass(h.e, &h.s, 4, None)
        .expect("multiclass level")
        .state;
    let snap = e.derive_with(&h.s, false);
    let pooled = snap.multiclass_slots.as_ref().expect("pooled slots");
    let pools = slot_pools(&snap);
    let expect: Vec<u32> = pooled
        .slots
        .iter()
        .map(|n| u32::try_from(*n).unwrap())
        .collect();
    assert_eq!(pools.levels.to_vec(), expect);
    // caster level 5: 4 / 3 / 2
    assert_eq!(rows(&view(e, &h.s)), vec![(1, 4, 0), (2, 3, 0), (3, 2, 0)]);
}

#[test]
fn a_lower_maximum_clamps_the_used_count() {
    let Some(e) = engine() else { return };
    let s = hero(e, "Wizard", 3);
    let s = use_slots(e, &s, SlotPool::Level(1), 4);
    let s = use_slots(e, &s, SlotPool::Level(2), 2);
    assert!(normalized(e, &s, &e.derive_with(&s, false)).is_none());

    let down = commands::level_down(e, &s).expect("level down").state;
    let v = view(e, &down);
    assert_eq!(rows(&v), vec![(1, 3, 3)], "level 2: three 1st level slots");
    let fixed = normalized(e, &down, &e.derive_with(&down, false)).expect("clamped");
    assert_eq!(
        fixed
            .play
            .spell_slots_used
            .iter()
            .map(|(k, v)| (*k, *v))
            .collect::<Vec<_>>(),
        vec![(1, 3)]
    );
    assert!(normalized(e, &fixed, &e.derive_with(&fixed, false)).is_none());
    // back up again: the slots used before stay clamped, nothing reappears
    let up = commands::level_up_main(e, &fixed, None)
        .expect("level up")
        .state;
    assert_eq!(rows(&view(e, &up)), vec![(1, 4, 3), (2, 2, 0)]);
}

fn cleric_with(e: &Engine, prepared: &[&str]) -> CharacterState {
    let mut s = hero(e, "Cleric", 1);
    for id in prepared {
        s = commands::set_prepared(e, &s, "Cleric", id, true)
            .expect("prepare")
            .state;
    }
    s
}

#[test]
fn concentration_takes_a_ready_concentration_spell_and_ends_on_a_long_rest() {
    let Some(e) = engine() else { return };
    let s = cleric_with(e, &[BLESS, CURE_WOUNDS, SHIELD_OF_FAITH]);
    let v = view(e, &s);
    let ids: Vec<&str> = v
        .concentration_spells
        .iter()
        .map(|c| c.id.as_str())
        .collect();
    assert!(
        ids.contains(&BLESS) && ids.contains(&SHIELD_OF_FAITH),
        "{ids:?}"
    );
    assert!(!ids.contains(&CURE_WOUNDS), "not a concentration spell");
    assert!(v.concentration.is_none());

    let s = conc(e, &s, Some(BLESS)).expect("concentrate").state;
    let c = view(e, &s).concentration.expect("concentrating");
    assert_eq!(
        (c.id.as_str(), c.name.as_str(), c.level),
        (BLESS, "Bless", 1)
    );
    assert!(matches!(
        conc(e, &s, Some(CURE_WOUNDS)),
        Err(CommandError::Spell(_))
    ));
    assert!(conc(e, &s, Some("ID_NO_SUCH_SPELL")).is_err());

    // a new spell replaces the old one; a short rest keeps it
    let s = conc(e, &s, Some(SHIELD_OF_FAITH)).expect("replace").state;
    let s = short_rest(e, &s);
    assert_eq!(
        view(e, &s).concentration.map(|c| c.id),
        Some(SHIELD_OF_FAITH.to_string())
    );
    let rested = long_rest(e, &s);
    assert!(view(e, &rested).concentration.is_none());
    assert_eq!(rested.play.concentration, None);

    // one tap ends it
    let s = conc(e, &s, None).expect("end").state;
    assert_eq!(s.play.concentration, None);
}

#[test]
fn concentration_on_a_spell_no_longer_ready_is_dropped() {
    let Some(e) = engine() else { return };
    let s = cleric_with(e, &[BLESS]);
    let s = conc(e, &s, Some(BLESS)).expect("concentrate").state;
    let s = commands::set_prepared(e, &s, "Cleric", BLESS, false)
        .expect("unprepare")
        .state;
    assert!(view(e, &s).concentration.is_none());
    let fixed = normalized(e, &s, &e.derive_with(&s, false)).expect("dropped");
    assert_eq!(fixed.play.concentration, None);
}

fn input(name: &str, max: u32, reset: TrackerReset) -> TrackerInput {
    TrackerInput {
        name: name.into(),
        max,
        reset,
        counts_up: false,
    }
}

fn tracker_used(s: &CharacterState) -> Vec<(String, u32)> {
    s.play
        .trackers
        .iter()
        .map(|t| (t.name.clone(), t.used))
        .collect()
}

#[test]
fn custom_trackers_are_added_edited_tracked_and_reset_by_their_own_rule() {
    let Some(e) = engine() else { return };
    let s = hero(e, "Fighter", 1);
    let s = commands::add_tracker(&s, &input("  Infection ", 6, TrackerReset::Never))
        .expect("add")
        .state;
    let s = commands::add_tracker(&s, &input("Splicer Points", 5, TrackerReset::LongRest))
        .expect("add")
        .state;
    let s = commands::add_tracker(&s, &input("Lucky Coin", 2, TrackerReset::ShortRest))
        .expect("add")
        .state;
    let ids: Vec<String> = s.play.trackers.iter().map(|t| t.id.clone()).collect();
    assert_eq!(ids.len(), 3);
    assert!(ids[0] != ids[1] && ids[1] != ids[2] && ids[0] != ids[2]);
    assert_eq!(s.play.trackers[0].name, "Infection", "the name is trimmed");

    let mut s = s;
    for (id, used) in ids.iter().zip([4, 3, 2]) {
        s = commands::spend_tracker(&s, id, used).expect("track").state;
    }
    assert!(
        commands::spend_tracker(&s, &ids[2], 1).is_err(),
        "above the maximum"
    );
    assert!(matches!(
        commands::spend_tracker(&s, "nope", 1),
        Err(CommandError::UnknownElement(_))
    ));

    let short = short_rest(e, &s);
    assert_eq!(
        tracker_used(&short),
        vec![
            ("Infection".into(), 4),
            ("Splicer Points".into(), 3),
            ("Lucky Coin".into(), 0)
        ]
    );
    let long = long_rest(e, &s);
    assert_eq!(
        tracker_used(&long),
        vec![
            ("Infection".into(), 4),
            ("Splicer Points".into(), 0),
            ("Lucky Coin".into(), 0)
        ]
    );

    // a lower maximum clamps the used count; the edit keeps the id
    let edited =
        commands::update_tracker(&s, &ids[0], &input("Contamination", 3, TrackerReset::Never))
            .expect("edit")
            .state;
    let t = &edited.play.trackers[0];
    assert_eq!(
        (t.id.as_str(), t.name.as_str(), t.max, t.used),
        (ids[0].as_str(), "Contamination", 3, 3)
    );
    assert_eq!(view(e, &edited).trackers[0].used, 3);

    let removed = commands::remove_tracker(&edited, &ids[1])
        .expect("delete")
        .state;
    assert_eq!(
        removed
            .play
            .trackers
            .iter()
            .map(|t| t.name.as_str())
            .collect::<Vec<_>>(),
        vec!["Contamination", "Lucky Coin"]
    );
    assert!(commands::remove_tracker(&removed, &ids[1]).is_err());
    // a new tracker never takes the id of one still present
    let again = commands::add_tracker(&removed, &input("New", 1, TrackerReset::Never))
        .expect("add")
        .state;
    let new_id = &again.play.trackers[2].id;
    assert!(!removed.play.trackers.iter().any(|t| &t.id == new_id));
}

#[test]
fn tracker_names_and_maximums_are_validated() {
    let Some(e) = engine() else { return };
    let s = hero(e, "Fighter", 1);
    let long_name = "x".repeat(commands::MAX_TRACKER_NAME + 1);
    for bad in [
        input("", 3, TrackerReset::Never),
        input("   ", 3, TrackerReset::Never),
        input(&long_name, 3, TrackerReset::Never),
        input("Bad\u{0007}name", 3, TrackerReset::Never),
        input("Zero", 0, TrackerReset::Never),
        input("Huge", commands::MAX_TRACKER_MAX + 1, TrackerReset::Never),
    ] {
        assert!(
            matches!(
                commands::add_tracker(&s, &bad),
                Err(CommandError::Tracker(_))
            ),
            "{:?} / {}",
            bad.name,
            bad.max
        );
    }
    let ok = commands::add_tracker(
        &s,
        &input("Max", commands::MAX_TRACKER_MAX, TrackerReset::Never),
    );
    assert!(ok.is_ok());
    let mut full = s.clone();
    for i in 0..commands::MAX_TRACKERS {
        full = commands::add_tracker(&full, &input(&format!("T{i}"), 1, TrackerReset::Never))
            .expect("add")
            .state;
    }
    assert!(matches!(
        commands::add_tracker(&full, &input("One more", 1, TrackerReset::Never)),
        Err(CommandError::Tracker(_))
    ));
    let id = full.play.trackers[0].id.clone();
    assert!(commands::update_tracker(&full, &id, &input("", 1, TrackerReset::Never)).is_err());
}

#[test]
fn a_character_without_spellcasting_has_no_slot_card() {
    let Some(e) = engine() else { return };
    let s = hero(e, "Fighter", 3);
    let v = view(e, &s);
    assert!(v.spell_slots.is_empty() && v.pact_slots.is_none());
    assert!(v.concentration_spells.is_empty());
    assert!(spend(e, &s, SlotPool::Level(1), 0).is_err());
    assert!(conc(e, &s, Some(BLESS)).is_err());
    // rests leave an untracked character as it was
    assert_eq!(long_rest(e, &s).play, s.play);
    assert!(s.play.is_empty());
}

#[test]
fn presses_are_relative_so_quick_presses_add_up_and_a_press_after_a_rest_spends_one() {
    let Some(e) = engine() else { return };
    let s = hero(e, "Wizard", 3);
    // two presses read from the same (stale) screen both count
    let s = use_slots(e, &s, SlotPool::Level(1), 1);
    let s = use_slots(e, &s, SlotPool::Level(1), 1);
    assert_eq!(rows(&view(e, &s))[0], (1, 4, 2));
    let s = use_slots(e, &s, SlotPool::Level(1), 2);
    // a press that reaches the host after a long rest spends one slot, not "4 of 4"
    let rested = long_rest(e, &s);
    let after = use_slots(e, &rested, SlotPool::Level(1), 1);
    assert_eq!(rows(&view(e, &after))[0], (1, 4, 1));
    // a restore press after the rest stays at none used, without an error
    let restored = restore(e, &rested, SlotPool::Level(1), 1);
    assert_eq!(rows(&view(e, &restored))[0], (1, 4, 0));
    assert!(
        commands::restore_spell_slots(&e.derive_with(&s, false), &s, SlotPool::Level(5), 1)
            .is_err()
    );

    let t = commands::add_tracker(&s, &input("Coin", 2, TrackerReset::LongRest))
        .expect("add")
        .state;
    let id = t.play.trackers[0].id.clone();
    let t = commands::spend_tracker(&t, &id, 1).expect("spend").state;
    let t = commands::spend_tracker(&t, &id, 1).expect("spend").state;
    assert!(commands::spend_tracker(&t, &id, 1).is_err(), "none left");
    let t = commands::restore_tracker(&long_rest(e, &t), &id, 1)
        .expect("restore")
        .state;
    assert_eq!(t.play.trackers[0].used, 0, "never below none used");
}

#[test]
fn tracker_ids_stay_unique_and_names_lose_hidden_characters() {
    let Some(e) = engine() else { return };
    let mut s = hero(e, "Fighter", 1);
    s = commands::add_tracker(&s, &input("A", 1, TrackerReset::Never))
        .expect("add")
        .state;
    // a hand-edited file holding the largest id: the next id must not repeat it
    s.play.trackers[0].id = format!("tracker-{}", u32::MAX);
    let s = commands::add_tracker(&s, &input("B", 1, TrackerReset::Never))
        .expect("add")
        .state;
    assert_ne!(s.play.trackers[0].id, s.play.trackers[1].id);

    let named = commands::add_tracker(
        &s,
        &input("\u{202E}In\u{200B}fection\u{2066} ", 6, TrackerReset::Never),
    )
    .expect("add")
    .state;
    assert_eq!(named.play.trackers[2].name, "Infection");
    assert!(matches!(
        commands::add_tracker(&s, &input("\u{202E}\u{200B}", 1, TrackerReset::Never)),
        Err(CommandError::Tracker(_))
    ));
}

#[test]
fn errors_echo_at_most_a_short_id() {
    let Some(e) = engine() else { return };
    let s = hero(e, "Cleric", 1);
    let long = "x".repeat(10_000);
    let err = commands::remove_tracker(&s, &long).unwrap_err().to_string();
    assert!(err.chars().count() < 120, "{}", err.len());
    let err = conc(e, &s, Some(&long)).unwrap_err().to_string();
    assert!(err.chars().count() < 160, "{}", err.len());
}

#[test]
fn counting_up_is_saved_and_editable() {
    let Some(e) = engine() else { return };
    let s = hero(e, "Fighter", 1);
    let mut up = input("Infection", 6, TrackerReset::Never);
    up.counts_up = true;
    let s = commands::add_tracker(&s, &up).expect("add").state;
    assert!(s.play.trackers[0].counts_up);
    let id = s.play.trackers[0].id.clone();
    let s = commands::update_tracker(&s, &id, &input("Infection", 6, TrackerReset::Never))
        .expect("edit")
        .state;
    assert!(!s.play.trackers[0].counts_up);
}

#[test]
fn a_tiefling_can_concentrate_on_its_racial_darkness() {
    let Some(e) = engine() else { return };
    let mut h = Hero::new(e, true);
    h.base([15, 14, 14, 10, 10, 15]);
    let tiefling = "ID_RACE_TIEFLING";
    if !e.has_element(borealis_content::Ruleset::Dnd2014, tiefling) {
        return;
    }
    h.race_class(tiefling, &class("Fighter"))
        .background(ACOLYTE)
        .level_to(5);
    let names: Vec<String> = view(e, &h.s)
        .concentration_spells
        .into_iter()
        .map(|c| c.name)
        .collect();
    assert!(names.contains(&"Darkness".to_string()), "{names:?}");
}
