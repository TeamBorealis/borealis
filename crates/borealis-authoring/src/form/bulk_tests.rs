//! Multi-select edits through the form (spec 12.5): edition and tags change only what they own, a spell's tags are
//! its lists (its `data.classes` mirror follows), a class keeps its edition (R4), nothing to do answers `None`.

use super::*;
use crate::expr_build::condition::ConditionContext;
use crate::form::{CatalogBlockCodec, NoLookup};
use serde_json::{json, Value};

fn el(v: Value) -> Element {
    serde_json::from_value(v).expect("element")
}

fn with<T>(f: impl FnOnce(&FormContext<'_>) -> T) -> T {
    let codec = CatalogBlockCodec;
    let cx = FormContext {
        conditions: ConditionContext::new(["Wizard"]),
        lookup: &NoLookup,
        blocks: &codec,
    };
    f(&cx)
}

fn feat() -> Element {
    el(
        json!({"id": "ID_HB_FEAT_GRIT", "name": "Grit", "type": "Feat", "source": "HB", "supports": ["General"],
        "setters": [{"name": "short", "value": "Tough."}], "description": "<p>Tough.</p>"}),
    )
}

#[test]
fn tags_are_added_once_and_removed_in_any_case() {
    with(|cx| {
        let change = BulkChange {
            add_tags: vec!["Origin".into(), "general".into()],
            ..BulkChange::default()
        };
        let next = bulk_patched(&feat(), &change, cx)
            .expect("ok")
            .expect("changed");
        assert_eq!(next.supports, vec!["General".to_string(), "Origin".into()]);
        assert_eq!(next.setters, feat().setters, "nothing else changes");
        assert_eq!(next.description, feat().description);
        let removed = bulk_patched(
            &next,
            &BulkChange {
                remove_tags: vec!["ORIGIN".into()],
                ..BulkChange::default()
            },
            cx,
        )
        .expect("ok")
        .expect("changed");
        assert_eq!(removed, feat(), "back to the stored element, byte for byte");
    });
}

#[test]
fn nothing_to_change_answers_none() {
    with(|cx| {
        let same = BulkChange {
            add_tags: vec!["General".into()],
            edition: Some(None),
            ..BulkChange::default()
        };
        assert_eq!(bulk_patched(&feat(), &same, cx).expect("ok"), None);
        assert!(BulkChange::default().is_empty());
    });
}

#[test]
fn the_edition_is_set_and_cleared() {
    with(|cx| {
        let to_2014 = BulkChange {
            edition: Some(Some(Ruleset::Dnd2014)),
            ..BulkChange::default()
        };
        let next = bulk_patched(&feat(), &to_2014, cx)
            .expect("ok")
            .expect("changed");
        assert_eq!(next.ruleset, Some(Ruleset::Dnd2014));
        let cleared = bulk_patched(
            &next,
            &BulkChange {
                edition: Some(None),
                ..BulkChange::default()
            },
            cx,
        )
        .expect("ok")
        .expect("changed");
        assert_eq!(cleared, feat());
    });
}

#[test]
fn a_class_keeps_its_edition() {
    with(|cx| {
        let class = el(
            json!({"id": "ID_HB_CLASS_KNIGHT", "name": "Knight", "type": "Class", "source": "HB",
            "ruleset": "dnd2024", "setters": [{"name": "hd", "value": "d10"}]}),
        );
        let change = BulkChange {
            edition: Some(Some(Ruleset::Dnd2014)),
            ..BulkChange::default()
        };
        let problems = bulk_patched(&class, &change, cx).expect_err("refused");
        assert_eq!(problems[0].path, "fields.edition");
        assert!(
            problems[0].message.contains("keeps its edition (2024)"),
            "{problems:?}"
        );
        let same = BulkChange {
            edition: Some(Some(Ruleset::Dnd2024)),
            add_tags: vec!["Martial".into()],
            ..BulkChange::default()
        };
        let tagged = bulk_patched(&class, &same, cx)
            .expect("ok")
            .expect("changed");
        assert_eq!(tagged.supports, vec!["Martial".to_string()]);
        assert_eq!(tagged.ruleset, Some(Ruleset::Dnd2024));
    });
}

#[test]
fn a_spells_tags_are_its_lists_and_the_mirror_follows() {
    with(|cx| {
        let spell = el(
            json!({"id": "ID_HB_SPELL_EMBER", "name": "Ember", "type": "Spell", "source": "HB",
            "ruleset": "dnd2024", "supports": ["Sorcerer"],
            "setters": [{"name": "level", "value": "1"}, {"name": "school", "value": "Evocation"},
                {"name": "time", "value": "1 action"}, {"name": "range", "value": "60 feet"},
                {"name": "duration", "value": "Instantaneous"}],
            "data": {"classes": ["Sorcerer"]}}),
        );
        let next = with_tag(&spell, "Wizard", true, cx)
            .expect("ok")
            .expect("changed");
        assert_eq!(next.supports, vec!["Sorcerer".to_string(), "Wizard".into()]);
        assert_eq!(next.data["classes"], json!(["Sorcerer", "Wizard"]));
        assert_eq!(
            with_tag(&next, "wizard", true, cx).expect("ok"),
            None,
            "on the list already"
        );
        let back = with_tag(&next, "Wizard", false, cx)
            .expect("ok")
            .expect("changed");
        assert_eq!(back, spell);
    });
}
