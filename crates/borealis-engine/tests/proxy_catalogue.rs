//! The proxy catalogue (`specs/proxy-items.md` 1.1 / 1.3, P2): one listed "Additional ..." item
//! per target and spellcasting name, generated categories in Aurora's order, and the detail text
//! of a proxy (P6).

#[path = "common/proxy_fixture.rs"]
mod proxy_fixture;

use borealis_content::Ruleset;
use proxy_fixture::{engine, BOOK};

const FEAT_PROXY: &str = "ID_TB_INTERNAL_ITEM_FEAT_PROXY_LOOKOUT";

/// A generated category name: "Additional " and the target type.
fn category(ty: &str) -> String {
    format!("Additional {ty}")
}

/// `(id, name, category)` of the catalogue rows of the synthetic book.
fn book_rows() -> Vec<(String, String, String)> {
    let uni = engine().universe(Ruleset::Dnd2014);
    let mut rows: Vec<_> = uni
        .proxies()
        .all()
        .filter(|p| p.source == BOOK)
        .map(|p| (p.id, p.name, p.category))
        .collect();
    rows.sort();
    rows
}

#[test]
fn the_catalogue_lists_one_proxy_per_target_and_spellcasting_name() {
    let row = |id: &str, name: &str, cat: &str| (id.to_string(), name.to_string(), cat.to_string());
    let mut want = vec![
        row(FEAT_PROXY, "Additional Feat, Lookout", "Additional Feat"),
        row(
            "ID_TB_INTERNAL_ITEM_LANGUAGE_PROXY_LANGUAGE_OLDTONGUE",
            &format!("{}, Old Tongue", category("Language")),
            &category("Language"),
        ),
        row(
            "ID_TB_INTERNAL_ITEM_PROFICIENCY_PROXY_PROFICIENCY_SKILL_DELVING",
            "Additional Proficiency, Skill Proficiency (Delving)",
            "Additional Proficiency",
        ),
        row(
            "ID_TB_INTERNAL_ITEM__SPELL_PROXY_SPELL_SPARK",
            "Additional Spell, Spark",
            "Additional Spell",
        ),
        row(
            "ID_TB_INTERNAL_ITEM_TESTER_MAGE_SPELL_PROXY_SPELL_SPARK",
            "Additional Tester Mage Spell, Spark",
            "Additional Tester Mage Spell",
        ),
    ];
    want.sort();
    assert_eq!(book_rows(), want);
}

#[test]
fn every_listed_proxy_resolves_to_an_item_with_its_name_and_category() {
    let uni = engine().universe(Ruleset::Dnd2014);
    let mut n = 0;
    for p in uni.proxies().all() {
        let el = &uni
            .get(&p.id)
            .unwrap_or_else(|| panic!("{} resolves", p.id))
            .el;
        assert_eq!(el.ty, "Item");
        assert_eq!(el.name, p.name, "{}", p.id);
        assert_eq!(el.setter_value("category"), Some(p.category.as_str()));
        n += 1;
    }
    assert!(n > 100, "core catalogue proxies are listed too ({n})");
}

#[test]
fn core_catalogue_targets_list_as_the_base_book_except_spell_focus_groups() {
    let uni = engine().universe(Ruleset::Dnd2014);
    let ids: Vec<String> = uni.proxies().all().map(|p| p.id).collect();
    for id in [
        "ID_PHB_INTERNAL_ITEM_PROFICIENCY_PROXY_PROFICIENCY_SKILL_STEALTH",
        "ID_PHB_INTERNAL_ITEM_PROXY_ASI_STRENGTH",
    ] {
        assert!(ids.iter().any(|i| i == id), "{id} listed");
    }
    assert!(ids.iter().all(|i| !i.contains("SPELLFOCUS_GROUP")));
    let mut unique = ids.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), ids.len(), "each id once");
    let stealth = uni
        .proxies()
        .in_category("Additional Proficiency")
        .find(|p| p.id.ends_with("_SKILL_STEALTH"))
        .unwrap();
    assert_eq!(
        stealth.name,
        "Additional Proficiency, Skill Proficiency (Stealth)"
    );
}

#[test]
fn generated_categories_come_in_auroras_order() {
    let uni = engine().universe(Ruleset::Dnd2014);
    assert_eq!(
        uni.proxies().categories(),
        [
            "Feat",
            "Language",
            "Proficiency",
            "Ability Score Improvement",
            "Spell",
            "Tester Mage Spell",
        ]
        .map(category)
    );
    let all: Vec<_> = uni.proxies().all().collect();
    for cat in uni.proxies().categories() {
        let rows: Vec<_> = uni.proxies().in_category(&cat).collect();
        assert_eq!(
            rows.len(),
            all.iter().filter(|p| p.category == cat).count(),
            "{cat}"
        );
    }
}

#[test]
fn spell_proxies_carry_the_cast_name_and_supports_as_keywords() {
    let uni = engine().universe(Ruleset::Dnd2014);
    let p = uni
        .proxies()
        .in_category("Additional Tester Mage Spell")
        .find(|p| p.source == BOOK)
        .unwrap();
    let kw = p.keywords();
    assert!(kw.iter().any(|k| k == "Tester Mage"), "{kw:?}");
    let feat = uni
        .proxies()
        .in_category("Additional Feat")
        .find(|p| p.id == FEAT_PROXY)
        .unwrap();
    assert_eq!(feat.keywords(), vec!["watch", "sentry"]);
}
