//! Decision HB5: encounter XP is each monster's stored XP (the printed value for homebrew), not the
//! table XP of its challenge rating. The table is the fallback only when no XP is stored.

use borealis_encounter::*;

fn cr(s: &str) -> Cr {
    s.parse().unwrap()
}

#[test]
fn a_printed_xp_off_the_table_counts_as_printed() {
    // Arrange: CR 1/4 (table 50 XP) printed with 75 XP, two of them.
    let group = MonsterGroup::with_stored_xp(cr("1/4"), Some(75), 2);

    // Act
    let r = evaluate_encounter(Ruleset::Edition2024, &[1; 4], &[group]).unwrap();

    // Assert
    assert_eq!(group.xp, 75);
    assert_eq!(r.total_xp, 150);
}

#[test]
fn the_2014_method_also_counts_the_printed_xp() {
    let t = Tables2014 {
        thresholds: (1..=20u64)
            .map(|n| [10 * n, 20 * n, 30 * n, 40 * n])
            .collect(),
        steps: vec![5, 10, 15, 20, 30, 40, 50],
        breakpoints: vec![1, 2, 3, 7, 11, 15],
        first_breakpoint_step: 1,
        small_party_below: 3,
        large_party_from: 6,
    };
    let r = evaluate_encounter_2014(
        &t,
        &[4; 4],
        &[MonsterGroup::with_stored_xp(cr("1"), Some(450), 1)],
    )
    .unwrap();
    assert_eq!((r.total_xp, r.adjusted_xp), (450, 450));
}

#[test]
fn no_stored_xp_falls_back_to_the_table() {
    assert_eq!(MonsterGroup::with_stored_xp(cr("5"), None, 1).xp, 1_800);
    assert_eq!(
        MonsterGroup::new(cr("5"), 3),
        MonsterGroup::with_stored_xp(cr("5"), None, 3)
    );
    let r = evaluate_encounter(
        Ruleset::Edition2024,
        &[1; 4],
        &[MonsterGroup::new(cr("1"), 2)],
    )
    .unwrap();
    assert_eq!(r.total_xp, 400);
}

#[test]
fn a_standard_stored_xp_matches_the_table() {
    // Official monsters store the table value; CR 0 prints "0 or 10" and stays 0 in encounter math.
    for (text, stored) in [("1/8", 25), ("1/2", 100), ("3", 700), ("30", 155_000)] {
        assert_eq!(
            MonsterGroup::with_stored_xp(cr(text), Some(stored), 1).xp,
            cr(text).xp()
        );
    }
    assert_eq!(
        MonsterGroup::with_stored_xp(cr("0"), Some(CR0_ALT_XP), 1).xp,
        0
    );
    assert_eq!(MonsterGroup::with_stored_xp(cr("0"), Some(0), 1).xp, 0);
}

#[test]
fn a_cr_0_monster_printed_off_the_table_counts_as_printed() {
    assert_eq!(MonsterGroup::with_stored_xp(cr("0"), Some(25), 1).xp, 25);
}

#[test]
fn warnings_still_read_the_challenge_rating() {
    let r = evaluate_encounter(
        Ruleset::Edition2024,
        &[1; 4],
        &[MonsterGroup::with_stored_xp(cr("2"), Some(10), 1)],
    )
    .unwrap();
    assert_eq!(r.total_xp, 10);
    assert!(r.warnings.contains(&Warning::MonsterAbovePartyLevel {
        cr: Cr::Whole(2),
        highest_level: 1
    }));
}
