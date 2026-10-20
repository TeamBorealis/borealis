//! Decision IM12: a typed stat block may have no challenge rating (a summoned spirit, a
//! level-scaled companion). It then has no XP; a note may stand in for the CR on a sheet. A
//! stat block with a CR keeps its XP and proficiency bonus, and stored data written before the
//! change reads and writes back unchanged (R4).

use borealis_content::schema::{validate_document, DocumentKind};
use borealis_content::{
    ContentFile, ContentStoreBuilder, IssueCode, MonsterData, PackOrigin, Severity,
    NO_CHALLENGE_RATING,
};
use serde_json::{json, Value};
use std::path::Path;

const ID: &str = "ID_TEST_MONSTER_LANTERN_WISP";

/// A synthetic creature built from the bundled goblin stat block.
fn wisp() -> MonsterData {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/srd-5.1-monsters");
    let store = ContentStoreBuilder::new()
        .add_pack_dir(&dir)
        .unwrap()
        .build();
    let e = store
        .by_type("Monster")
        .find(|s| s.element.name == "Goblin")
        .expect("goblin")
        .element
        .clone();
    MonsterData::from_element(&e).unwrap()
}

fn without_cr(note: Option<&str>) -> MonsterData {
    let mut m = wisp();
    m.cr = None;
    m.xp = None;
    m.xp_lair = None;
    m.proficiency_bonus = None;
    m.cr_note = note.map(str::to_string);
    m
}

fn errors(m: &MonsterData) -> Vec<String> {
    let file = ContentFile {
        elements: vec![m.to_element(ID, "Lantern Wisp", "Test Homebrew")],
        ..ContentFile::default()
    };
    ContentStoreBuilder::new()
        .add_file("homebrew.json", PackOrigin::Custom, file)
        .build()
        .validate()
        .into_iter()
        .filter(|i| i.severity == Severity::Error && i.code == IssueCode::InvalidMonsterData)
        .map(|i| i.message)
        .collect()
}

#[test]
fn a_stat_block_without_a_challenge_rating_is_valid_and_has_no_xp() {
    let m = without_cr(Some("Scales with spell level"));
    assert!(errors(&m).is_empty(), "{:?}", errors(&m));
    assert!(m.warnings().is_empty());
    assert_eq!(m.challenge_text(), "Scales with spell level");
    assert_eq!(without_cr(None).challenge_text(), NO_CHALLENGE_RATING);
    let el = m.to_element(ID, "Lantern Wisp", "Test Homebrew");
    for key in ["cr", "xp", "xp_lair", "proficiency_bonus"] {
        assert!(!el.data.contains_key(key), "{key} is not written");
    }
    assert_eq!(el.data["cr_note"], json!("Scales with spell level"));
    assert_eq!(MonsterData::from_element(&el).unwrap(), m, "round trip");
}

fn schema_violations(m: &MonsterData) -> usize {
    let file = ContentFile {
        elements: vec![m.to_element(ID, "Lantern Wisp", "Test Homebrew")],
        ..ContentFile::default()
    };
    let doc = serde_json::to_value(&file).unwrap();
    validate_document(DocumentKind::Content, &doc)
        .unwrap()
        .len()
}

#[test]
fn the_content_schema_accepts_a_stat_block_without_cr_and_requires_xp_with_one() {
    assert_eq!(schema_violations(&without_cr(Some("Level 2"))), 0);
    assert_eq!(schema_violations(&wisp()), 0);
    let mut bare = wisp();
    bare.xp = None;
    assert!(schema_violations(&bare) > 0, "a CR needs its XP");
}

#[test]
fn xp_needs_a_challenge_rating_and_a_challenge_rating_needs_its_numbers() {
    let mut xp_alone = without_cr(None);
    xp_alone.xp = Some(50);
    assert!(errors(&xp_alone)
        .iter()
        .any(|e| e.contains("XP without a challenge rating")));
    let mut noted = wisp();
    noted.cr_note = Some("Level 3".into());
    assert!(errors(&noted).iter().any(|e| e.contains("note")));
    let mut bare = wisp();
    bare.xp = None;
    bare.proficiency_bonus = None;
    let e = errors(&bare);
    assert!(e.iter().any(|x| x.contains("missing for CR")), "{e:?}");
    assert!(e.iter().any(|x| x.contains("CR 1/4 gives 2")), "{e:?}");
}

#[test]
fn stored_stat_blocks_with_a_challenge_rating_read_and_write_back_unchanged() {
    let el = wisp().to_element(ID, "Lantern Wisp", "Test Homebrew");
    let stored: Value = serde_json::to_value(&el.data).unwrap();
    assert_eq!(
        (&stored["cr"], &stored["xp"], &stored["proficiency_bonus"]),
        (&json!("1/4"), &json!(50), &json!(2))
    );
    assert!(stored.get("cr_note").is_none());
    let again = MonsterData::from_element(&el).unwrap();
    let rewritten = again.to_element(ID, "Lantern Wisp", "Test Homebrew");
    assert_eq!(serde_json::to_value(&rewritten.data).unwrap(), stored);
    assert_eq!(again.challenge_text(), "1/4");
}
