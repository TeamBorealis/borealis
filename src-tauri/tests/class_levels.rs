//! B2a (#34) through the real command layer: a character created at level 9 divides its levels
//! between classes (`split_multiclass`, `set_class_levels`) and comes out the same as one levelled
//! up the usual way: hit points, proficiencies, saving throws and spell slots. Every response
//! matches the TS contract.

mod contract_support;
use borealis_lib::dto::{
    AbilityMethodDto, CharacterSnapshot, NewCharacterRequest, OptionsPatch, Ruleset,
};
use borealis_lib::error::ErrorCode;
use borealis_lib::state::{AppPaths, AppState};
use contract_support::Contract;
use serde_json::{json, Value};
use std::path::PathBuf;

struct Build {
    ruleset: Ruleset,
    species: &'static str,
    fighter: &'static str,
    background: &'static str,
    wizard: &'static str,
    bard: &'static str,
}

const SRD_2014: Build = Build {
    ruleset: Ruleset::Dnd2014,
    species: "ID_RACE_HUMAN",
    fighter: "ID_WOTC_PHB_CLASS_FIGHTER",
    background: "ID_BACKGROUND_ACOLYTE",
    wizard: "ID_WOTC_PHB_MULTICLASS_WIZARD",
    bard: "ID_WOTC_PHB_MULTICLASS_BARD",
};

const SRD_2024: Build = Build {
    ruleset: Ruleset::Dnd2024,
    species: "ID_2024_SPECIES_HUMAN",
    fighter: "ID_2024_CLASS_FIGHTER",
    background: "ID_2024_BACKGROUND_SOLDIER",
    wizard: "ID_2024_MULTICLASS_WIZARD",
    bard: "ID_2024_MULTICLASS_BARD",
};

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

    /// A human Fighter created at `level` (STR 15, INT 13, CHA 8) with multiclassing on.
    fn fighter(&self, b: &Build, level: u32) -> CharacterSnapshot {
        let request = NewCharacterRequest {
            name: "Split".into(),
            ruleset: b.ruleset,
            starting_level: level,
            gender: String::new(),
            ability_method: AbilityMethodDto::Manual,
            options: OptionsPatch {
                multiclassing: Some(true),
                ..OptionsPatch::default()
            },
            portrait: None,
            group: None,
        };
        let app = &self.state;
        let s = app.create_character(b.ruleset, &request).expect("create");
        let id = s.id.clone();
        let s = app
            .apply_selection(&id, &slot_id(&s, "Race"), b.species)
            .expect("species");
        let s = app
            .apply_selection(&id, &slot_id(&s, "Class"), b.fighter)
            .expect("class");
        app.apply_selection(&id, &slot_id(&s, "Background"), b.background)
            .expect("background");
        for (ability, score) in [("str", 15), ("con", 14), ("int", 13), ("cha", 8)] {
            app.set_base_score(&id, ability, score).expect("score");
        }
        app.get_snapshot(&id).expect("snapshot")
    }

    /// Fighter 5, NEW MULTICLASS, Wizard picked, three Wizard level ups.
    fn levelled_normally(&self, b: &Build) -> CharacterSnapshot {
        let app = &self.state;
        let id = self.fighter(b, 5).id;
        let s = app.add_multiclass(&id).expect("new multiclass");
        app.apply_selection(&id, &slot_id(&s, "Multiclass (Level 6)"), b.wizard)
            .expect("pick Wizard");
        for _ in 0..3 {
            app.level_up(&id, "multi-6").expect("Wizard level");
        }
        app.get_snapshot(&id).expect("snapshot")
    }
}

fn slot_id(snap: &CharacterSnapshot, title: &str) -> String {
    snap.slots
        .iter()
        .find(|s| s.title == title)
        .unwrap_or_else(|| panic!("no slot {title}"))
        .id
        .clone()
}

fn classes(s: &CharacterSnapshot) -> Vec<(String, u32)> {
    s.level
        .classes
        .iter()
        .map(|c| (c.name.clone().unwrap_or_default(), c.level))
        .collect()
}

/// What the builder shows of a character, without its identity.
fn built(s: &CharacterSnapshot) -> Value {
    let v = serde_json::to_value(s).expect("json");
    json!({
        "level": v["level"],
        "abilities": v["abilities"],
        "stats": v["stats"],
        "slots": v["slots"],
        "progress": v["progress"],
        "spellcasting": v["spellcasting"],
        "attacks": v["attacks"],
    })
}

fn proficiencies(s: &CharacterSnapshot) -> Vec<String> {
    s.stats
        .proficiencies
        .iter()
        .flat_map(|g| g.values.iter().cloned())
        .collect()
}

fn split_matches_levelling_up(b: &Build) {
    let app = App::new();
    let nine = app.fighter(b, 9);
    let split = app.checked(
        "split_multiclass",
        app.state
            .split_multiclass(&nine.id, 4, Some(b.wizard))
            .expect("split"),
    );
    let normal = app.levelled_normally(b);
    assert_eq!(
        classes(&split),
        [("Fighter".into(), 5), ("Wizard".into(), 4)]
    );
    assert_eq!(split.level.total, 9, "the total level stays");
    assert_eq!(split.level.xp, nine.level.xp, "experience is untouched");
    assert_eq!(built(&split), built(&normal), "{:?}", b.ruleset);

    assert_eq!(
        split.stats.hp_max,
        10 + 4 * 6 + 4 * 4 + 9 * 2,
        "{:?}",
        b.ruleset
    );
    let profs = proficiencies(&split);
    assert!(
        profs.iter().any(|p| p.contains("Heavy")),
        "the Fighter's starting armor: {profs:?}"
    );
    let wizard_slots: Vec<(u32, i32)> = split
        .spellcasting
        .slots
        .iter()
        .map(|r| (r.level, r.total))
        .filter(|(_, t)| *t > 0)
        .collect();
    assert_eq!(wizard_slots, [(1, 4), (2, 3)], "Wizard 4 slots");
    let save = |k: &str| serde_json::to_value(split.abilities.save).expect("json")[k].clone();
    assert_eq!(save("int")["proficient"], json!(false), "{}", save("int"));
    assert_eq!(save("str")["proficient"], json!(true), "{}", save("str"));

    // removing the multiclass gives its levels back
    let back = app.checked(
        "set_class_levels",
        app.state
            .set_class_levels(&nine.id, "multi-6", 0)
            .expect("remove Wizard"),
    );
    assert_eq!(classes(&back), [("Fighter".into(), 9)]);
    assert_eq!(built(&back), built(&nine), "Fighter 9 again");
}

#[test]
fn split_fighter_9_into_fighter_5_wizard_4_2014() {
    split_matches_levelling_up(&SRD_2014);
}

#[test]
fn split_fighter_9_into_fighter_5_wizard_4_2024() {
    split_matches_levelling_up(&SRD_2024);
}

#[test]
fn class_levels_move_both_ways_and_follow_the_multiclass_rules() {
    let app = App::new();
    let b = &SRD_2014;
    let id = app.fighter(b, 9).id;
    let code =
        |r: Result<CharacterSnapshot, borealis_lib::error::ApiError>| r.expect_err("refused").code;
    assert_eq!(
        code(app.state.split_multiclass(&id, 2, Some(b.bard))),
        ErrorCode::RequirementFailed,
        "CHA 8 does not meet the Bard prerequisite"
    );
    assert_eq!(
        code(app.state.split_multiclass(&id, 9, None)),
        ErrorCode::RequirementFailed,
        "the main class keeps a level"
    );
    assert_eq!(
        code(app.state.set_class_levels(&id, "main", 3)),
        ErrorCode::InvalidArgument
    );

    let s = app
        .state
        .split_multiclass(&id, 2, Some(b.wizard))
        .expect("Wizard 2");
    let s = app.checked(
        "set_class_levels",
        app.state
            .set_class_levels(&id, &s.level.classes[1].slot_id, 5)
            .expect("Wizard 5"),
    );
    assert_eq!(classes(&s), [("Fighter".into(), 4), ("Wizard".into(), 5)]);
    let s = app
        .state
        .set_class_levels(&id, &s.level.classes[1].slot_id, 1)
        .expect("Wizard 1");
    assert_eq!(classes(&s), [("Fighter".into(), 8), ("Wizard".into(), 1)]);

    // HR3: with the house rule the Bard is allowed
    let on = OptionsPatch {
        ignore_multiclass_prerequisites: Some(true),
        ..OptionsPatch::default()
    };
    app.state
        .set_character_options(&id, &on)
        .expect("house rule");
    let s = app
        .state
        .split_multiclass(&id, 2, Some(b.bard))
        .expect("Bard with the house rule");
    assert_eq!(
        classes(&s),
        [
            ("Fighter".into(), 6),
            ("Wizard".into(), 1),
            ("Bard".into(), 2)
        ]
    );
    assert!(
        s.level.classes[2].prerequisites_unmet,
        "marked as overridden"
    );

    // no class given: the new multiclass waits for the picker
    let s = app.state.split_multiclass(&id, 1, None).expect("empty");
    assert_eq!(s.level.classes.len(), 4);
    assert!(s.level.classes[3].name.is_none());
    assert!(s.slots.iter().any(|x| x.title == "Multiclass (Level 9)"));
}
