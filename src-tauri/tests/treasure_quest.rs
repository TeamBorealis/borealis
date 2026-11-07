//! B-218: Manage (Equipment > Additional treasure) TREASURE and QUEST ITEMS are the character's
//! own `details.treasure` / `details.quest`, the fields the Aurora importer fills and the sheet
//! prints. Typed text reaches the sheet, an Aurora import shows its text in Manage, and the text an
//! older version kept in the UI extras is still shown until the box is next edited. Responses are
//! checked against the TS contract.

mod contract_support;
use borealis_lib::dto::{
    AbilityMethodDto, CharacterSnapshot, NewCharacterRequest, OptionsPatch, Ruleset,
};
use borealis_lib::dto_ext::SheetOptionsDto;
use borealis_lib::state::{AppPaths, AppState};
use contract_support::Contract;
use serde_json::{json, Value};
use std::path::PathBuf;

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

    fn hero(&self, ruleset: Ruleset) -> String {
        let request = NewCharacterRequest {
            name: "Wren".into(),
            ruleset,
            starting_level: 1,
            gender: String::new(),
            ability_method: AbilityMethodDto::Manual,
            options: OptionsPatch::default(),
            portrait: None,
            group: None,
        };
        let s = self
            .state
            .create_character(ruleset, &request)
            .expect("create");
        self.checked("create_character", s).id
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

    fn reopen(&self, id: &str) -> CharacterSnapshot {
        self.state.save_character(id).expect("save");
        self.state.close_character(id, false).expect("close");
        let s = self.state.load_character(id).expect("reopen");
        self.checked("load_character", s)
    }
}

const SAPPHIRE: &str = "A sapphire the size of a thumbnail";
const LETTER: &str = "The sealed letter for the abbot";

#[test]
fn text_typed_in_manage_is_stored_natively_and_printed_on_the_sheet() {
    for ruleset in [Ruleset::Dnd2014, Ruleset::Dnd2024] {
        let app = App::new();
        let id = app.hero(ruleset);
        assert!(!app.sheet(&id).contains(SAPPHIRE));

        let s = app.details(&id, json!({"treasure": SAPPHIRE, "questItems": LETTER}));
        assert_eq!(s.details.treasure, SAPPHIRE);
        assert_eq!(s.details.quest_items, LETTER);
        let html = app.sheet(&id);
        assert!(html.contains(SAPPHIRE), "{ruleset:?}: treasure printed");
        assert!(html.contains(LETTER), "{ruleset:?}: quest items printed");

        let file = app.file(&id);
        assert_eq!(file["details"]["treasure"], SAPPHIRE, "the native field");
        assert_eq!(file["details"]["quest"], LETTER);
        assert_eq!(file["ui"]["treasure"], "", "no copy in the UI extras");
        assert_eq!(file["ui"]["questItems"], "");

        let back = app.reopen(&id);
        assert_eq!(back.details.treasure, SAPPHIRE, "kept through reopen");
        assert_eq!(back.details.quest_items, LETTER);

        // emptying the box empties the sheet
        let s = app.details(&id, json!({"treasure": "", "questItems": ""}));
        assert_eq!(s.details.treasure, "");
        assert_eq!(s.details.quest_items, "");
        let html = app.sheet(&id);
        assert!(!html.contains(SAPPHIRE) && !html.contains(LETTER));
    }
}

#[test]
fn an_aurora_import_shows_its_treasure_and_quest_items_in_manage() {
    let app = App::new();
    let sample = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../crates/borealis-character/tests/data/example.dnd5e");
    let xml = std::fs::read_to_string(&sample).expect("example.dnd5e");
    let (empty_treasure, empty_quest) = ("<treasure><![CDATA[]]></treasure>", "<quest></quest>");
    assert!(xml.contains(empty_treasure) && xml.contains(empty_quest));
    let xml = xml
        .replace(
            empty_treasure,
            &format!("<treasure><![CDATA[{SAPPHIRE}]]></treasure>"),
        )
        .replace(empty_quest, &format!("<quest>{LETTER}</quest>"));
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
    assert_eq!(s.details.treasure, SAPPHIRE, "Manage shows the treasure");
    assert_eq!(
        s.details.quest_items, LETTER,
        "Manage shows the quest items"
    );
    let html = app.sheet(&s.id);
    assert!(html.contains(SAPPHIRE) && html.contains(LETTER));

    // an edit in Manage replaces the imported text on the sheet (not contract-checked: an
    // imported point-buy snapshot trips the checker on `pointBuy.costs` keys, see B-191 handoff)
    let s = app
        .state
        .update_details(&s.id, &json!({"treasure": "Two rubies"}))
        .expect("details");
    assert_eq!(s.details.treasure, "Two rubies");
    let html = app.sheet(&s.id);
    assert!(html.contains("Two rubies") && !html.contains(SAPPHIRE));
}

#[test]
fn an_older_file_with_text_in_the_ui_extras_keeps_showing_it_until_edited() {
    let app = App::new();
    let id = app.hero(Ruleset::Dnd2014);
    let mut file = app.file(&id);
    file["ui"]["treasure"] = json!(SAPPHIRE);
    file["ui"]["questItems"] = json!(LETTER);
    let path = app.state.character_path(&id).expect("path");
    app.state.close_character(&id, false).expect("close");
    std::fs::write(&path, serde_json::to_string(&file).expect("json")).expect("write");

    let s = app.state.load_character(&id).expect("reopen");
    let s = app.checked("load_character", s);
    assert_eq!(s.details.treasure, SAPPHIRE, "Manage shows the older text");
    assert_eq!(s.details.quest_items, LETTER);
    let html = app.sheet(&id);
    assert!(
        html.contains(SAPPHIRE) && html.contains(LETTER),
        "and the sheet prints it"
    );
    assert_eq!(
        app.file(&id)["ui"]["treasure"],
        SAPPHIRE,
        "kept in the file"
    );

    // an edit moves it to the native field; emptying it does not bring the old text back
    let s = app.details(&id, json!({"treasure": "Two rubies"}));
    assert_eq!(s.details.treasure, "Two rubies");
    assert_eq!(s.details.quest_items, LETTER, "the other box is untouched");
    let file = app.file(&id);
    assert_eq!(file["details"]["treasure"], "Two rubies");
    assert_eq!(file["ui"]["treasure"], "");
    let s = app.details(&id, json!({"treasure": ""}));
    assert_eq!(s.details.treasure, "");
    assert!(!app.sheet(&id).contains(SAPPHIRE));
}
