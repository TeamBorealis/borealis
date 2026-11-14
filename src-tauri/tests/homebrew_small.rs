//! WB-19 (homebrew creator small types and the generic "Other element" form, spec 4.10, 4.12) through the real
//! `AppState` on a temporary data folder with the bundled content:
//!
//! * every small type and the generic form are made from New element, saved, and get their `ID_<CODE>_<TYPE>_<NAME>`
//!   id (spec 5.3);
//! * a new Language is offered by a new character's language choice;
//! * a new Skill is offered by a skill choice that asks for its tag, and a Proficiency group gives a character the
//!   narrower proficiencies it covers (a new class, made in the class form, has both);
//! * a new Deity points at each of its three required fields with a plain sentence until they are filled;
//! * a new Condition keeps its rule blocks, and a character who has it switched on gets its rules (HC1, HtCaM) and
//!   keeps it switched on through a save and load;
//! * the generic form offers the open types loaded content uses and the setter names a type uses.

use borealis_authoring::expr_build::condition::ConditionContext;
use borealis_authoring::form::{BlockCodec, CatalogBlockCodec, FormContext, FormState, NoLookup};
use borealis_character::Character;
use borealis_content::{Rule, Ruleset as ContentRuleset};
use borealis_engine::commands::{self, slot_key};
use borealis_engine::{CharacterState, Choice, Engine};
use borealis_lib::content_dto::ContentProgress;
use borealis_lib::content_packs::EventSink;
use borealis_lib::dto::Ruleset;
use borealis_lib::homebrew_dto::{
    HomebrewDraftView, HomebrewPackRequest, HomebrewPageRequest, HomebrewPickerFilters,
    HomebrewPickerTarget,
};
use borealis_lib::session::new_id;
use borealis_lib::state::{AppPaths, AppState};
use serde_json::{json, Value};
use std::path::PathBuf;

const PADDED: &str = "ID_PROFICIENCY_ARMOR_PROFICIENCY_PADDED";
const LEATHER: &str = "ID_PROFICIENCY_ARMOR_PROFICIENCY_LEATHER";

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

impl App {
    fn new() -> Self {
        let data = tempfile::tempdir().unwrap();
        let state = AppState::new(AppPaths {
            content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
            data: data.path().to_path_buf(),
        })
        .unwrap();
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

    fn new_draft(&self, kind: &str) -> HomebrewDraftView {
        self.state
            .homebrew_new_draft(&self.pack, kind, None, None, None)
            .unwrap()
    }

    fn update(&self, v: &HomebrewDraftView, form: &FormState) -> HomebrewDraftView {
        self.state.homebrew_update_draft(&v.draft_id, form).unwrap()
    }

    /// Fill `fields` into a new draft of `kind`, save it and return its id.
    fn make(&self, kind: &str, fields: &[(&str, Value)]) -> String {
        let v = self.new_draft(kind);
        let mut form = v.form.clone();
        for (k, value) in fields {
            form.fields.insert((*k).into(), value.clone());
        }
        let v = self.update(&v, &form);
        assert_eq!(errors(&v), Vec::<(String, String)>::new(), "{kind}");
        self.state
            .homebrew_save_draft(&v.draft_id, &Quiet)
            .unwrap()
            .element_ids[0]
            .clone()
    }

    fn saved_one(&self, id: &str) -> Value {
        let path = self
            .data
            .path()
            .join("library")
            .join(&self.pack)
            .join("content.json");
        let content: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        content["elements"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["id"] == id)
            .cloned()
            .unwrap_or_else(|| panic!("{id} saved"))
    }

    /// The names a pseudo picker type (`Element Type`, `Setter Name`) offers.
    fn words(&self, types: &[&str]) -> Vec<String> {
        self.state
            .homebrew_reference_options(
                &HomebrewPickerTarget {
                    types: types.iter().map(|t| (*t).to_string()).collect(),
                    tags: None,
                    multiple: None,
                },
                "",
                &HomebrewPickerFilters::default(),
                HomebrewPageRequest {
                    offset: 0,
                    limit: 500,
                },
                None,
            )
            .unwrap()
            .items
            .into_iter()
            .map(|r| r.name)
            .collect()
    }
}

fn errors(v: &HomebrewDraftView) -> Vec<(String, String)> {
    v.problems
        .iter()
        .filter(|p| p.severity == "error")
        .map(|p| (p.path.clone().unwrap_or_default(), p.message.clone()))
        .collect()
}

fn text(t: &str) -> Value {
    json!({"type": "doc", "content": [{"type": "paragraph", "content": [{"type": "text", "text": t}]}]})
}

fn reference(id: &str) -> Value {
    json!({"id": id, "name": "", "type": "Proficiency"})
}

/// The block the form holds for `rule` (as the block dialog makes it).
fn block(rule: Value, key: &str) -> Value {
    let rule: Rule = serde_json::from_value(rule).unwrap();
    let codec = CatalogBlockCodec;
    let cx = FormContext {
        conditions: ConditionContext::new(Vec::<String>::new()),
        lookup: &NoLookup,
        blocks: &codec,
    };
    serde_json::to_value(codec.read(&rule, key.into(), &cx)).unwrap()
}

/// A 2024 character of `class` at level 1.
fn first_level(e: &Engine, class: &str) -> CharacterState {
    let mut s = commands::new_character(e, ContentRuleset::Dnd2024);
    let snap = e.derive(&s);
    let key = slot_key(&snap, "ID_LEVEL_1", "Class", 1).expect("class slot");
    s = commands::select(e, &s, &key, Choice::Element(class.into()))
        .unwrap()
        .state;
    s
}

/// The open slots that offer `id`, by name.
fn slots_offering(e: &Engine, s: &CharacterState, id: &str) -> Vec<String> {
    e.derive(s)
        .slots
        .into_iter()
        .filter(|x| x.options.iter().any(|o| o.id == id))
        .map(|x| x.name)
        .collect()
}

#[test]
fn every_small_type_and_the_generic_form_are_made_from_new_element_with_their_id() {
    let app = App::new();
    let required: &[(&str, &[(&str, Value)])] = &[
        ("skill", &[("ability", json!("Intelligence"))]),
        (
            "deity",
            &[
                ("alignment", json!("Neutral Good")),
                ("domains", json!(["Light"])),
                ("symbol", json!("A rising sun")),
            ],
        ),
        ("other", &[("type", json!("Property"))]),
    ];
    let tokens = [
        ("language", "LANGUAGE"),
        ("proficiency", "PROFICIENCY"),
        ("condition", "CONDITION"),
        ("damageType", "DAMAGE_TYPE"),
        ("skill", "SKILL"),
        ("deity", "DEITY"),
        ("vision", "VISION"),
        ("size", "SIZE"),
        ("alignment", "ALIGNMENT"),
        ("weaponProperty", "WEAPON_PROPERTY"),
        ("weaponGroup", "WEAPON_GROUP"),
        ("weaponCategory", "WEAPON_CATEGORY"),
        ("armorGroup", "ARMOR_GROUP"),
        ("masteryProperty", "MASTERY"),
        ("focusGroup", "FOCUS_GROUP"),
        ("dragonmark", "DRAGONMARK"),
        ("rulesText", "RULE"),
        ("other", "PROPERTY"),
    ];
    for (kind, token) in tokens {
        let mut fields = vec![
            ("name", json!("Ember Mark")),
            ("description", text("Embers.")),
        ];
        if let Some((_, more)) = required.iter().find(|(k, _)| *k == kind) {
            fields.extend(more.iter().cloned());
        }
        let id = app.make(kind, &fields);
        assert_eq!(id, format!("ID_EI_{token}_EMBER_MARK"), "{kind}");
        // it reopens in its own form
        let v = app
            .state
            .homebrew_open_draft(&app.pack, Some(&id), None)
            .unwrap();
        assert_eq!(v.form.kind, kind);
        app.state.homebrew_discard_draft(&v.draft_id).unwrap();
    }
    let rules = app.saved_one("ID_EI_RULE_EMBER_MARK");
    assert_eq!(rules["supports"], json!(["Rules Glossary"]));
    let mastery = app.saved_one("ID_EI_MASTERY_EMBER_MARK");
    assert_eq!(mastery["data"], json!({"kind": "weapon_mastery"}));
    assert_eq!(mastery["supports"], json!(["Mastery Property"]));
    let condition = app.saved_one("ID_EI_CONDITION_EMBER_MARK");
    assert_eq!(condition["supports"], json!(["Condition"]));
}

#[test]
fn a_new_language_is_offered_by_a_new_character_s_language_choice() {
    let app = App::new();
    let id = app.make(
        "language",
        &[
            ("name", json!("Old Tongue")),
            ("edition", json!("dnd2024")),
            ("rarity", json!("standard")),
            ("script", json!("Dwarvish")),
            ("speakers", json!("Ember folk")),
            ("description", text("The speech of the isles.")),
        ],
    );
    assert_eq!(id, "ID_EI_LANGUAGE_OLD_TONGUE");
    let saved = app.saved_one(&id);
    assert_eq!(saved["type"], "Language");
    assert_eq!(
        saved["setters"],
        json!([{"name": "standard", "value": "true"}, {"name": "script", "value": "Dwarvish"},
               {"name": "speakers", "value": "Ember folk"}])
    );
    assert_eq!(saved["supports"], json!(["Standard"]));
    assert_eq!(saved["data"], json!({"rarity": "standard"}));

    let hub = app.state.hub().unwrap();
    let e = &hub.engine;
    let s = commands::new_character(e, ContentRuleset::Dnd2024);
    let common = slots_offering(e, &s, "ID_2024_LANGUAGE_DWARVISH");
    assert!(!common.is_empty(), "a language choice at creation");
    let offering = slots_offering(e, &s, &id);
    assert_eq!(offering, common, "offered wherever Dwarvish is");
}

/// A new 2024 class made in the class form: armor `armor`, and `blocks` as its other rules.
fn class_with(app: &App, name: &str, armor: &[&str], blocks: Vec<Value>) -> String {
    let v = app.new_draft("class");
    let mut form = v.form.clone();
    form.fields.insert("name".into(), json!(name));
    form.fields.insert("hitDie".into(), json!("d10"));
    form.fields.insert(
        "skills".into(),
        json!({"count": 1, "skills": [{"id": "ID_PROFICIENCY_SKILL_ATHLETICS", "name": "", "type": "Proficiency"}]}),
    );
    form.fields.insert(
        "armor".into(),
        Value::Array(armor.iter().map(|a| reference(a)).collect()),
    );
    form.fields
        .insert("description".into(), text("Knights of gloom."));
    form.blocks = blocks
        .into_iter()
        .map(|b| serde_json::from_value(b).unwrap())
        .collect();
    let v = app.update(&v, &form);
    assert_eq!(errors(&v), Vec::<(String, String)>::new());
    app.state
        .homebrew_save_draft(&v.draft_id, &Quiet)
        .unwrap()
        .element_ids[0]
        .clone()
}

#[test]
fn a_new_skill_is_offered_by_its_tag_and_a_proficiency_group_gives_what_it_covers() {
    let app = App::new();
    let skill = app.make(
        "skill",
        &[
            ("name", json!("Alchemy")),
            ("ability", json!("Intelligence")),
            ("tags", json!(["Skill", "Intelligence"])),
            ("description", text("Brewing and reagents.")),
        ],
    );
    assert_eq!(skill, "ID_EI_SKILL_ALCHEMY");
    let saved = app.saved_one(&skill);
    assert_eq!(saved["type"], "Skill");
    assert_eq!(
        saved["setters"],
        json!([{"name": "ability", "value": "Intelligence"}])
    );
    assert_eq!(saved["supports"], json!(["Skill", "Intelligence"]));

    // a proficiency group covering padded and leather armor, made in the form with the proficiency picker
    let group = app.make(
        "proficiency",
        &[
            ("name", json!("Armor Proficiency (Hide Weaves)")),
            ("covers", json!([reference(PADDED), reference(LEATHER)])),
            ("description", text("Padded and leather armor.")),
        ],
    );
    assert_eq!(group, "ID_EI_PROFICIENCY_ARMOR_PROFICIENCY_HIDE_WEAVES");
    assert_eq!(
        app.saved_one(&group)["rules"],
        json!([{"kind": "grant", "id": PADDED, "type": "Proficiency"},
               {"kind": "grant", "id": LEATHER, "type": "Proficiency"}])
    );
    // it reopens with what it covers in its field, not as blocks
    let v = app
        .state
        .homebrew_open_draft(&app.pack, Some(&group), None)
        .unwrap();
    assert!(v.form.blocks.is_empty());
    assert_eq!(v.form.fields["covers"][1]["id"], LEATHER);
    app.state.homebrew_discard_draft(&v.draft_id).unwrap();

    let choice = block(
        json!({"kind": "select", "type": "Skill", "name": "Lore Skill (Gloom Knight)", "supports": "Intelligence"}),
        "b0",
    );
    let class = class_with(&app, "Gloom Knight", &[&group], vec![choice]);
    let hub = app.state.hub().unwrap();
    let e = &hub.engine;
    let s = first_level(e, &class);
    let offering = slots_offering(e, &s, &skill);
    assert_eq!(offering, ["Lore Skill (Gloom Knight)"]);
    let names: Vec<String> = e
        .derive(&s)
        .elements
        .into_iter()
        .map(|x| x.element.id)
        .collect();
    for id in [group.as_str(), PADDED, LEATHER] {
        assert!(names.iter().any(|n| n == id), "{id} in {names:?}");
    }
}

#[test]
fn a_new_deity_says_at_each_required_field_what_it_needs() {
    let app = App::new();
    let v = app.new_draft("deity");
    let mut form = v.form.clone();
    form.fields.insert("name".into(), json!("Asha"));
    form.fields
        .insert("description".into(), text("The ember goddess."));
    let v = app.update(&v, &form);
    let errs = errors(&v);
    for (field, sentence) in [
        ("fields.alignment", "A deity needs an alignment."),
        ("fields.domains", "A deity needs domains."),
        ("fields.symbol", "A deity needs a symbol."),
    ] {
        let at: Vec<&str> = errs
            .iter()
            .filter(|(p, _)| p == field)
            .map(|(_, m)| m.as_str())
            .collect();
        assert_eq!(at, [sentence], "{field}: {errs:?}");
    }
    assert_eq!(errs.len(), 3, "{errs:?}");
    form.fields
        .insert("alignment".into(), json!("Chaotic Good"));
    form.fields
        .insert("domains".into(), json!(["Light", "Forge"]));
    form.fields
        .insert("symbol".into(), json!("A flame in a hand"));
    let v = app.update(&v, &form);
    assert_eq!(errors(&v), Vec::<(String, String)>::new());
    let id = app
        .state
        .homebrew_save_draft(&v.draft_id, &Quiet)
        .unwrap()
        .element_ids[0]
        .clone();
    assert_eq!(
        app.saved_one(&id)["setters"],
        json!([{"name": "alignment", "value": "Chaotic Good"},
               {"name": "domains", "value": "Light, Forge"},
               {"name": "symbol", "value": "A flame in a hand"}])
    );
}

#[test]
fn a_new_condition_keeps_its_rule_blocks() {
    let app = App::new();
    let v = app.new_draft("condition");
    assert_eq!(v.form.fields["listed"], true);
    let mut form = v.form.clone();
    form.fields.insert("name".into(), json!("Smoldering"));
    form.fields.insert("edition".into(), json!("dnd2024"));
    form.fields
        .insert("description".into(), text("Embers slow you."));
    form.blocks = vec![serde_json::from_value(block(
        json!({"kind": "stat", "name": "speed", "value": "-10"}),
        "b0",
    ))
    .unwrap()];
    let v = app.update(&v, &form);
    assert_eq!(errors(&v), Vec::<(String, String)>::new());
    let id = app
        .state
        .homebrew_save_draft(&v.draft_id, &Quiet)
        .unwrap()
        .element_ids[0]
        .clone();
    let saved = app.saved_one(&id);
    assert_eq!(saved["supports"], json!(["Condition"]));
    assert_eq!(saved["data"], json!({"tag": "Condition"}));
    assert_eq!(
        saved["rules"],
        json!([{"kind": "stat", "name": "speed", "value": "-10"}])
    );
}

#[test]
fn the_generic_form_offers_open_types_and_the_setter_names_a_type_uses() {
    let app = App::new();
    let types = app.words(&["Element Type"]);
    for t in ["Companion", "Familiar Trait", "Property"] {
        assert!(types.iter().any(|x| x == t), "{t} in {types:?}");
    }
    for t in ["Spell", "Source", "Support", "Language"] {
        assert!(!types.iter().any(|x| x == t), "{t} not in {types:?}");
    }
    // a type with a form of its own is refused
    let v = app.new_draft("other");
    let mut form = v.form.clone();
    form.fields.insert("name".into(), json!("Ember Wolf"));
    form.fields.insert("type".into(), json!("Spell"));
    let v = app.update(&v, &form);
    assert_eq!(
        errors(&v)[0],
        (
            "fields.type".to_string(),
            "Spell elements have their own form: make it with New element > Spell.".to_string()
        )
    );
    assert!(app.state.homebrew_save_draft(&v.draft_id, &Quiet).is_err());
    // a companion: the setter names offered are the ones the content format requires of it
    let required = app.words(&["Setter Name", "Companion"]);
    assert_eq!(required.len(), 11, "{required:?}");
    form.fields.insert("type".into(), json!("Companion"));
    let v = app.update(&v, &form);
    let missing: Vec<String> = errors(&v).into_iter().map(|(p, _)| p).collect();
    assert_eq!(missing, vec!["fields.setters".to_string(); 11]);
    // its properties, one with an attribute
    let mut rows: Vec<Value> = [
        ("strength", "14"),
        ("dexterity", "15"),
        ("constitution", "13"),
        ("intelligence", "3"),
        ("wisdom", "12"),
        ("charisma", "6"),
        ("type", "Beast"),
        ("size", "Medium"),
        ("challenge", "1/4"),
        ("alignment", "Unaligned"),
        ("ac", "13"),
    ]
    .iter()
    .map(|(n, v)| json!({"name": n, "value": v, "attributes": {}}))
    .collect();
    rows.push(json!({"name": "bite", "value": "1d6", "attributes": {"type": "piercing"}}));
    form.fields.insert("setters".into(), Value::Array(rows));
    form.fields
        .insert("description".into(), text("A wolf of embers."));
    let v = app.update(&v, &form);
    assert_eq!(errors(&v), Vec::<(String, String)>::new());
    let id = app
        .state
        .homebrew_save_draft(&v.draft_id, &Quiet)
        .unwrap()
        .element_ids[0]
        .clone();
    assert_eq!(id, "ID_EI_COMPANION_EMBER_WOLF");
    let saved = app.saved_one(&id);
    assert_eq!(saved["type"], "Companion");
    assert_eq!(
        saved["setters"][11],
        json!({"name": "bite", "value": "1d6", "attributes": {"type": "piercing"}})
    );
    // the saved companion's own setter names are offered now
    let names = app.words(&["Setter Name", "Companion"]);
    assert_eq!(names.len(), 12);
    assert!(names.iter().any(|n| n == "bite"));
}

#[test]
fn item_categories_and_tags_loaded_content_uses_are_offered() {
    let app = App::new();
    let categories = app.words(&["Item Category"]);
    for c in ["Adventuring Gear", "Weapons", "Wondrous Items", "Potions"] {
        assert!(categories.iter().any(|x| x == c), "{c} in {categories:?}");
    }
    let tags = app.words(&["Tag", "Feat"]);
    for t in [
        "Origin Feat",
        "General Feat",
        "Epic Boon Feat",
        "Fighting Style Feat",
    ] {
        assert!(tags.iter().any(|x| x == t), "{t} in {tags:?}");
    }
    assert!(
        !tags.iter().any(|t| t == "Wizard"),
        "a spell's list is not a feat's tag"
    );
    // the item forms suggest them: Category from the item categories, Tags from the kind's element types
    let kinds = borealis_authoring::form::descriptors();
    let field = |kind: &str, key: &str| {
        kinds
            .iter()
            .find(|k| k.id == kind)
            .and_then(|k| k.fields.iter().find(|f| f.key == key))
            .and_then(|f| f.picker.as_ref())
            .map(|p| p.types.clone())
    };
    assert_eq!(
        field("item", "category"),
        Some(vec!["Item Category".to_string()])
    );
    assert_eq!(
        field("feat", "tags"),
        Some(vec!["Tag".to_string(), "Feat".to_string()]),
        "the tags of the kind's own element types"
    );
}

/// A 2024 Dwarf (walking speed 30) opened as a character session; its id.
fn dwarf(app: &App) -> String {
    let hub = app.state.hub().unwrap();
    let e = &hub.engine;
    let s = commands::new_character(e, ContentRuleset::Dnd2024);
    let key = slot_key(&e.derive(&s), "ID_LEVEL_1", "Race", 1).expect("species slot");
    let s = commands::select(e, &s, &key, Choice::Element("ID_2024_SPECIES_DWARF".into()))
        .unwrap()
        .state;
    let mut c = Character::new("Brakka");
    c.build = s;
    let id = new_id();
    let path = app.state.character_path(&id).unwrap();
    c.save(&path).unwrap();
    app.state.insert_session(&id, c, path, false).unwrap();
    id
}

fn condition<'a>(snap: &'a borealis_lib::dto::CharacterSnapshot, id: &str) -> &'a Value {
    snap.play
        .conditions
        .as_array()
        .expect("the conditions list")
        .iter()
        .find(|c| c["id"] == id)
        .unwrap_or_else(|| panic!("{id} listed"))
}

#[test]
fn a_new_condition_switched_on_applies_its_rules_and_is_saved_with_the_character() {
    let app = App::new();
    let v = app.new_draft("condition");
    let mut form = v.form.clone();
    form.fields.insert("name".into(), json!("Smoldering"));
    form.fields.insert("edition".into(), json!("dnd2024"));
    form.fields
        .insert("description".into(), text("Embers slow you."));
    form.blocks = vec![serde_json::from_value(block(
        json!({"kind": "stat", "name": "speed", "value": "-10"}),
        "b0",
    ))
    .unwrap()];
    let v = app.update(&v, &form);
    let cond = app
        .state
        .homebrew_save_draft(&v.draft_id, &Quiet)
        .unwrap()
        .element_ids[0]
        .clone();
    let id = dwarf(&app);
    let before = app.state.get_snapshot(&id).unwrap();
    assert_eq!(before.play.speeds.walk, 30);
    let listed = condition(&before, &cond);
    assert_eq!(listed["on"], false);
    assert_eq!(listed["effects"], json!(["Speed -10"]));
    assert_eq!(condition(&before, "ID_2024_CONDITION_PRONE")["on"], false);

    let on = app.state.set_condition(&id, &cond, true).unwrap();
    assert_eq!(condition(&on, &cond)["on"], true);
    assert_eq!(on.play.speeds.walk, 20);
    app.state.save_character(&id).unwrap();
    app.state.close_character(&id, false).unwrap();
    let again = app.state.load_character(&id).unwrap();
    assert_eq!(
        condition(&again, &cond)["on"],
        true,
        "kept with the character"
    );
    assert_eq!(again.play.speeds.walk, 20);
    let off = app.state.set_condition(&id, &cond, false).unwrap();
    assert_eq!(off.play.speeds.walk, 30);
    assert_eq!(
        app.state
            .set_condition(&id, "ID_2024_CONDITION_EXHAUSTION", true)
            .unwrap_err()
            .code,
        borealis_lib::error::ErrorCode::NotFound,
        "Exhaustion has its levels"
    );
}
