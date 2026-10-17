//! Decision HB5: the encounter builder rates each monster by its stored XP (the printed value for
//! homebrew), not by the table XP of its challenge rating. Official monsters are unchanged.

mod common;

use std::path::Path;
use std::sync::OnceLock;

use borealis_assist::encounter::assistant::EncounterSource;
use borealis_assist::{
    encounter_assistant, monster_index, suggest_deterministic, Constraints, EncounterOptions,
    Method, MonsterEntry, Party,
};
use borealis_content::{
    ContentFile, ContentStore, ContentStoreBuilder, MonsterData, PackOrigin, Ruleset,
};
use borealis_encounter::{Band, Cr};
use common::FakeRunner;
use serde_json::json;

/// SYNTHETIC homebrew: a CR 1/4 creature (table 50 XP) whose stat block prints 75 XP.
const HOMEBREW: &str = "ID_TEST_MONSTER_BRINDLEWICK_MOSSGNAWER";
const PRINTED_XP: u64 = 75;
const PARTY: [u8; 4] = [1, 1, 1, 1]; // 2024 budget: Low up to 200, Moderate up to 300

fn content() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content")
}

fn official() -> &'static ContentStore {
    static STORE: OnceLock<ContentStore> = OnceLock::new();
    STORE.get_or_init(|| {
        let mut b = ContentStoreBuilder::new();
        for pack in ["core", "srd-5.1", "srd-5.1-monsters", "srd-5.2.1-monsters"] {
            b = b.add_pack_dir(&content().join(pack)).expect("pack loads");
        }
        b.build()
    })
}

fn with_homebrew() -> &'static ContentStore {
    static STORE: OnceLock<ContentStore> = OnceLock::new();
    STORE.get_or_init(|| {
        let mut b = ContentStoreBuilder::new();
        for pack in ["core", "srd-5.2.1-monsters"] {
            b = b.add_pack_dir(&content().join(pack)).expect("pack loads");
        }
        let base = b.clone().build();
        let warrior = base
            .by_type("Monster")
            .find(|s| s.element.name == "Goblin Warrior")
            .expect("goblin warrior")
            .element
            .clone();
        let mut m = MonsterData::from_element(&warrior).unwrap();
        assert_eq!(m.cr.as_text(), "1/4");
        m.xp = u32::try_from(PRINTED_XP).unwrap();
        let file = ContentFile {
            elements: vec![m.to_element(HOMEBREW, "Brindlewick Mossgnawer", "Test Homebrew")],
            ..ContentFile::default()
        };
        b.add_file("homebrew.json", PackOrigin::Custom, file)
            .build()
    })
}

fn homebrew_entry() -> MonsterEntry {
    monster_index(with_homebrew(), Ruleset::Dnd2024)
        .into_iter()
        .find(|m| m.id == HOMEBREW)
        .expect("homebrew indexed")
}

#[test]
fn the_index_carries_the_printed_xp_of_a_homebrew_monster() {
    let m = homebrew_entry();
    assert_eq!(m.cr, "1/4".parse::<Cr>().unwrap());
    assert_eq!(m.xp, PRINTED_XP);
}

#[test]
fn official_monsters_keep_the_table_xp() {
    for ruleset in [Ruleset::Dnd2014, Ruleset::Dnd2024] {
        let idx = monster_index(official(), ruleset);
        assert!(idx.len() > 100);
        for m in &idx {
            assert_eq!(m.xp, m.cr.xp(), "{} ({})", m.id, m.cr);
        }
    }
}

#[test]
fn deterministic_suggestion_totals_the_printed_xp() {
    // Arrange: a pool of only the homebrew monster.
    let pool = vec![homebrew_entry()];

    // Act
    let s = suggest_deterministic(
        &PARTY,
        Band::Low,
        &Constraints::default(),
        &pool,
        &Method::Srd521,
    )
    .unwrap();

    // Assert: two at 75 = 150 XP is Low; a third (225) would be Moderate. At the table 50 XP it
    // would have been four (200).
    assert_eq!(s.monsters.len(), 1);
    let line = &s.monsters[0];
    assert_eq!((line.count, line.xp_each, line.total_xp), (2, 75, 150));
    assert_eq!(s.report.total_xp, 150);
    assert_eq!(s.report.band, Band::Low);
    assert!(s.reached);
}

#[tokio::test]
async fn the_assistant_rates_a_proposal_by_the_printed_xp() {
    // Three at the printed 75 XP = 225 (Moderate); at the table 50 XP it would be 150 (Low), off
    // band, and the runner would be asked again.
    let reply = json!({"monsters": [{"id": HOMEBREW, "count": 3}], "tactics": "They swarm."});
    let runner = FakeRunner::new(&[&reply.to_string()]);
    let opts = EncounterOptions {
        band: Band::Moderate,
        ruleset: Ruleset::Dnd2024,
        ..EncounterOptions::default()
    };

    let p = encounter_assistant(
        &runner,
        with_homebrew(),
        &Party::Levels(PARTY.to_vec()),
        "",
        &opts,
    )
    .await
    .unwrap();

    assert_eq!(runner.calls(), 1);
    assert_ne!(p.source, EncounterSource::Deterministic);
    assert_eq!(p.report.total_xp, 225);
    assert_eq!(p.report.band, Band::Moderate);
    let line = p.monsters.iter().find(|m| m.id == HOMEBREW).expect("line");
    assert_eq!((line.xp_each, line.total_xp), (75, 225));
}
