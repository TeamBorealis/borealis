use borealis_encounter::edition2024::{budget_per_character, XP_BUDGET_PER_CHARACTER};
use borealis_encounter::*;

fn cr(s: &str) -> Cr {
    s.parse().unwrap()
}

/// SYNTHETIC fixture (not DMG numbers): exercises the algorithm only.
fn fixture() -> Tables2014 {
    Tables2014 {
        thresholds: (1..=20u64)
            .map(|n| [10 * n, 20 * n, 30 * n, 40 * n])
            .collect(),
        steps: vec![5, 10, 15, 20, 30, 40, 50],
        breakpoints: vec![1, 2, 3, 7, 11, 15],
        first_breakpoint_step: 1,
        small_party_below: 3,
        large_party_from: 6,
    }
}

#[test]
fn cr_xp_spot_checks_srd() {
    assert_eq!(cr("0").xp(), 0);
    assert_eq!(CR0_ALT_XP, 10);
    assert_eq!(cr("1/8").xp(), 25);
    assert_eq!(cr("1/4").xp(), 50);
    assert_eq!(cr("1/2").xp(), 100);
    assert_eq!(cr("1").xp(), 200);
    assert_eq!(cr("5").xp(), 1_800);
    assert_eq!(cr("13").xp(), 10_000);
    assert_eq!(cr("20").xp(), 25_000);
    assert_eq!(cr("27").xp(), 105_000);
    assert_eq!(cr("30").xp(), 155_000);
}

#[test]
fn cr_parsing() {
    assert_eq!("1/4".parse::<Cr>().unwrap(), Cr::Quarter);
    assert_eq!(" 1/2 ".parse::<Cr>().unwrap(), Cr::Half);
    assert!("1/3".parse::<Cr>().is_err());
    assert!("31".parse::<Cr>().is_err());
    assert!("".parse::<Cr>().is_err());
    assert!(cr("1/2") < cr("1") && cr("1/8") < cr("1/4"));
}

#[test]
fn cr_serde_round_trip() {
    for s in ["0", "1/8", "1/4", "1/2", "7", "30"] {
        let v = cr(s);
        let j = serde_json::to_string(&v).unwrap();
        assert_eq!(j, format!("\"{s}\""));
        assert_eq!(serde_json::from_str::<Cr>(&j).unwrap(), v);
    }
    assert_eq!(serde_json::from_str::<Cr>("3").unwrap(), Cr::Whole(3));
    assert!(serde_json::from_str::<Cr>("\"9/9\"").is_err());
}

#[test]
fn budget_table_spot_checks_srd_521() {
    assert_eq!(budget_per_character(1), Some([50, 75, 100]));
    assert_eq!(budget_per_character(3), Some([150, 225, 400]));
    assert_eq!(budget_per_character(15), Some([3_300, 5_400, 7_800]));
    assert_eq!(budget_per_character(20), Some([6_400, 13_200, 22_000]));
    assert_eq!(budget_per_character(0), None);
    assert_eq!(budget_per_character(21), None);
    assert_eq!(XP_BUDGET_PER_CHARACTER.len(), 20);
}

#[test]
fn srd_examples_2024() {
    // Example 1: 4 x L1 low = 200
    assert_eq!(
        suggest_budget(Ruleset::Edition2024, &[1; 4], Band::Low).unwrap(),
        200
    );
    // Example 2: 5 x L3 moderate = 1,125
    assert_eq!(
        suggest_budget(Ruleset::Edition2024, &[3; 5], Band::Moderate).unwrap(),
        1_125
    );
    // Example 3: 6 x L15 high = 46,800
    assert_eq!(
        suggest_budget(Ruleset::Edition2024, &[15; 6], Band::High).unwrap(),
        46_800
    );
    let r = evaluate_encounter(Ruleset::Edition2024, &[1; 4], &[(cr("1"), 1)]).unwrap();
    assert_eq!((r.total_xp, r.band, r.remaining.low), (200, Band::Low, 0));
}

#[test]
fn band_edges_2024() {
    let p = [3u8; 5]; // 750 / 1125 / 2000
    let band = |xp_cr: &[MonsterGroup]| {
        evaluate_encounter(Ruleset::Edition2024, &p, xp_cr)
            .unwrap()
            .band
    };
    assert_eq!(band(&[]), Band::Trivial);
    assert_eq!(band(&[(cr("1/2"), 7), (cr("1/4"), 1)]), Band::Low); // 750
    assert_eq!(
        band(&[(cr("1/2"), 7), (cr("1/4"), 1), (cr("0"), 1)]),
        Band::Low
    );
    assert_eq!(band(&[(cr("1/2"), 7), (cr("1/4"), 2)]), Band::Moderate); // 800
    assert_eq!(band(&[(cr("3"), 1), (cr("1"), 2)]), Band::Moderate); // 1100
    assert_eq!(
        band(&[(cr("3"), 1), (cr("1"), 2), (cr("1/8"), 1)]),
        Band::Moderate
    ); // 1125
    assert_eq!(
        band(&[(cr("3"), 1), (cr("1"), 2), (cr("1/4"), 1)]),
        Band::High
    ); // 1150
    assert_eq!(band(&[(cr("5"), 1), (cr("1"), 1)]), Band::High); // 2000
    assert_eq!(
        band(&[(cr("5"), 1), (cr("1"), 1), (cr("1/8"), 1)]),
        Band::Extreme
    );
}

#[test]
fn mixed_party_sums() {
    let r = evaluate_encounter(Ruleset::Edition2024, &[1, 2], &[]).unwrap();
    assert_eq!(
        (r.thresholds.low, r.thresholds.moderate, r.thresholds.high),
        (150, 225, 300)
    );
}

#[test]
fn input_validation() {
    let m = [(cr("1"), 1)];
    assert_eq!(
        evaluate_encounter(Ruleset::Edition2024, &[], &m),
        Err(EncounterError::EmptyParty)
    );
    assert_eq!(
        evaluate_encounter(Ruleset::Edition2024, &[0], &m),
        Err(EncounterError::InvalidLevel(0))
    );
    assert_eq!(
        evaluate_encounter(Ruleset::Edition2024, &[21], &m),
        Err(EncounterError::InvalidLevel(21))
    );
    assert_eq!(
        suggest_budget(Ruleset::Edition2024, &[1], Band::Extreme),
        Err(EncounterError::NoBudgetForBand(Band::Extreme))
    );
    assert_eq!(
        evaluate_encounter(Ruleset::Edition2014, &[1], &m),
        Err(EncounterError::Tables2014NotInSrd)
    );
}

#[test]
fn warnings() {
    let r = evaluate_encounter(Ruleset::Edition2024, &[1; 4], &[(cr("2"), 1)]).unwrap();
    assert!(r.warnings.contains(&Warning::MonsterAbovePartyLevel {
        cr: Cr::Whole(2),
        highest_level: 1
    }));
    // 1 monster vs 4 PCs is not lopsided by the 1:2 rule; 1 vs 4 -> 2 < 4 is.
    assert!(r.warnings.iter().any(|w| w.is_heuristic()));
    let r = evaluate_encounter(
        Ruleset::Edition2024,
        &[5; 2],
        &[(cr("1/4"), 5), (cr("0"), 1)],
    )
    .unwrap();
    assert!(r
        .warnings
        .iter()
        .any(|w| matches!(w, Warning::ManyCreatures { .. })));
    assert!(r.warnings.contains(&Warning::ZeroCrCreatures));
    let r = evaluate_encounter(
        Ruleset::Edition2024,
        &[5; 4],
        &[(cr("1"), 1), (cr("2"), 1), (cr("3"), 1), (cr("4"), 1)],
    )
    .unwrap();
    assert!(r.warnings.contains(&Warning::ManyStatBlocks { groups: 4 }));
    assert!(!r.warnings.iter().any(|w| w.is_heuristic()));
}

#[test]
fn multiplier_steps_2014() {
    let t = fixture();
    t.validate().unwrap();
    let m = |n, p| t.multiplier(n, p).tenths;
    // party of 4: no shift
    assert_eq!(
        [1, 2, 3, 6, 7, 10, 11, 14, 15, 99].map(|n| m(n, 4)),
        [10, 15, 20, 20, 30, 30, 40, 40, 50, 50]
    );
    // small party shifts up one step, clamped at the top
    assert_eq!([m(1, 2), m(2, 2), m(15, 2)], [15, 20, 50]);
    // large party shifts down one step
    assert_eq!([m(1, 6), m(2, 7), m(3, 6)], [5, 10, 15]);
    // boundary party sizes
    assert_eq!((m(1, 3), m(1, 5), m(1, 2), m(1, 6)), (10, 10, 15, 5));
}

#[test]
fn band_edges_2014() {
    let t = fixture(); // L1 x4: 40/80/120/160
    let r = evaluate_encounter_2014(&t, &[1; 4], &[(cr("1"), 1)]).unwrap();
    assert_eq!(
        (r.total_xp, r.multiplier_tenths, r.adjusted_xp),
        (200, Some(10), 200)
    );
    assert_eq!(r.band, Band::Extreme); // >= 160
    assert_eq!(r.thresholds.extreme, Some(160));
    assert_eq!(r.remaining.extreme, Some(-40));
    // below easy -> trivial, exactly at thresholds -> that band
    let p = [20u8]; // 200/400/600/800 solo; small party shift +1 for 1 monster => x1.5
    let r = evaluate_encounter_2014(&t, &p, &[(cr("1/2"), 1)]).unwrap();
    assert_eq!((r.adjusted_xp, r.band), (150, Band::Trivial));
    let r = evaluate_encounter_2014(&t, &p, &[(cr("1"), 1)]).unwrap();
    assert_eq!((r.adjusted_xp, r.band), (300, Band::Low));
}

#[test]
fn suggest_budget_2014_and_errors() {
    let t = fixture();
    assert_eq!(
        suggest_budget_2014(&t, &[2, 3], Band::Moderate).unwrap(),
        100
    );
    assert_eq!(
        suggest_budget_2014(&t, &[2, 3], Band::Extreme).unwrap(),
        200
    );
    let mut bad = fixture();
    bad.thresholds.pop();
    assert!(matches!(
        evaluate_encounter_2014(&bad, &[1], &[]),
        Err(EncounterError::InvalidTables(
            Tables2014Error::ThresholdRows(19)
        ))
    ));
}

#[test]
fn serde_round_trips() {
    let r = evaluate_encounter(
        Ruleset::Edition2024,
        &[3; 4],
        &[(cr("1/4"), 3), (cr("2"), 1)],
    )
    .unwrap();
    let j = serde_json::to_string(&r).unwrap();
    assert_eq!(serde_json::from_str::<EncounterReport>(&j).unwrap(), r);
    let t = fixture();
    let j = serde_json::to_string(&t).unwrap();
    assert_eq!(serde_json::from_str::<Tables2014>(&j).unwrap(), t);
    let r = evaluate_encounter_2014(&t, &[4; 4], &[(cr("1"), 2)]).unwrap();
    let j = serde_json::to_string(&r).unwrap();
    assert_eq!(serde_json::from_str::<EncounterReport>(&j).unwrap(), r);
}
