mod common;
use common::*;

#[test]
fn builds_and_evaluates_an_empty_character() {
    each_variant("smoke", |e, v| {
        let h = Hero::new(e, true);
        let snap = h.snap();
        eprintln!(
            "{v:?}: slots={} diag={:?}",
            snap.slots.len(),
            snap.diagnostics
        );
        assert_eq!(snap.level, 1);
        assert_eq!(snap.proficiency_bonus, 2);
        assert_eq!(snap.armor_class, 10);
        assert!(snap.slots.iter().any(|s| s.name == "Race"));
        assert_eq!(snap.abilities[0].score, 10);
    });
}
