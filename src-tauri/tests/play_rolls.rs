//! Play tab rolls (GitHub #1, #14, decision U16(b)) through the real command layer: every number the
//! UI rolls against comes from the snapshot, with the 2024 Exhaustion penalty to D20 Tests already
//! in it. Ability check bonuses, the to-hit of every attack row (manual ones included), the spell
//! attack roll, which spells are ready to cast, the hit dice and the die of a resource (Bardic
//! Inspiration). Every response is checked against the TS contract.

mod contract_support;
use borealis_lib::content_dto::AttackInput;
use borealis_lib::dto::{
    AbilityMethodDto, CharacterSnapshot, NewCharacterRequest, OptionsPatch, Ruleset,
};
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

    /// The snapshot as the UI receives it, after checking it against the TS response type.
    fn json(&self, command: &str, snap: &CharacterSnapshot) -> Value {
        let v = serde_json::to_value(snap).expect("json");
        let errors = self.contract.response(command, &v);
        assert!(errors.is_empty(), "{command}: {errors:?}");
        v
    }

    fn create(&self, ruleset: Ruleset, race: &str, class: &str, level: u32) -> CharacterSnapshot {
        let request = NewCharacterRequest {
            name: "Roller".into(),
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
        let s = self
            .state
            .apply_selection(&s.id, &slot_id(&s, "Race"), race)
            .expect("race");
        let mut s = self
            .state
            .apply_selection(&s.id, &slot_id(&s, "Class"), class)
            .expect("class");
        while s.level.total < level {
            s = self.state.level_up(&s.id, "main").expect("level up");
        }
        s
    }

    fn attack(&self, id: &str, name: &str, bonus: &str) -> CharacterSnapshot {
        let input = AttackInput {
            id: None,
            name: name.into(),
            range: "5 ft.".into(),
            attack_bonus: bonus.into(),
            damage: "1d6+2 slashing".into(),
            description: String::new(),
            ability: None,
            show_in_combat: true,
            as_card: None,
        };
        self.state.upsert_attack(id, &input).expect("attack")
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

fn attack<'a>(snap: &'a Value, name: &str) -> &'a Value {
    snap["attacks"]
        .as_array()
        .expect("attacks")
        .iter()
        .find(|a| a["name"] == name)
        .unwrap_or_else(|| panic!("no attack {name}"))
}

fn resource<'a>(snap: &'a Value, name: &str) -> &'a Value {
    snap["play"]["resources"]
        .as_array()
        .expect("resources")
        .iter()
        .find(|r| r["name"] == name)
        .unwrap_or_else(|| panic!("no resource {name} in {}", snap["play"]["resources"]))
}

fn spell<'a>(snap: &'a Value, name: &str) -> &'a Value {
    snap["spellcasting"]["classes"][0]["spells"]
        .as_array()
        .expect("spells")
        .iter()
        .find(|s| s["name"] == name)
        .unwrap_or_else(|| panic!("no spell {name}"))
}

#[test]
fn ability_checks_and_attack_rolls_carry_the_2024_exhaustion_penalty() {
    let app = App::new();
    let s = app.create(
        Ruleset::Dnd2024,
        "ID_2024_SPECIES_HUMAN",
        "ID_2024_CLASS_FIGHTER",
        1,
    );
    let id = s.id.clone();
    app.state.set_base_score(&id, "str", 16).expect("str");
    app.attack(&id, "Thrown Rock", "+5");
    app.attack(&id, "Breath", "DC 13 DEX");
    let s0 = app.json("upsert_attack", &app.attack(&id, "Odd", "see notes"));
    assert_eq!(s0["abilities"]["check"]["str"], 3);
    assert_eq!(
        s0["abilities"]["check"]["dex"],
        s0["abilities"]["modifier"]["dex"]
    );
    let unarmed = attack(&s0, "Unarmed Strike");
    assert_eq!(
        unarmed["toHit"].as_i64().unwrap().to_string(),
        unarmed["attackBonus"]
            .as_str()
            .unwrap()
            .trim_start_matches('+')
    );
    assert_eq!(attack(&s0, "Thrown Rock")["toHit"], 5);
    assert_eq!(attack(&s0, "Breath")["toHit"], Value::Null);
    assert_eq!(attack(&s0, "Odd")["toHit"], Value::Null);

    let s2 = app.json(
        "set_exhaustion",
        &app.state.set_exhaustion(&id, 2).expect("exhaustion"),
    );
    assert_eq!(s2["play"]["exhaustion"]["d20Penalty"], -4);
    assert_eq!(s2["abilities"]["check"]["str"], -1);
    assert_eq!(attack(&s2, "Thrown Rock")["toHit"], 1);
    // engine rows already hold the penalty: the to-hit is the shown bonus
    let unarmed2 = attack(&s2, "Unarmed Strike");
    assert_eq!(
        unarmed2["toHit"].as_i64(),
        unarmed["toHit"].as_i64().map(|n| n - 4)
    );
    // saves, skills and initiative already carry it
    assert_eq!(
        s2["abilities"]["save"]["str"]["bonus"].as_i64(),
        s0["abilities"]["save"]["str"]["bonus"]
            .as_i64()
            .map(|n| n - 4)
    );
}

#[test]
fn a_2014_character_has_no_d20_penalty_from_exhaustion() {
    let app = App::new();
    let s = app.create(
        Ruleset::Dnd2014,
        "ID_RACE_HUMAN",
        "ID_WOTC_PHB_CLASS_FIGHTER",
        1,
    );
    let id = s.id.clone();
    app.attack(&id, "Thrown Rock", "+5");
    let s = app.json(
        "set_exhaustion",
        &app.state.set_exhaustion(&id, 2).expect("exhaustion"),
    );
    assert_eq!(s["abilities"]["check"], s["abilities"]["modifier"]);
    assert_eq!(attack(&s, "Thrown Rock")["toHit"], 5);
}

#[test]
fn hit_dice_roll_the_class_die_plus_the_constitution_modifier() {
    let app = App::new();
    let s = app.create(
        Ruleset::Dnd2024,
        "ID_2024_SPECIES_HUMAN",
        "ID_2024_CLASS_WIZARD",
        3,
    );
    let id = s.id.clone();
    let s = app.json(
        "set_base_score",
        &app.state.set_base_score(&id, "con", 14).expect("con"),
    );
    assert_eq!(
        s["play"]["hitDice"],
        json!([{"className": "Wizard", "die": 6, "total": 3, "notation": "1d6+2"}])
    );
    let s = app.json(
        "set_base_score",
        &app.state.set_base_score(&id, "con", 8).expect("con"),
    );
    assert_eq!(s["play"]["hitDice"][0]["notation"], "1d6-1");
}

#[test]
fn bardic_inspiration_carries_its_die_in_both_editions() {
    let app = App::new();
    let s = app.create(
        Ruleset::Dnd2024,
        "ID_2024_SPECIES_HUMAN",
        "ID_2024_CLASS_BARD",
        1,
    );
    let id = s.id.clone();
    let s = app.state.set_base_score(&id, "cha", 14).expect("cha");
    let s = app.json("set_base_score", &s);
    assert_eq!(resource(&s, "Bardic Inspiration")["die"], "1d6");
    let mut snap = app.state.level_up(&id, "main").expect("level up");
    while snap.level.total < 5 {
        snap = app.state.level_up(&id, "main").expect("level up");
    }
    let s = app.json("level_up", &snap);
    assert_eq!(resource(&s, "Bardic Inspiration")["die"], "1d8");

    let s = app.create(
        Ruleset::Dnd2014,
        "ID_RACE_HUMAN",
        "ID_WOTC_PHB_CLASS_BARD",
        1,
    );
    let s = app.state.set_base_score(&s.id, "cha", 14).expect("cha");
    let s = app.json("set_base_score", &s);
    assert_eq!(resource(&s, "Bardic Inspiration")["die"], "1d6");

    let s = app.create(
        Ruleset::Dnd2024,
        "ID_2024_SPECIES_HUMAN",
        "ID_2024_CLASS_FIGHTER",
        1,
    );
    let s = app.json("level_up", &s);
    assert_eq!(resource(&s, "Second Wind")["die"], Value::Null);
}

#[test]
fn the_spell_attack_roll_and_the_spells_ready_to_cast() {
    let app = App::new();
    let s = app.create(
        Ruleset::Dnd2024,
        "ID_2024_SPECIES_HUMAN",
        "ID_2024_CLASS_CLERIC",
        1,
    );
    let id = s.id.clone();
    app.state.set_base_score(&id, "wis", 16).expect("wis");
    let s = app.json(
        "set_spell_prepared",
        &app.state
            .set_spell_prepared(&id, "main", "ID_2024_SPELL_BLESS", true)
            .expect("prepare"),
    );
    let caster = &s["spellcasting"]["classes"][0];
    assert_eq!(caster["attackRoll"], caster["attackBonus"]);
    assert_eq!(spell(&s, "Bless")["ready"], true);
    assert_eq!(spell(&s, "Cure Wounds")["ready"], false);
    for c in s["spellcasting"]["classes"][0]["spells"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["level"] == 0)
    {
        assert_eq!(c["ready"], true, "{}", c["name"]);
    }

    let s = app.json(
        "set_exhaustion",
        &app.state.set_exhaustion(&id, 1).expect("exhaustion"),
    );
    let caster = &s["spellcasting"]["classes"][0];
    assert_eq!(
        caster["attackRoll"].as_i64(),
        caster["attackBonus"].as_i64().map(|n| n - 2)
    );
}
