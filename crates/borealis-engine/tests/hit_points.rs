//! Play tab hit points and hit dice (B-194, decision PL1) on the bundled core + SRD 5.1 + SRD
//! 5.2.1 packs, in both editions: damage comes off temporary hit points first and stops at 0,
//! healing stops at the maximum, temporary hit points replace each other, a spent hit die heals
//! its roll plus the Constitution modifier, and a Long Rest restores hit points and hit dice by
//! the edition's rule (2014: half the total, at least one; 2024: all). A Short Rest changes no hit
//! points, and resources still recover as before.

use borealis_content::{ContentStoreBuilder, Ruleset};
use borealis_engine::commands::{self, slot_key, CommandError};
use borealis_engine::hit_points::{hit_dice, hit_points, HitPointsView};
use borealis_engine::state::CharacterState;
use borealis_engine::{Choice, Engine};
use std::path::Path;
use std::sync::OnceLock;

const FIGHTER_2014: &str = "ID_WOTC_PHB_CLASS_FIGHTER";
const FIGHTER_2024: &str = "ID_2024_CLASS_FIGHTER";
/// Constitution 14: +2.
const SCORES: [i32; 6] = [15, 12, 14, 10, 12, 8];

fn engine() -> &'static Engine {
    static ENGINE: OnceLock<Engine> = OnceLock::new();
    ENGINE.get_or_init(|| {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
        let mut b = ContentStoreBuilder::new();
        for pack in ["core", "srd-5.1", "srd-5.2.1"] {
            b = b.add_pack_dir(&root.join(pack)).expect("pack loads");
        }
        Engine::new(&b.build())
    })
}

fn fighter(ruleset: Ruleset, class: &str, level: u32) -> CharacterState {
    let e = engine();
    let s = commands::new_character(e, ruleset);
    let key = slot_key(&e.derive(&s), "ID_LEVEL_1", "Class", 1).expect("class slot");
    let mut s = commands::select(e, &s, &key, Choice::Element(class.into()))
        .expect("class")
        .state;
    s = commands::set_base_scores(e, &s, SCORES)
        .expect("scores")
        .state;
    while s.level() < level {
        s = commands::level_up_main(e, &s, None)
            .expect("level up")
            .state;
    }
    s
}

fn hp(s: &CharacterState) -> HitPointsView {
    hit_points(&s.play, &engine().derive_with(s, false))
}

/// (left, total) of the character's one class's hit dice.
fn dice(s: &CharacterState) -> (u32, u32) {
    let d = hit_dice(&s.play, &engine().derive_with(s, false));
    assert_eq!(d.len(), 1, "one class");
    (d[0].total - d[0].used, d[0].total)
}

fn class_id(s: &CharacterState) -> String {
    hit_dice(&s.play, &engine().derive_with(s, false))[0]
        .class_id
        .clone()
}

fn damage(s: &CharacterState, n: u32) -> CharacterState {
    commands::take_damage(&engine().derive_with(s, false), s, n)
        .expect("damage")
        .state
}

fn heal(s: &CharacterState, n: u32) -> CharacterState {
    commands::regain_hit_points(&engine().derive_with(s, false), s, n)
        .expect("heal")
        .state
}

fn temp(s: &CharacterState, n: u32) -> CharacterState {
    commands::set_temp_hp(s, n).expect("temp").state
}

fn spend(s: &CharacterState, roll: u32) -> Result<CharacterState, CommandError> {
    let id = class_id(s);
    commands::spend_hit_die(&engine().derive_with(s, false), s, &id, roll).map(|o| o.state)
}

fn long_rest(s: &CharacterState) -> CharacterState {
    commands::long_rest(engine(), s).expect("long rest").state
}

fn short_rest(s: &CharacterState) -> CharacterState {
    commands::short_rest(engine(), s).expect("short rest").state
}

fn both() -> [CharacterState; 2] {
    [
        fighter(Ruleset::Dnd2014, FIGHTER_2014, 4),
        fighter(Ruleset::Dnd2024, FIGHTER_2024, 4),
    ]
}

#[test]
fn a_new_character_is_at_full_hit_points_with_every_hit_die() {
    for s in both() {
        let max = u32::try_from(engine().derive(&s).hit_points).expect("max");
        assert!(max > 0);
        assert_eq!(
            hp(&s),
            HitPointsView {
                max,
                current: max,
                temp: 0
            }
        );
        assert_eq!(dice(&s), (4, 4));
        assert!(s.play.is_empty(), "nothing is stored at full hit points");
    }
}

#[test]
fn damage_goes_to_temporary_hit_points_first_and_stops_at_zero() {
    for s in both() {
        let max = hp(&s).max;
        let s = temp(&s, 5);
        let s = damage(&s, 7);
        assert_eq!(
            hp(&s),
            HitPointsView {
                max,
                current: max - 2,
                temp: 0
            }
        );
        let s = temp(&damage(&s, 1), 3);
        let s = damage(&s, 2);
        assert_eq!((hp(&s).current, hp(&s).temp), (max - 3, 1));
        let s = damage(&s, 9_999);
        assert_eq!((hp(&s).current, hp(&s).temp), (0, 0));
        let s = damage(&s, 5);
        assert_eq!(hp(&s).current, 0, "never below 0");
    }
}

#[test]
fn healing_stops_at_the_maximum_and_leaves_temporary_hit_points() {
    for s in both() {
        let max = hp(&s).max;
        let s = temp(&damage(&s, 6), 4);
        let s = heal(&s, 2);
        assert_eq!(
            hp(&s),
            HitPointsView {
                max,
                current: max - 4,
                temp: 4
            }
        );
        let s = heal(&s, 100);
        assert_eq!(
            hp(&s),
            HitPointsView {
                max,
                current: max,
                temp: 4
            }
        );
    }
}

#[test]
fn temporary_hit_points_replace_each_other_and_zero_ends_them() {
    for s in both() {
        let s = temp(&s, 10);
        let s = temp(&s, 12);
        assert_eq!(hp(&s).temp, 12, "12 or 10, not 22");
        let s = temp(&s, 4);
        assert_eq!(hp(&s).temp, 4, "the player picks the new ones");
        assert_eq!(hp(&temp(&s, 0)).temp, 0);
    }
}

#[test]
fn amounts_out_of_range_are_refused() {
    let s = fighter(Ruleset::Dnd2014, FIGHTER_2014, 1);
    let snap = engine().derive_with(&s, false);
    for bad in [0, 10_000] {
        assert!(matches!(
            commands::take_damage(&snap, &s, bad),
            Err(CommandError::HitPoints(_))
        ));
        assert!(matches!(
            commands::regain_hit_points(&snap, &s, bad),
            Err(CommandError::HitPoints(_))
        ));
    }
    assert!(commands::set_temp_hp(&s, 10_000).is_err());
}

#[test]
fn a_spent_hit_die_heals_its_roll_plus_the_constitution_modifier() {
    for s in both() {
        let max = hp(&s).max;
        let s = damage(&s, 20.min(max));
        let before = hp(&s).current;
        let s = spend(&s, 6).expect("spend");
        assert_eq!(hp(&s).current, (before + 6 + 2).min(max));
        assert_eq!(dice(&s), (3, 4));
        // a roll the d10 cannot show, and no die left
        assert!(matches!(spend(&s, 11), Err(CommandError::HitPoints(_))));
        assert!(matches!(spend(&s, 0), Err(CommandError::HitPoints(_))));
        let mut s = s;
        for _ in 0..3 {
            s = spend(&s, 1).expect("spend");
        }
        assert_eq!(dice(&s), (0, 4));
        assert!(matches!(spend(&s, 1), Err(CommandError::Level(_))));
        // healing never goes above the maximum
        assert_eq!(hp(&s).current, max.min(before + 8 + 3 * 3));
        // Restore gives a die back and heals nothing
        let id = class_id(&s);
        let back = commands::restore_hit_dice(&engine().derive_with(&s, false), &s, &id, 1)
            .expect("restore")
            .state;
        assert_eq!(dice(&back), (1, 4));
        assert_eq!(hp(&back).current, hp(&s).current);
        let unknown = commands::restore_hit_dice(&engine().derive_with(&s, false), &s, "nope", 1);
        assert!(matches!(unknown, Err(CommandError::UnknownElement(_))));
    }
}

#[test]
fn the_2024_minimum_heal_is_one_and_2014_has_none() {
    // Constitution 6: -2, so a roll of 1 totals -1
    let weak = [15, 12, 6, 10, 12, 8];
    for (ruleset, class, expect) in [
        (Ruleset::Dnd2014, FIGHTER_2014, 0),
        (Ruleset::Dnd2024, FIGHTER_2024, 1),
    ] {
        let s = fighter(ruleset, class, 2);
        let s = commands::set_base_scores(engine(), &s, weak)
            .expect("scores")
            .state;
        let s = damage(&s, 3);
        let before = hp(&s).current;
        let s = spend(&s, 1).expect("spend");
        assert_eq!(hp(&s).current - before, expect, "{ruleset:?}");
    }
}

#[test]
fn a_short_rest_keeps_hit_points_and_hit_dice() {
    for s in both() {
        let s = temp(&spend(&damage(&s, 9), 2).expect("spend"), 3);
        let rested = short_rest(&s);
        assert_eq!(rested.play.hp_lost, s.play.hp_lost);
        assert_eq!(rested.play.temp_hp, 3);
        assert_eq!(rested.play.hit_dice_used, s.play.hit_dice_used);
    }
}

#[test]
fn a_2014_long_rest_restores_hit_points_and_half_the_hit_dice() {
    let mut s = fighter(Ruleset::Dnd2014, FIGHTER_2014, 5);
    s = temp(&damage(&s, 30), 4);
    for _ in 0..5 {
        s = spend(&s, 1).expect("spend");
    }
    assert_eq!(dice(&s), (0, 5));
    let rested = long_rest(&s);
    let max = hp(&s).max;
    assert_eq!(
        hp(&rested),
        HitPointsView {
            max,
            current: max,
            temp: 0
        }
    );
    assert_eq!(dice(&rested), (2, 5), "half of five, rounded down");
    let again = long_rest(&rested);
    assert_eq!(dice(&again), (4, 5));
    assert_eq!(dice(&long_rest(&again)), (5, 5));
    // a level 1 character regains its one die
    let one = spend(&damage(&fighter(Ruleset::Dnd2014, FIGHTER_2014, 1), 3), 1).expect("spend");
    assert_eq!(dice(&one), (0, 1));
    assert_eq!(dice(&long_rest(&one)), (1, 1));
}

#[test]
fn a_2024_long_rest_restores_every_hit_point_and_hit_die() {
    let mut s = fighter(Ruleset::Dnd2024, FIGHTER_2024, 5);
    s = temp(&damage(&s, 30), 4);
    for _ in 0..5 {
        s = spend(&s, 1).expect("spend");
    }
    let rested = long_rest(&s);
    let max = hp(&s).max;
    assert_eq!(
        hp(&rested),
        HitPointsView {
            max,
            current: max,
            temp: 0
        }
    );
    assert_eq!(dice(&rested), (5, 5));
    assert!(rested.play.hit_dice_used.is_empty());
}

#[test]
fn rests_still_recover_resources_as_before() {
    for s in both() {
        let snap = engine().derive_with(&s, false);
        let pool = snap
            .resources
            .iter()
            .find(|r| r.short_rest_regain.is_some() || r.long_rest_regain_all)
            .expect("a fighter has a recharging pool")
            .clone();
        let used = commands::spend_resource(engine(), &s, &pool.id, 1)
            .expect("spend")
            .state;
        let used = damage(&used, 5);
        let after = if pool.short_rest_regain.is_some() {
            short_rest(&used)
        } else {
            long_rest(&used)
        };
        assert_eq!(
            after.play.resources_used.get(&pool.id),
            None,
            "{}",
            pool.name
        );
    }
}

#[test]
fn a_lower_level_shows_at_most_its_hit_dice_and_a_lost_maximum_shows_zero() {
    let mut s = fighter(Ruleset::Dnd2014, FIGHTER_2014, 3);
    for _ in 0..3 {
        s = spend(&s, 1).expect("spend");
    }
    let id = class_id(&s);
    let mut fewer = fighter(Ruleset::Dnd2014, FIGHTER_2014, 2);
    fewer.play.hit_dice_used.insert(id, 3);
    fewer.play.hp_lost = 999;
    assert_eq!(dice(&fewer), (0, 2));
    assert_eq!(hp(&fewer).current, 0);
    assert_eq!(hp(&heal(&fewer, 1)).current, 1, "healing starts from 0");
}
