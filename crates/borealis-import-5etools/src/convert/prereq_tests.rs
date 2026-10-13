//! Prerequisite conversion on hand-written, invented data.

use super::super::prereq_index::{FeatureRow, PrereqIndex, SpellRow};
use super::super::Shared;
use super::{prerequisites, Cx, Prereq, NOT_CHECKED};
use crate::text::RefKind;
use borealis_content::expr::RequirementsExpr;
use serde_json::{json, Value};
use std::collections::BTreeSet;

fn set(ids: &[&str]) -> BTreeSet<String> {
    ids.iter().map(|s| (*s).to_string()).collect()
}

fn shared() -> Shared {
    let mut sh = Shared::default();
    for (kind, name, source, id) in [
        (RefKind::Feat, "brine adept", "ZZA", "ID_FEAT_BRINE"),
        (RefKind::Feat, "tide strike", "ZZA", "ID_FEAT_TIDE"),
        (RefKind::Feat, "tide strike; ebb", "ZZA", "ID_FEAT_TIDE_EBB"),
        (
            RefKind::Feat,
            "tide strike; flow",
            "ZZA",
            "ID_FEAT_TIDE_FLOW",
        ),
        (RefKind::Background, "reef warden", "ZZA", "ID_BG_REEF"),
        (
            RefKind::OptionalFeature,
            "pact of the oar",
            "ZZA",
            "ID_OF_OAR",
        ),
    ] {
        sh.links
            .insert((kind, name.into(), source.into()), id.into());
    }
    sh
}

fn index() -> PrereqIndex {
    let mut ix = PrereqIndex::default();
    ix.races.insert("merfolk".into(), set(&["ID_RACE_MERFOLK"]));
    ix.races
        .insert("mark of tides".into(), set(&["ID_RACE_MARKED"]));
    ix.races.insert("pixie".into(), set(&["ID_RACE_PIXIE"]));
    ix.small_races = set(&["ID_RACE_PIXIE"]);
    ix.subraces.insert(
        ("merfolk".into(), "deep".into()),
        set(&["ID_SUBRACE_DEEP_MERFOLK"]),
    );
    ix.subclasses.insert(
        ("tidecaller".into(), "undertow".into()),
        set(&["ID_SUBCLASS_UNDERTOW"]),
    );
    ix.feat_categories
        .insert("D".into(), set(&["ID_FEAT_MARK_A", "ID_FEAT_MARK_B"]));
    ix.optfeatures
        .insert("pact of the oar".into(), set(&["ID_OF_OAR"]));
    ix.features = vec![
        FeatureRow {
            name: "spellcasting".into(),
            class: "tidecaller".into(),
            sub: false,
            id: "ID_CF_TIDE_SPELLCASTING".into(),
        },
        FeatureRow {
            name: "brine curse".into(),
            class: "warlock".into(),
            sub: true,
            id: "ID_SF_BRINE_CURSE".into(),
        },
    ];
    let spell = |name: &str, level, damage, attacks: &[&str], id: &str| SpellRow {
        name: name.into(),
        level,
        classes: vec!["warlock".into()],
        damage,
        attacks: attacks.iter().map(|a| (*a).to_string()).collect(),
        id: id.into(),
    };
    ix.spells = vec![
        spell("salt bolt", 0, true, &["R"], "ID_SPELL_SALT_BOLT"),
        spell("foam ward", 0, false, &[], "ID_SPELL_FOAM_WARD"),
        spell("brine burst", 0, true, &[], "ID_SPELL_BRINE_BURST"),
        spell("hexwave", 1, false, &[], "ID_SPELL_HEXWAVE"),
    ];
    ix
}

fn run(items: &[Value], own: &str) -> Prereq {
    let sh = shared();
    let ix = index();
    let p = prerequisites(
        items,
        &Cx {
            sh: &sh,
            ix: &ix,
            own_id: own,
        },
    );
    assert!(
        RequirementsExpr::parse(&p.requirements).is_ok(),
        "{} must parse",
        p.requirements
    );
    p
}

#[test]
fn level_and_ability_alternatives() {
    let p = run(
        &[json!({"level": 4, "ability": [{"str": 13}, {"dex": 13}]})],
        "",
    );
    assert_eq!(p.requirements, "([str:13]||[dex:13])&&[character:4]");
    assert_eq!(
        p.text,
        "Strength 13 or higher or Dexterity 13 or higher, Level 4+"
    );
    let p = run(
        &[
            json!({"level": 4, "ability": [{"str": 13}]}),
            json!({"level": 4, "ability": [{"dex": 13}]}),
        ],
        "",
    );
    assert_eq!(
        p.requirements,
        "([str:13]&&[character:4])||([dex:13]&&[character:4])"
    );
    assert_eq!(run(&[], ""), Prereq::default());
}

#[test]
fn class_and_subclass_levels() {
    let p = run(
        &[json!({"level": {"level": 5, "class": {"name": "Tidecaller"}}})],
        "",
    );
    assert_eq!(p.requirements, "[tidecaller:5]");
    let p = run(
        &[
            json!({"level": {"level": 9, "class": {"name": "Tidecaller"}, "subclass": {"name": "Undertow"}}}),
        ],
        "",
    );
    assert_eq!(p.requirements, "[tidecaller:9]&&ID_SUBCLASS_UNDERTOW");
    assert_eq!(p.text, "Tidecaller (Undertow) level 9+");
}

#[test]
fn spellcasting_uses_the_structured_check_or_feature_ids() {
    let p = run(&[json!({"spellcasting2020": true, "level": 4})], "");
    assert_eq!(p.requires_feature.as_deref(), Some("Spellcasting"));
    assert_eq!(p.requirements, "[character:4]");
    assert!(p.unmapped.is_empty());
    let p = run(&[json!({"feature": ["Fighting Style"]})], "");
    assert_eq!(p.requires_feature.as_deref(), Some("Fighting Style"));
    assert_eq!(p.text, "Feature: Fighting Style");
    // with alternatives the feature must be an id inside the expression
    let p = run(
        &[
            json!({"spellcastingFeature": true}),
            json!({"background": [{"name": "Reef Warden"}]}),
        ],
        "",
    );
    assert_eq!(p.requires_feature, None);
    assert_eq!(p.requirements, "ID_CF_TIDE_SPELLCASTING||ID_BG_REEF");
}

#[test]
fn races_subraces_and_small_races() {
    let p = run(
        &[
            json!({"race": [{"name": "merfolk"}, {"name": "small race", "displayEntry": "a Small race"}]}),
        ],
        "",
    );
    assert_eq!(p.requirements, "(ID_RACE_MERFOLK||ID_RACE_PIXIE)");
    assert_eq!(p.text, "Race: Merfolk or a Small race");
    let p = run(
        &[json!({"race": [{"name": "merfolk", "subrace": "deep"}]})],
        "",
    );
    assert_eq!(p.requirements, "ID_SUBRACE_DEEP_MERFOLK");
    assert_eq!(p.text, "Race: Merfolk (Deep)");
    let p = run(&[json!({"race": [{"name": "Merfolk (Deep)"}]})], "");
    assert_eq!(p.requirements, "ID_SUBRACE_DEEP_MERFOLK");
    let p = run(&[json!({"race": [{"name": "selkie"}]})], "");
    assert_eq!(p.requirements, "");
    assert_eq!(p.unmapped, vec!["Race: Selkie"]);
    assert_eq!(p.text, format!("Race: Selkie {NOT_CHECKED}"));
}

#[test]
fn feats_versions_and_categories() {
    let p = run(&[json!({"feat": ["brine adept|zza"], "level": 4})], "");
    assert_eq!(p.requirements, "ID_FEAT_BRINE&&[character:4]");
    assert_eq!(p.text, "Feat: Brine Adept, Level 4+");
    let p = run(
        &[json!({"feat": ["tide strike|zza|Tide Strike (Ebb Strike)"]})],
        "",
    );
    assert_eq!(p.requirements, "ID_FEAT_TIDE_EBB");
    let p = run(&[json!({"feat": ["tide strike|zza"]})], "");
    assert_eq!(
        p.requirements,
        "(ID_FEAT_TIDE||ID_FEAT_TIDE_EBB||ID_FEAT_TIDE_FLOW)"
    );
    let p = run(
        &[json!({"exclusiveFeatCategory": ["D"], "campaign": ["Seaworld"]})],
        "ID_FEAT_MARK_A",
    );
    assert_eq!(p.requirements, "!ID_FEAT_MARK_B");
    assert_eq!(p.informational, vec!["Campaign: Seaworld"]);
    assert!(p.unmapped.is_empty());
    let p = run(&[json!({"featCategory": ["D"]})], "ID_FEAT_X");
    assert_eq!(p.requirements, "(ID_FEAT_MARK_A||ID_FEAT_MARK_B)");
    let p = run(&[json!({"other": "No other dragonmark"})], "ID_FEAT_MARK_A");
    assert_eq!(p.requirements, "!ID_FEAT_MARK_B&&!ID_RACE_MARKED");
}

#[test]
fn pacts_optional_features_and_spells() {
    let p = run(
        &[json!({"level": {"level": 5, "class": {"name": "Warlock"}}, "pact": "Oar"})],
        "",
    );
    assert_eq!(p.requirements, "[warlock:5]&&ID_OF_OAR");
    let p = run(&[json!({"optionalfeature": ["pact of the oar|zza"]})], "");
    assert_eq!(p.requirements, "ID_OF_OAR");
    let p = run(&[json!({"spell": ["salt bolt#c"]})], "");
    assert_eq!(p.requirements, "ID_SPELL_SALT_BOLT");
    assert_eq!(p.text, "Spell: Salt Bolt cantrip");
    let p = run(&[json!({"spell": ["hexwave/curse#x"]})], "");
    assert_eq!(p.requirements, "(ID_SF_BRINE_CURSE||ID_SPELL_HEXWAVE)");
    let p = run(
        &[
            json!({"spell": [{"choose": "level=0|class=Warlock", "entry": "a cantrip that deals damage"}]}),
        ],
        "",
    );
    assert_eq!(p.requirements, "(ID_SPELL_BRINE_BURST||ID_SPELL_SALT_BOLT)");
    let p = run(
        &[json!({"spell": [{"choose": "level=0|class=Warlock|spell attack=m;r", "entry": "x"}]})],
        "",
    );
    assert_eq!(p.requirements, "ID_SPELL_SALT_BOLT");
    let p = run(
        &[json!({"spell": [{"choose": "level=0|components=v", "entry": "x"}]})],
        "",
    );
    assert_eq!(p.unmapped.len(), 1, "unknown filters are not guessed");
}

#[test]
fn proficiencies_and_text_only_keys() {
    let p = run(
        &[json!({"proficiency": [{"armor": "medium"}, {"weaponGroup": "martial"}]})],
        "",
    );
    assert_eq!(
        p.requirements,
        "(ID_PROFICIENCY_ARMOR_PROFICIENCY_MEDIUM_ARMOR||ID_PROFICIENCY_WEAPON_PROFICIENCY_MARTIAL_WEAPONS)"
    );
    assert_eq!(p.text, "Proficiency: medium armor or martial weapons");
    let p = run(
        &[
            json!({"otherSummary": {"entry": "When Gaining the Level 2 Tidecaller \"Fighting Style\" Feature", "entrySummary": "Special"}}),
        ],
        "",
    );
    assert_eq!(p.requirements, "[tidecaller:2]");
    // an infusion target is not a character requirement
    let p = run(
        &[json!({"item": ["A cloak"], "level": {"level": 2, "class": {"name": "Tinker"}}})],
        "",
    );
    assert_eq!(p.requirements, "[tinker:2]");
    assert!(p.unmapped.is_empty());
    assert!(p.text.contains(NOT_CHECKED));
    // renown cannot be checked: the rest of the alternative still is
    let p = run(
        &[
            json!({"otherSummary": {"entry": "Renown 3+ with a guild", "entrySummary": "Guild Renown 3+"}, "level": 3}),
        ],
        "",
    );
    assert_eq!(p.requirements, "[character:3]");
    assert_eq!(p.unmapped, vec!["Renown 3+ with a guild"]);
    // an alternative with nothing checkable makes the whole prerequisite unchecked
    let p = run(&[json!({"alignment": ["L"]}), json!({"level": 3})], "");
    assert_eq!(p.requirements, "");
    assert_eq!(p.unmapped.len(), 1);
}
