//! Several companions (GitHub #13, decision U16(c)) through the real command layer: the granted
//! companion keeps working as before, companions are added by hand with a bundled creature,
//! renamed, given traits and bonuses, removed, saved and loaded back, and the sheet prints the
//! first one. Every response is checked against the TS contract.

mod contract_support;
use borealis_lib::dto::{
    AbilityMethodDto, CharacterSnapshot, NewCharacterRequest, OptionsPatch, Ruleset,
};
use borealis_lib::dto_ext::SheetOptionsDto;
use borealis_lib::error::{ApiError, ErrorCode};
use borealis_lib::state::{AppPaths, AppState};
use contract_support::Contract;
use serde_json::{json, Value};
use std::path::PathBuf;

const WOLF: &str = "ID_SRD51_MONSTER_WOLF";
const OWL: &str = "ID_SRD51_MONSTER_OWL";
const PNG: &str = "data:image/png;base64,iVBORw0KGgo=";

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

    /// The response as the UI receives it, after checking it against the TS response type.
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
        self.state
            .create_character(Ruleset::Dnd2014, &request)
            .expect("create")
    }

    fn add(&self, id: &str, creature: Option<&str>, name: Option<&str>) -> CharacterSnapshot {
        let s = self
            .state
            .add_companion(id, creature, name)
            .expect("add companion");
        self.checked("add_companion", s)
    }

    fn update(&self, id: &str, companion: Option<&str>, patch: Value) -> CharacterSnapshot {
        let s = self
            .state
            .update_companion(id, companion, &patch)
            .expect("update companion");
        self.checked("update_companion", s)
    }

    fn saved(&self, id: &str) -> Value {
        self.state.save_character(id).expect("save");
        let path = self.state.character_path(id).expect("path");
        serde_json::from_str(&std::fs::read_to_string(path).expect("read")).expect("json")
    }
}

fn code<T: std::fmt::Debug>(r: Result<T, ApiError>) -> ErrorCode {
    r.expect_err("refused").code
}

fn ids(s: &CharacterSnapshot) -> Vec<&str> {
    s.companions.iter().map(|c| c.id.as_str()).collect()
}

#[test]
fn a_character_without_companions_has_none_and_saves_no_key() {
    let app = App::new();
    let s = app.hero();
    assert!(s.companions.is_empty());
    assert!(s.companion.is_none());
    assert!(app.saved(&s.id).get("companions").is_none());
}

#[test]
fn the_granted_companion_works_as_before_and_comes_first() {
    let app = App::new();
    let id = app.hero().id;
    // the old call shape: no companion id, name and picture only
    let s = app.update(&id, None, json!({"name": "Biscuit", "portrait": PNG}));
    assert_eq!(ids(&s), ["granted"]);
    let g = s.companion.as_ref().expect("granted");
    assert!(g.granted);
    assert_eq!(
        (g.name.as_str(), g.portrait.as_deref()),
        ("Biscuit", Some(PNG))
    );
    let file = app.saved(&id);
    assert_eq!(file["details"]["companion_name"], "Biscuit");
    assert_eq!(file["ui"]["companionPortrait"], PNG);
    assert!(file.get("companions").is_none(), "one companion, no edits");

    let s = app.add(&id, Some(WOLF), Some("Ash"));
    assert_eq!(ids(&s), ["granted", "companion-1"]);
    assert_eq!(s.companion.as_ref().map(|c| c.id.as_str()), Some("granted"));
    assert_eq!(
        code(app.state.remove_companion(&id, "granted")),
        ErrorCode::RequirementFailed
    );
    // notes, traits and bonuses on the granted companion are saved under `companions.granted`
    let s = app.update(
        &id,
        Some("granted"),
        json!({"notes": "Sleeps by the fire.", "traits": [{"name": "Loyal", "text": "Never flees."}]}),
    );
    let g = &s.companions[0];
    assert_eq!(g.notes, "Sleeps by the fire.");
    assert_eq!(g.custom_traits.len(), 1);
    assert_eq!(
        app.saved(&id)["companions"]["granted"]["traits"][0]["name"],
        "Loyal"
    );
}

#[test]
fn companions_are_added_edited_removed_and_saved() {
    let app = App::new();
    let id = app.hero().id;
    let s = app.add(&id, Some(WOLF), None);
    let wolf = &s.companions[0];
    assert!(!wolf.granted);
    assert_eq!(wolf.name, "Wolf", "no name shows the creature's");
    assert_eq!((wolf.armor_class, wolf.hp, wolf.speed), (13, 11, 40));
    assert_eq!(
        s.companion.as_ref().map(|c| c.id.as_str()),
        Some("companion-1")
    );
    let creature_traits = wolf.traits.len();
    assert!(creature_traits > 0, "the stat block's traits");

    let s = app.add(&id, Some(OWL), Some("Hoot"));
    let s = app.add(&s.id, None, Some("Stray"));
    assert_eq!(ids(&s), ["companion-1", "companion-2", "companion-3"]);
    assert!(s.companions[2].creature.is_none());

    let s = app.update(
        &id,
        Some("companion-1"),
        json!({"name": "Ash", "armorClassBonus": 1, "hitPointsBonus": 4, "speedBonus": 10,
               "traits": [{"name": "Pack Leader", "text": "Allies near it fight well."}],
               "notes": "Found in the north woods.", "portrait": PNG}),
    );
    let ash = &s.companions[0];
    assert_eq!(ash.name, "Ash");
    assert_eq!((ash.armor_class, ash.hp, ash.speed), (14, 15, 50));
    assert_eq!(ash.traits.len(), creature_traits + 1);
    assert_eq!(
        ash.traits.last().map(|t| t.name.as_str()),
        Some("Pack Leader")
    );
    assert_eq!(ash.portrait.as_deref(), Some(PNG));
    // the creature can be swapped
    let s = app.update(&id, Some("companion-3"), json!({"creatureId": OWL}));
    assert_eq!(
        s.companions[2].creature.as_ref().map(|c| c.name.as_str()),
        Some("Owl")
    );

    let s = app
        .state
        .remove_companion(&id, "companion-2")
        .expect("remove");
    let s = app.checked("remove_companion", s);
    assert_eq!(ids(&s), ["companion-1", "companion-3"]);

    let file = app.saved(&id);
    assert_eq!(file["companions"]["added"][0]["creature"], WOLF);
    assert_eq!(file["companions"]["added"][0]["speed_bonus"], 10);
    app.state.close_character(&id, false).expect("close");
    let back = app.state.load_character(&id).expect("reload");
    assert_eq!(back.companions, s.companions);
}

#[test]
fn bad_companion_input_is_refused_and_changes_nothing() {
    let app = App::new();
    let id = app.hero().id;
    let s = app.add(&id, Some(WOLF), None);
    let refused = [
        app.state
            .add_companion(&id, Some("ID_PHB_SPELL_BLESS"), None),
        app.state
            .add_companion(&id, Some("ID_NO_SUCH_CREATURE"), None),
        app.state.add_companion(&id, None, Some(&"x".repeat(61))),
        app.state
            .update_companion(&id, Some("companion-1"), &json!({"speedBonus": 100})),
        app.state
            .update_companion(&id, Some("companion-1"), &json!({"traits": [{"name": ""}]})),
        app.state
            .update_companion(&id, Some("companion-1"), &json!({"traits": "x"})),
        app.state
            .update_companion(&id, Some("companion-1"), &json!({"wings": 2})),
        app.state.update_companion(
            &id,
            Some("companion-1"),
            &json!({"portrait": "data:text/plain,hi"}),
        ),
        app.state
            .update_companion(&id, None, &json!({"creatureId": WOLF})),
        app.state
            .update_companion(&id, Some("companion-9"), &json!({"name": "x"})),
        app.state.remove_companion(&id, "companion-9"),
    ];
    let codes: Vec<_> = refused.into_iter().map(code).collect();
    use ErrorCode::{InvalidArgument as I, NotFound as N};
    assert_eq!(codes, [I, N, I, I, I, I, I, I, I, N, N]);
    assert_eq!(
        app.state.get_snapshot(&id).expect("snapshot").companions,
        s.companions
    );
    for _ in 1..borealis_character::companions::MAX_ADDED_COMPANIONS {
        app.add(&id, None, None);
    }
    assert_eq!(
        code(app.state.add_companion(&id, None, None)),
        ErrorCode::RequirementFailed
    );
}

#[test]
fn the_sheet_prints_the_first_companion() {
    let app = App::new();
    let id = app.hero().id;
    app.add(&id, Some(OWL), Some("Hoot"));
    app.add(&id, Some(WOLF), Some("Ash"));
    let sheet = app
        .state
        .render_sheet(&id, &SheetOptionsDto::default())
        .expect("sheet");
    assert!(sheet.html.contains("Hoot"), "the first companion's page");
    assert!(
        !sheet.html.contains("Ash"),
        "one companion page (B-21 defers more)"
    );
}
