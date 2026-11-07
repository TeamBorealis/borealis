//! B-191: Manage > Character ALIGNMENT and DEITY are the Alignment and Deity selection slots, as
//! in Aurora. A value naming an element the slot offers selects it (and the sheet prints it);
//! other text is kept as custom text that Manage and the sheet show; an Aurora import shows its
//! alignment in Manage; text from older files is kept. Responses are checked against the TS
//! contract.

mod contract_support;
use borealis_lib::dto::{
    AbilityMethodDto, CharacterSnapshot, NewCharacterRequest, OptionsPatch, Ruleset,
};
use borealis_lib::dto_ext::SheetOptionsDto;
use borealis_lib::state::{AppPaths, AppState};
use contract_support::Contract;
use serde_json::{json, Value};
use std::path::PathBuf;

const LAWFUL_GOOD: &str = "ID_ALIGNMENT_LAWFUL_GOOD";
const NEUTRAL: &str = "ID_ALIGNMENT_NEUTRAL";

struct App {
    state: AppState,
    contract: Contract,
    _dir: tempfile::TempDir,
}

impl App {
    fn new() -> Self {
        borealis_engine::invariants::enable_for_thread();
        let dir = tempfile::tempdir().expect("temp dir");
        let paths = AppPaths {
            content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
            data: dir.path().to_path_buf(),
        };
        Self {
            state: AppState::new(paths).expect("state"),
            contract: Contract::load(),
            _dir: dir,
        }
    }

    fn checked(&self, command: &str, snap: CharacterSnapshot) -> CharacterSnapshot {
        let v = serde_json::to_value(&snap).expect("json");
        let errors = self.contract.response(command, &v);
        assert!(errors.is_empty(), "{command}: {errors:?}");
        snap
    }

    fn hero(&self) -> CharacterSnapshot {
        let request = NewCharacterRequest {
            name: "Wren".into(),
            ruleset: Ruleset::Dnd2014,
            starting_level: 1,
            gender: String::new(),
            ability_method: AbilityMethodDto::Manual,
            options: OptionsPatch::default(),
            portrait: None,
            group: None,
        };
        let s = self
            .state
            .create_character(Ruleset::Dnd2014, &request)
            .expect("create");
        self.checked("create_character", s)
    }

    fn details(&self, id: &str, patch: Value) -> CharacterSnapshot {
        let s = self.state.update_details(id, &patch).expect("details");
        self.checked("update_details", s)
    }

    fn sheet(&self, id: &str) -> String {
        self.state
            .render_sheet(id, &SheetOptionsDto::default())
            .expect("sheet")
            .html
    }

    fn file(&self, id: &str) -> Value {
        self.state.save_character(id).expect("save");
        let path = self.state.character_path(id).expect("path");
        serde_json::from_str(&std::fs::read_to_string(path).expect("read")).expect("json")
    }
}

/// The element chosen in the slot `ty` (the level 1 "Alignment" and "Deity" rules).
fn chosen(s: &CharacterSnapshot, ty: &str) -> Option<String> {
    s.slots
        .iter()
        .find(|x| x.title == ty)
        .unwrap_or_else(|| panic!("no {ty} slot"))
        .chosen
        .as_ref()
        .map(|c| c.id.clone())
}

#[test]
fn manage_lists_the_alignment_elements_and_a_pick_selects_one_that_the_sheet_prints() {
    let app = App::new();
    let s = app.hero();
    let id = s.id.clone();
    assert_eq!(
        s.detail_choices.alignment,
        [
            "Lawful Good",
            "Neutral Good",
            "Chaotic Good",
            "Lawful Neutral",
            "Neutral",
            "Chaotic Neutral",
            "Lawful Evil",
            "Neutral Evil",
            "Chaotic Evil"
        ],
        "the Alignment slot's elements, in content order"
    );
    assert!(s.detail_choices.deity.is_empty(), "the SRD has no deities");
    assert!(!app.sheet(&id).contains("Lawful Good"));

    let s = app.details(&id, json!({"alignment": "Lawful Good"}));
    assert_eq!(chosen(&s, "Alignment").as_deref(), Some(LAWFUL_GOOD));
    assert_eq!(s.details.alignment, "Lawful Good");
    assert!(
        app.sheet(&id).contains("Lawful Good"),
        "the sheet prints it"
    );
    let file = app.file(&id);
    assert_eq!(
        file["ui"]["alignment"], "",
        "an element keeps no custom text"
    );

    // the old Manage label of Neutral, any case and spacing
    let s = app.details(&id, json!({"alignment": " true neutral "}));
    assert_eq!(chosen(&s, "Alignment").as_deref(), Some(NEUTRAL));
    assert_eq!(s.details.alignment, "Neutral");

    // empty clears the selection
    let s = app.details(&id, json!({"alignment": ""}));
    assert_eq!(chosen(&s, "Alignment"), None);
    assert_eq!(s.details.alignment, "");
}

#[test]
fn custom_text_empties_the_slot_and_the_sheet_prints_it() {
    let app = App::new();
    let id = app.hero().id;
    app.details(&id, json!({"alignment": "Lawful Good"}));

    let s = app.details(
        &id,
        json!({"alignment": "Chaotic Curious", "deity": "The Lantern Keeper"}),
    );
    assert_eq!(
        chosen(&s, "Alignment"),
        None,
        "custom text replaces the pick"
    );
    assert_eq!(s.details.alignment, "Chaotic Curious");
    assert_eq!(s.details.deity, "The Lantern Keeper");
    let html = app.sheet(&id);
    assert!(html.contains("Chaotic Curious"), "custom alignment printed");
    assert!(html.contains("The Lantern Keeper"), "custom deity printed");
    assert!(!html.contains("Lawful Good"));

    // kept through save and reopen
    app.state.save_character(&id).expect("save");
    app.state.close_character(&id, false).expect("close");
    let back = app.state.load_character(&id).expect("reopen");
    assert_eq!(back.details.alignment, "Chaotic Curious");
    assert_eq!(back.details.deity, "The Lantern Keeper");

    // picking an element again drops the custom text
    let s = app.details(&id, json!({"alignment": "Neutral Evil"}));
    assert_eq!(s.details.alignment, "Neutral Evil");
    assert_eq!(app.file(&id)["ui"]["alignment"], "");
}

#[test]
fn an_older_file_with_only_text_keeps_it_and_a_chosen_element_wins_over_text() {
    let app = App::new();
    let id = app.hero().id;
    let mut file = app.file(&id);
    file["ui"]["alignment"] = json!("True Neutral");
    file["ui"]["deity"] = json!("Old Oak");
    let path = app.state.character_path(&id).expect("path");
    app.state.close_character(&id, false).expect("close");
    std::fs::write(&path, serde_json::to_string(&file).expect("json")).expect("write");

    let s = app.state.load_character(&id).expect("reopen");
    let s = app.checked("load_character", s);
    assert_eq!(chosen(&s, "Alignment"), None, "loading changes nothing");
    assert_eq!(s.details.alignment, "True Neutral");
    assert_eq!(s.details.deity, "Old Oak");
    let html = app.sheet(&id);
    assert!(html.contains("True Neutral") && html.contains("Old Oak"));
    let saved = app.file(&id);
    assert_eq!(saved["ui"]["alignment"], "True Neutral", "kept in the file");
    assert_eq!(saved["ui"]["deity"], "Old Oak");

    // a pick made in the builder's Alignment slot is what Manage and the sheet show
    let slot = s
        .slots
        .iter()
        .find(|x| x.title == "Alignment")
        .expect("slot")
        .id
        .clone();
    let s = app
        .state
        .apply_selection(&id, &slot, LAWFUL_GOOD)
        .expect("pick");
    assert_eq!(s.details.alignment, "Lawful Good");
    assert!(app.sheet(&id).contains("Lawful Good"));
    assert_eq!(app.file(&id)["ui"]["alignment"], "True Neutral");
}

#[test]
fn an_aurora_import_shows_its_alignment_in_manage() {
    let app = App::new();
    let sample = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../crates/borealis-character/tests/data/example.dnd5e");
    let xml = std::fs::read(&sample).expect("example.dnd5e");
    use base64::Engine as _;
    let b64 = base64::engine::general_purpose::STANDARD.encode(xml);
    let report = app
        .state
        .import_character("example.dnd5e", &b64)
        .expect("import");
    let s = app
        .state
        .load_character(&report.summary.id)
        .expect("open import");
    assert_eq!(
        chosen(&s, "Alignment").as_deref(),
        Some("ID_ALIGNMENT_NEUTRAL_GOOD")
    );
    assert_eq!(s.details.alignment, "Neutral Good", "Manage shows it");
    assert!(s
        .detail_choices
        .alignment
        .iter()
        .any(|a| a == "Neutral Good"));
    assert!(app.sheet(&s.id).contains("Neutral Good"));
}

const OLD_OAK: &str = "ID_HOMEBREW_DEITY_OLD_OAK";
const HEARTH_MOTHER: &str = "ID_HOMEBREW_DEITY_HEARTH_MOTHER";

/// Two homebrew deities (the bundled SRD content has none).
fn install_deities(app: &AppState) {
    let deity = |name: &str, id: &str| {
        format!(
            r#"<element name="{name}" type="Deity" source="Hearth Pantheon" id="{id}"><setters><set name="alignment">NG</set><set name="domains">Life</set><set name="symbol">A hearth</set></setters></element>"#
        )
    };
    let xml = format!(
        r#"<elements><info><name>Hearth Pantheon</name><update version="0.0.1"><file name="hearth.xml" url="https://example.invalid/hearth.xml"/></update></info>{}{}</elements>"#,
        deity("Old Oak", OLD_OAK),
        deity("Hearth Mother", HEARTH_MOTHER)
    );
    use base64::Engine as _;
    let req = borealis_lib::content_dto::ImportPackRequest {
        file_name: "hearth.xml".into(),
        content_base64: base64::engine::general_purpose::STANDARD.encode(xml),
        format: None,
    };
    app.import_pack(&req, &borealis_lib::content_packs::NullSink)
        .expect("install");
}

#[test]
fn manage_lists_the_deity_elements_and_a_pick_selects_one_that_the_sheet_prints() {
    let app = App::new();
    install_deities(&app.state);
    let s = app.hero();
    let id = s.id.clone();
    assert_eq!(s.detail_choices.deity, ["Hearth Mother", "Old Oak"]);

    let s = app.details(&id, json!({"deity": "hearth mother"}));
    assert_eq!(chosen(&s, "Deity").as_deref(), Some(HEARTH_MOTHER));
    assert_eq!(s.details.deity, "Hearth Mother");
    assert!(app.sheet(&id).contains("Hearth Mother"));

    // a deity the content does not have stays custom text and empties the slot
    let s = app.details(&id, json!({"deity": "The Lantern Keeper"}));
    assert_eq!(chosen(&s, "Deity"), None);
    assert_eq!(s.details.deity, "The Lantern Keeper");
    let html = app.sheet(&id);
    assert!(html.contains("The Lantern Keeper") && !html.contains("Hearth Mother"));
}
