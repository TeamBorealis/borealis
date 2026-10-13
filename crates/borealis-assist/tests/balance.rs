//! Deterministic balance reports against SRD 5.1 baselines, and the engine load check.

mod common;

use borealis_assist::balance::{CharacterCheck, Severity};
use borealis_assist::{balance_report, load_into_engine, Baselines};
use borealis_content::file::ContentFile;
use borealis_content::{Element, Ruleset};
use serde_json::{json, Value};

fn baselines() -> Baselines {
    let ctx = common::pack_ctx();
    Baselines::from_store(&ctx.base.clone().build(), Ruleset::Dnd2014)
}

fn element(doc: &Value) -> Element {
    ContentFile::from_json_str(&doc.to_string())
        .unwrap()
        .elements
        .remove(0)
}

fn spell_doc(id: &str, text: &str, level: &str) -> Value {
    json!({"format":"borealis-content/1","elements":[{
        "id": id, "name": id, "type":"Spell","source":"Homebrew","supports":["Wizard"],
        "description": format!("<p>{text}</p>"),
        "setters":[{"name":"level","value":level},{"name":"school","value":"Evocation"},
                   {"name":"time","value":"1 action"},{"name":"duration","value":"Instantaneous"},
                   {"name":"range","value":"120 feet"}]}]})
}

#[test]
fn overpowered_first_level_aoe_is_high_severity() {
    let doc = spell_doc("ID_HB_SPELL_DOOM_NOVA", "Each creature in a 30-foot-radius sphere must make a Dexterity saving throw. A creature takes 10d10 fire damage on a failed save, or half as much on a success.", "1");
    let r = balance_report(&baselines(), &element(&doc));
    let f = r
        .flags
        .iter()
        .find(|f| f.metric == "spell.damage_avg")
        .expect("flag");
    assert_eq!(f.severity, Severity::High, "{f:?}");
    assert!((f.value - 55.0).abs() < 1e-9);
    assert!(
        f.baseline_range[1] < 20.0,
        "SRD level 1 area max is small: {f:?}"
    );
    assert!(f.note.contains("level 1 area"));
    assert!(r.has_high());
}

#[test]
fn balanced_first_level_cone_passes() {
    let doc = spell_doc("ID_HB_SPELL_CINDER_FAN", "Each creature in a 15-foot cone must make a Dexterity saving throw. A creature takes 3d6 fire damage on a failed save, or half as much on a success.", "1");
    let r = balance_report(&baselines(), &element(&doc));
    assert_eq!(r.max_severity(), Severity::Ok, "{:?}", r.flags);
    assert_eq!(r.flagged().count(), 0);
}

#[test]
fn monster_hp_far_above_its_cr_is_flagged() {
    let store = common::pack_ctx().base.build();
    let mut m = store
        .by_type("Monster")
        .map(|s| s.element.clone())
        .find(|e| e.data.get("cr").and_then(Value::as_str) == Some("1"))
        .expect("a CR 1 monster");
    let honest = balance_report(&baselines(), &m);
    assert!(
        honest.max_severity() <= Severity::Low,
        "{:?}",
        honest.flagged().collect::<Vec<_>>()
    );
    m.data.insert("hp".into(), json!({"average": 400}));
    let r = balance_report(&baselines(), &m);
    let hp = r.flags.iter().find(|f| f.metric == "monster.hp").unwrap();
    assert_eq!(hp.severity, Severity::High, "{hp:?}");
    assert!(hp.note.contains("CR 1"));
}

#[test]
fn feat_item_and_species_are_graded() {
    let b = baselines();
    let feat = json!({"format":"borealis-content/1","elements":[{"id":"ID_HB_FEAT_TITAN","name":"Titan","type":"Feat","source":"Homebrew",
        "rules":[{"kind":"stat","name":"strength","value":"2"},{"kind":"stat","name":"constitution","value":"2"}]}]});
    let r = balance_report(&b, &element(&feat));
    let asi = r
        .flags
        .iter()
        .find(|f| f.metric == "feat.asi_total")
        .unwrap();
    assert_eq!(asi.severity, Severity::High);

    let item = json!({"format":"borealis-content/1","elements":[{"id":"ID_HB_ITEM_BLADE","name":"Blade, +3","type":"Magic Item","source":"Homebrew",
        "setters":[{"name":"rarity","value":"Uncommon"}]}]});
    let r = balance_report(&b, &element(&item));
    assert!(
        r.flags
            .iter()
            .any(|f| f.metric == "item.bonus" && f.severity >= Severity::Medium),
        "{:?}",
        r.flags
    );

    let sp = json!({"format":"borealis-content/1","elements":[{"id":"ID_HB_RACE_SPRINTER","name":"Sprinter","type":"Race","source":"Homebrew",
        "rules":[{"kind":"stat","name":"innate speed","value":"60","bonus":"base"}]}]});
    let r = balance_report(&b, &element(&sp));
    let speed = r
        .flags
        .iter()
        .find(|f| f.metric == "species.speed")
        .unwrap();
    assert_eq!(speed.severity, Severity::High, "{speed:?}");
}

#[test]
fn engine_check_loads_a_species_and_rejects_dangling_refs() {
    let ctx = common::pack_ctx();
    let ok = json!({"format":"borealis-content/1","elements":[{"id":"ID_HB_RACE_MARSHFOLK","name":"Marshfolk","type":"Race","source":"Homebrew",
        "rules":[{"kind":"stat","name":"constitution","value":"1"},{"kind":"grant","id":"ID_SIZE_MEDIUM","type":"Size"},
                 {"kind":"grant","id":"ID_LANGUAGE_COMMON","type":"Language"}]}]});
    let c = load_into_engine(&ctx.base, ctx.ruleset, &ok);
    assert!(c.passes(), "{c:?}");
    assert_eq!(c.characters[0].1, CharacterCheck::Taken);

    let bad = json!({"format":"borealis-content/1","elements":[{"id":"ID_HB_RACE_GHOST","name":"Ghost","type":"Race","source":"Homebrew",
        "rules":[{"kind":"grant","id":"ID_DOES_NOT_EXIST_ANYWHERE","type":"Racial Trait"}]}]});
    let c = load_into_engine(&ctx.base, ctx.ruleset, &bad);
    assert!(!c.passes());
    assert!(
        c.dangling_refs
            .iter()
            .any(|m| m.contains("ID_DOES_NOT_EXIST_ANYWHERE")),
        "{c:?}"
    );
}

#[test]
fn engine_check_lets_a_test_character_take_a_feat() {
    let ctx = common::pack_ctx();
    let feat = json!({"format":"borealis-content/1","elements":[{"id":"ID_HB_FEAT_STURDY","name":"Sturdy","type":"Feat","source":"Homebrew",
        "rules":[{"kind":"stat","name":"constitution","value":"1"}]}]});
    let c = load_into_engine(&ctx.base, ctx.ruleset, &feat);
    assert!(c.passes(), "{c:?}");
    assert_eq!(c.characters[0].1, CharacterCheck::Taken);
}
