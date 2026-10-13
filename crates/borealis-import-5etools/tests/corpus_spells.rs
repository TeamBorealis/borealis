//! Local-only checks of additional spells on a real 5etools `data/` folder (expanded lists,
//! innate / limited spells, spell choices, alternative groups). Enabled by
//! `BOREALIS_5ETOOLS_DIR`; skipped otherwise. Refers to entities by id only.

use borealis_content::{Element, Rule};
use borealis_import_5etools::{import_folder, ImportOptions};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;

fn custom<'a>(el: &'a Element, kind: &str) -> Vec<&'a serde_json::Map<String, Value>> {
    el.rules
        .iter()
        .filter_map(|r| match r {
            Rule::Custom(c) if c.kind == kind => Some(&c.fields),
            _ => None,
        })
        .collect()
}

#[test]
fn five_etools_additional_spells() {
    let Ok(dir) = std::env::var("BOREALIS_5ETOOLS_DIR") else {
        eprintln!("five_etools_additional_spells: BOREALIS_5ETOOLS_DIR not set, skipping");
        return;
    };
    let result = import_folder(&PathBuf::from(dir), &ImportOptions::default()).expect("import");
    let all: HashMap<&str, &Element> = result
        .packs
        .iter()
        .flat_map(|p| p.elements())
        .map(|e| (e.id.as_str(), e))
        .collect();
    let get = |id: &str| *all.get(id).unwrap_or_else(|| panic!("{id} missing"));

    // a warlock patron's expanded list extends the Warlock list (levels follow the slots)
    let fiend = get("ID_5ET_PHB_SUBCLASS_WARLOCK_FIEND");
    let sc = fiend.spellcasting.as_ref().expect("extension block");
    assert_eq!(
        (sc.name.as_deref(), sc.extend),
        (Some("Warlock"), Some(true))
    );
    assert!(sc
        .extends
        .iter()
        .any(|l| l.supports == "ID_5ET_PHB_SPELL_FIREBALL"));
    assert_eq!(sc.extends.len(), 10);

    // alternative patron groups: one option element each, chosen in a subclass select
    let genie = get("ID_5ET_TCE_SUBCLASS_WARLOCK_GENIE");
    assert!(genie
        .rules
        .iter()
        .any(|r| matches!(r, Rule::Select(s) if s.ty == "Archetype Feature")));
    let dao = get("ID_5ET_TCE_SUBCLASS_WARLOCK_GENIE_DAO");
    assert_eq!(dao.spellcasting.as_ref().unwrap().extends.len(), 10);
    assert!(dao.rules.iter().any(
        |r| matches!(r, Rule::Grant(g) if g.ty.as_deref() == Some("Grants") && g.level == Some(9))
    ));
    let land = get("ID_5ET_PHB_SUBCLASS_DRUID_LAND_ARCTIC");
    assert!(land
        .rules
        .iter()
        .any(|r| matches!(r, Rule::Grant(g) if g.prepared == Some(true) && g.spellcasting.as_deref() == Some("Druid"))));

    // a race with limited innate spells: its own block, ability and short-rest free casts
    let firbolg = get("ID_5ET_VGM_RACE_FIRBOLG");
    let sc = firbolg.spellcasting.as_ref().expect("own block");
    assert_eq!(
        (sc.name.as_deref(), sc.ability.as_deref()),
        (Some("Firbolg"), Some("Wisdom"))
    );
    let casts = custom(firbolg, "grant_spell");
    assert_eq!(casts.len(), 2);
    assert!(casts
        .iter()
        .all(|c| c["free_cast"]["recharge"] == "short_rest" && c["spell_list"] == "Firbolg"));

    // a feat granting a spell with a daily free cast, a filtered choice and an ability choice
    let fey = get("ID_5ET_TCE_FEAT_FEY_TOUCHED");
    let casts = custom(fey, "grant_spell");
    assert_eq!(casts.len(), 1);
    assert_eq!(casts[0]["free_cast"]["recharge"], "long_rest");
    let choice = fey
        .rules
        .iter()
        .find_map(|r| match r {
            Rule::Select(s) if s.ty == "Spell" => Some(s),
            _ => None,
        })
        .expect("spell choice");
    let supports = choice.supports.as_deref().unwrap();
    assert!(
        supports.contains("Divination")
            && supports.contains("Enchantment")
            && supports.contains('1')
    );
    assert_eq!(
        custom(fey, "spell_free_cast")[0]["select"],
        Value::from(choice.name.clone())
    );
    assert!(fey
        .rules
        .iter()
        .any(|r| matches!(r, Rule::Select(s) if s.ty == "Grants" && s.name.starts_with("Spellcasting Ability"))));
    assert!(all.contains_key("ID_5ET_TCE_FEAT_FEY_TOUCHED_SPELLCASTING_CHA"));

    // spell-slot keyed always-prepared spells: an all-classes extension with prepared entries
    let adept = get("ID_5ET_AU_FEAT_ABJURATION_ADEPT");
    let sc = adept.spellcasting.as_ref().unwrap();
    assert_eq!((sc.extend, sc.all), (Some(true), Some(true)));
    assert_eq!(
        sc.extends
            .iter()
            .filter(|l| l.prepared == Some(true))
            .count(),
        5
    );

    // a class-level expanded list waits for its class level
    let bard = get("ID_5ET_XPHB_CLASS_BARD");
    assert!(bard.rules.iter().any(|r| matches!(r, Rule::Grant(g)
        if g.id == "ID_5ET_XPHB_CLASS_BARD_EXPANDED_SPELLS_10" && g.level == Some(10))));

    // a non-casting class's subclass gets a block of its own, from the level of its spells
    let shadow = get("ID_5ET_PHB_SUBCLASS_MONK_SHADOW");
    let helper = get("ID_5ET_PHB_SUBCLASS_MONK_SHADOW_SPELLCASTING");
    assert_eq!(
        helper.spellcasting.as_ref().unwrap().ability.as_deref(),
        Some("Wisdom")
    );
    assert!(shadow.rules.iter().any(|r| matches!(r, Rule::Grant(g)
        if g.id == helper.id && g.level == Some(3))));
    assert!(custom(shadow, "grant_spell")
        .iter()
        .any(|c| c["resource_cost"]["cost"] == 2));

    // invocations grant Warlock spells
    let invocations = all
        .values()
        .filter(|e| e.supports.iter().any(|s| s == "OF_EI"))
        .filter(|e| !custom(e, "grant_spell").is_empty())
        .count();
    assert!(invocations >= 20, "invocations with spells: {invocations}");
}
