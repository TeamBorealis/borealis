//! Review M1 of issue 5: Aurora never restricts elements of the types Ability Score Improvement,
//! Level, Multiclass, Skill and Support (`SourcesManager.GetUndefinedSourceNames` leaves them out
//! of every source item). A 2024 character with the 2014 Player's Handbook unchecked still gets
//! the ability score choice of a Tasha's-style feat whose select lists the PHB's ASI elements.

use borealis_content::{ContentFile, ContentStoreBuilder, PackOrigin, Ruleset};
use borealis_engine::commands;
use borealis_engine::state::CharacterState;
use borealis_engine::Engine;
use serde_json::json;
use std::path::Path;

const PHB: &str = "Player’s Handbook";
const FEAT: &str = "ID_TEST_FEAT_CHEFLIKE";
const SELECT: &str = "Ability Score Increase (Cheflike)";

fn engine() -> Engine {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let asi = |key: &str, name: &str| {
        json!({"id": format!("ID_PHB_FEAT_ASI_{key}"), "name": name, "type": "Ability Score Improvement",
               "source": PHB, "rules": [{"kind": "stat", "name": name.to_lowercase(), "value": "1"}]})
    };
    let elements = vec![
        json!({"id": "ID_TEST_SOURCE_PHB", "name": PHB, "type": "Source", "source": "Core",
               "setters": [{"name": "abbreviation", "value": "PHB"}]}),
        asi("CONSTITUTION", "Constitution"),
        asi("WISDOM", "Wisdom"),
        json!({"id": FEAT, "name": "Cheflike", "type": "Feat", "source": "Test Cauldron",
               "rules": [{"kind": "select", "type": "Ability Score Improvement", "name": SELECT,
                          "supports": "ID_PHB_FEAT_ASI_CONSTITUTION|ID_PHB_FEAT_ASI_WISDOM"}]}),
        json!({"id": "ID_PHB_FEAT_RESTRICTED", "name": "Restricted Feat", "type": "Feat", "source": PHB}),
    ];
    let file: ContentFile = serde_json::from_value(
        json!({"format": "borealis-content/1", "ruleset": "dnd2014", "elements": elements}),
    )
    .expect("content");
    let store = ["core", "srd-5.1", "srd-5.2.1"]
        .iter()
        .fold(ContentStoreBuilder::new(), |b, p| {
            b.add_pack_dir(&root.join(p)).expect("pack")
        })
        .add_file("cheflike.json", PackOrigin::Custom, file)
        .build();
    Engine::new(&store)
}

fn options(e: &Engine, s: &CharacterState) -> Vec<String> {
    let snap = e.derive_with(s, true);
    let slot = snap
        .slots
        .iter()
        .find(|k| k.name == SELECT)
        .expect("the feat's select");
    slot.options.iter().map(|o| o.id.clone()).collect()
}

#[test]
fn ability_score_options_of_an_unchecked_book_stay_offered() {
    let e = engine();
    for restricted in [PHB, "player’s handbook"] {
        let mut s = commands::new_character(&e, Ruleset::Dnd2024);
        s.allow_cross_edition = Some(true);
        s.restricted_sources = vec![restricted.to_string()];
        // the feat as a root pick (as the Additional Feat item or a feat select would bring it)
        s.options.push(FEAT.into());
        assert_eq!(
            options(&e, &s),
            vec!["ID_PHB_FEAT_ASI_CONSTITUTION", "ID_PHB_FEAT_ASI_WISDOM"],
            "{restricted}"
        );
        // a feat of the unchecked book stays restricted (any casing)
        let snap = e.derive_with(&s, true);
        let feat_slot = snap.slots.iter().find(|k| k.ty == "Feat");
        if let Some(slot) = feat_slot {
            assert!(
                slot.options
                    .iter()
                    .all(|o| o.id != "ID_PHB_FEAT_RESTRICTED"),
                "{restricted}"
            );
        }
    }
}
