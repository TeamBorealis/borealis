//! Editing an automatic (weapon) attack row (GitHub #27): Aurora's attack editor lets the user pick
//! the ability the bonus and damage are worked out from, change the row's texts and leave it off
//! the sheet. Every change is saved with the character and survives a reopen; every response is
//! checked against the TS contract.

mod contract_support;
use borealis_lib::content_dto::AttackInput;
use borealis_lib::dto::{
    AbilityMethodDto, Attack, CharacterSnapshot, NewCharacterRequest, OptionsPatch, Ruleset,
};
use borealis_lib::dto_ext::EquipRequest;
use borealis_lib::state::{AppPaths, AppState};
use contract_support::Contract;
use std::path::PathBuf;

const LONGSWORD: &str = "ID_2024_WEAPON_LONGSWORD";
const RAPIER: &str = "ID_2024_WEAPON_RAPIER";

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

    /// A 2024 hero with Strength 16, Dexterity 10 and Charisma 18 holding `weapon`.
    fn hero_with(&self, weapon: &str) -> (String, String) {
        let request = NewCharacterRequest {
            name: "Edit".into(),
            ruleset: Ruleset::Dnd2024,
            starting_level: 1,
            gender: String::new(),
            ability_method: AbilityMethodDto::Manual,
            options: OptionsPatch::default(),
            portrait: None,
            group: None,
        };
        let s = self
            .state
            .create_character(Ruleset::Dnd2024, &request)
            .expect("create");
        let id = s.id.clone();
        self.state.set_base_score(&id, "str", 16).expect("str");
        self.state.set_base_score(&id, "dex", 10).expect("dex");
        self.state.set_base_score(&id, "cha", 18).expect("cha");
        let s = self
            .state
            .add_item(&id, weapon, 1, false, None)
            .expect("weapon");
        let uid = s.inventory.items.last().expect("line").id.clone();
        self.state
            .equip_item(&id, &uid, EquipRequest::Primary)
            .expect("equip");
        (id, uid)
    }

    fn save(
        &self,
        id: &str,
        a: &Attack,
        change: impl FnOnce(&mut AttackInput),
    ) -> CharacterSnapshot {
        let mut input = input_of(a);
        change(&mut input);
        let s = self.state.upsert_attack(id, &input).expect("upsert");
        self.checked("upsert_attack", s)
    }
}

/// What the attack editor sends back unchanged for row `a`.
fn input_of(a: &Attack) -> AttackInput {
    AttackInput {
        id: Some(a.id.clone()),
        name: a.name.clone(),
        range: a.range.clone(),
        attack_bonus: a.attack_bonus.clone(),
        damage: a.damage.clone(),
        description: a.description.clone(),
        ability: a.ability.clone(),
        show_in_combat: a.show_in_combat,
        as_card: None,
        calculated: false,
    }
}

fn row<'a>(s: &'a CharacterSnapshot, uid: &str) -> &'a Attack {
    s.attacks
        .iter()
        .find(|a| a.id == uid)
        .unwrap_or_else(|| panic!("no attack {uid}"))
}

#[test]
fn picking_another_ability_recalculates_a_weapon_row_and_is_saved() {
    let app = App::new();
    let (id, sword) = app.hero_with(LONGSWORD);
    let s = app.checked("get_snapshot", app.state.get_snapshot(&id).expect("snap"));
    let before = row(&s, &sword).clone();
    assert!(before.auto);
    assert_eq!(before.ability.as_deref(), Some("str"));
    assert_eq!(before.damage, "1d8+3 slashing");
    // the editor previews every ability
    let lines = before.ability_lines.as_ref().expect("ability lines");
    assert_eq!(lines.len(), 6);
    let cha = lines.iter().find(|l| l.ability == "cha").expect("cha line");
    assert_eq!(cha.damage, "1d8+4 slashing");

    let s = app.save(&id, &before, |i| i.ability = Some("cha".into()));
    let after = row(&s, &sword);
    assert_eq!(after.ability.as_deref(), Some("cha"));
    assert_eq!(after.attack_bonus, cha.attack_bonus);
    assert_eq!(after.damage, "1d8+4 slashing");
    assert_eq!(after.name, before.name, "untouched texts follow the weapon");

    app.state.save_character(&id).expect("save");
    app.state.close_character(&id, false).expect("close");
    let back = app.state.load_character(&id).expect("reopen");
    assert_eq!(row(&back, &sword).ability.as_deref(), Some("cha"));
    assert_eq!(row(&back, &sword).damage, "1d8+4 slashing");
}

#[test]
fn a_weapon_row_keeps_typed_texts_and_can_leave_the_sheet() {
    let app = App::new();
    let (id, sword) = app.hero_with(LONGSWORD);
    let s = app.state.get_snapshot(&id).expect("snap");
    let a = row(&s, &sword).clone();
    let s = app.save(&id, &a, |i| {
        i.name = "Oathblade".into();
        i.range = "Reach".into();
        i.description = "Glows near fiends".into();
        i.damage = "1d8+3 radiant".into();
        i.show_in_combat = false;
    });
    let edited = row(&s, &sword).clone();
    assert_eq!(edited.name, "Oathblade");
    assert_eq!(edited.range, "Reach");
    assert_eq!(edited.description, "Glows near fiends");
    assert_eq!(edited.damage, "1d8+3 radiant");
    assert_eq!(edited.attack_bonus, a.attack_bonus);
    assert!(!edited.show_in_combat);
    assert!(edited.auto);

    // clearing a text goes back to the worked-out value
    let s = app.save(&id, &edited, |i| {
        i.name = String::new();
        i.damage = String::new();
        i.show_in_combat = true;
    });
    let reset = row(&s, &sword);
    assert_eq!(reset.name, a.name);
    assert_eq!(reset.damage, a.damage);
    assert_eq!(reset.range, "Reach");
    assert!(reset.show_in_combat);

    app.state.save_character(&id).expect("save");
    app.state.close_character(&id, false).expect("close");
    let back = app.state.load_character(&id).expect("reopen");
    assert_eq!(row(&back, &sword).range, "Reach");
    assert_eq!(row(&back, &sword).description, "Glows near fiends");
}

#[test]
fn keeping_the_rules_ability_does_not_pin_it() {
    // A finesse weapon follows the better of Strength and Dexterity. Saving the row with the
    // ability it already uses changes nothing: a later higher Dexterity still wins.
    let app = App::new();
    let (id, rapier) = app.hero_with(RAPIER);
    let s = app.state.get_snapshot(&id).expect("snap");
    let a = row(&s, &rapier).clone();
    assert_eq!(a.ability.as_deref(), Some("str"));
    app.save(&id, &a, |i| i.name = a.name.clone());
    let s = app.state.set_base_score(&id, "dex", 18).expect("dex");
    assert_eq!(row(&s, &rapier).ability.as_deref(), Some("dex"));
}

#[test]
fn an_unknown_row_or_ability_is_refused() {
    let app = App::new();
    let (id, sword) = app.hero_with(LONGSWORD);
    let s = app.state.get_snapshot(&id).expect("snap");
    let mut input = input_of(row(&s, &sword));
    input.ability = Some("luck".into());
    assert!(app.state.upsert_attack(&id, &input).is_err());
    let mut input = input_of(row(&s, &sword));
    input.id = Some("item-999".into());
    assert!(app.state.upsert_attack(&id, &input).is_err());
}
