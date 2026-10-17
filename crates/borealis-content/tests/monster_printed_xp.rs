//! Decision HB4: a monster whose XP differs from the standard XP of its challenge rating keeps
//! the XP it was given (homebrew prints such numbers). The content validator reports it as a
//! warning, never an error, and never changes the stored number.

use borealis_content::{
    ContentFile, ContentStoreBuilder, Cr, IssueCode, MonsterData, PackOrigin, Severity,
};
use serde_json::Value;
use std::path::Path;

const ID: &str = "ID_TEST_MONSTER_FEN_QUILLBACK";

/// A synthetic CR 1/4 creature built from the bundled goblin stat block.
fn quillback() -> MonsterData {
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

fn validated(m: &MonsterData) -> (Vec<borealis_content::Issue>, Value) {
    let file = ContentFile {
        elements: vec![m.to_element(ID, "Fen Quillback", "Test Homebrew")],
        ..ContentFile::default()
    };
    let store = ContentStoreBuilder::new()
        .add_file("homebrew.json", PackOrigin::Custom, file)
        .build();
    let stored = store.get(ID).expect("loaded").element.data["xp"].clone();
    (store.validate(), stored)
}

#[test]
fn a_printed_xp_off_the_table_is_kept_and_warned_not_an_error() {
    // Arrange: CR 1/4 (standard 50 XP) printed with 75 XP.
    let mut m = quillback();
    assert_eq!(m.cr, Cr::parse("1/4").unwrap());
    m.xp = 75;

    // Act
    let hard = m.validate();
    let soft = m.warnings();
    let (issues, stored) = validated(&m);

    // Assert: no hard problem, one warning naming both numbers, the printed XP stays.
    assert!(hard.iter().all(|i| i.field != "xp"), "{hard:?}");
    assert_eq!(soft.len(), 1, "{soft:?}");
    assert_eq!(soft[0].field, "xp");
    assert!(
        soft[0].message.contains("75") && soft[0].message.contains("50"),
        "{}",
        soft[0].message
    );
    assert!(
        issues.iter().all(|i| i.severity != Severity::Error),
        "{issues:#?}"
    );
    let xp: Vec<_> = issues
        .iter()
        .filter(|i| i.code == IssueCode::NonStandardMonsterXp)
        .collect();
    assert_eq!(xp.len(), 1, "{issues:#?}");
    assert_eq!(xp[0].severity, Severity::Warning);
    assert_eq!(xp[0].element.as_deref(), Some(ID));
    assert_eq!(xp[0].path.as_deref(), Some("data.xp"));
    assert_eq!(stored, Value::from(75));
}

#[test]
fn a_standard_xp_gives_no_warning() {
    // Arrange: the table value, and CR 0 printed as 0 XP.
    let m = quillback();
    let mut zero = quillback();
    zero.cr = Cr::parse("0").unwrap();
    zero.proficiency_bonus = 2;
    zero.xp = 0;

    // Act / Assert
    for m in [m, zero] {
        assert_eq!(m.warnings(), vec![]);
        let (issues, _) = validated(&m);
        // Only the note that no Source element defines "Test Homebrew".
        assert!(
            issues.iter().all(|i| i.severity == Severity::Info),
            "{issues:#?}"
        );
    }
}
