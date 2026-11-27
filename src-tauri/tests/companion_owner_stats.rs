//! A companion whose statistics come from its owner: its element states them as rules
//! (`companion:hp:max` += `level`, `companion:ac` += an ability modifier, a skill and a save
//! proficient with the owner's proficiency bonus), and the snapshot's companion carries the
//! engine's values of those rules, not the stat block's printed leading number ("5 + your
//! level" is not 5). Made-up content: a "Lantern Wisp" granted by a feat, on an SRD fighter.

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use borealis_engine::added_picks::AddedKind;
use borealis_lib::content_dto::{CompanionState, ImportPackRequest};
use borealis_lib::content_packs::NullSink;
use borealis_lib::dto::{
    AbilityMethodDto, CharacterSnapshot, NewCharacterRequest, OptionsPatch, Ruleset,
};
use borealis_lib::state::{AppPaths, AppState};
use std::path::PathBuf;

const HUMAN: &str = "ID_RACE_HUMAN";
const FIGHTER: &str = "ID_WOTC_PHB_CLASS_FIGHTER";
const FEAT: &str = "ID_TEST_FEAT_LANTERN_BOND";
const WISP: &str = "ID_TEST_COMPANION_LANTERN_WISP";
const FIXED: &str = "ID_TEST_COMPANION_STONE_TOAD";
const TOAD_FEAT: &str = "ID_TEST_FEAT_TOAD_BOND";
const MOUSE_FEAT: &str = "ID_TEST_FEAT_MOUSE_BOND";
const MOUSE: &str = "ID_TEST_COMPANION_MOSS_MOUSE";

const PACK: &str = r#"<elements><info><name>Wisp Kit</name><update version="0.0.1"><file name="wisp.xml" url="https://example.invalid/wisp.xml"/></update></info>
  <element name="Lantern Bond" type="Feat" source="Wisp Kit" id="ID_TEST_FEAT_LANTERN_BOND">
    <description><p>A lantern wisp follows you.</p></description>
    <rules><select type="Companion" name="Lantern Wisp" supports="Lantern Wisp" default="ID_TEST_COMPANION_LANTERN_WISP" /></rules>
  </element>
  <element name="Lantern Wisp" type="Companion" source="Wisp Kit" id="ID_TEST_COMPANION_LANTERN_WISP">
    <supports>Lantern Wisp</supports>
    <setters>
      <set name="size">Tiny</set><set name="type">Fey</set><set name="alignment">neutral</set>
      <set name="ac">12 + your Strength modifier</set>
      <set name="hp">5 + your level</set>
      <set name="speed">10 ft., fly 40 ft.</set>
      <set name="strength">3</set><set name="dexterity">16</set><set name="constitution">10</set>
      <set name="intelligence">8</set><set name="wisdom">12</set><set name="charisma">10</set>
      <set name="saves">Dex +3 plus PB</set>
      <set name="skills">Perception +1 plus PB</set>
      <set name="challenge">-</set>
    </setters>
    <rules>
      <stat name="companion:ac" value="12" bonus="base" />
      <stat name="companion:ac" value="strength:modifier" bonus="ability" />
      <stat name="companion:hp:max" value="5" bonus="base" />
      <stat name="companion:hp:max" value="level" bonus="level" />
      <stat name="companion:speed" value="10" bonus="base" />
      <stat name="companion:perception:proficiency" value="proficiency" bonus="proficiency" />
      <stat name="companion:dexterity:save:proficiency" value="proficiency" bonus="proficiency" />
    </rules>
  </element>
  <element name="Toad Bond" type="Feat" source="Wisp Kit" id="ID_TEST_FEAT_TOAD_BOND">
    <description><p>A stone toad follows you.</p></description>
    <rules><select type="Companion" name="Stone Toad" supports="Stone Toad" default="ID_TEST_COMPANION_STONE_TOAD" /></rules>
  </element>
  <element name="Mouse Bond" type="Feat" source="Wisp Kit" id="ID_TEST_FEAT_MOUSE_BOND">
    <description><p>A moss mouse follows you.</p></description>
    <rules><select type="Companion" name="Moss Mouse" supports="Moss Mouse" default="ID_TEST_COMPANION_MOSS_MOUSE" /></rules>
  </element>
  <element name="Moss Mouse" type="Companion" source="Wisp Kit" id="ID_TEST_COMPANION_MOSS_MOUSE">
    <supports>Moss Mouse</supports>
    <setters>
      <set name="size">Tiny</set><set name="type">Beast</set><set name="alignment">unaligned</set>
      <set name="ac">12</set><set name="hp">3 (1d4 + 1)</set><set name="speed">20 ft.</set>
      <set name="strength">2</set><set name="dexterity">14</set><set name="constitution">10</set>
      <set name="intelligence">2</set><set name="wisdom">10</set><set name="charisma">4</set>
      <set name="challenge">0</set>
    </setters>
    <rules>
      <stat name="companion:stealth:misc" value="4" bonus="base" />
      <stat name="companion:wisdom:save:misc" value="2" bonus="base" />
    </rules>
  </element>
  <element name="Stone Toad" type="Companion" source="Wisp Kit" id="ID_TEST_COMPANION_STONE_TOAD">
    <supports>Stone Toad</supports>
    <setters>
      <set name="size">Small</set><set name="type">Beast</set><set name="alignment">unaligned</set>
      <set name="ac">14 (natural armor)</set><set name="hp">9 (2d6 + 2)</set><set name="speed">20 ft.</set>
      <set name="strength">10</set><set name="dexterity">8</set><set name="constitution">12</set>
      <set name="intelligence">2</set><set name="wisdom">10</set><set name="charisma">3</set>
      <set name="challenge">1/8</set>
    </setters>
  </element>
</elements>"#;

fn app() -> (tempfile::TempDir, AppState) {
    borealis_engine::invariants::enable_for_thread();
    let dir = tempfile::tempdir().expect("temp dir");
    let paths = AppPaths {
        content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
        data: dir.path().to_path_buf(),
    };
    let state = AppState::new(paths).expect("state");
    let req = ImportPackRequest {
        file_name: "wisp.xml".into(),
        content_base64: STANDARD.encode(PACK),
        format: None,
    };
    state.import_pack(&req, &NullSink).expect("install");
    (dir, state)
}

fn slot(snap: &CharacterSnapshot, pred: impl Fn(&str, &str) -> bool) -> Option<String> {
    snap.slots
        .iter()
        .find(|s| pred(&s.title, &s.element_type))
        .map(|s| s.id.clone())
}

/// A level 1 SRD fighter with the Lantern Bond feat and its companion chosen.
fn owner(app: &AppState) -> CharacterSnapshot {
    owner_with(app, FEAT, WISP)
}

/// A level 1 SRD fighter with `feat` and its `companion` chosen.
fn owner_with(app: &AppState, feat: &str, companion_id: &str) -> CharacterSnapshot {
    let request = NewCharacterRequest {
        name: "Brann".into(),
        ruleset: Ruleset::Dnd2014,
        starting_level: 1,
        gender: String::new(),
        ability_method: AbilityMethodDto::PointBuy,
        options: OptionsPatch::default(),
        portrait: None,
        group: None,
    };
    let s = app
        .create_character(Ruleset::Dnd2014, &request)
        .expect("create");
    let id = s.id.clone();
    let s = app
        .apply_selection(&id, &slot(&s, |t, _| t == "Race").expect("race"), HUMAN)
        .expect("race");
    let s = app
        .apply_selection(&id, &slot(&s, |t, _| t == "Class").expect("class"), FIGHTER)
        .expect("class");
    let _ = s;
    let s = app
        .add_pick(&id, AddedKind::Feat, feat, None)
        .expect("feat");
    match slot(&s, |_, ty| ty == "companion") {
        Some(companion)
            if s.companions
                .first()
                .and_then(|c| c.creature.as_ref())
                .is_none() =>
        {
            app.apply_selection(&id, &companion, companion_id)
                .expect("companion")
        }
        _ => s,
    }
}

fn granted(s: &CharacterSnapshot) -> &CompanionState {
    s.companions
        .iter()
        .find(|c| c.granted)
        .expect("the granted companion")
}

fn bonus(list: &[borealis_lib::content_dto::CompanionSkill], name: &str) -> i32 {
    list.iter().find(|x| x.name == name).expect(name).bonus
}

#[test]
fn a_granted_companion_takes_its_owners_numbers_from_the_rules() {
    let (_dir, app) = app();
    let s = owner(&app);
    let wisp = granted(&s);
    assert_eq!(wisp.creature.as_ref().map(|c| c.id.as_str()), Some(WISP));
    let str_mod = s.abilities.modifier.str;
    let pb = s.stats.proficiency_bonus;
    // "5 + your level", "12 + your Strength modifier": the owner's, not the leading number.
    assert_eq!(wisp.hp, 5 + 1, "{wisp:?}");
    assert_eq!(wisp.hp_note, None);
    assert_eq!(wisp.armor_class, 12 + str_mod);
    assert_eq!(wisp.armor_class_note, None);
    assert_eq!(wisp.speed, 10);
    // "Dex +3 plus PB" and "Perception +1 plus PB": the modifier plus the owner's PB.
    assert_eq!(bonus(&wisp.saves, "Dexterity"), 3 + pb);
    assert_eq!(bonus(&wisp.skills, "Perception"), 1 + pb);
    assert_eq!(wisp.passive_perception, 10 + 1 + pb);
}

#[test]
fn the_numbers_follow_the_owner_and_the_users_bonuses_stay_on_top() {
    let (_dir, app) = app();
    let s = owner(&app);
    let id = s.id.clone();
    let up = app.level_up(&id, "main").expect("level 2");
    assert_eq!(granted(&up).hp, 5 + 2);
    let edited = app
        .update_companion(
            &id,
            Some("granted"),
            &serde_json::json!({ "hitPointsBonus": 3, "armorClassBonus": 1 }),
        )
        .expect("bonuses");
    let wisp = granted(&edited);
    assert_eq!(wisp.hp, 5 + 2 + 3);
    assert_eq!(wisp.armor_class, 12 + edited.abilities.modifier.str + 1);
}

#[test]
fn an_added_companion_and_a_fixed_stat_block_print_as_before() {
    let (_dir, app) = app();
    let s = owner(&app);
    let s = app.add_companion(&s.id, Some(FIXED), None).expect("toad");
    let toad = s.companions.iter().find(|c| !c.granted).expect("added");
    assert_eq!((toad.hp, toad.armor_class, toad.speed), (9, 14, 20));
}

#[test]
fn a_granted_companion_without_companion_rules_prints_as_before() {
    let (_dir, app) = app();
    let s = owner_with(&app, TOAD_FEAT, FIXED);
    let toad = granted(&s);
    assert_eq!(toad.creature.as_ref().map(|c| c.id.as_str()), Some(FIXED));
    assert_eq!((toad.hp, toad.armor_class, toad.speed), (9, 14, 20));
    assert_eq!(bonus(&toad.saves, "Dexterity"), -1);
    assert_eq!(bonus(&toad.skills, "Stealth"), -1);
}

#[test]
fn a_rule_that_only_adds_misc_counts_for_a_skill_and_a_save() {
    let (_dir, app) = app();
    let s = owner_with(&app, MOUSE_FEAT, MOUSE);
    let mouse = granted(&s);
    assert_eq!(mouse.creature.as_ref().map(|c| c.id.as_str()), Some(MOUSE));
    // Dex 14 is +2 and Wis 10 is +0; the rules add 4 and 2 on top, with no proficiency rule.
    assert_eq!(bonus(&mouse.skills, "Stealth"), 2 + 4);
    assert_eq!(bonus(&mouse.saves, "Wisdom"), 2);
    assert_eq!(bonus(&mouse.saves, "Dexterity"), 2);
}
