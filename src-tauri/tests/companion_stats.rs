//! A companion's saving throws, senses and actions (GitHub #40) through the real command layer,
//! with bundled SRD 5.2.1 creatures: the snapshot carries the six saves, the special senses,
//! languages and damage and condition lines, and the sheet's companion page prints them. A
//! familiar's actions stay on the page: the SRD's Find Familiar says a familiar can't attack, but
//! Aurora's companion page lists the creature's actions all the same (U7).

mod contract_support;
use borealis_lib::content_dto::CompanionState;
use borealis_lib::dto::{
    AbilityMethodDto, CharacterSnapshot, NewCharacterRequest, OptionsPatch, Ruleset,
};
use borealis_lib::dto_ext::SheetOptionsDto;
use borealis_lib::state::{AppPaths, AppState};
use contract_support::Contract;
use std::path::PathBuf;

const CAT: &str = "ID_SRD521_MONSTER_CAT";
const IMP: &str = "ID_SRD521_MONSTER_IMP";

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

    fn hero(&self) -> String {
        let request = NewCharacterRequest {
            name: "Wren".into(),
            ruleset: Ruleset::Dnd2024,
            starting_level: 1,
            gender: String::new(),
            ability_method: AbilityMethodDto::Manual,
            options: OptionsPatch::default(),
            portrait: None,
            group: None,
        };
        self.state
            .create_character(Ruleset::Dnd2024, &request)
            .expect("create")
            .id
    }

    /// Add `creature` by hand; the response is checked against the TS contract.
    fn add(&self, id: &str, creature: &str) -> CharacterSnapshot {
        let s = self
            .state
            .add_companion(id, Some(creature), None)
            .expect("add companion");
        let v = serde_json::to_value(&s).expect("json");
        let errors = self.contract.response("add_companion", &v);
        assert!(errors.is_empty(), "add_companion: {errors:?}");
        s
    }

    fn sheet_html(&self, id: &str) -> String {
        self.state
            .render_sheet(id, &SheetOptionsDto::default())
            .expect("sheet")
            .html
    }
}

fn last(s: &CharacterSnapshot) -> &CompanionState {
    s.companions.last().expect("a companion")
}

#[test]
fn a_companion_has_its_six_saving_throws() {
    let app = App::new();
    let id = app.hero();
    let s = app.add(&id, CAT);
    let saves: Vec<(&str, &str, i32)> = last(&s)
        .saves
        .iter()
        .map(|v| (v.name.as_str(), v.ability.as_str(), v.bonus))
        .collect();
    // The SRD cat: Dexterity save +4 (proficient), the others the ability modifier.
    assert_eq!(
        saves,
        [
            ("Strength", "STR", -4),
            ("Dexterity", "DEX", 4),
            ("Constitution", "CON", 0),
            ("Intelligence", "INT", -4),
            ("Wisdom", "WIS", 1),
            ("Charisma", "CHA", -2),
        ]
    );
    let html = app.sheet_html(&id);
    assert!(html.contains("Saving Throws"), "no saves section");
    assert!(html.contains("+4 Dexterity"), "Dexterity save");
    assert!(html.contains("\u{2212}4 Strength"), "Strength save");
}

#[test]
fn a_companion_shows_its_special_senses_and_languages() {
    let app = App::new();
    let id = app.hero();
    let s = app.add(&id, CAT);
    let cat = last(&s);
    assert_eq!(cat.senses, "Darkvision 60 ft.");
    assert_eq!(cat.passive_perception, 13);
    assert_eq!(cat.languages, "\u{2014}");
    assert!(cat.defenses.is_empty());
    let html = app.sheet_html(&id);
    assert!(html.contains("Senses Darkvision 60 ft."), "senses line");
    assert!(html.contains("13 Passive Perception"));

    let s = app.add(&id, IMP);
    let imp = last(&s);
    assert_eq!(
        imp.senses,
        "Darkvision 120 ft. (unimpeded by magical Darkness)"
    );
    assert_eq!(imp.languages, "Common, Infernal");
    let defenses: Vec<(&str, &str)> = imp
        .defenses
        .iter()
        .map(|d| (d.name.as_str(), d.text.as_str()))
        .collect();
    assert_eq!(
        defenses,
        [
            ("Damage Resistances", "Cold"),
            ("Damage Immunities", "Fire, Poison"),
            ("Condition Immunities", "Poisoned"),
        ]
    );
}

#[test]
fn a_familiars_actions_stay_on_the_companion_page() {
    let app = App::new();
    let id = app.hero();
    let s = app.add(&id, CAT);
    let actions: Vec<&str> = last(&s).actions.iter().map(|a| a.name.as_str()).collect();
    assert_eq!(actions, ["Scratch"]);
    let html = app.sheet_html(&id);
    assert!(html.contains("Actions"), "actions section");
    assert!(html.contains("Scratch."), "the cat's action");
}
