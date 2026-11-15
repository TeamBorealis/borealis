//! WB-27 (homebrew creator spec 18.4, E2E flows 1-9): the "real character" half of each end-to-end flow, through the
//! real `AppState` on a temporary data folder with the bundled content. Each test makes the element with the creator's
//! commands (`homebrew_new_draft`, `homebrew_update_draft`, `homebrew_check_draft`, the trial, `homebrew_save_draft`,
//! export and import, Send/Receive), then a character of the engine takes it and the derived snapshot is checked.
//!
//! * Flow 1: a 2024 2nd-level spell on the Wizard list is offered to a level-3 Wizard's spellbook, taken, prepared and
//!   cast from a 2nd-level slot.
//! * Flow 2: a feat with a prerequisite, an ability increase and a proficiency is tried out, then taken at a level-4
//!   Fighter's feat choice; a Fighter failing the prerequisite is not offered it.
//! * Flow 3: a half-caster class with a subclass at 3, ASIs and an Equipment Package: Try it's ladder and the deep
//!   check are clean, a character levels it 1-20, and another multiclasses into Fighter and back.
//! * Flow 4: a 2024 species with a lineage choice and darkvision; a 2014 race with a subrace.
//! * Flow 5: a 2024 background with an ability distribution and an origin feat; a 2014 background with characteristics.
//! * Flow 6: a +1 magic weapon on a base-item expression, equipped, and armor with stealth disadvantage.
//! * Flow 7: a copy of the SRD Fireball edited and saved; a deep copy of the SRD Fighter taken by a character.
//! * Flow 8: a draft edited and never saved survives the app being killed (undo/redo is UI-side, not here).
//! * Flow 9: export, remove, import and edit again; a pack sent over the real device link arrives editable.
//!
//! The web mock suite `e2e/tests/hc-flows.e2e.ts` drives the UI half of the same flows (flow 10, the phone runs, is a
//! device run).

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use borealis_authoring::expr_build::condition::ConditionRow;
use borealis_authoring::expr_build::{Ability, ExprValue, GroupMode, Node};
use borealis_authoring::form::FormState;
use borealis_content::Ruleset as ContentRuleset;
use borealis_engine::commands::{self, slot_key, SlotPool};
use borealis_engine::{CharacterSnapshot, CharacterState, Choice, Engine, SlotKey};
use borealis_lib::content_dto::{ContentProgress, PackKind};
use borealis_lib::content_packs::EventSink;
use borealis_lib::content_preview::{ImportFileInput, PreviewImportRequest};
use borealis_lib::dto::Ruleset;
use borealis_lib::homebrew_dto::{
    HomebrewDraftView, HomebrewPackRequest, HomebrewTrialSetup, HomebrewTrialView,
};
use borealis_lib::state::{AppPaths, AppState};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

const LEVEL1: &str = "ID_LEVEL_1";
const FIGHTER: &str = "ID_2024_CLASS_FIGHTER";
const FIGHTER_MC: &str = "ID_2024_MULTICLASS_FIGHTER";
const WIZARD: &str = "ID_2024_CLASS_WIZARD";
const HUMAN: &str = "ID_2024_SPECIES_HUMAN";
const ALERT: &str = "ID_2024_FEAT_ALERT";
const FIREBALL: &str = "ID_2024_SPELL_FIREBALL";
const LONGSWORD: &str = "ID_2024_WEAPON_LONGSWORD";
const CLUB: &str = "ID_2024_WEAPON_CLUB";
const MARTIAL_MELEE: &str = "ID_INTERNAL_WEAPON_CATEGORY_MARTIAL_MELEE";
/// The highest character level.
const MAX_LEVEL: u32 = 20;
/// Str 16 (+3), Dex 14, Con 13, Int 10, Wis 12, Cha 8: a Fighter.
const STRONG: [i32; 6] = [16, 14, 13, 10, 12, 8];
/// Str 8: under any Strength 13 prerequisite.
const WEAK: [i32; 6] = [8, 15, 14, 13, 12, 10];
/// Int 15: a Wizard.
const SCHOLAR: [i32; 6] = [8, 14, 13, 15, 12, 10];
/// Str 14 and Wis 15: the half caster of flow 3, also able to multiclass into Fighter (Str 13).
const WARDEN_SCORES: [i32; 6] = [14, 12, 14, 10, 15, 8];

struct Quiet;
impl EventSink for Quiet {
    fn content_progress(&self, _: &ContentProgress) {}
    fn import_progress(&self, _: &ContentProgress) {}
    fn content_reloaded(&self, _: &str) {}
}

fn paths(data: &Path) -> AppPaths {
    AppPaths {
        content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
        data: data.to_path_buf(),
    }
}

fn create_pack(state: &AppState, name: &str, code: &str, edition: Ruleset) -> String {
    state
        .homebrew_create_pack(
            &HomebrewPackRequest {
                name: name.into(),
                code: code.into(),
                edition,
                author: None,
                description: None,
                license: None,
            },
            &Quiet,
        )
        .unwrap()
        .id
}

/// The app on a temporary data folder with the 2024 pack "Ember Isles" (code EI).
struct App {
    data: tempfile::TempDir,
    state: AppState,
    pack: String,
}

impl App {
    fn new() -> Self {
        let data = tempfile::tempdir().unwrap();
        let state = AppState::new(paths(data.path())).unwrap();
        let pack = create_pack(&state, "Ember Isles", "EI", Ruleset::Dnd2024);
        Self { data, state, pack }
    }

    fn new_draft(&self, pack: &str, kind: &str) -> HomebrewDraftView {
        self.state
            .homebrew_new_draft(pack, kind, None, None, None)
            .unwrap()
    }

    /// A copy of a loaded element (`deep`: with its features, traits and subclasses).
    fn copy(&self, kind: &str, id: &str, deep: bool) -> HomebrewDraftView {
        self.state
            .homebrew_new_draft(&self.pack, kind, Some(id), None, Some(deep))
            .unwrap()
    }

    /// A child of `kind` made inside the draft `parent` (`key`: inside that child form of it).
    fn child(
        &self,
        pack: &str,
        kind: &str,
        parent: &HomebrewDraftView,
        key: Option<&str>,
    ) -> HomebrewDraftView {
        self.state
            .homebrew_new_draft_in(pack, kind, None, Some(&parent.draft_id), key, None)
            .unwrap()
    }

    /// The draft with `change` applied to a copy of its form, as the UI sends an edit.
    fn edit(
        &self,
        v: &HomebrewDraftView,
        change: impl FnOnce(&mut FormState),
    ) -> HomebrewDraftView {
        let mut form = v.form.clone();
        change(&mut form);
        self.state
            .homebrew_update_draft(&v.draft_id, &form)
            .unwrap()
    }

    /// Save a draft that has no problem left: the saved ids.
    fn save(&self, v: &HomebrewDraftView) -> Vec<String> {
        assert_eq!(errors(v), Vec::<(String, String)>::new());
        self.state
            .homebrew_save_draft(&v.draft_id, &Quiet)
            .unwrap()
            .element_ids
    }

    /// The engine checks and the 1-20 level sweep (spec 8.1 step 6), warnings and errors only.
    fn deep(&self, v: &HomebrewDraftView) -> Vec<(String, String)> {
        self.state
            .homebrew_check_draft(&v.draft_id)
            .unwrap()
            .into_iter()
            .filter(|p| p.severity != "info")
            .map(|p| (p.code, p.message))
            .collect()
    }

    fn try_it(&self, v: &HomebrewDraftView, setup: HomebrewTrialSetup) -> HomebrewTrialView {
        self.state
            .homebrew_trial_start(&v.draft_id, &setup)
            .unwrap()
    }

    /// A saved element of `pack`, as on disk.
    fn saved(&self, pack: &str, id: &str) -> Value {
        let path = self.data.path().join("library").join(pack);
        let content: Value =
            serde_json::from_str(&std::fs::read_to_string(path.join("content.json")).unwrap())
                .unwrap();
        content["elements"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["id"] == id)
            .cloned()
            .unwrap_or_else(|| panic!("{id} saved"))
    }
}

fn errors(v: &HomebrewDraftView) -> Vec<(String, String)> {
    v.problems
        .iter()
        .filter(|p| p.severity == "error")
        .map(|p| (p.path.clone().unwrap_or_default(), p.message.clone()))
        .collect()
}

// ---- form values, as the UI writes them ----

/// The form with key `key` among `form` and its descendants.
fn form_mut<'f>(form: &'f mut FormState, key: &str) -> &'f mut FormState {
    fn find<'f>(f: &'f mut FormState, key: &str) -> Option<&'f mut FormState> {
        if f.key == key {
            return Some(f);
        }
        f.children.iter_mut().find_map(|c| find(c, key))
    }
    find(form, key).unwrap_or_else(|| panic!("form {key}"))
}

fn set(form: &mut FormState, key: &str, field: &str, v: Value) {
    form_mut(form, key).fields.insert(field.into(), v);
}

fn set_all(form: &mut FormState, key: &str, fields: Value) {
    for (k, v) in fields.as_object().expect("fields") {
        set(form, key, k, v.clone());
    }
}

/// The key of the last child of form `key` (the child the backend just made there).
fn last_child(v: &HomebrewDraftView, key: &str) -> String {
    let mut form = v.form.clone();
    form_mut(&mut form, key)
        .children
        .last()
        .map(|c| c.key.clone())
        .unwrap_or_else(|| panic!("a child of {key}"))
}

/// The key of the root's child named `name`.
fn child_named(form: &FormState, name: &str) -> String {
    form.children
        .iter()
        .find(|c| c.fields.get("name") == Some(&json!(name)))
        .map(|c| c.key.clone())
        .unwrap_or_else(|| panic!("a child named {name}"))
}

fn reference(id: &str, ty: &str) -> Value {
    json!({"id": id, "name": "", "type": ty})
}

fn doc(text: &str) -> Value {
    json!({"type": "doc", "content": [{"type": "paragraph", "content": [{"type": "text", "text": text}]}]})
}

/// A condition of every row, as the condition builder writes it.
fn all_of(rows: Vec<ConditionRow>) -> Value {
    let model = Node::group(GroupMode::All, rows.into_iter().map(Node::row).collect());
    serde_json::to_value(ExprValue::new(model)).unwrap()
}

/// A one-tag "From" filter, as the block dialog writes it.
fn from_tag(tag: &str) -> Value {
    json!({"value": {"shape": "model", "model": {"specific": false,
        "root": {"node": "row", "row": {"row": "tag", "tag": tag}}}}})
}

/// A rule block (the block dialog's values).
fn block(kind: &str, fields: Value) -> borealis_authoring::form::Block {
    serde_json::from_value(
        json!({"key": format!("u-{kind}"), "kind": kind, "sentence": "", "fields": fields}),
    )
    .unwrap()
}

/// A block that gives proficiency in `skill` (a Proficiency id).
fn grants_skill(id: &str, name: &str) -> borealis_authoring::form::Block {
    block(
        "grant",
        json!({"id": {"id": id, "name": name, "type": "Proficiency"}, "type": "Proficiency"}),
    )
}

/// `form` with the LEVELS entry of child `key` at `level`.
fn at_level(form: &mut FormState, key: &str, level: u32) {
    let levels = form
        .fields
        .get_mut("levels")
        .unwrap()
        .as_array_mut()
        .unwrap();
    for e in levels.iter_mut().filter(|e| e["child"] == json!(key)) {
        e["level"] = json!(level);
    }
}

// ---- characters, through the engine ----

fn pick(e: &Engine, s: &CharacterState, owner: &str, name: &str, id: &str) -> CharacterState {
    let snap = e.derive(s);
    let key = slot_key(&snap, owner, name, 1).unwrap_or_else(|| panic!("no slot {owner}/{name}"));
    choose(e, s, &key, id)
}

fn choose(e: &Engine, s: &CharacterState, key: &SlotKey, id: &str) -> CharacterState {
    commands::select(e, s, key, Choice::Element(id.into()))
        .unwrap_or_else(|err| panic!("select {id}: {err}"))
        .state
}

/// A 2024 character of `class` at level 1 with these scores.
fn first_level(e: &Engine, class: &str, scores: [i32; 6]) -> CharacterState {
    let s = commands::new_character(e, ContentRuleset::Dnd2024);
    let s = pick(e, &s, LEVEL1, "Class", class);
    commands::set_base_scores(e, &s, scores).unwrap().state
}

fn level_to(e: &Engine, mut s: CharacterState, level: u32) -> CharacterState {
    while s.level() < level {
        s = commands::level_up_main(e, &s, None).unwrap().state;
    }
    s
}

/// The first open choice offering `id`.
fn offering(snap: &CharacterSnapshot, id: &str) -> Option<SlotKey> {
    snap.slots
        .iter()
        .find(|x| x.chosen.is_none() && x.options.iter().any(|o| o.id == id))
        .map(|x| x.key.clone())
}

fn element_ids(snap: &CharacterSnapshot) -> Vec<String> {
    snap.elements.iter().map(|x| x.element.id.clone()).collect()
}

fn score(snap: &CharacterSnapshot, ability: &str) -> i32 {
    snap.abilities
        .iter()
        .find(|a| a.name == ability)
        .map(|a| a.score)
        .unwrap_or_else(|| panic!("{ability}"))
}

/// The list item of choice `owner`/`name` whose text is `text`, picked.
fn pick_item(e: &Engine, s: &CharacterState, owner: &str, text: &str) -> CharacterState {
    let snap = e.derive(s);
    let slot = snap
        .slots
        .iter()
        .find(|x| x.owner.id == owner && x.list_items.iter().any(|(_, t)| t.contains(text)))
        .unwrap_or_else(|| panic!("a choice of {owner} with {text}"));
    let item = slot
        .list_items
        .iter()
        .find(|(_, t)| t.contains(text))
        .unwrap()
        .0;
    commands::select(e, s, &slot.key, Choice::ListItem(item))
        .expect("list pick")
        .state
}

fn trial_features(view: &HomebrewTrialView) -> Vec<String> {
    serde_json::to_value(&view.snapshot.play.features)
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|f| f["name"].as_str().map(str::to_string))
        .collect()
}

// ---- flow 1: a 2024 spell ----

#[test]
fn flow_1_a_2024_spell_is_offered_to_a_wizard_who_takes_and_casts_it() {
    /// A level-3 Wizard has four 1st- and two 2nd-level slots.
    const WIZARD_3_SLOTS: [i32; 9] = [4, 2, 0, 0, 0, 0, 0, 0, 0];
    let app = App::new();
    let v = app.new_draft(&app.pack, "spell");
    let v = app.edit(&v, |f| {
        set_all(
            f,
            "e",
            json!({"name": "Cinder Arc", "school": "Evocation", "level": "2", "range": "60 feet",
                "spellLists": ["Wizard"], "description": doc("An arc of cinders.")}),
        );
    });
    let ids = app.save(&v);
    assert_eq!(ids, ["ID_EI_SPELL_CINDER_ARC"]);
    let spell = &ids[0];

    let hub = app.state.hub().unwrap();
    let e = &hub.engine;
    // a level-1 Wizard can't learn a 2nd-level spell yet
    let first = first_level(e, WIZARD, SCHOLAR);
    assert!(offering(&e.derive(&first), spell).is_none());
    // at level 3 the spellbook choice offers it; taken, it is in the Wizard's spells at its level
    let s = level_to(e, first, 3);
    let snap = e.derive(&s);
    let key = offering(&snap, spell).expect("a level-3 Wizard's spell choice offers it");
    let s = choose(e, &s, &key, spell);
    let s = commands::set_prepared(e, &s, "Wizard", spell, true)
        .unwrap()
        .state;
    let snap = e.derive(&s);
    let wizard = snap.caster("Wizard").expect("the Wizard casts");
    assert_eq!(wizard.slots, WIZARD_3_SLOTS);
    let entry = wizard
        .spells
        .iter()
        .find(|x| x.id == *spell)
        .expect("the spell is the Wizard's");
    assert_eq!((entry.level, entry.prepared), (2, true));
    // cast with a 2nd-level slot: one of the two is used, and there is no third
    let cast = commands::spend_spell_slots(&snap, &s, SlotPool::Level(2), 1)
        .unwrap()
        .state;
    assert_eq!(cast.play.spell_slots_used.get(&2), Some(&1));
    let after = e.derive(&cast);
    assert!(commands::spend_spell_slots(&after, &cast, SlotPool::Level(2), 2).is_err());
}

// ---- flow 2: a feat with a prerequisite ----

const FEAT_ID: &str = "ID_EI_FEAT_IRON_GRIP";
const FEAT_NAME: &str = "Iron Grip";
const FIGHTER_FEAT_CHOICE: &str = "Ability Score Improvement (Fighter 4)";

/// A General feat: level 4 and Strength 13, Strength or Dexterity +1 (the SRD Grappler's increase), Intimidation.
fn iron_grip(app: &App) -> HomebrewDraftView {
    let grappler = app.copy("feat", "ID_2024_FEAT_GRAPPLER", false);
    let increase = grappler
        .form
        .blocks
        .iter()
        .find(|b| b.kind == "ability_increase")
        .expect("the increase block")
        .clone();
    app.state
        .homebrew_discard_draft(&grappler.draft_id)
        .unwrap();
    let v = app.new_draft(&app.pack, "feat");
    app.edit(&v, |f| {
        set_all(
            f,
            "e",
            json!({"name": FEAT_NAME, "category": "General", "description": doc("Your grip is iron."),
                "requirements": all_of(vec![
                    ConditionRow::CharacterLevel { min: 4 },
                    ConditionRow::Ability { ability: Ability::Strength, min: 13 },
                ])}),
        );
        f.blocks = vec![
            increase,
            grants_skill("ID_PROFICIENCY_SKILL_INTIMIDATION", "Intimidation"),
        ];
    })
}

/// A 2024 Fighter at level 4 with these scores: the level-4 feat choice, if it offers the feat.
fn fighter_4(e: &Engine, scores: [i32; 6]) -> (CharacterState, SlotKey, bool) {
    let s = level_to(e, first_level(e, FIGHTER, scores), 4);
    let snap = e.derive(&s);
    let slot = snap
        .slots
        .iter()
        .find(|x| x.name == FIGHTER_FEAT_CHOICE)
        .unwrap_or_else(|| panic!("no {FIGHTER_FEAT_CHOICE}"));
    let offered = slot.options.iter().any(|o| o.id == FEAT_ID);
    (s.clone(), slot.key.clone(), offered)
}

#[test]
fn flow_2_a_feat_with_a_prerequisite_is_tried_then_taken_at_level_4() {
    let app = App::new();
    let v = iron_grip(&app);
    assert_eq!(errors(&v), Vec::<(String, String)>::new());
    // Try it: a Fighter takes it at its first feat choice, level 4
    let view = app.try_it(&v, HomebrewTrialSetup::default());
    assert_eq!(view.snapshot.level.total, 4);
    assert!(trial_features(&view).iter().any(|n| n == FEAT_NAME));
    assert!(
        view.problems.iter().all(|p| p.severity != "error"),
        "{:?}",
        view.problems
    );
    app.state.homebrew_trial_end(&view.trial_id).unwrap();
    assert_eq!(app.save(&v), [FEAT_ID]);
    assert_eq!(
        app.saved(&app.pack, FEAT_ID)["requirements"],
        "[character:4]&&[str:13]"
    );

    let hub = app.state.hub().unwrap();
    let e = &hub.engine;
    let (_, _, offered) = fighter_4(e, WEAK);
    assert!(!offered, "Strength 8 fails the prerequisite: not offered");
    let (s, key, offered) = fighter_4(e, STRONG);
    assert!(offered, "the level-4 feat choice offers it");
    let before = e.derive(&s);
    assert!(!before.skill("intimidation").is_some_and(|x| x.proficient));
    let s = choose(e, &s, &key, FEAT_ID);
    let s = pick_item(e, &s, FEAT_ID, "Strength");
    let snap = e.derive(&s);
    assert!(snap.has_element(FEAT_ID));
    assert_eq!(score(&snap, "strength"), score(&before, "strength") + 1);
    assert!(
        snap.skill("intimidation").is_some_and(|x| x.proficient),
        "the proficiency is granted"
    );
}

// ---- flow 3: a half-caster class 1-20 ----

const WARDEN: &str = "Dawn Warden";
const WARDEN_ID: &str = "ID_EI_CLASS_DAWN_WARDEN";
const OATH: &str = "Warden Oath";
const SUBCLASS_ID: &str = "ID_EI_SUBCLASS_OATH_OF_EMBERS";
const VOW_ID: &str = "ID_EI_SUBCLASS_FEATURE_OATH_OF_EMBERS_EMBER_VOW";
/// The 2024 half caster's slots (round up): at level 5 and at level 20.
const HALF_CASTER_5: [i32; 9] = [4, 2, 0, 0, 0, 0, 0, 0, 0];
const HALF_CASTER_20: [i32; 9] = [4, 3, 3, 3, 2, 0, 0, 0, 0];
/// Spells prepared at levels 1-20.
const PREPARED: [u32; 20] = [
    2, 3, 4, 5, 6, 6, 7, 7, 9, 9, 10, 10, 11, 11, 12, 12, 14, 14, 15, 15,
];

/// The Spellcasting feature's fields: a Wisdom half caster (round up) preparing from the Paladin list.
fn spellcasting_fields() -> Value {
    json!({"name": "Spellcasting", "casts": true, "casterType": "halfUp", "ability": "Wisdom",
        "preparation": "prepared", "spellList": "Paladin", "swap": true, "prepared": PREPARED,
        "preparedChange": "longRestOne"})
}

/// A new 2024 class with a Spellcasting feature and an Equipment Package made inside it, and its subclass choice named.
fn dawn_warden(app: &App) -> HomebrewDraftView {
    let v = app.new_draft(&app.pack, "class");
    let v = app.child(&app.pack, "classFeature", &v, None);
    let casting = last_child(&v, "e");
    let v = app.child(&app.pack, "equipmentPackage", &v, None);
    let package = last_child(&v, "e");
    let subclass = child_named(&v.form, "Subclass");
    app.edit(&v, |f| {
        set_all(
            f,
            "e",
            json!({"name": WARDEN, "hitDie": "d10", "savingThrows": ["Wisdom", "Charisma"],
                "skills": {"count": 2, "skills": [
                    reference("ID_PROFICIENCY_SKILL_ATHLETICS", "Proficiency"),
                    reference("ID_PROFICIENCY_SKILL_INSIGHT", "Proficiency"),
                    reference("ID_PROFICIENCY_SKILL_PERSUASION", "Proficiency")]},
                "armor": [reference("ID_PROFICIENCY_ARMOR_PROFICIENCY_LIGHT_ARMOR", "Proficiency")],
                "weapons": [reference("ID_PROFICIENCY_WEAPON_PROFICIENCY_SIMPLE_WEAPONS", "Proficiency")],
                "mcRequirements": all_of(vec![ConditionRow::Ability { ability: Ability::Wisdom, min: 13 }])}),
        );
        set_all(f, &casting, spellcasting_fields());
        set_all(
            f,
            &package,
            json!({"name": "Starting Equipment (Dawn Warden)", "options": [{"label": "A", "items": [],
                "coins": {"cp": 0, "sp": 0, "ep": 0, "gp": 50, "pp": 0}}]}),
        );
        set_all(f, &subclass, json!({"name": OATH, "subclassLabel": OATH}));
    })
}

/// A subclass of the saved Dawn Warden with one feature at level 3.
fn oath_of_embers(app: &App) -> Vec<String> {
    let v = app.new_draft(&app.pack, "subclass");
    let v = app.edit(&v, |f| {
        set_all(
            f,
            "e",
            json!({"name": "Oath of Embers", "description": doc("Wardens of the ember."),
                "parentClass": {"id": WARDEN_ID, "name": WARDEN, "type": "Class"}}),
        );
    });
    assert_eq!(v.form.fields["levelGained"], 3);
    let v = app.child(&app.pack, "subclassFeature", &v, None);
    let vow = last_child(&v, "e");
    let v = app.edit(&v, |f| {
        set_all(
            f,
            &vow,
            json!({"name": "Ember Vow", "description": doc("You vow.")}),
        );
        at_level(f, &vow, 3);
    });
    assert_eq!(app.deep(&v), Vec::<(String, String)>::new());
    app.save(&v)
}

/// Try it's ladder on the saved class: clean on every level, the half caster's slots, the subclass choice at 3.
fn ladder_is_clean(app: &App) {
    let v = app
        .state
        .homebrew_open_draft(&app.pack, Some(WARDEN_ID), None)
        .unwrap();
    assert_eq!(app.deep(&v), Vec::<(String, String)>::new());
    let view = app.try_it(&v, HomebrewTrialSetup::default());
    assert_eq!(view.ladder.len(), MAX_LEVEL as usize);
    let noisy: Vec<(u32, String)> = view
        .ladder
        .iter()
        .flat_map(|r| {
            r.problems
                .iter()
                .filter(|p| p.severity != "info")
                .map(|p| (r.level, p.message.clone()))
        })
        .collect();
    assert_eq!(noisy, Vec::<(u32, String)>::new(), "the ladder is clean");
    let slots = |level: usize| -> Vec<u32> { view.ladder[level - 1].slots.clone() };
    let wanted =
        |t: [i32; 9]| -> Vec<u32> { t.iter().map(|n| u32::try_from(*n).unwrap()).collect() };
    assert_eq!(slots(5), wanted(HALF_CASTER_5));
    assert_eq!(slots(20), wanted(HALF_CASTER_20));
    assert!(
        view.ladder[2].choices.iter().any(|c| c.contains(OATH)),
        "{:?}",
        view.ladder[2].choices
    );
    app.state.homebrew_trial_end(&view.trial_id).unwrap();
}

/// A new character levels the class 1-20: the package at 1, slots at 5 and 20, the subclass at 3, ASIs at 4/8/12/16.
fn levels_to_20(e: &Engine) {
    const ASI_LEVELS: [u32; 4] = [4, 8, 12, 16];
    let mut s = first_level(e, WARDEN_ID, WARDEN_SCORES);
    let snap = e.derive(&s);
    assert_eq!(snap.classes[0].hit_die, Some(10));
    assert!(
        snap.equipment_packages
            .iter()
            .any(|p| p.package.starts_with("ID_EI_EQUIPMENT_PACKAGE_")),
        "the class's Equipment Package"
    );
    let mut asi = Vec::new();
    while s.level() < MAX_LEVEL {
        s = commands::level_up_main(e, &s, None).unwrap().state;
        let snap = e.derive(&s);
        let level = s.level();
        if level == 3 {
            let key =
                offering(&snap, SUBCLASS_ID).expect("the subclass choice at 3 offers the subclass");
            s = choose(e, &s, &key, SUBCLASS_ID);
            assert!(e.derive(&s).has_element(VOW_ID), "its level-3 feature");
        }
        let name = format!("Ability Score Improvement ({WARDEN} {level})");
        if snap.slots.iter().any(|x| x.name == name) {
            asi.push(level);
        }
        if level == 5 {
            assert_eq!(snap.caster(WARDEN).expect("casts").slots, HALF_CASTER_5);
        }
    }
    assert_eq!(asi, ASI_LEVELS);
    let snap = e.derive(&s);
    assert_eq!(snap.level, MAX_LEVEL);
    assert_eq!(snap.caster(WARDEN).unwrap().slots, HALF_CASTER_20);
}

/// A level-5 Dawn Warden takes a Fighter level, then a sixth level of its own class.
fn multiclasses_into_fighter_and_back(e: &Engine) {
    let s = level_to(e, first_level(e, WARDEN_ID, WARDEN_SCORES), 5);
    let opened = commands::level_up_new_multiclass(e, &s, None)
        .unwrap()
        .state;
    let snap = e.derive(&opened);
    let slot = snap
        .slots
        .iter()
        .find(|x| x.name.starts_with("Multiclass"))
        .expect("the multiclass choice");
    assert!(
        slot.options.iter().any(|o| o.id == FIGHTER_MC),
        "Strength 14 qualifies"
    );
    let s = choose(e, &opened, &slot.key.clone(), FIGHTER_MC);
    let s = commands::level_up_main(e, &s, None).unwrap().state;
    let snap = e.derive(&s);
    let classes: Vec<(String, u32)> = snap
        .classes
        .iter()
        .map(|c| {
            (
                c.class.as_ref().map(|r| r.id.clone()).unwrap_or_default(),
                c.level,
            )
        })
        .collect();
    // a multiclass level's class is the class's multiclass entry
    assert_eq!(
        classes,
        [(WARDEN_ID.to_string(), 6), (FIGHTER_MC.to_string(), 1)]
    );
    assert_eq!(snap.level, 7);
    // Dawn Warden 6 alone casts: a half caster of level 6 has the slots of level 5
    assert_eq!(snap.caster(WARDEN).unwrap().slots, HALF_CASTER_5);
}

#[test]
fn flow_3_a_half_caster_class_with_a_subclass_levels_to_20_and_multiclasses() {
    let app = App::new();
    let v = dawn_warden(&app);
    assert_eq!(errors(&v), Vec::<(String, String)>::new());
    assert_eq!(app.deep(&v), Vec::<(String, String)>::new());
    let ids = app.save(&v);
    assert_eq!(ids[0], WARDEN_ID);
    assert_eq!(oath_of_embers(&app)[0], SUBCLASS_ID);
    ladder_is_clean(&app);
    let hub = app.state.hub().unwrap();
    levels_to_20(&hub.engine);
    multiclasses_into_fighter_and_back(&hub.engine);
}

// ---- flow 4: species and races ----

#[test]
fn flow_4_a_species_with_a_lineage_choice_and_darkvision() {
    /// The species' darkvision range in feet.
    const DARKVISION: i32 = 60;
    let app = App::new();
    let v = app.new_draft(&app.pack, "species");
    let v = app.child(&app.pack, "racialTrait", &v, None);
    let lineage = last_child(&v, "e");
    let v = app.child(&app.pack, "speciesTraitOption", &v, Some(&lineage));
    let ash = last_child(&v, &lineage);
    let v = app.child(&app.pack, "speciesTraitOption", &v, Some(&lineage));
    let cinder = last_child(&v, &lineage);
    let v = app.edit(&v, |f| {
        set_all(
            f,
            "e",
            json!({"name": "Emberkin", "darkvision": DARKVISION}),
        );
        set(f, &lineage, "name", json!("Ember Lineage"));
        set(f, &ash, "name", json!("Ash"));
        set(f, &cinder, "name", json!("Cinder"));
        form_mut(f, &ash).blocks.push(grants_skill(
            "ID_PROFICIENCY_SKILL_PERCEPTION",
            "Perception",
        ));
    });
    let ids = app.save(&v);
    let [species, lineage_id, ash_id, cinder_id] = <[String; 4]>::try_from(ids).unwrap();

    let hub = app.state.hub().unwrap();
    let e = &hub.engine;
    let s = commands::new_character(e, ContentRuleset::Dnd2024);
    let s = pick(e, &s, LEVEL1, "Race", &species);
    let snap = e.derive(&s);
    let options: Vec<&str> = snap
        .slot(&lineage_id, "Ember Lineage")
        .expect("the lineage choice")
        .options
        .iter()
        .map(|o| o.id.as_str())
        .collect();
    assert_eq!(options, [ash_id.as_str(), cinder_id.as_str()]);
    let s = pick(e, &s, &lineage_id, "Ember Lineage", &ash_id);
    let snap = e.derive(&s);
    assert!(snap.has_element(&ash_id));
    assert!(
        snap.skill("perception").is_some_and(|x| x.proficient),
        "the option's rule"
    );
    let dark = snap
        .senses
        .iter()
        .find(|x| x.name == "darkvision")
        .expect("darkvision");
    assert_eq!(dark.range, DARKVISION);
}

#[test]
fn flow_4_a_2014_race_with_a_subrace_gives_both_increases() {
    let app = App::new();
    let pack = create_pack(&app.state, "Old World", "OW", Ruleset::Dnd2014);
    let race = app.new_draft(&pack, "race");
    let race = app.edit(&race, |f| {
        set_all(f, "e", json!({"name": "Ashlander", "constitution": 2}));
        f.blocks.push(block(
            "select",
            json!({"type": "Sub Race", "name": "Ashlander Subrace", "supports": from_tag("Ashlander")}),
        ));
    });
    let race_id = app.save(&race)[0].clone();
    let sub = app.new_draft(&pack, "subRace");
    let sub = app.edit(&sub, |f| {
        set_all(
            f,
            "e",
            json!({"name": "Ridge Ashlander", "parent": "Ashlander", "wisdom": 1}),
        );
    });
    let sub_id = app.save(&sub)[0].clone();

    let hub = app.state.hub().unwrap();
    let e = &hub.engine;
    let s = commands::new_character(e, ContentRuleset::Dnd2014);
    let base = e.derive(&s);
    let s = pick(e, &s, LEVEL1, "Race", &race_id);
    let snap = e.derive(&s);
    let key = offering(&snap, &sub_id).expect("the race's subrace choice offers it");
    let s = choose(e, &s, &key, &sub_id);
    let snap = e.derive(&s);
    assert_eq!(
        score(&snap, "constitution"),
        score(&base, "constitution") + 2
    );
    assert_eq!(score(&snap, "wisdom"), score(&base, "wisdom") + 1);
}

// ---- flow 5: backgrounds ----

#[test]
fn flow_5_a_2024_background_distributes_scores_and_gives_its_origin_feat() {
    let app = App::new();
    let v = app.new_draft(&app.pack, "background");
    let v = app.edit(&v, |f| {
        set_all(
            f,
            "e",
            json!({"name": "Ember Warden", "abilityScores": ["Strength", "Constitution", "Wisdom"],
                "originFeat": {"feat": reference(ALERT, "Feat")},
                "skills": [reference("ID_PROFICIENCY_SKILL_NATURE", "Proficiency"),
                    reference("ID_PROFICIENCY_SKILL_SURVIVAL", "Proficiency")]}),
        );
    });
    let background = app.save(&v)[0].clone();

    let hub = app.state.hub().unwrap();
    let e = &hub.engine;
    let s = commands::new_character(e, ContentRuleset::Dnd2024);
    let s = pick(e, &s, LEVEL1, "Race", HUMAN);
    let s = pick(e, &s, LEVEL1, "Class", FIGHTER);
    let s = commands::set_base_scores(e, &s, STRONG).unwrap().state;
    let s = pick(e, &s, LEVEL1, "Background", &background);
    let before = e.derive(&s);
    let s = pick_item(e, &s, &background, "Strength +2, Wisdom +1");
    let snap = e.derive(&s);
    assert_eq!(score(&snap, "strength"), score(&before, "strength") + 2);
    assert_eq!(score(&snap, "wisdom"), score(&before, "wisdom") + 1);
    assert_eq!(score(&snap, "constitution"), score(&before, "constitution"));
    assert!(snap.has_element(ALERT), "the origin feat");
    for skill in ["nature", "survival"] {
        assert!(snap.skill(skill).is_some_and(|x| x.proficient), "{skill}");
    }
}

#[test]
fn flow_5_a_2014_background_offers_its_characteristics() {
    let app = App::new();
    let pack = create_pack(&app.state, "Old World", "OW", Ruleset::Dnd2014);
    let lists = [
        (
            "personalityTraits",
            "Personality Trait",
            vec!["I hum.", "I count stars.", "I keep maps."],
        ),
        ("ideals", "Ideal", vec!["Freedom.", "Duty."]),
        ("bonds", "Bond", vec!["My caravan."]),
        ("flaws", "Flaw", vec!["I boast.", "I wander off."]),
    ];
    let v = app.new_draft(&pack, "background");
    let v = app.edit(&v, |f| {
        set_all(
            f,
            "e",
            json!({"name": "Wayfarer", "skills": [reference("ID_PROFICIENCY_SKILL_SURVIVAL", "Proficiency"),
                reference("ID_PROFICIENCY_SKILL_ATHLETICS", "Proficiency")]}),
        );
        for (field, _, items) in &lists {
            set(f, "e", field, json!({"count": 1, "items": items}));
        }
    });
    let background = app.save(&v)[0].clone();

    let hub = app.state.hub().unwrap();
    let e = &hub.engine;
    let s = commands::new_character(e, ContentRuleset::Dnd2014);
    let s = pick(e, &s, LEVEL1, "Background", &background);
    let snap = e.derive(&s);
    for (_, choice, items) in &lists {
        let offered: Vec<&str> = snap
            .slot(&background, choice)
            .unwrap_or_else(|| panic!("the {choice} choice"))
            .list_items
            .iter()
            .map(|(_, t)| t.as_str())
            .collect();
        assert_eq!(offered, *items, "{choice}");
    }
    let s = pick_item(e, &s, &background, "Duty.");
    let ideal = e.derive(&s);
    assert!(ideal
        .slot(&background, "Ideal")
        .unwrap()
        .list_choice
        .is_some());
    assert!(snap.skill("survival").is_some_and(|x| x.proficient));
}

// ---- flow 6: magic weapon and armor ----

/// A +1 magic weapon on any martial melee weapon (a base-item expression).
fn emberbrand(app: &App) -> String {
    let martial = json!({"value": {"shape": "model", "model": {"specific": true,
        "root": {"node": "row", "row": {"row": "element", "id": MARTIAL_MELEE}}}}});
    let v = app.new_draft(&app.pack, "magicItem");
    let v = app.edit(&v, |f| {
        set_all(
            f,
            "e",
            json!({"name": "Emberbrand", "magicType": "Weapon", "rarity": "Uncommon",
                "appliesToWeapon": martial, "enhancement": 1, "description": doc("It glows.")}),
        );
    });
    app.save(&v)[0].clone()
}

/// Heavy armor of AC 17 with stealth disadvantage, as a user fills the form: "Worn or held on" is left as a new armor
/// starts it. Like a new weapon (which starts in one hand), it must be wearable: the engine equips only items worn or
/// held somewhere, and every SRD body armor is `slot` body.
fn ember_plate(app: &App) -> String {
    let v = app.new_draft(&app.pack, "armor");
    let v = app.edit(&v, |f| {
        set_all(
            f,
            "e",
            json!({"name": "Ember Plate", "armorType": "Heavy", "baseAc": 17, "stealth": true, "strength": 15}),
        );
    });
    app.save(&v)[0].clone()
}

/// `s` with `base` (adorned by `adorner`) added and equipped, and its line's uid.
fn wield(
    e: &Engine,
    s: &CharacterState,
    base: &str,
    adorner: Option<&str>,
) -> (CharacterState, String) {
    let added = match adorner {
        Some(a) => commands::add_adorned_item(e, s, base, a, 1),
        None => commands::add_item(e, s, base, 1),
    }
    .unwrap()
    .state;
    let uid = added.inventory.items.last().unwrap().uid.clone();
    let worn = commands::equip(e, &added, &uid, None)
        .unwrap_or_else(|err| panic!("{base} is worn or held: {err}"))
        .state;
    (worn, uid)
}

#[test]
fn flow_6_a_plus_1_weapon_on_a_base_item_expression_hits_one_better() {
    let app = App::new();
    let brand = emberbrand(&app);
    let hub = app.state.hub().unwrap();
    let e = &hub.engine;
    // it goes on a martial melee weapon, not on a simple one
    let uni = e.universe(ContentRuleset::Dnd2024);
    let item = uni.get(&brand).expect("loaded");
    assert!(borealis_engine::item_base::needs_base(uni, item));
    assert!(borealis_engine::item_base::can_adorn(
        item,
        uni.get(LONGSWORD).unwrap()
    ));
    assert!(!borealis_engine::item_base::can_adorn(
        item,
        uni.get(CLUB).unwrap()
    ));

    let fighter = first_level(e, FIGHTER, STRONG);
    let attack = |s: &CharacterState, uid: &str| {
        e.derive(s)
            .attacks
            .into_iter()
            .find(|a| a.uid == uid)
            .unwrap_or_else(|| panic!("an attack of {uid}"))
    };
    let (plain, uid) = wield(e, &fighter, LONGSWORD, None);
    let plain = attack(&plain, &uid);
    let (magic, uid) = wield(e, &fighter, LONGSWORD, Some(&brand));
    let magic = attack(&magic, &uid);
    assert_eq!(magic.attack, plain.attack + 1, "+1 to hit");
    // Strength +3: 1d8+3 plain, 1d8+4 with the +1
    assert!(plain.damage.starts_with("1d8+3"), "{}", plain.damage);
    assert!(
        magic.damage.starts_with("1d8+4"),
        "+1 damage: {}",
        magic.damage
    );
}

#[test]
fn flow_6_armor_with_stealth_disadvantage_is_worn_and_the_sheet_shows_it() {
    /// Armor Class of Ember Plate (heavy armor adds no Dexterity).
    const PLATE_AC: i32 = 17;
    let app = App::new();
    let plate = ember_plate(&app);
    let hub = app.state.hub().unwrap();
    let e = &hub.engine;
    let fighter = first_level(e, FIGHTER, STRONG);
    assert!(!e.derive(&fighter).stealth_disadvantage);
    let (armored, _) = wield(e, &fighter, &plate, None);
    let snap = e.derive(&armored);
    assert!(
        snap.stealth_disadvantage,
        "the sheet shows stealth disadvantage"
    );
    assert_eq!(snap.armor_class, PLATE_AC);
}

// ---- flow 7: copies of SRD elements ----

#[test]
fn flow_7_a_copied_fireball_edited_and_saved_is_a_spell_of_its_own() {
    /// The copy is made a 4th-level spell: a Wizard learns it at 7, not at 5 (where Fireball is learned).
    const NEW_LEVEL: u32 = 4;
    let app = App::new();
    let v = app.copy("spell", FIREBALL, false);
    assert_eq!(v.form.fields["name"], json!("Fireball (copy)"));
    let v = app.edit(&v, |f| {
        set_all(
            f,
            "e",
            json!({"name": "Emberstorm", "level": NEW_LEVEL.to_string()}),
        );
    });
    let id = app.save(&v)[0].clone();
    assert_eq!(id, "ID_EI_SPELL_EMBERSTORM");

    let hub = app.state.hub().unwrap();
    assert_eq!(hub.store.element(&id).unwrap().name, "Emberstorm");
    assert_eq!(
        hub.store.element(FIREBALL).unwrap().name,
        "Fireball",
        "the original stays"
    );
    let e = &hub.engine;
    let at_5 = e.derive(&level_to(e, first_level(e, WIZARD, SCHOLAR), 5));
    assert!(offering(&at_5, FIREBALL).is_some());
    assert!(offering(&at_5, &id).is_none(), "a 4th-level spell");
    let s = level_to(e, first_level(e, WIZARD, SCHOLAR), 7);
    let key = offering(&e.derive(&s), &id).expect("offered at 7");
    let snap = e.derive(&choose(e, &s, &key, &id));
    let entry = snap
        .caster("Wizard")
        .unwrap()
        .spells
        .iter()
        .find(|x| x.id == id)
        .expect("in the Wizard's spells")
        .clone();
    assert_eq!(entry.level, NEW_LEVEL);
}

#[test]
fn flow_7_a_deep_copied_srd_fighter_is_taken_by_a_character() {
    let app = App::new();
    let v = app.copy("class", FIGHTER, true);
    assert!(
        v.form.children.len() > 1,
        "its features and subclasses are copied"
    );
    assert_eq!(app.deep(&v), Vec::<(String, String)>::new());
    let ids = app.save(&v);
    let copy = ids[0].clone();
    assert_eq!(copy, "ID_EI_CLASS_FIGHTER_COPY");
    let subclass = ids
        .iter()
        .find(|i| i.starts_with("ID_EI_SUBCLASS_") && !i.starts_with("ID_EI_SUBCLASS_FEATURE_"))
        .expect("a copied subclass")
        .clone();

    let hub = app.state.hub().unwrap();
    let e = &hub.engine;
    let s = level_to(e, first_level(e, &copy, STRONG), 3);
    let key = offering(&e.derive(&s), &subclass)
        .expect("the copy's subclass choice offers the copied subclass");
    let s = level_to(e, choose(e, &s, &key, &subclass), 4);
    let got = element_ids(&e.derive(&s));
    assert!(got.contains(&copy) && got.contains(&subclass), "{got:?}");
    assert!(
        got.iter()
            .any(|i| i.starts_with("ID_EI_CLASS_FEATURE_FIGHTER_COPY_")),
        "{got:?}"
    );
    assert!(
        !got.iter()
            .any(|i| i.starts_with("ID_2024_CLASS_FEATURE_FIGHTER")
                || i.starts_with("ID_2024_SUBCLASS")),
        "no original feature: {got:?}"
    );
}

// ---- flow 8: a draft survives a kill ----

#[test]
fn flow_8_a_draft_edited_when_the_app_is_killed_is_restored() {
    let app = App::new();
    let saved = {
        let v = app.new_draft(&app.pack, "spell");
        let v = app.edit(&v, |f| {
            set_all(
                f,
                "e",
                json!({"name": "Ember Lance", "school": "Evocation"}),
            );
        });
        app.save(&v)[0].clone()
    };
    // a saved spell opened and edited, and a new one never saved: neither is saved or discarded
    let open = app
        .state
        .homebrew_open_draft(&app.pack, Some(&saved), None)
        .unwrap();
    let edited = app.edit(&open, |f| set(f, "e", "range", json!("90 feet")));
    let fresh = app.new_draft(&app.pack, "spell");
    let fresh = app.edit(&fresh, |f| {
        set_all(
            f,
            "e",
            json!({"name": "Half Done", "school": "Abjuration", "level": "3"}),
        );
    });
    // the kill: no destructor runs, nothing is flushed
    let App { data, state, pack } = app;
    std::mem::forget(state);

    let state = AppState::new(paths(data.path())).unwrap();
    // the workspace lists a never-saved element from its summary (item IDpe8): this pack, no element, its kind
    let summaries = state.homebrew_list_drafts().unwrap();
    let new_one = summaries
        .iter()
        .find(|d| d.draft_id == fresh.draft_id)
        .expect("the never-saved draft is listed after the restart");
    assert_eq!(new_one.pack_id, pack);
    assert_eq!(new_one.element_id, None);
    assert_eq!(new_one.kind, "spell");
    let mut listed: Vec<(String, String)> = state
        .homebrew_list_drafts()
        .unwrap()
        .into_iter()
        .map(|d| (d.draft_id, d.name))
        .collect();
    listed.sort();
    let mut want = vec![
        (edited.draft_id.clone(), "Ember Lance".to_string()),
        (fresh.draft_id.clone(), "Half Done".to_string()),
    ];
    want.sort();
    assert_eq!(listed, want);
    let back = state
        .homebrew_open_draft(&pack, Some(&saved), None)
        .unwrap();
    assert_eq!(back.draft_id, edited.draft_id, "resumed");
    assert_eq!(back.form.fields["range"], json!("90 feet"));
    let again = state
        .homebrew_open_draft(&pack, None, Some(&fresh.draft_id))
        .unwrap();
    assert_eq!(again.form.fields["name"], json!("Half Done"));
    assert_eq!(again.form.fields["level"], json!("3"));
    assert_eq!(again.form.fields["school"], json!("Abjuration"));
}

// ---- flow 9: export, remove, import; Send/Receive ----

#[test]
fn flow_9_export_remove_import_and_edit_again() {
    let app = App::new();
    let v = app.copy("spell", FIREBALL, false);
    let spell = app.save(&v)[0].clone();
    let file = app.state.homebrew_export_file(&app.pack, true).unwrap();
    app.state.remove_pack(&app.pack, &Quiet).unwrap();
    assert!(
        app.state.hub().unwrap().store.element(&spell).is_none(),
        "removed"
    );

    let input: ImportFileInput = serde_json::from_value(
        json!({"fileName": file.file_name, "contentBase64": STANDARD.encode(&file.bytes)}),
    )
    .unwrap();
    let preview = app
        .state
        .preview_import_pack(&PreviewImportRequest {
            files: vec![input],
            folder: None,
        })
        .unwrap();
    let installed = app
        .state
        .install_import_preview_as(&preview.preview_id, None, false, &Quiet)
        .unwrap();
    assert_eq!(installed.pack.expect("installed").id, app.pack);
    let pack = app
        .state
        .list_packs()
        .unwrap()
        .into_iter()
        .find(|p| p.id == app.pack)
        .unwrap();
    assert_eq!(pack.kind, PackKind::Homebrew, "editable again");

    // edit again: the imported spell opens in the creator and saves
    let v = app
        .state
        .homebrew_open_draft(&app.pack, Some(&spell), None)
        .unwrap();
    assert_eq!(v.form.kind, "spell");
    let v = app.edit(&v, |f| set(f, "e", "name", json!("Emberball")));
    assert_eq!(app.save(&v), std::slice::from_ref(&spell));
    assert_eq!(
        app.state.hub().unwrap().store.element(&spell).unwrap().name,
        "Emberball"
    );
}

/// Flow 9's Send/Receive over the real device link (TCP on localhost, QR pairing), as `device_sync.rs` drives it.
#[cfg(feature = "device-link")]
mod link {
    use super::{create_pack, paths, Quiet, FIREBALL};
    use borealis_lib::content_dto::PackKind;
    use borealis_lib::content_packs::{EventSink, NullSink};
    use borealis_lib::device_sync::{DeviceSync, StateRef, SyncConfig, SyncEvents};
    use borealis_lib::dto::Ruleset;
    use borealis_lib::state::AppState;
    use borealis_lib::sync_dto::{LinkPackChoice, LinkPhase, LinkSelection, LinkStatus};
    use borealis_link::{MemoryStore, SecureStore};
    use serde_json::json;
    use std::net::{IpAddr, Ipv4Addr};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    /// How long a link step may take on a loaded machine.
    const STEP_LIMIT: Duration = Duration::from_secs(20);
    const POLL: Duration = Duration::from_millis(20);

    struct NoStatus;
    impl SyncEvents for NoStatus {
        fn status(&self, _: &LinkStatus) {}
        fn content(&self) -> &dyn EventSink {
            &NullSink
        }
    }

    struct Device {
        data: tempfile::TempDir,
        state: Arc<AppState>,
        sync: Arc<DeviceSync>,
    }

    fn device(name: &str) -> Device {
        let data = tempfile::tempdir().unwrap();
        let state = Arc::new(AppState::new(paths(data.path())).unwrap());
        let store = Arc::new(MemoryStore::default());
        let config = SyncConfig {
            bind_ip: Some(IpAddr::V4(Ipv4Addr::LOCALHOST)),
            device_name: name.into(),
            discovery: false,
        };
        let sync = DeviceSync::new(
            Arc::clone(&state) as Arc<dyn StateRef>,
            Arc::new(NoStatus),
            config,
            move || Ok(Arc::clone(&store) as Arc<dyn SecureStore>),
        );
        Device {
            data,
            state,
            sync: Arc::new(sync),
        }
    }

    async fn wait(d: &Device, id: &str, want: LinkPhase) -> LinkStatus {
        let start = Instant::now();
        loop {
            let s = d.sync.status(id).unwrap();
            if s.phase == want {
                return s;
            }
            assert!(
                !matches!(
                    s.phase,
                    LinkPhase::Failed | LinkPhase::Cancelled | LinkPhase::Declined
                ),
                "{id}: wanted {want:?}, got {s:?}"
            );
            assert!(start.elapsed() < STEP_LIMIT, "{id}: timed out at {s:?}");
            tokio::time::sleep(POLL).await;
        }
    }

    fn file(d: &Device, pack: &str, name: &str) -> Vec<u8> {
        std::fs::read(d.data.path().join("library").join(pack).join(name)).unwrap()
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn flow_9_a_homebrew_pack_sent_over_the_link_arrives_editable() {
        let mac = device("Mac");
        let pack = create_pack(&mac.state, "Ember Isles", "EI", Ruleset::Dnd2024);
        let v = mac
            .state
            .homebrew_new_draft(&pack, "spell", Some(FIREBALL), None, Some(false))
            .unwrap();
        let spell = mac
            .state
            .homebrew_save_draft(&v.draft_id, &Quiet)
            .unwrap()
            .element_ids[0]
            .clone();

        // the Mac shows the QR to send its packs; the phone scans it, sees the preview and accepts
        let selection = LinkSelection {
            character_ids: Vec::new(),
            packs: LinkPackChoice::All,
        };
        let shown = mac.sync.start_send(selection).await.unwrap();
        let phone = device("Phone");
        let qr = shown.qr_payload.clone().expect("a QR");
        let scan = phone.sync.connect(&qr, None, None).await.unwrap();
        let preview = wait(&phone, &scan.session_id, LinkPhase::Preview).await;
        let rows = preview.preview.expect("a preview").items;
        assert_eq!(rows.len(), 1, "the pack is one item: {rows:?}");
        assert_eq!(rows[0].action, "new");
        phone.sync.decide(&scan.session_id, true).unwrap();
        wait(&phone, &scan.session_id, LinkPhase::Done).await;
        wait(&mac, &shown.session_id, LinkPhase::Done).await;

        // it arrives editable: creator.json as sent, a homebrew pack, its spell opens in the creator and saves
        assert_eq!(
            file(&phone, &pack, "creator.json"),
            file(&mac, &pack, "creator.json")
        );
        let listed = phone.state.list_packs().unwrap();
        let received = listed.iter().find(|p| p.id == pack).expect("received");
        assert_eq!(received.kind, PackKind::Homebrew);
        let v = phone
            .state
            .homebrew_open_draft(&pack, Some(&spell), None)
            .unwrap();
        assert_eq!(v.form.kind, "spell");
        let mut form = v.form.clone();
        form.fields.insert("name".into(), json!("Phone Fire"));
        let v = phone
            .state
            .homebrew_update_draft(&v.draft_id, &form)
            .unwrap();
        phone
            .state
            .homebrew_save_draft(&v.draft_id, &Quiet)
            .unwrap();
        assert_eq!(
            phone
                .state
                .hub()
                .unwrap()
                .store
                .element(&spell)
                .unwrap()
                .name,
            "Phone Fire"
        );
    }
}
