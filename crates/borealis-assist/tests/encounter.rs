//! Encounter balancer: deterministic layer and the AI layer on a fake runner.

mod common;

use std::path::Path;
use std::sync::OnceLock;

use borealis_assist::encounter::assistant::EncounterSource;
use borealis_assist::{
    encounter_assistant, evaluate, monster_index, suggest_deterministic, AssistError, Constraints,
    EncounterOptions, Method, MonsterEntry, Party,
};
use borealis_content::{ContentStore, ContentStoreBuilder, Ruleset};
use borealis_encounter::{Band, Cr, MonsterGroup, Ruleset as MathRuleset, Tables2014};
use common::FakeRunner;
use serde_json::json;

fn store() -> &'static ContentStore {
    static STORE: OnceLock<ContentStore> = OnceLock::new();
    STORE.get_or_init(|| {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
        let mut b = ContentStoreBuilder::new();
        for pack in ["core", "srd-5.1", "srd-5.1-monsters", "srd-5.2.1-monsters"] {
            b = b.add_pack_dir(&root.join(pack)).expect("pack loads");
        }
        b.build()
    })
}

/// SYNTHETIC tables (not DMG numbers): exercise the 2014 algorithm only.
fn tables() -> Tables2014 {
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

const PARTY: [u8; 4] = [5, 5, 5, 5];

fn pool2024() -> Vec<MonsterEntry> {
    monster_index(store(), Ruleset::Dnd2024)
}

fn reply(monsters: &[(&str, u32)], tactics: &str) -> String {
    let m: Vec<_> = monsters
        .iter()
        .map(|(id, n)| json!({"id": id, "count": n}))
        .collect();
    json!({"monsters": m, "tactics": tactics}).to_string()
}

fn opts<'a>(band: Band, ruleset: Ruleset) -> EncounterOptions<'a> {
    EncounterOptions {
        band,
        ruleset,
        ..EncounterOptions::default()
    }
}

#[test]
fn index_carries_cr_xp_type_and_maps_to() {
    let idx = pool2024();
    let owlbear = idx.iter().find(|m| m.name == "Owlbear").expect("owlbear");
    assert_eq!(owlbear.cr, "3".parse::<Cr>().unwrap());
    assert_eq!(owlbear.xp, 700);
    assert_eq!(owlbear.creature_type, "monstrosity");
    assert!(owlbear
        .maps_to
        .as_deref()
        .is_some_and(|m| m.starts_with("ID_SRD51_")));
    assert!(idx.windows(2).all(|w| w[0].cr <= w[1].cr));
}

#[test]
fn each_ruleset_uses_its_own_monster_pack() {
    let old = monster_index(store(), Ruleset::Dnd2014);
    let new = pool2024();
    assert!(old.len() > 100 && new.len() > 300);
    assert!(old.iter().all(|m| m.id.starts_with("ID_SRD51_")));
    assert!(new.iter().all(|m| m.id.starts_with("ID_SRD521_")));
    assert!(monster_index(store(), Ruleset::Any).is_empty());
}

#[tokio::test]
async fn assistant_draws_from_the_pack_of_its_ruleset() {
    for (ruleset, prefix) in [
        (Ruleset::Dnd2014, "ID_SRD51_"),
        (Ruleset::Dnd2024, "ID_SRD521_"),
    ] {
        // Model fails: the deterministic fallback shows which pool was used.
        let runner = FakeRunner::failing();
        let p = encounter_assistant(
            &runner,
            store(),
            &Party::Levels(PARTY.to_vec()),
            "anything",
            &opts(Band::Moderate, ruleset),
        )
        .await
        .unwrap();
        assert_eq!(p.source, EncounterSource::Deterministic);
        assert!(!p.monsters.is_empty());
        assert!(
            p.monsters.iter().all(|m| m.id.starts_with(prefix)),
            "{ruleset:?}"
        );
        assert!(p.fallback_reason.is_some());
    }
}

#[test]
fn party_of_four_level_five_hits_each_band_budget_2024() {
    let pool = pool2024();
    let mut previous = 0;
    for band in [Band::Low, Band::Moderate, Band::High] {
        let s = suggest_deterministic(
            &PARTY,
            band,
            &Constraints::default(),
            &pool,
            &Method::Srd521,
        )
        .unwrap();
        assert!(s.reached, "{band:?} not reached: {:?}", s.report);
        assert_eq!(s.report.band, band);
        let total = s.report.total_xp;
        assert!(
            total <= s.budget && total > previous,
            "{band:?}: {total} vs ({previous}, {}]",
            s.budget
        );
        // Fills the budget closely: at least 80% of it.
        assert!(
            total * 5 >= s.budget * 4,
            "{band:?}: {total} of {}",
            s.budget
        );
        assert!(s.report.monster_count <= 8);
        // The reported report is what the evaluator says about those exact groups.
        let groups: Vec<_> = s
            .monsters
            .iter()
            .map(|m| MonsterGroup {
                cr: m.cr,
                xp: m.xp_each,
                count: m.count,
            })
            .collect();
        assert_eq!(
            evaluate(&PARTY, &groups, &Method::Srd521).unwrap(),
            s.report
        );
        previous = s.budget;
    }
}

#[test]
fn constraints_limit_types_and_count() {
    let c = Constraints {
        types: vec!["undead".into()],
        max_count: Some(3),
        theme_tags: vec![],
    };
    let s =
        suggest_deterministic(&PARTY, Band::Moderate, &c, &pool2024(), &Method::Srd521).unwrap();
    assert!(s.monsters.iter().all(|m| m.creature_type == "undead"));
    assert!(s.report.monster_count <= 3);
    let none = Constraints {
        theme_tags: vec!["no-such-theme".into()],
        ..Constraints::default()
    };
    assert!(matches!(
        suggest_deterministic(&PARTY, Band::Low, &none, &pool2024(), &Method::Srd521),
        Err(AssistError::Invalid(_))
    ));
    assert!(suggest_deterministic(
        &PARTY,
        Band::Extreme,
        &Constraints::default(),
        &pool2024(),
        &Method::Srd521
    )
    .is_err());
}

#[test]
fn method_2014_needs_user_tables() {
    let err = Method::classic(None).unwrap_err();
    assert!(err.to_string().contains("unavailable"), "{err}");
    let bad = Tables2014 {
        thresholds: vec![],
        ..tables()
    };
    assert!(Method::classic(Some(&bad)).is_err());
    // The default is the SRD method for both editions.
    let g = [MonsterGroup::new(Cr::Whole(3), 2)];
    let default = evaluate(&PARTY, &g, &Method::Srd521).unwrap();
    assert_eq!(default.ruleset, MathRuleset::Edition2024);
    // With tables, 2014 works end to end, including suggestions.
    let t = tables();
    let m = Method::classic(Some(&t)).unwrap();
    assert_eq!(
        evaluate(&PARTY, &g, &m).unwrap().ruleset,
        MathRuleset::Edition2014
    );
    let pool = monster_index(store(), Ruleset::Dnd2014);
    let s =
        suggest_deterministic(&PARTY, Band::Moderate, &Constraints::default(), &pool, &m).unwrap();
    assert_eq!(s.report.ruleset, MathRuleset::Edition2014);
    assert!(s.reached, "{:?}", s.report);
}

#[test]
fn party_from_snapshots_takes_each_level() {
    let e = common::engine();
    let a = e.derive(&common::fighter(3, [15, 12, 14, 10, 10, 10]));
    let b = e.derive(&common::fighter(5, [15, 12, 14, 10, 10, 10]));
    assert_eq!(Party::Snapshots(vec![a, b]).levels().unwrap(), vec![3, 5]);
    assert!(Party::Levels(vec![]).levels().is_err());
    assert!(Party::Levels(vec![21]).levels().is_err());
}

fn good_lines(band: Band) -> Vec<(String, u32)> {
    let s = suggest_deterministic(
        &PARTY,
        band,
        &Constraints::default(),
        &pool2024(),
        &Method::Srd521,
    )
    .unwrap();
    s.monsters.iter().map(|m| (m.id.clone(), m.count)).collect()
}

#[tokio::test]
async fn hallucinated_ids_and_numbers_are_stripped() {
    let mut lines = good_lines(Band::Moderate);
    lines.push(("ID_SRD521_MONSTER_VORPAL_BUNNY".into(), 3));
    let refs: Vec<(&str, u32)> = lines.iter().map(|(i, n)| (i.as_str(), *n)).collect();
    let tactics =
        "They strike from the treeline. Each has 999 hit points and CR 77. The rest hold the ford.";
    let text = reply(&refs, tactics);
    // The id is invalid in both attempts, so the repair round runs and then the id is stripped.
    let runner = FakeRunner::new(&[&text, &text]);
    let p = encounter_assistant(
        &runner,
        store(),
        &Party::Levels(PARTY.to_vec()),
        "a river ambush",
        &opts(Band::Moderate, Ruleset::Dnd2024),
    )
    .await
    .unwrap();
    assert_eq!(runner.calls(), 2);
    assert_eq!(p.source, EncounterSource::Model, "{:?}", p.fallback_reason);
    assert!(p.removed.iter().any(|r| r.id.contains("VORPAL")));
    assert!(p.monsters.iter().all(|m| !m.id.contains("VORPAL")));
    assert_eq!(p.report.band, Band::Moderate);
    assert!(
        !p.tactics.contains("999") && !p.tactics.contains("77"),
        "{}",
        p.tactics
    );
    assert!(p.tactics.contains("treeline") && p.tactics.contains("ford"));
    assert!(p.removed_claims.iter().any(|c| c == "999"));
    // The candidate list shown to the model does not contain the made-up id.
    assert!(!runner.prompt(0).contains("VORPAL"));
}

#[tokio::test]
async fn off_band_answer_is_repaired_once_with_engine_feedback() {
    let bad = reply(&[("ID_SRD521_MONSTER_AXE_BEAK", 1)], "A lone bird.");
    let good_lines = good_lines(Band::Moderate);
    let refs: Vec<(&str, u32)> = good_lines.iter().map(|(i, n)| (i.as_str(), *n)).collect();
    let good = reply(&refs, "They fight as a pack.");
    let runner = FakeRunner::new(&[&bad, &good]);
    let p = encounter_assistant(
        &runner,
        store(),
        &Party::Levels(PARTY.to_vec()),
        "",
        &opts(Band::Moderate, Ruleset::Dnd2024),
    )
    .await
    .unwrap();
    assert_eq!(runner.calls(), 2);
    assert!(
        runner.prompt(1).contains("the engine computes"),
        "{}",
        runner.prompt(1)
    );
    assert_eq!(p.source, EncounterSource::Model);
    assert_eq!(p.attempts, 2);
    assert_eq!(p.report.band, Band::Moderate);
    assert_eq!(p.tactics, "They fight as a pack.");
}

#[tokio::test]
async fn still_off_band_after_repair_falls_back_to_deterministic() {
    let bad = reply(&[("ID_SRD521_MONSTER_AXE_BEAK", 1)], "A lone bird.");
    let runner = FakeRunner::new(&[&bad, &bad, &bad]);
    let p = encounter_assistant(
        &runner,
        store(),
        &Party::Levels(PARTY.to_vec()),
        "",
        &opts(Band::High, Ruleset::Dnd2024),
    )
    .await
    .unwrap();
    assert_eq!(runner.calls(), 2, "at most one repair round");
    assert_eq!(p.source, EncounterSource::Deterministic);
    assert!(p
        .fallback_reason
        .as_deref()
        .unwrap_or("")
        .contains("repair"));
    assert_eq!(p.report.band, Band::High);
    assert!(p
        .monsters
        .iter()
        .all(|m| m.id != "ID_SRD521_MONSTER_AXE_BEAK"));
    assert_eq!(p.tactics_source, borealis_assist::ProseSource::Template);
}

#[tokio::test]
async fn proposal_totals_come_from_the_engine() {
    let good_lines = good_lines(Band::Low);
    let refs: Vec<(&str, u32)> = good_lines.iter().map(|(i, n)| (i.as_str(), *n)).collect();
    let runner = FakeRunner::new(&[&reply(&refs, "Hit and run.")]);
    let p = encounter_assistant(
        &runner,
        store(),
        &Party::Levels(PARTY.to_vec()),
        "",
        &opts(Band::Low, Ruleset::Dnd2024),
    )
    .await
    .unwrap();
    assert_eq!(runner.calls(), 1);
    let sum: u64 = p.monsters.iter().map(|m| m.total_xp).sum();
    assert_eq!(sum, p.report.total_xp);
    assert_eq!(p.method, "SRD 5.2.1 XP budget");
}
