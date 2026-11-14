//! WB-22 (homebrew creator copies, spec 11, 12.3, E2E scenario 7) through the real `AppState` on a temporary data
//! folder with the bundled content:
//!
//! * a copy of the SRD Fireball, edited and saved, loads as a spell of its own (the original unchanged);
//! * a deep copy of the SRD 2024 Wizard saves with its features, package, Spellcasting feature and subclasses, every
//!   reference pointing at the copies (no dangling reference, no original named); a new character takes it and levels
//!   1-20 with the copied features and the copied subclass, and both classes are offered;
//! * a shallow copy grants the original features;
//! * a copy of an imported (non-SRD) element marks the pack not redistributable; "Start from a template" lists bundled
//!   elements only;
//! * Duplicate of a saved class is deep too.
//!
//! It also writes the web mock's copy sources (`ui/src/api/mock/homebrew-copies.ts`, `BOREALIS_UPDATE_SNAPSHOTS=1`).

use borealis_authoring::form::{to_ui, FormState};
use borealis_content::Ruleset as ContentRuleset;
use borealis_engine::commands::{self, slot_key};
use borealis_engine::{CharacterState, Choice, Engine};
use borealis_lib::content_dto::ContentProgress;
use borealis_lib::content_packs::EventSink;
use borealis_lib::dto::Ruleset;
use borealis_lib::homebrew_dto::{
    HomebrewDraftView, HomebrewPackRequest, HomebrewPageRequest, HomebrewPickerFilters,
    HomebrewPickerRow, HomebrewPickerTarget,
};
use borealis_lib::state::{AppPaths, AppState};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

const WIZARD: &str = "ID_2024_CLASS_WIZARD";
const FIREBALL: &str = "ID_2024_SPELL_FIREBALL";
const COPY: &str = "ID_EI_CLASS_WIZARD_COPY";

struct Quiet;
impl EventSink for Quiet {
    fn content_progress(&self, _: &ContentProgress) {}
    fn import_progress(&self, _: &ContentProgress) {}
    fn content_reloaded(&self, _: &str) {}
}

struct App {
    data: tempfile::TempDir,
    state: AppState,
    pack: String,
}

fn paths(data: &Path) -> AppPaths {
    AppPaths {
        content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
        data: data.to_path_buf(),
    }
}

impl App {
    fn new() -> Self {
        Self::with_data(tempfile::tempdir().unwrap())
    }

    fn with_data(data: tempfile::TempDir) -> Self {
        let state = AppState::new(paths(data.path())).unwrap();
        let pack = state
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
            .id;
        Self { data, state, pack }
    }

    fn copy(&self, kind: &str, id: &str, deep: bool) -> HomebrewDraftView {
        self.state
            .homebrew_new_draft(&self.pack, kind, Some(id), None, Some(deep))
            .unwrap()
    }

    fn read(&self, file: &str) -> Value {
        let path = self.data.path().join("library").join(&self.pack).join(file);
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    fn saved(&self) -> Vec<Value> {
        self.read("content.json")["elements"]
            .as_array()
            .unwrap()
            .clone()
    }

    fn saved_one(&self, id: &str) -> Value {
        self.saved()
            .into_iter()
            .find(|e| e["id"] == id)
            .unwrap_or_else(|| panic!("{id} saved"))
    }

    fn rows(&self, ty: &str, query: &str, srd_only: bool) -> Vec<HomebrewPickerRow> {
        self.state
            .homebrew_reference_options(
                &HomebrewPickerTarget {
                    types: vec![ty.into()],
                    tags: None,
                    multiple: None,
                },
                query,
                &HomebrewPickerFilters {
                    srd_only: Some(srd_only),
                    ..HomebrewPickerFilters::default()
                },
                HomebrewPageRequest {
                    offset: 0,
                    limit: 50,
                },
                None,
            )
            .unwrap()
            .items
    }
}

fn errors(v: &HomebrewDraftView) -> Vec<(String, String)> {
    v.problems
        .iter()
        .filter(|p| p.severity == "error")
        .map(|p| (p.path.clone().unwrap_or_default(), p.message.clone()))
        .collect()
}

fn original(app: &App, id: &str) -> Value {
    let hub = app.state.hub().unwrap();
    serde_json::to_value(hub.store.element(id).expect("loaded")).unwrap()
}

/// A 2024 character of `class` at level 1.
fn first_level(e: &Engine, class: &str) -> CharacterState {
    let mut s = commands::new_character(e, ContentRuleset::Dnd2024);
    let snap = e.derive(&s);
    let key = slot_key(&snap, "ID_LEVEL_1", "Class", 1).expect("class slot");
    s = commands::select(e, &s, &key, Choice::Element(class.into()))
        .unwrap()
        .state;
    commands::set_base_scores(e, &s, [8, 14, 13, 15, 12, 10])
        .unwrap()
        .state
}

fn element_ids(e: &Engine, s: &CharacterState) -> Vec<String> {
    e.derive(s)
        .elements
        .into_iter()
        .map(|x| x.element.id)
        .collect()
}

/// The open slot whose options include `id`: its key and option ids.
fn slot_offering(
    e: &Engine,
    s: &CharacterState,
    ty: &str,
) -> Option<(borealis_engine::SlotKey, String, Vec<String>)> {
    e.derive(s).slots.into_iter().find_map(|x| {
        let ids: Vec<String> = x.options.iter().map(|o| o.id.clone()).collect();
        ids.iter()
            .any(|i| i.contains(ty))
            .then_some((x.key, x.name, ids))
    })
}

#[test]
fn a_copied_srd_fireball_edited_and_saved_loads_as_its_own_spell() {
    let app = App::new();
    let before = original(&app, FIREBALL);
    let v = app.copy("spell", FIREBALL, false);
    assert_eq!(v.form.kind, "spell");
    assert_eq!(v.form.fields["name"], json!("Fireball (copy)"));
    assert_eq!(
        v.form.fields["edition"],
        json!("dnd2024"),
        "it keeps its ruleset"
    );
    assert_eq!(v.form.fields["source"], json!("Ember Isles"));
    assert!(
        !v.form
            .preserved
            .iter()
            .any(|p| p.key == "data.higher_level"),
        "the unbound 2024 mirror key is dropped: {:?}",
        v.form.preserved
    );
    assert_eq!(errors(&v), Vec::<(String, String)>::new());
    let mut form = v.form.clone();
    form.fields.insert("name".into(), json!("Emberball"));
    form.fields.insert("range".into(), json!("120 feet"));
    let v = app.state.homebrew_update_draft(&v.draft_id, &form).unwrap();
    assert_eq!(errors(&v), Vec::<(String, String)>::new());
    let saved = app.state.homebrew_save_draft(&v.draft_id, &Quiet).unwrap();
    assert_eq!(saved.element_ids, ["ID_EI_SPELL_EMBERBALL"]);
    let spell = app.saved_one("ID_EI_SPELL_EMBERBALL");
    assert_eq!(spell["ruleset"], "dnd2024");
    assert_eq!(spell["source"], "Ember Isles");
    assert_eq!(spell["supports"], json!(["Sorcerer", "Wizard"]));
    assert!(spell["data"].get("higher_level").is_none());
    assert_eq!(spell["data"]["level"], before["data"]["level"]);
    assert_eq!(
        spell["description"], before["description"],
        "the description is the original's"
    );
    let range = spell["setters"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "range")
        .expect("range");
    assert_eq!(range["value"], "120 feet");
    // a separate spell: both loaded, the original unchanged
    let hub = app.state.hub().unwrap();
    assert_eq!(
        hub.store.element("ID_EI_SPELL_EMBERBALL").unwrap().name,
        "Emberball"
    );
    assert_eq!(original(&app, FIREBALL), before);
    let rows = app.rows("Spell", "ball", false);
    let names: Vec<&str> = rows.iter().map(|r| r.name.as_str()).collect();
    assert!(
        names.contains(&"Emberball") && names.contains(&"Fireball"),
        "{names:?}"
    );
    // provenance: SRD text, so the pack stays redistributable
    let creator = app.read("creator.json");
    assert_eq!(
        creator["elements"]["ID_EI_SPELL_EMBERBALL"]["copiedFrom"],
        json!({"id": FIREBALL, "packId": "srd-5.2.1", "srd": true})
    );
    assert_eq!(
        creator["elements"]["ID_EI_SPELL_EMBERBALL"]["nonSrdText"],
        false
    );
    let pack = app
        .state
        .list_packs()
        .unwrap()
        .into_iter()
        .find(|p| p.id == app.pack)
        .unwrap();
    assert_ne!(pack.redistributable, Some(false));
}

#[test]
fn a_deep_copied_srd_wizard_levels_to_20_with_its_own_features_and_subclass() {
    let app = App::new();
    let before = original(&app, WIZARD);
    let v = app.copy("class", WIZARD, true);
    assert_eq!(v.form.fields["name"], json!("Wizard (copy)"));
    assert_eq!(v.form.fields["edition"], json!("dnd2024"));
    // the class editor names the subclasses saved with it (they are not on screen)
    assert_eq!(v.saved_with, ["Evoker"]);
    let children: Vec<&str> = v
        .form
        .children
        .iter()
        .map(|c| c.fields["name"].as_str().unwrap_or_default())
        .collect();
    assert_eq!(
        children,
        [
            "Starting Equipment (Wizard)",
            "Spellcasting",
            "Ritual Adept",
            "Arcane Recovery",
            "Scholar",
            "Wizard Subclass",
            "Ability Score Improvement",
            "Memorize Spell",
            "Spell Mastery",
            "Epic Boon",
            "Signature Spells"
        ]
    );
    assert!(
        v.form.fields["levels"]
            .as_array()
            .unwrap()
            .iter()
            .all(|e| e.get("child").is_some()),
        "the LEVELS grid lists the copies: {}",
        v.form.fields["levels"]
    );
    assert_eq!(errors(&v), Vec::<(String, String)>::new());
    let deep: Vec<(String, String)> = app
        .state
        .homebrew_check_draft(&v.draft_id)
        .unwrap()
        .into_iter()
        .filter(|p| p.severity != "info")
        .map(|p| (p.code, p.message))
        .collect();
    assert_eq!(deep, Vec::<(String, String)>::new());
    let saved = app.state.homebrew_save_draft(&v.draft_id, &Quiet).unwrap();
    assert_eq!(saved.element_ids[0], COPY);
    for id in [
        "ID_EI_EQUIPMENT_PACKAGE_WIZARD_COPY_STARTING_EQUIPMENT_WIZARD",
        "ID_EI_CLASS_FEATURE_WIZARD_COPY_SPELLCASTING",
        "ID_EI_CLASS_FEATURE_WIZARD_COPY_WIZARD_SUBCLASS",
        "ID_EI_SUBCLASS_EVOKER",
        "ID_EI_SUBCLASS_FEATURE_EVOKER_OVERCHANNEL",
    ] {
        assert!(
            saved.element_ids.iter().any(|s| s == id),
            "{id} in {:?}",
            saved.element_ids
        );
    }
    assert_eq!(saved.element_ids.len(), 18, "{:?}", saved.element_ids);
    // no saved element names an original part of the Wizard (the grants, the package, the subclass choice, the
    // multiclass section): every reference is to a copy, and the validator found nothing dangling
    let text = serde_json::to_string(&app.saved()).unwrap();
    for original in [
        "ID_2024_CLASS_FEATURE_WIZARD",
        "ID_2024_SUBCLASS",
        "ID_2024_EQUIPMENT_PACKAGE_CLASS_WIZARD",
        "ID_2024_MULTICLASS_WIZARD",
        "ID_2024_CLASS_WIZARD\"",
    ] {
        assert!(!text.contains(original), "{original} named in {text}");
    }
    let class = app.saved_one(COPY);
    assert!(
        class["data"].get("features_by_level").is_none(),
        "a mirror key"
    );
    assert!(class["data"].get("hit_die").is_some(), "a bound key");
    let choice = app.saved_one("ID_EI_CLASS_FEATURE_WIZARD_COPY_WIZARD_SUBCLASS");
    assert_eq!(choice["rules"][0]["supports"], "Wizard copy Subclass");
    let evoker = app.saved_one("ID_EI_SUBCLASS_EVOKER");
    assert_eq!(evoker["supports"], json!(["Wizard copy Subclass"]));
    assert_eq!(evoker["data"]["class"], COPY);
    let creator = app.read("creator.json");
    assert_eq!(
        creator["elements"]["ID_EI_SUBCLASS_EVOKER"]["copiedFrom"],
        json!({"id": "ID_2024_SUBCLASS_WIZARD_EVOKER", "packId": "srd-5.2.1", "srd": true})
    );
    assert_eq!(original(&app, WIZARD), before, "the original is unchanged");

    // a new character takes the copy and levels 1-20 with the copied features and subclass
    let hub = app.state.hub().unwrap();
    let e = &hub.engine;
    let classes: Vec<String> = e
        .derive(&commands::new_character(e, ContentRuleset::Dnd2024))
        .slots
        .into_iter()
        .find(|x| x.name.starts_with("Class"))
        .expect("the class slot")
        .options
        .into_iter()
        .map(|o| o.id)
        .collect();
    assert!(
        classes.iter().any(|c| c == WIZARD) && classes.iter().any(|c| c == COPY),
        "both are offered: {classes:?}"
    );
    let mut s = first_level(e, COPY);
    assert_eq!(
        e.derive(&s).spellcasting[0].slots[0],
        2,
        "two 1st-level slots"
    );
    while s.level() < 3 {
        s = commands::level_up_main(e, &s, None).unwrap().state;
    }
    let (key, name, options) = slot_offering(e, &s, "SUBCLASS").expect("the subclass choice");
    assert!(
        options.iter().any(|o| o == "ID_EI_SUBCLASS_EVOKER")
            && !options.iter().any(|o| o.starts_with("ID_2024_")),
        "{name}: {options:?}"
    );
    s = commands::select(e, &s, &key, Choice::Element("ID_EI_SUBCLASS_EVOKER".into()))
        .unwrap()
        .state;
    while s.level() < 20 {
        s = commands::level_up_main(e, &s, None).unwrap().state;
    }
    assert_eq!(e.derive(&s).level, 20);
    let got = element_ids(e, &s);
    for want in [
        COPY,
        "ID_EI_CLASS_FEATURE_WIZARD_COPY_SPELLCASTING",
        "ID_EI_CLASS_FEATURE_WIZARD_COPY_SIGNATURE_SPELLS",
        "ID_EI_SUBCLASS_EVOKER",
        "ID_EI_SUBCLASS_FEATURE_EVOKER_OVERCHANNEL",
    ] {
        assert!(got.iter().any(|g| g == want), "{want} at level 20");
    }
    assert!(
        !got.iter()
            .any(|g| g.starts_with("ID_2024_CLASS_FEATURE_WIZARD")
                || g.starts_with("ID_2024_SUBCLASS")),
        "no original feature: {got:?}"
    );
    let snap = e.derive(&s);
    assert_eq!(snap.spellcasting[0].slots[8], 1, "a 9th-level slot at 20");

    // the original Wizard never offers the copy's subclass
    let mut w = first_level(e, WIZARD);
    while w.level() < 3 {
        w = commands::level_up_main(e, &w, None).unwrap().state;
    }
    let (_, _, options) = slot_offering(e, &w, "SUBCLASS").expect("the Wizard's subclass choice");
    assert!(options
        .iter()
        .any(|o| o == "ID_2024_SUBCLASS_WIZARD_EVOKER"));
    assert!(
        !options.iter().any(|o| o.starts_with("ID_EI_")),
        "{options:?}"
    );

    // Duplicate (Ctrl+D) of the saved copy is deep too: its own features and subclass
    let d = app
        .state
        .homebrew_duplicate_element(&app.pack, COPY)
        .unwrap();
    assert_eq!(d.form.fields["name"], json!("Wizard (copy) (copy)"));
    assert_eq!(d.form.children.len(), 11);
    let mut form = d.form.clone();
    form.fields.insert("name".into(), json!("Battle Mage"));
    let d = app.state.homebrew_update_draft(&d.draft_id, &form).unwrap();
    let saved = app.state.homebrew_save_draft(&d.draft_id, &Quiet).unwrap();
    assert_eq!(saved.element_ids[0], "ID_EI_CLASS_BATTLE_MAGE");
    assert!(
        saved
            .element_ids
            .iter()
            .any(|s| s == "ID_EI_SUBCLASS_EVOKER_2"),
        "{:?}",
        saved.element_ids
    );
    assert_eq!(
        app.saved_one("ID_EI_SUBCLASS_EVOKER_2")["supports"],
        json!(["Battle Mage Subclass"])
    );
    assert_eq!(
        app.saved_one("ID_EI_SUBCLASS_EVOKER")["supports"],
        json!(["Wizard copy Subclass"]),
        "the first copy keeps its own"
    );
}

#[test]
fn a_shallow_copy_grants_the_original_features() {
    let app = App::new();
    let v = app.copy("class", WIZARD, false);
    assert!(v.form.children.is_empty());
    let saved = app.state.homebrew_save_draft(&v.draft_id, &Quiet).unwrap();
    assert_eq!(saved.element_ids, [COPY]);
    let class = app.saved_one(COPY);
    let grants: Vec<&str> = class["rules"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["type"] == "Class Feature")
        .map(|r| r["id"].as_str().unwrap())
        .collect();
    assert!(
        grants.contains(&"ID_2024_CLASS_FEATURE_WIZARD_SPELLCASTING"),
        "{grants:?}"
    );
    assert!(grants
        .iter()
        .all(|g| g.starts_with("ID_2024_CLASS_FEATURE_WIZARD_")));
    let hub = app.state.hub().unwrap();
    let e = &hub.engine;
    let s = first_level(e, COPY);
    let got = element_ids(e, &s);
    assert!(
        got.iter()
            .any(|g| g == "ID_2024_CLASS_FEATURE_WIZARD_ARCANE_RECOVERY"),
        "{got:?}"
    );
}

#[test]
fn a_species_and_backgrounds_copy_their_parts() {
    let app = App::new();
    // the 2024 Elf: its traits and the lineage options, offered by the copied choice only
    let v = app.copy("species", "ID_2024_SPECIES_ELF", true);
    assert_eq!(v.form.children.len(), 5);
    assert_eq!(errors(&v), Vec::<(String, String)>::new());
    let saved = app.state.homebrew_save_draft(&v.draft_id, &Quiet).unwrap();
    assert_eq!(saved.element_ids.len(), 9, "{:?}", saved.element_ids);
    let text = serde_json::to_string(&app.saved()).unwrap();
    assert!(
        !text.contains("ID_2024_TRAIT_ELF") && !text.contains("ID_2024_OPTION_ELVEN"),
        "{text}"
    );
    let elf = app.saved_one("ID_EI_SPECIES_ELF_COPY");
    assert!(elf["rules"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["id"] == "ID_EI_TRAIT_ELF_COPY_ELVEN_LINEAGE"));
    let lineage = app.saved_one("ID_EI_TRAIT_ELF_COPY_ELVEN_LINEAGE");
    let offered = lineage["rules"][0]["supports"].as_str().unwrap();
    assert_eq!(offered.split('|').count(), 3, "{offered}");
    assert!(offered
        .split('|')
        .all(|id| id.starts_with("ID_EI_TRAIT_OPTION_ELVEN_LINEAGE_")));
    let drow = app.saved_one("ID_EI_TRAIT_OPTION_ELVEN_LINEAGE_DROW");
    assert_eq!(
        drow["supports"],
        json!(null),
        "the SRD Elf never offers it: {drow}"
    );
    let hub = app.state.hub().unwrap();
    let srd = hub
        .store
        .element("ID_2024_TRAIT_ELF_ELVEN_LINEAGE")
        .unwrap();
    assert!(serde_json::to_string(&srd.rules)
        .unwrap()
        .contains("\"supports\":\"Elven Lineage\""));
    drop(hub);

    // the 2024 Acolyte: its starting-equipment package; the 2014 Acolyte: its background feature
    let v = app.copy("background", "ID_2024_BACKGROUND_ACOLYTE", true);
    let saved = app.state.homebrew_save_draft(&v.draft_id, &Quiet).unwrap();
    assert_eq!(saved.element_ids.len(), 2, "{:?}", saved.element_ids);
    let acolyte = app.saved_one(&saved.element_ids[0]);
    let package = &saved.element_ids[1];
    assert_eq!(acolyte["data"]["equipment_package"], json!(package));
    assert_eq!(
        app.saved_one(package)["data"]["owner"],
        json!(saved.element_ids[0])
    );
    let v = app.copy("background", "ID_BACKGROUND_ACOLYTE", true);
    assert_eq!(v.form.fields["edition"], json!("dnd2014"));
    let saved = app.state.homebrew_save_draft(&v.draft_id, &Quiet).unwrap();
    assert_eq!(saved.element_ids.len(), 2, "{:?}", saved.element_ids);
    let text = serde_json::to_string(&app.saved_one(&saved.element_ids[0])).unwrap();
    assert!(
        text.contains(&saved.element_ids[1]) && !text.contains("ID_BACKGROUND_FEATURE_SHELTER"),
        "{text}"
    );
}

/// An imported book (the user's own, not SRD): its spell has a description.
fn with_imported_book() -> App {
    let data = tempfile::tempdir().unwrap();
    let dir = data.path().join("library").join("imported-book");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("pack.json"),
        json!({"format": "borealis-pack/1", "id": "imported-book", "name": "Gloom Tome", "version": "1",
               "origin": "custom", "rulesets": ["dnd2024"], "files": [{"path": "content.json", "kind": "content"}]})
        .to_string(),
    )
    .unwrap();
    std::fs::write(
        dir.join("content.json"),
        json!({"format": "borealis-content/1", "elements": [
            {"id": "ID_GT_SOURCE_GLOOM_TOME", "name": "Gloom Tome", "type": "Source", "source": "Gloom Tome",
             "setters": [{"name": "abbreviation", "value": "GT"}, {"name": "url", "value": ""}]},
            {"id": "ID_GT_SPELL_SHADOW_BOLT", "name": "Shadow Bolt", "type": "Spell", "source": "Gloom Tome",
             "description": "<p>A bolt of shadow.</p>",
             "setters": [{"name": "level", "value": "1"}, {"name": "school", "value": "Evocation"},
                         {"name": "time", "value": "1 action"}, {"name": "range", "value": "60 feet"},
                         {"name": "duration", "value": "Instantaneous"}, {"name": "hasVerbalComponent", "value": "true"}]}]})
        .to_string(),
    )
    .unwrap();
    App::with_data(data)
}

#[test]
fn a_copy_of_imported_text_marks_the_pack_not_redistributable() {
    let app = with_imported_book();
    // "Start from a template" lists bundled SRD elements only; "New from copy..." lists every loaded element, with the
    // user's own source name
    assert!(app.rows("Spell", "shadow bolt", true).is_empty());
    let rows = app.rows("Spell", "shadow bolt", false);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].source_name, "Gloom Tome");
    assert!(app
        .rows("Spell", "fireball", true)
        .iter()
        .any(|r| r.id == FIREBALL));
    let pack_before = app
        .state
        .list_packs()
        .unwrap()
        .into_iter()
        .find(|p| p.id == app.pack)
        .unwrap();
    assert_ne!(pack_before.redistributable, Some(false));

    let v = app.copy("spell", "ID_GT_SPELL_SHADOW_BOLT", false);
    assert_eq!(v.form.fields["name"], json!("Shadow Bolt (copy)"));
    let saved = app.state.homebrew_save_draft(&v.draft_id, &Quiet).unwrap();
    let id = &saved.element_ids[0];
    let creator = app.read("creator.json");
    assert_eq!(
        creator["elements"][id]["copiedFrom"],
        json!({"id": "ID_GT_SPELL_SHADOW_BOLT", "packId": "imported-book", "srd": false})
    );
    assert_eq!(creator["elements"][id]["nonSrdText"], true);
    let pack = app
        .state
        .list_packs()
        .unwrap()
        .into_iter()
        .find(|p| p.id == app.pack)
        .unwrap();
    assert_eq!(pack.redistributable, Some(false));
}

#[test]
fn a_copy_of_another_edition_warns_and_a_class_keeps_its_edition() {
    let app = App::new();
    // a 2014 spell in this 2024 pack keeps its ruleset and is adapted when taken
    let v = app.copy("spell", "ID_PHB_SPELL_FIREBALL", false);
    assert_eq!(v.form.fields["edition"], json!("dnd2014"));
    let warned: Vec<&str> = v
        .problems
        .iter()
        .filter(|p| p.code == "form.edition_adapted")
        .map(|p| p.message.as_str())
        .collect();
    assert_eq!(
        warned,
        ["2014 content in a 2024 pack is adapted when a character takes it."]
    );
    // a class never changes edition (R4)
    let v = app.copy("class", WIZARD, false);
    let mut form = v.form.clone();
    form.fields.insert("edition".into(), json!("dnd2014"));
    let v = app.state.homebrew_update_draft(&v.draft_id, &form).unwrap();
    assert!(
        errors(&v)
            .iter()
            .any(|(path, m)| path == "fields.edition" && m.starts_with("A class keeps its edition")),
        "{:?}",
        errors(&v)
    );
}

/// The web mock's copy sources: the picker rows, the forms and the DATA views the real backend answers for a copy of
/// the SRD Fireball, of the 2024 Wizard (with its features, and its subclasses saved with it), and of the SRD weapons
/// and armor the mock's sample content names (H10ac: "Start from a template" finds them, and "New from copy" of a
/// sample weapon or armor carries the real form), so the web e2e copies real forms.
#[test]
fn copies_match_the_mock() {
    let app = App::new();
    let mut sources = Vec::new();
    let items: Vec<(&str, String, String, &str, bool)> = [
        ("Weapon", "Dagger"),
        ("Weapon", "Handaxe"),
        ("Weapon", "Mace"),
        ("Weapon", "Spear"),
        ("Weapon", "Longsword"),
        ("Weapon", "Greatsword"),
        ("Weapon", "Rapier"),
        ("Weapon", "Shortbow"),
        ("Armor", "Leather Armor"),
        ("Armor", "Chain Shirt"),
        ("Armor", "Chain Mail"),
        ("Armor", "Plate Armor"),
        ("Armor", "Shield"),
    ]
    .into_iter()
    .map(|(ty, name)| {
        let id = format!(
            "ID_2024_{}_{}",
            ty.to_uppercase(),
            name.to_uppercase().replace(' ', "_")
        );
        (
            ty,
            name.to_lowercase(),
            id,
            if ty == "Weapon" { "weapon" } else { "armor" },
            false,
        )
    })
    .collect();
    let named = [
        (
            "Spell",
            "fireball".to_string(),
            FIREBALL.to_string(),
            "spell",
            false,
        ),
        (
            "Class",
            "wizard".to_string(),
            WIZARD.to_string(),
            "class",
            true,
        ),
    ];
    for (ty, query, id, kind, deep) in named.into_iter().chain(items) {
        let (query, id) = (query.as_str(), id.as_str());
        let row = app
            .rows(ty, query, true)
            .into_iter()
            .find(|r| r.id == id)
            .expect("row");
        let copied = app.copy(kind, id, false);
        let (form, generated) = (copied.form, copied.generated);
        let deep_copy = if deep {
            let v = app.copy(kind, id, true);
            // the first edit writes the draft file, which holds what is saved with the copy
            let v = app
                .state
                .homebrew_update_draft(&v.draft_id, &v.form)
                .unwrap();
            let file: Value = serde_json::from_str(
                &std::fs::read_to_string(
                    app.data
                        .path()
                        .join("homebrew-drafts")
                        .join(format!("{}.json", v.draft_id)),
                )
                .unwrap(),
            )
            .unwrap();
            let companions: Vec<FormState> =
                serde_json::from_value(file["copy"]["companions"].clone()).unwrap();
            json!({"form": v.form, "companions": companions.iter().map(to_ui).collect::<Vec<_>>()})
        } else {
            Value::Null
        };
        sources.push(json!({"row": row, "form": form, "generated": generated, "deep": deep_copy}));
    }
    let text = format!(
        "// Generated by src-tauri/tests/homebrew_copy.rs from the real copies (WB-22). Do not edit:\n\
         // run that test with BOREALIS_UPDATE_SNAPSHOTS=1.\n\
         import type {{ HomebrewForm, HomebrewGeneratedView, HomebrewPickerRow }} from \"../types/homebrew\";\n\n\
         /** SRD elements the web mock can copy: the picker row, the copy's form and DATA view, and a deep copy (with the\n\
         \x20* forms saved with it) for a class. */\n\
         export interface MockCopySource {{\n  row: HomebrewPickerRow;\n  form: HomebrewForm;\n  \
         generated: HomebrewGeneratedView;\n  \
         deep: {{ form: HomebrewForm; companions: HomebrewForm[] }} | null;\n}}\n\n\
         export const MOCK_COPY_SOURCES: MockCopySource[] = {};\n",
        serde_json::to_string_pretty(&sources).unwrap()
    );
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../ui/src/api/mock/homebrew-copies.ts");
    if std::env::var_os("BOREALIS_UPDATE_SNAPSHOTS").is_some() {
        std::fs::write(&path, &text).unwrap();
    }
    let on_disk = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        on_disk == text,
        "ui/src/api/mock/homebrew-copies.ts is stale: run this test with BOREALIS_UPDATE_SNAPSHOTS=1"
    );
}
