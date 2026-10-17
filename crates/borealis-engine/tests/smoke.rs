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

#[test]
fn ci_flag_reads_like_ci_runners_set_it() {
    for on in ["1", "true", "TRUE", "yes"] {
        assert!(ci_flag(on), "{on}");
    }
    for off in ["", " ", "0", "false", "False"] {
        assert!(!ci_flag(off), "{off:?}");
    }
}

#[test]
fn bundled_variant_is_always_available() {
    // the public-CI variant: core + SRD 5.1 packs from the repository
    assert!(engine(Variant::Bundled).is_some());
}
