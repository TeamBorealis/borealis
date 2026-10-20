//! Decision IM12a: a typed stat block's armor class and hit points may be printed as a formula
//! (a summon whose numbers follow the spell). The number is then absent and a note holds the
//! text, shown where the number would be. Each is a number or a note, never both; stored data
//! written before the change reads and writes back byte for byte (R4). SYNTHETIC data only.

use borealis_content::schema::{validate_document, DocumentKind};
use borealis_content::{
    ContentFile, ContentStoreBuilder, IssueCode, MonsterData, PackOrigin, Severity,
    NO_CHALLENGE_RATING,
};
use serde_json::{json, Value};
use std::path::Path;

const ID: &str = "ID_TEST_MONSTER_CINDER_WARDEN";
const AC_TEXT: &str = "9 + the warden's ember count";
const HP_TEXT: &str = "12 + 6 for each ember";

fn content() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content")
}

/// A synthetic creature built from the bundled goblin stat block.
fn goblin() -> MonsterData {
    let store = ContentStoreBuilder::new()
        .add_pack_dir(&content().join("srd-5.1-monsters"))
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

/// A summon without a challenge rating whose armor class and hit points are formulas.
fn warden() -> MonsterData {
    let mut m = goblin();
    m.cr = None;
    m.xp = None;
    m.proficiency_bonus = None;
    m.cr_note = Some("Scales with spell level".into());
    m.ac.clear();
    m.ac_note = Some(AC_TEXT.into());
    m.hp = None;
    m.hp_note = Some(HP_TEXT.into());
    m
}

fn file_of(m: &MonsterData) -> ContentFile {
    ContentFile {
        elements: vec![m.to_element(ID, "Cinder Warden", "Test Homebrew")],
        ..ContentFile::default()
    }
}

fn errors(m: &MonsterData) -> Vec<String> {
    ContentStoreBuilder::new()
        .add_file("homebrew.json", PackOrigin::Custom, file_of(m))
        .build()
        .validate()
        .into_iter()
        .filter(|i| i.severity == Severity::Error && i.code == IssueCode::InvalidMonsterData)
        .map(|i| i.message)
        .collect()
}

fn schema_violations(m: &MonsterData) -> usize {
    let doc = serde_json::to_value(file_of(m)).unwrap();
    validate_document(DocumentKind::Content, &doc)
        .unwrap()
        .len()
}

#[test]
fn formula_armor_class_and_hit_points_are_valid_and_shown_as_text() {
    let m = warden();
    assert!(errors(&m).is_empty(), "{:?}", errors(&m));
    assert_eq!(schema_violations(&m), 0);
    assert_eq!((m.armor_class(), m.hit_points()), (None, None));
    assert_eq!(m.armor_class_text(), AC_TEXT);
    assert_eq!(m.hit_points_text(), HP_TEXT);
    let el = m.to_element(ID, "Cinder Warden", "Test Homebrew");
    assert!(!el.data.contains_key("ac") && !el.data.contains_key("hp"));
    assert_eq!(
        (&el.data["ac_note"], &el.data["hp_note"]),
        (&json!(AC_TEXT), &json!(HP_TEXT))
    );
    assert_eq!(MonsterData::from_element(&el).unwrap(), m, "round trip");
}

#[test]
fn numbers_show_as_printed_and_nothing_shows_a_dash() {
    let g = goblin();
    assert_eq!(g.armor_class(), Some(15));
    assert_eq!(g.armor_class_text(), "15 (leather armor, shield)");
    assert_eq!(g.hit_points(), Some(7));
    assert_eq!(g.hit_points_text(), "7 (2d6)");
    let mut bare = warden();
    bare.ac_note = None;
    bare.hp_note = None;
    assert_eq!(bare.armor_class_text(), NO_CHALLENGE_RATING);
    assert_eq!(bare.hit_points_text(), NO_CHALLENGE_RATING);
}

#[test]
fn each_needs_a_number_or_a_note_and_never_both() {
    let mut bare = warden();
    bare.ac_note = None;
    bare.hp_note = None;
    let e = errors(&bare);
    assert!(
        e.iter().any(|x| x.contains("armor class or a note")),
        "{e:?}"
    );
    assert!(
        e.iter().any(|x| x.contains("hit points or a note")),
        "{e:?}"
    );
    assert!(schema_violations(&bare) > 0);

    let g = goblin();
    let mut both = warden();
    both.ac = g.ac.clone();
    both.hp = g.hp.clone();
    let e = errors(&both);
    assert!(
        e.iter().any(|x| x.contains("armor class note next to")),
        "{e:?}"
    );
    assert!(
        e.iter().any(|x| x.contains("hit point note next to")),
        "{e:?}"
    );
    assert!(schema_violations(&both) > 0);
}

/// R4: every bundled stat block reads and writes back to the same bytes.
#[test]
fn stored_stat_blocks_read_and_write_back_byte_for_byte() {
    let mut seen = 0;
    for pack in ["srd-5.1-monsters", "srd-5.2.1-monsters"] {
        let store = ContentStoreBuilder::new()
            .add_pack_dir(&content().join(pack))
            .unwrap()
            .build();
        for s in store.by_type("Monster") {
            let e = &s.element;
            let stored = serde_json::to_string(&e.data).unwrap();
            let m = MonsterData::from_element(e).unwrap();
            assert!(m.ac_note.is_none() && m.hp_note.is_none(), "{}", e.id);
            let back = m.to_element(e.id.clone(), e.name.clone(), e.source.clone());
            let rewritten: Value = serde_json::to_value(&back.data).unwrap();
            assert_eq!(
                serde_json::to_string(&rewritten).unwrap(),
                stored,
                "{}",
                e.id
            );
            seen += 1;
        }
    }
    assert!(seen > 600, "{seen} stat blocks");
}
