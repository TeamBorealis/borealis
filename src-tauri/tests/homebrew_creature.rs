//! WB-20 (homebrew creator creatures, spec 4.11) through the real `AppState` on a temporary data folder with the
//! bundled content:
//!
//! * a new 2024 creature says what it still needs at its fields (the creature type, then the armor class and hit
//!   points as the stat block validator finds them), and an XP of its own is the non-standard XP warning;
//! * made in the form (AC, HP dice, speeds, scores, CR, one attack action), it validates clean, saves as a typed
//!   `Monster` stat block, is listed in the compendium as a monster and reads there as a stat block (the same entry the
//!   draft's live preview showed), and a level-2 Druid is offered it as a Wild Shape form;
//! * it opens again in the creature form and saves unchanged.

use borealis_content::{MonsterData, Ruleset as ContentRuleset};
use borealis_engine::commands::{self, slot_key};
use borealis_engine::{CharacterState, Choice, Engine};
use borealis_lib::content_dto::ContentProgress;
use borealis_lib::content_packs::EventSink;
use borealis_lib::dto::Ruleset;
use borealis_lib::dto_ext::CompendiumQuery;
use borealis_lib::homebrew_dto::{HomebrewDraftView, HomebrewPackRequest};
use borealis_lib::state::{AppPaths, AppState};
use serde_json::{json, Value};
use std::path::PathBuf;

const BOAR: &str = "ID_EI_CREATURE_CINDER_BOAR";
const LEVEL1: &str = "ID_LEVEL_1";

struct Quiet;
impl EventSink for Quiet {
    fn content_progress(&self, _: &ContentProgress) {}
    fn import_progress(&self, _: &ContentProgress) {}
    fn content_reloaded(&self, _: &str) {}
}

struct App {
    data: tempfile::TempDir,
    state: AppState,
}

impl App {
    fn new() -> Self {
        let data = tempfile::tempdir().unwrap();
        let state = AppState::new(AppPaths {
            content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
            data: data.path().to_path_buf(),
        })
        .unwrap();
        Self { data, state }
    }

    fn pack(&self) -> String {
        self.state
            .homebrew_create_pack(
                &HomebrewPackRequest {
                    name: "Ember Isles".into(),
                    code: "EI".into(),
                    edition: Ruleset::Dnd2024,
                    author: None,
                    description: None,
                    license: None,
                },
                &Quiet,
            )
            .unwrap()
            .id
    }

    fn set(&self, v: &HomebrewDraftView, fields: &[(&str, Value)]) -> HomebrewDraftView {
        let mut form = v.form.clone();
        for (k, value) in fields {
            form.fields.insert((*k).to_string(), value.clone());
        }
        self.state
            .homebrew_update_draft(&v.draft_id, &form)
            .unwrap()
    }

    fn saved(&self, pack: &str, id: &str) -> Value {
        let path = self
            .data
            .path()
            .join("library")
            .join(pack)
            .join("content.json");
        let content: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        content["elements"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["id"] == id)
            .unwrap_or_else(|| panic!("{id} saved"))
            .clone()
    }
}

fn problems(v: &HomebrewDraftView, severity: &str) -> Vec<(String, String, String)> {
    let mut out: Vec<(String, String, String)> = v
        .problems
        .iter()
        .filter(|p| p.severity == severity)
        .map(|p| {
            (
                p.path.clone().unwrap_or_default(),
                p.code.clone(),
                p.message.clone(),
            )
        })
        .collect();
    out.sort();
    out
}

fn pick(e: &Engine, s: &CharacterState, name: &str, id: &str) -> CharacterState {
    let snap = e.derive(s);
    let key = slot_key(&snap, LEVEL1, name, 1).unwrap_or_else(|| panic!("no slot {name}"));
    commands::select(e, s, &key, Choice::Element(id.into()))
        .unwrap_or_else(|err| panic!("select {id}: {err}"))
        .state
}

fn boar_fields() -> Vec<(&'static str, Value)> {
    vec![
        (
            "armorClass",
            json!({"entries": [{"value": 11, "source": "natural armor"}], "note": ""}),
        ),
        (
            "hitPoints",
            json!({"average": null, "formula": "2d8+2", "note": ""}),
        ),
        ("speed", json!({"walk": 40})),
        ("initiative", json!({"modifier": 0, "score": 10})),
        (
            "abilities",
            json!({"str": 13, "dex": 11, "con": 12, "int": 2, "wis": 9, "cha": 5}),
        ),
        (
            "challenge",
            json!({"cr": "1/4", "note": "", "xp": null, "xpLair": null, "proficiencyBonus": null}),
        ),
        (
            "actions",
            json!([{"name": "Tusk", "text": "Melee Attack Roll: +3, reach 5 ft. Hit: 4 (1d6 + 1) Slashing damage.",
                "attack": {"kind": "melee", "to_hit": 3, "reach": 5,
                    "damage": [{"average": 4, "dice": "1d6+1", "type": "slashing"}]}}]),
        ),
    ]
}

#[test]
fn a_new_creature_validates_shows_in_the_compendium_and_is_a_wild_shape_form() {
    let app = App::new();
    let pack = app.pack();
    let v = app
        .state
        .homebrew_new_draft(&pack, "creature", None, None, None)
        .unwrap();
    assert_eq!(v.form.kind, "creature");
    assert_eq!(v.form.fields["edition"], json!("dnd2024"));

    // what a new creature still needs, said at its fields
    let v = app.set(&v, &[("name", json!("Cinder Boar"))]);
    assert_eq!(
        problems(&v, "error"),
        [(
            "fields.creatureType".to_string(),
            "form.required".to_string(),
            "A creature needs a creature type.".to_string()
        )]
    );
    let v = app.set(&v, &[("creatureType", json!("beast"))]);
    assert_eq!(
        problems(&v, "error"),
        [
            (
                "fields.armorClass".to_string(),
                "invalid_monster_data".to_string(),
                "Give the creature an Armor Class above 0, or words that say how it is worked out."
                    .to_string()
            ),
            (
                "fields.hitPoints".to_string(),
                "invalid_monster_data".to_string(),
                "Give the creature hit points above 0, or words that say how they are worked out."
                    .to_string()
            ),
        ]
    );

    // made in the form: clean, and its live preview is the compendium's stat block
    let v = app.set(&v, &boar_fields());
    assert_eq!(problems(&v, "error"), Vec::new());
    assert_eq!(problems(&v, "warning"), Vec::new());
    let preview = v.preview.clone().expect("a preview");
    let facts = |f: &[borealis_lib::dto_ext::ElementFact]| -> Vec<(String, String)> {
        f.iter()
            .map(|f| (f.label.clone(), f.value.clone()))
            .collect()
    };
    for want in [
        ("Armor Class", "11 (natural armor)"),
        ("Hit Points", "11 (2d8+2)"),
        ("Challenge", "1/4 (50 XP)"),
    ] {
        let want = (want.0.to_string(), want.1.to_string());
        assert!(facts(&preview.facts).contains(&want), "{want:?}");
    }
    assert!(preview.description_html.contains("Tusk"));

    // an XP of its own: the non-standard XP warning at the challenge field
    let own = app.set(
        &v,
        &[(
            "challenge",
            json!({"cr": "1/4", "note": "", "xp": 60, "xpLair": null, "proficiencyBonus": null}),
        )],
    );
    let warnings = problems(&own, "warning");
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert_eq!(warnings[0].0, "fields.challenge");
    assert_eq!(warnings[0].1, "non_standard_monster_xp");
    let v = app.set(&own, &boar_fields());

    let saved = app.state.homebrew_save_draft(&v.draft_id, &Quiet).unwrap();
    assert_eq!(saved.element_ids, [BOAR]);
    let stored = app.saved(&pack, BOAR);
    assert_eq!(stored["type"], json!("Monster"));
    assert_eq!(stored["data"]["ruleset"], json!("dnd2024"));
    assert_eq!(
        stored["data"]["hp"],
        json!({"average": 11, "formula": "2d8+2"})
    );
    assert_eq!(stored["data"]["xp"], json!(50));
    assert_eq!(stored["data"]["proficiency_bonus"], json!(2));
    assert_eq!(stored["data"]["senses"], json!({"passive_perception": 9}));
    let el: borealis_content::Element = serde_json::from_value(stored.clone()).unwrap();
    let m = MonsterData::from_element(&el).expect("a typed stat block");
    assert!(m.validate().is_empty() && m.warnings().is_empty());

    // the compendium lists it as a monster and reads it as a stat block
    let page = app
        .state
        .search_compendium(&CompendiumQuery {
            query: "cinder".into(),
            types: Some(vec!["monster".into()]),
            sources: None,
            ruleset: None,
            limit: 20,
            offset: 0,
        })
        .unwrap();
    assert!(
        page.items.iter().any(|i| i.id == BOAR && i.ty == "monster"),
        "{:?}",
        page.items.iter().map(|i| &i.id).collect::<Vec<_>>()
    );
    let entry = app.state.get_element(BOAR).unwrap();
    assert_eq!(facts(&entry.facts), facts(&preview.facts));
    assert!(entry.description_html.contains("<h4>Actions</h4>"));

    // Wild Shape: a level-2 Druid may take its form (a Beast of CR 1/4 that doesn't fly)
    let hub = app.state.hub().unwrap();
    let e = &hub.engine;
    let mut s = commands::new_character(e, ContentRuleset::Dnd2024);
    s = pick(e, &s, "Race", "ID_2024_SPECIES_HUMAN");
    s = pick(e, &s, "Class", "ID_2024_CLASS_DRUID");
    s = commands::level_up_main(e, &s, None).expect("level 2").state;
    let snap = e.derive(&s);
    let ws = snap
        .wild_shape
        .as_ref()
        .expect("a level-2 Druid has Wild Shape");
    let form = ws
        .options
        .iter()
        .find(|f| f.id == BOAR)
        .unwrap_or_else(|| panic!("the boar is a form: {:?}", ws.options.len()));
    assert_eq!((form.ac, form.hp, form.cr.as_str()), (11, 11, "1/4"));
    assert_eq!(form.speed, "40 ft.");
    drop(hub);

    // it opens again in the creature form and saves unchanged
    let again = app
        .state
        .homebrew_open_draft(&pack, Some(BOAR), None)
        .unwrap();
    assert_eq!(again.form.kind, "creature");
    assert!(again.form.preserved.is_empty());
    let touched = app
        .state
        .homebrew_update_draft(&again.draft_id, &again.form)
        .unwrap();
    app.state
        .homebrew_save_draft(&touched.draft_id, &Quiet)
        .unwrap();
    assert_eq!(app.saved(&pack, BOAR), stored);
}
