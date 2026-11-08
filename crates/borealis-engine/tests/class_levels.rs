//! B2a (#34): dividing a character's levels between classes without levelling up one at a time.
//! A split must give the same character as levelling up normally: hit points, the multiclass
//! proficiencies (never the starting-class ones) and spell slots included.

mod common;
use borealis_engine::commands::{self, CommandError};
use borealis_engine::snapshot::CharacterSnapshot;
use borealis_engine::state::{CharacterState, ClassSlot, HpRolls};
use common::*;

const HEAVY_ARMOR: &str = "ID_PROFICIENCY_ARMOR_PROFICIENCY_HEAVY_ARMOR";
const MEDIUM_ARMOR: &str = "ID_PROFICIENCY_ARMOR_PROFICIENCY_MEDIUM_ARMOR";
const STR_SAVE: &str = "ID_PROFICIENCY_SAVINGTHROW_STRENGTH";
const INT_SAVE: &str = "ID_PROFICIENCY_SAVINGTHROW_INTELLIGENCE";

fn mc(name: &str) -> String {
    format!("ID_WOTC_PHB_MULTICLASS_{}", name.to_uppercase())
}

fn saves(s: &CharacterSnapshot) -> Vec<i32> {
    s.abilities.iter().map(|a| a.save).collect()
}

fn class_levels(s: &CharacterSnapshot) -> Vec<(String, u32)> {
    s.classes
        .iter()
        .map(|c| {
            (
                c.class
                    .as_ref()
                    .map_or_else(String::new, |r| r.name.clone()),
                c.level,
            )
        })
        .collect()
}

/// A human with the picks of golden vector TV-07 (STR 16, INT 13), levelled to `level` in `main`.
fn hero<'e>(e: &'e borealis_engine::Engine, main: &str, level: u32) -> Hero<'e> {
    let mut h = Hero::new(e, true);
    h.base([15, 12, 14, 12, 10, 8])
        .race_class(RACE_HUMAN, &class(main))
        .background(ACOLYTE);
    if main == "Fighter" {
        h.pick_any(
            "Class Feature",
            "ID_WOTC_PHB_CLASS_FEATURE_FIGHTINGSTYLE_DEFENSE",
        );
        h.pick_any("Proficiency", &skill("Athletics"));
        h.pick_any("Proficiency", &skill("Perception"));
    } else {
        h.pick_any("Proficiency", &skill("Arcana"));
        h.pick_any("Proficiency", &skill("History"));
    }
    h.level_to(level);
    h
}

/// The usual path: `main` to `main_levels`, NEW MULTICLASS, pick `multi`, level it up.
fn levelled_normally(
    e: &borealis_engine::Engine,
    main: &str,
    main_levels: u32,
    multi: &str,
    multi_levels: u32,
) -> CharacterState {
    let mut h = hero(e, main, main_levels);
    h.s = commands::level_up_new_multiclass(e, &h.s, None)
        .expect("new multiclass")
        .state;
    let start = h.s.level();
    h.pick(
        &format!("ID_LEVEL_{start}"),
        &format!("Multiclass (Level {start})"),
        1,
        &mc(multi),
    );
    for _ in 1..multi_levels {
        h.s = commands::level_up_multiclass(e, &h.s, start, None)
            .expect("multiclass level")
            .state;
    }
    h.s
}

fn split(
    e: &borealis_engine::Engine,
    s: &CharacterState,
    levels: u32,
    multi: &str,
) -> CharacterState {
    commands::split_new_multiclass(e, s, levels, Some(&mc(multi)))
        .unwrap_or_else(|err| panic!("split {levels} into {multi}: {err}"))
        .state
}

fn sorted(mut s: CharacterState) -> CharacterState {
    s.selections.sort_by(|a, b| a.slot.cmp(&b.slot));
    s
}

#[test]
fn fighter_9_split_into_fighter_5_wizard_4_matches_levelling_up() {
    each_variant("b2a-split", |e, v| {
        let nine = hero(e, "Fighter", 9).s;
        let split = split(e, &nine, 4, "Wizard");
        let normal = levelled_normally(e, "Fighter", 5, "Wizard", 4);
        assert_eq!(split.level(), 9, "{v:?}: the total level stays 9");
        assert_eq!(
            split.levels,
            [
                vec![ClassSlot::Main; 5],
                vec![ClassSlot::Multi { starting_level: 6 }; 4]
            ]
            .concat(),
            "{v:?}: the level order a level up builds"
        );
        assert_eq!(sorted(split.clone()), sorted(normal.clone()), "{v:?}");

        let (a, b) = (e.derive(&split), e.derive(&normal));
        assert_eq!(a, b, "{v:?}: the same character as levelling up normally");
        assert_eq!(
            class_levels(&a),
            [("Fighter".into(), 5), ("Wizard".into(), 4)]
        );
        assert_eq!(a.experience, 48_000, "experience is untouched");
        // d10 max + 4 x 6 (Fighter) + 4 x 4 (Wizard), +2 Constitution a level
        assert_eq!(a.hit_points, 10 + 24 + 16 + 9 * 2, "{v:?}");
        assert_eq!(saves(&a), [7, 1, 6, 1, 0, -1], "{v:?}: Fighter saves only");
        assert!(
            a.has_element(HEAVY_ARMOR),
            "the starting class keeps heavy armor"
        );
        assert!(
            !a.has_element(INT_SAVE),
            "a multiclass gives no saving throws"
        );
        let w = a.caster("Wizard").expect("Wizard spellcasting");
        assert_eq!((w.slots[0], w.slots[1], w.slots[2]), (4, 3, 0), "{v:?}");
        assert_eq!(
            a.stat("extra attack:count"),
            2,
            "Fighter 5 keeps Extra Attack"
        );
    });
}

#[test]
fn a_split_multiclass_gets_multiclass_proficiencies_not_starting_ones() {
    each_variant("b2a-proficiencies", |e, v| {
        let nine = hero(e, "Wizard", 9).s;
        let split = split(e, &nine, 4, "Fighter");
        let normal = levelled_normally(e, "Wizard", 5, "Fighter", 4);
        let (a, b) = (e.derive(&split), e.derive(&normal));
        assert_eq!(a, b, "{v:?}");
        assert_eq!(
            class_levels(&a),
            [("Wizard".into(), 5), ("Fighter".into(), 4)]
        );
        assert!(
            a.has_element(MEDIUM_ARMOR),
            "{v:?}: the Fighter multiclass armor"
        );
        assert!(
            !a.has_element(HEAVY_ARMOR),
            "{v:?}: heavy armor is a starting proficiency"
        );
        assert!(!a.has_element(STR_SAVE), "{v:?}: so are the Fighter saves");
        let w = a.caster("Wizard").expect("Wizard spellcasting");
        assert_eq!(
            (w.slots[0], w.slots[1], w.slots[2]),
            (4, 3, 2),
            "{v:?}: Wizard 5"
        );
    });
}

#[test]
fn removing_a_multiclass_gives_its_levels_back_to_the_main_class() {
    each_variant("b2a-remove", |e, v| {
        let nine = hero(e, "Fighter", 9).s;
        let split = split(e, &nine, 4, "Wizard");
        let back = commands::set_multiclass_levels(e, &split, 6, 0)
            .expect("remove the multiclass")
            .state;
        assert_eq!(back.levels, vec![ClassSlot::Main; 9], "{v:?}");
        assert!(
            back.selections.iter().all(|s| s.slot.owner != "ID_LEVEL_6"),
            "{v:?}: the Wizard pick is gone"
        );
        assert_eq!(e.derive(&back), e.derive(&nine), "{v:?}: Fighter 9 again");
    });
}

#[test]
fn moving_levels_between_classes_keeps_the_total_and_renumbers_later_multiclasses() {
    each_variant("b2a-renumber", |e, v| {
        // Fighter 5 / Wizard 2, then 2 Fighter levels to a Rogue: Fighter 3 / Wizard 2 / Rogue 2,
        // the Wizard starting at level 4 and the Rogue at 6
        let mut s = levelled_normally(e, "Fighter", 5, "Wizard", 2);
        s = split(e, &s, 2, "Rogue");
        assert_eq!(
            class_levels(&e.derive(&s)),
            [
                ("Fighter".into(), 3),
                ("Wizard".into(), 2),
                ("Rogue".into(), 2)
            ]
        );
        s = commands::set_hp_roll(&s, &ClassSlot::Multi { starting_level: 6 }, 2, Some(3))
            .expect("roll")
            .state;

        let more = commands::set_multiclass_levels(e, &s, 4, 3)
            .expect("Wizard 3")
            .state;
        let snap = e.derive(&more);
        assert_eq!(more.level(), 7, "{v:?}");
        assert_eq!(
            class_levels(&snap),
            [
                ("Fighter".into(), 2),
                ("Wizard".into(), 3),
                ("Rogue".into(), 2)
            ],
            "{v:?}: the level comes from the main class, the Rogue keeps its pick"
        );
        assert_eq!(
            more.levels,
            [
                vec![ClassSlot::Main; 2],
                vec![ClassSlot::Multi { starting_level: 3 }; 2],
                vec![ClassSlot::Multi { starting_level: 5 }; 2],
                vec![ClassSlot::Multi { starting_level: 3 }],
            ]
            .concat()
        );
        assert_eq!(
            more.hp_rolls,
            [HpRolls {
                class: ClassSlot::Multi { starting_level: 5 },
                rolls: vec![0, 3]
            }],
            "{v:?}: the Rogue's roll moves with it"
        );

        let fewer = commands::set_multiclass_levels(e, &more, 3, 0)
            .expect("drop Wizard")
            .state;
        assert_eq!(
            class_levels(&e.derive(&fewer)),
            [("Fighter".into(), 5), ("Rogue".into(), 2)],
            "{v:?}"
        );
        assert_eq!(
            fewer.levels,
            [
                vec![ClassSlot::Main; 2],
                vec![ClassSlot::Multi { starting_level: 3 }; 2],
                vec![ClassSlot::Main; 3],
            ]
            .concat(),
            "{v:?}: the Rogue now starts at level 3"
        );
    });
}

#[test]
fn split_hit_point_rolls_follow_their_class() {
    each_variant("b2a-rolls", |e, v| {
        let mut h = hero(e, "Fighter", 1);
        h.s.options
            .retain(|o| o != "ID_INTERNAL_OPTION_ALLOW_AVERAGE_HP");
        for roll in [7, 8, 9, 10, 2, 3, 4, 5] {
            h.s = commands::level_up_main(e, &h.s, Some(roll))
                .expect("level")
                .state;
        }
        let split = split(e, &h.s, 4, "Wizard");
        assert_eq!(
            split.hp_rolls,
            [HpRolls {
                class: ClassSlot::Main,
                rolls: vec![0, 7, 8, 9, 10]
            }],
            "{v:?}: Fighter keeps its first five rolls, the Wizard levels have none"
        );
        let wizard = ClassSlot::Multi { starting_level: 6 };
        let rolled = commands::set_hp_roll(&split, &wizard, 2, Some(1))
            .expect("roll")
            .state;
        let moved = commands::set_multiclass_levels(e, &rolled, 6, 1)
            .expect("Wizard 1")
            .state;
        assert_eq!(
            moved.hp_rolls,
            [HpRolls { class: ClassSlot::Main, rolls: vec![0, 7, 8, 9, 10] }],
            "{v:?}: the dropped Wizard level takes its roll with it; Fighter's new levels are unrolled"
        );
    });
}

#[test]
fn choices_at_moved_levels_are_dropped() {
    each_variant("b2a-drop", |e, v| {
        let mut h = hero(e, "Fighter", 3);
        h.pick_any("Archetype", "ID_WOTC_PHB_ARCHETYPE_CHAMPION");
        h.level_to(9);
        let split = split(e, &h.s, 7, "Wizard");
        let snap = e.derive(&split);
        assert_eq!(
            class_levels(&snap),
            [("Fighter".into(), 2), ("Wizard".into(), 7)],
            "{v:?}"
        );
        assert!(
            snap.classes[0].archetypes.is_empty(),
            "{v:?}: Fighter 3's archetype is dropped"
        );
        assert!(
            !split.selections.iter().any(|s| s.choice
                == borealis_engine::state::Choice::Element(
                    "ID_WOTC_PHB_ARCHETYPE_CHAMPION".into()
                )),
            "{v:?}"
        );
        assert!(
            snap.invalid_selections.is_empty(),
            "{v:?}: nothing stale is left"
        );
    });
}

#[test]
fn a_split_follows_the_multiclass_rules() {
    each_variant("b2a-rules", |e, v| {
        let nine = hero(e, "Fighter", 9).s;
        let main_only = |r: Result<_, CommandError>| matches!(r, Err(CommandError::Level(_)));
        assert!(
            main_only(commands::split_new_multiclass(e, &nine, 9, None)),
            "{v:?}: Fighter keeps a level"
        );
        assert!(
            main_only(commands::split_new_multiclass(e, &nine, 0, None)),
            "{v:?}"
        );
        assert!(
            main_only(commands::set_multiclass_levels(e, &nine, 6, 2)),
            "{v:?}: no such multiclass"
        );

        // CHA 8: the Bard prerequisite (Charisma 13) is not met
        let bard = commands::split_new_multiclass(e, &nine, 2, Some(&mc("Bard")));
        assert!(
            matches!(bard, Err(CommandError::NotAllowed(_))),
            "{v:?}: {bard:?}"
        );
        let mut ignored = nine.clone();
        ignored.house_rules.ignore_multiclass_prerequisites = true;
        let bard = split(e, &ignored, 2, "Bard");
        assert_eq!(
            class_levels(&e.derive(&bard)),
            [("Fighter".into(), 7), ("Bard".into(), 2)],
            "{v:?}: HR3"
        );

        let empty = commands::split_new_multiclass(e, &nine, 3, None)
            .expect("empty slot")
            .state;
        let snap = e.derive(&empty);
        assert_eq!(
            class_levels(&snap),
            [("Fighter".into(), 6), (String::new(), 3)],
            "{v:?}"
        );
        assert!(
            snap.slot("ID_LEVEL_7", "Multiclass (Level 7)").is_some(),
            "{v:?}: picked on Build > Class"
        );

        let mut off = nine.clone();
        off.options
            .retain(|o| o != "ID_INTERNAL_OPTION_ALLOW_MULTICLASSING");
        assert!(
            main_only(commands::split_new_multiclass(e, &off, 2, None)),
            "{v:?}: multiclassing off"
        );
    });
}
