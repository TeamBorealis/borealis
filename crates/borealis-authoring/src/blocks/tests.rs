use super::*;
use borealis_content::rules::{CustomRule, GrantRule, SelectRule, StatRule};
use serde_json::{json, Value};
use std::collections::BTreeMap;

struct Names(BTreeMap<&'static str, (&'static str, &'static str)>);

impl SentenceContext for Names {
    fn element(&self, id: &str) -> Option<(String, String)> {
        self.0
            .get(id)
            .map(|(n, t)| ((*n).to_string(), (*t).to_string()))
    }
    fn class_names(&self) -> Vec<String> {
        vec!["Warlock".into(), "Paladin".into()]
    }
}

fn names() -> Names {
    Names(BTreeMap::from([
        ("ID_STEALTH", ("Stealth", "Proficiency")),
        ("ID_FIREBALL", ("Fireball", "Spell")),
        ("ID_MC_PALADIN", ("Paladin", "Multiclass")),
        ("ID_ALERT", ("Alert", "Feat")),
    ]))
}

fn custom(kind: &str, fields: Value) -> Rule {
    let Value::Object(fields) = fields else {
        unreachable!()
    };
    Rule::Custom(CustomRule {
        kind: kind.into(),
        fields,
    })
}

#[test]
fn catalog_order_core_then_custom_sorted_then_effects() {
    let kinds: Vec<&str> = catalog().iter().map(|b| b.kind).collect();
    assert_eq!(&kinds[..4], &CORE_KINDS);
    let custom: Vec<&str> = catalog()
        .iter()
        .filter(|b| !CORE_KINDS.contains(&b.kind) && !b.effect_only())
        .map(|b| b.kind)
        .collect();
    let mut sorted = custom.clone();
    sorted.sort_unstable();
    assert_eq!(custom, sorted);
    assert_eq!(kinds.last(), Some(&"speed_multiply"));
    assert!(block("exhaustion").is_some_and(BlockSpec::core_only));
    assert!(block("resource").is_some_and(|b| b.common));
}

#[test]
fn kind_of_reads_lists_and_keeps_unknown_rules() {
    let list = Rule::Select(SelectRule {
        ty: "List".into(),
        name: "Ideal".into(),
        ..SelectRule::default()
    });
    assert_eq!(kind_of(&list), Some("list"));
    assert_eq!(kind_of(&custom("resource", json!({}))), Some("resource"));
    assert_eq!(kind_of(&custom("no_such_kind", json!({}))), None);
    assert_eq!(kind_of(&Rule::Unknown(Default::default())), None);
}

#[test]
fn controls_have_wire_names() {
    assert_eq!(Control::Formula(FormulaDialect::SaveDc).wire(), "formula");
    assert_eq!(
        Control::NumberOr {
            min: 0,
            max: 1,
            words: &[],
            custom: false
        }
        .wire(),
        "number"
    );
    const KV: [BlockField; 2] = [
        BlockField::new("a", "A", Control::Text),
        BlockField::new("b", "B", Control::Text),
    ];
    assert_eq!(Control::Map(&KV).parts().len(), 2);
}

#[test]
fn values_fit_their_controls() {
    let max = block("resource").and_then(|b| b.field("max")).expect("max");
    assert!(value_fits(
        &max.control,
        &json!({"type": "table", "key": "rages"})
    ));
    assert!(value_fits(
        &max.control,
        &json!({"type": "formula", "expr": "max(1, wisdom:modifier)"})
    ));
    assert!(!value_fits(
        &max.control,
        &json!({"type": "table", "value": 2})
    ));
    assert!(!value_fits(&max.control, &json!({"type": "dawn"})));
    let uses = Control::Uses;
    for ok in [
        json!(2),
        json!("proficiency"),
        json!("wisdom"),
        json!({"type": "table", "key": "x"}),
    ] {
        assert!(value_fits(&uses, &ok), "{ok}");
    }
    assert!(!value_fits(&uses, &json!("luck")));
    let regain = block("resource")
        .and_then(|b| b.field("regain"))
        .expect("regain");
    assert!(value_fits(
        &regain.control,
        &json!({"short_rest": 1, "long_rest": "all"})
    ));
    assert!(!value_fits(&regain.control, &json!({"dawn": 1})));
    let radius = block("aura")
        .and_then(|b| b.field("radius_by_level"))
        .expect("radius");
    assert!(value_fits(&radius.control, &json!({"6": 10, "18": 30})));
    assert!(!value_fits(&radius.control, &json!({"sixth": 10})));
}

#[test]
fn misfits_name_unoffered_fields_and_bad_values() {
    let b = block("resource").expect("resource");
    let fields =
        json!({"id": "rage", "max": {"type": "fixed", "value": 2}, "counter": "dc", "level": 3});
    assert_eq!(
        misfits(b, fields.as_object().unwrap()),
        vec!["counter".to_string()]
    );
    let bad = json!({"id": "rage", "max": "two"});
    assert_eq!(
        misfits(b, bad.as_object().unwrap()),
        vec!["max".to_string()]
    );
    // Verbatim readers accept one value where the block writes a list.
    let fcc = block("free_cast_choice").expect("free_cast_choice");
    assert!(misfits(fcc, json!({"recharge": "long_rest"}).as_object().unwrap()).is_empty());
    let r = custom("resource", json!({"id": "x", "recharge": "long_rest"}));
    assert_eq!(
        fits(&r),
        Some(vec!["recharge".to_string()]),
        "the resource reader reads lists only"
    );
}

#[test]
fn core_sentences() {
    let cx = names();
    let g = Rule::Grant(GrantRule {
        id: "ID_STEALTH".into(),
        level: Some(3),
        requirements: Some("!ID_MC_PALADIN".into()),
        ..GrantRule::default()
    });
    assert_eq!(
        sentence(&g, &cx),
        "From level 3: gives proficiency: Stealth, only if first class only"
    );
    let s = Rule::Select(SelectRule {
        ty: "Language".into(),
        name: "Language (Sage)".into(),
        number: Some(2),
        supports: Some("Standard||Exotic".into()),
        ..SelectRule::default()
    });
    assert_eq!(
        sentence(&s, &cx),
        "Lets the player choose: Language (Sage) (2 picks), from Standard or Exotic"
    );
    let st = Rule::Stat(StatRule {
        name: "darkvision:range".into(),
        value: "60".into(),
        equipped: Some("[armor:none]".into()),
        ..StatRule::default()
    });
    assert_eq!(
        sentence(&st, &cx),
        "Changes Darkvision range: +60, while wearing no armor"
    );
    let neg = Rule::Stat(StatRule {
        name: "ac:misc".into(),
        value: "-dexterity:modifier".into(),
        requirements: Some("[str:13]&&[warlock:2]".into()),
        ..StatRule::default()
    });
    assert_eq!(
        sentence(&neg, &cx),
        "Changes Armor Class bonus: minus Dexterity modifier, only if Strength 13 or higher and Warlock level 2 or higher"
    );
}

#[test]
fn custom_sentences_use_names_labels_and_words() {
    let cx = names();
    let cases = [
        (
            custom("resource", json!({"id": "rage", "name": "Rage", "max": {"type": "table", "key": "rages"},
                "recharge": ["long_rest"]})),
            "Limited uses: Rage (the class table's rages column), regained on long rest",
        ),
        (
            custom("grant_spell", json!({"spell": "ID_FIREBALL", "mode": "always_prepared", "level": 5,
                "free_cast": {"uses": "proficiency", "recharge": "long_rest"}})),
            "From level 5: gives the spell Fireball (always prepared), free casts equal to proficiency bonus per long rest",
        ),
        (
            custom("aura", json!({"radius_feet": 10, "bonus_saving_throws": {"formula": "max(1, charisma:modifier)"}})),
            "Aura of 10 feet",
        ),
        (
            custom("save_dc", json!({"id": "channel divinity", "formula": "8 + proficiency + wisdom:modifier"})),
            "Save DC of channel divinity: 8 + proficiency bonus + Wisdom modifier",
        ),
        (
            custom("ability_increase", json!({"options": [{"abilities": 1, "amount": 2}, {"abilities": 2, "amount": 1}]})),
            "Ability score increase: +2 to any ability or +1 to any 2 abilities",
        ),
        (custom("evasion", json!({"level": 7})), "From level 7: evasion"),
        (
            custom("spellbook", json!({"initial_spells": 6, "per_level": 2})),
            "Spellbook (spells at first: 6; spells added per level: 2)",
        ),
        (custom("grant_feat", json!({"feat": "ID_ALERT"})), "Gives the feat Alert"),
        (custom("grant_feat", json!({"feat": "ID_GONE"})), "Gives the feat a missing element"),
        (
            custom("toggle", json!({"id": "rage", "name": "Rage", "effects": [{"kind": "stat", "name": "speed", "value": "10"}]})),
            "On/off state: Rage (1 effect)",
        ),
    ];
    for (rule, want) in cases {
        assert_eq!(sentence(&rule, &cx), want);
    }
    assert_eq!(
        sentence(&custom("no_such_kind", json!({})), &NoNames),
        "Kept as is: a rule this creator does not edit"
    );
}

#[test]
fn block_specs_serialize_for_the_wire() {
    let b = block("resource").expect("resource");
    assert_eq!(b.group.id(), "uses");
    assert_eq!(b.group.label(), "Uses & resources");
    assert_eq!(BlockGroup::ALL.len(), 9);
    assert!(BlockGroup::ALL
        .iter()
        .all(|g| catalog().iter().any(|b| b.group == *g)));
}
