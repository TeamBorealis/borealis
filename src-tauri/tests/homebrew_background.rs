//! WB-14 (homebrew creator backgrounds, spec 4.3) through the real `AppState` on a temporary data folder with the
//! bundled content:
//!
//! * a new 2024 background made in the form (three abilities, Magic Initiate with the Druid spell list, two skills, a
//!   tool, a starting-equipment package made inside it) saves in the SRD shape, and a level-1 character who takes it
//!   gets the +2/+1 or +1/+1/+1 choice over its three abilities (capped at 20), the feat with its list (Druid
//!   cantrips only), the two skills, the tool and the package;
//! * a new 2014 background (two skills, a choice of two languages, a feature and a variant made inside it, the four
//!   characteristics) gives a 2014 character its skills, the language choice, the feature, the four characteristic
//!   choices and the optional variant;
//! * a new 2024 background says what it still needs at its fields, and both open again in their forms and save
//!   unchanged.

use borealis_authoring::form::FormState;
use borealis_content::Ruleset as ContentRuleset;
use borealis_engine::commands::{self, slot_key};
use borealis_engine::{CharacterSnapshot, CharacterState, Choice, Engine};
use borealis_lib::content_dto::ContentProgress;
use borealis_lib::content_packs::EventSink;
use borealis_lib::dto::Ruleset;
use borealis_lib::homebrew_dto::{HomebrewDraftView, HomebrewPackRequest};
use borealis_lib::state::{AppPaths, AppState};
use serde_json::{json, Value};
use std::path::PathBuf;

const WARDEN: &str = "ID_EI_BACKGROUND_EMBER_WARDEN";
const WARDEN_PACKAGE: &str = "ID_EI_EQUIPMENT_PACKAGE_EMBER_WARDEN_STARTING_EQUIPMENT";
const HERMIT: &str = "ID_OW_BACKGROUND_HERMIT";
const MAGIC_INITIATE: &str = "ID_2024_FEAT_MAGIC_INITIATE";
const CALLIGRAPHER: &str = "ID_PROFICIENCY_TOOL_PROFICIENCY_CALLIGRAPHERS_SUPPLIES";
const LEVEL1: &str = "ID_LEVEL_1";

struct Quiet;
impl EventSink for Quiet {
    fn content_progress(&self, _: &ContentProgress) {}
    fn import_progress(&self, _: &ContentProgress) {}
    fn content_reloaded(&self, _: &str) {}
}

struct App {
    data: tempfile::TempDir,
    state: AppState,
}

impl App {
    fn new() -> Self {
        let data = tempfile::tempdir().unwrap();
        let state = AppState::new(AppPaths {
            content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
            data: data.path().to_path_buf(),
        })
        .unwrap();
        Self { data, state }
    }

    fn pack(&self, name: &str, code: &str, edition: Ruleset) -> String {
        self.state
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

    fn update(&self, v: &HomebrewDraftView, form: &FormState) -> HomebrewDraftView {
        self.state.homebrew_update_draft(&v.draft_id, form).unwrap()
    }

    fn child(&self, pack: &str, kind: &str, parent: &HomebrewDraftView) -> HomebrewDraftView {
        self.state
            .homebrew_new_draft(pack, kind, None, Some(&parent.draft_id), None)
            .unwrap()
    }

    fn saved(&self, pack: &str, id: &str) -> Value {
        let path = self
            .data
            .path()
            .join("library")
            .join(pack)
            .join("content.json");
        let content: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        content["elements"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["id"] == id)
            .unwrap_or_else(|| panic!("{id} saved"))
            .clone()
    }

    /// Open the saved element again, save it unchanged, and give back what was written.
    fn resave(&self, pack: &str, id: &str) -> Value {
        let again = self
            .state
            .homebrew_open_draft(pack, Some(id), None)
            .unwrap();
        assert_eq!(again.form.kind, "background");
        assert!(again.form.blocks.is_empty(), "every rule is a field's");
        let touched = self.update(&again, &again.form);
        self.state
            .homebrew_save_draft(&touched.draft_id, &Quiet)
            .unwrap();
        self.saved(pack, id)
    }
}

fn errors(v: &HomebrewDraftView) -> Vec<(String, String)> {
    v.problems
        .iter()
        .filter(|p| p.severity == "error")
        .map(|p| (p.path.clone().unwrap_or_default(), p.message.clone()))
        .collect()
}

fn reference(id: &str, ty: &str) -> Value {
    json!({"id": id, "name": "", "type": ty})
}

fn set(form: &mut FormState, key: &str, v: Value) {
    form.fields.insert(key.into(), v);
}

fn set_child(form: &mut FormState, key: &str, field: &str, v: Value) {
    let c = form
        .children
        .iter_mut()
        .find(|c| c.key == key)
        .expect("the child");
    c.fields.insert(field.into(), v);
}

/// A tag filter of any of these tags, as the builder writes it.
fn any_of(tags: &[&str]) -> Value {
    use borealis_authoring::expr_build::tag_filter::{TagFilter, TagRow};
    use borealis_authoring::expr_build::{ExprValue, GroupMode, Node};
    let rows = tags
        .iter()
        .map(|t| {
            Node::row(TagRow::Tag {
                tag: (*t).to_string(),
            })
        })
        .collect();
    serde_json::to_value(ExprValue::new(TagFilter {
        specific: false,
        root: Node::group(GroupMode::Any, rows),
    }))
    .unwrap()
}

fn pick(
    e: &Engine,
    s: &CharacterState,
    owner: &str,
    name: &str,
    n: u32,
    id: &str,
) -> CharacterState {
    let snap = e.derive(s);
    let key =
        slot_key(&snap, owner, name, n).unwrap_or_else(|| panic!("no slot {owner}/{name}#{n}"));
    commands::select(e, s, &key, Choice::Element(id.into()))
        .unwrap_or_else(|err| panic!("select {id}: {err}"))
        .state
}

fn pick_item(
    e: &Engine,
    s: &CharacterState,
    owner: &str,
    name: &str,
    text: &str,
) -> CharacterState {
    let snap = e.derive(s);
    let slot = snap
        .slot(owner, name)
        .unwrap_or_else(|| panic!("no slot {owner}/{name}"));
    let item = slot
        .list_items
        .iter()
        .find(|(_, t)| t == text)
        .unwrap_or_else(|| panic!("{text} in {:?}", slot.list_items))
        .0;
    commands::select(e, s, &slot.key.clone(), Choice::ListItem(item))
        .expect("list pick")
        .state
}

fn texts(snap: &CharacterSnapshot, owner: &str, name: &str) -> Vec<String> {
    snap.slot(owner, name)
        .unwrap_or_else(|| panic!("no slot {owner}/{name}"))
        .list_items
        .iter()
        .map(|(_, t)| t.clone())
        .collect()
}

#[test]
fn a_new_2024_background_gives_a_level_1_character_its_scores_feat_skills_tool_and_package() {
    let app = App::new();
    let pack = app.pack("Ember Isles", "EI", Ruleset::Dnd2024);
    let v = app
        .state
        .homebrew_new_draft(&pack, "background", None, None, None)
        .unwrap();
    assert_eq!(v.form.kind, "background");
    assert_eq!(v.form.fields["edition"], json!("dnd2024"));
    // what a new 2024 background still needs, said at its fields
    let named = v.form.with_field("name", json!("Ember Warden"));
    let v = app.update(&v, &named);
    let mut needs = errors(&v);
    needs.sort();
    assert_eq!(
        needs,
        [
            (
                "fields.abilityScores".to_string(),
                "Pick three abilities.".to_string()
            ),
            (
                "fields.originFeat".to_string(),
                "Choose the origin feat.".to_string()
            ),
            ("fields.skills".to_string(), "Pick two skills.".to_string()),
        ]
    );

    // the starting equipment: a package made inside the background (its EQUIPMENT tab)
    let v = app.child(&pack, "equipmentPackage", &v);
    let package = v.form.children.last().expect("the package").key.clone();
    assert_eq!(v.form.fields["equipment"], json!([{"child": package}]));
    let mut form = v.form.clone();
    set_child(&mut form, &package, "name", json!("Starting Equipment"));
    set_child(
        &mut form,
        &package,
        "options",
        json!([{"label": "A", "items": [], "coins": {"cp": 0, "sp": 0, "ep": 0, "gp": 15, "pp": 0}},
               {"label": "B", "items": [], "coins": {"cp": 0, "sp": 0, "ep": 0, "gp": 50, "pp": 0}}]),
    );
    set(
        &mut form,
        "abilityScores",
        json!(["Wisdom", "Strength", "Constitution"]),
    );
    set(
        &mut form,
        "originFeat",
        json!({"feat": reference(MAGIC_INITIATE, "Feat"), "params": {"spell_list": "Druid"}}),
    );
    set(
        &mut form,
        "skills",
        json!([
            reference("ID_PROFICIENCY_SKILL_NATURE", "Proficiency"),
            reference("ID_PROFICIENCY_SKILL_SURVIVAL", "Proficiency")
        ]),
    );
    set(
        &mut form,
        "tools",
        json!([reference(CALLIGRAPHER, "Proficiency")]),
    );
    let v = app.update(&v, &form);
    assert_eq!(errors(&v), Vec::<(String, String)>::new());
    let saved = app.state.homebrew_save_draft(&v.draft_id, &Quiet).unwrap();
    assert_eq!(saved.element_ids, [WARDEN, WARDEN_PACKAGE]);

    let bg = app.saved(&pack, WARDEN);
    assert_eq!(
        bg["rules"],
        json!([
            {"kind": "ability_distribution", "name": "Ability Scores (Ember Warden)",
             "pool": ["strength", "constitution", "wisdom"], "options": [[2, 1], [1, 1, 1]], "cap": 20},
            {"kind": "grant_feat", "feat": MAGIC_INITIATE, "params": {"spell_list": "Druid"}},
            {"kind": "grant", "id": "ID_PROFICIENCY_SKILL_NATURE", "type": "Proficiency"},
            {"kind": "grant", "id": "ID_PROFICIENCY_SKILL_SURVIVAL", "type": "Proficiency"},
            {"kind": "grant", "id": CALLIGRAPHER, "type": "Proficiency"},
            {"kind": "equipment_package", "ref": WARDEN_PACKAGE, "name": "Starting Equipment (Ember Warden)"}
        ])
    );
    assert_eq!(
        bg["data"]["ability_scores"],
        json!(["strength", "constitution", "wisdom"])
    );
    assert_eq!(bg["data"]["origin_feat"], json!(MAGIC_INITIATE));
    assert_eq!(
        bg["data"]["origin_feat_params"],
        json!({"spell_list": "Druid"})
    );
    assert_eq!(bg["data"]["skills"], json!(["Nature", "Survival"]));
    assert_eq!(bg["data"]["tool"], json!("Calligrapher's supplies"));
    assert_eq!(bg["data"]["equipment_package"], json!(WARDEN_PACKAGE));
    assert_eq!(
        app.saved(&pack, WARDEN_PACKAGE)["data"]["owner"],
        json!(WARDEN)
    );

    // a level-1 2024 character who takes it
    let hub = app.state.hub().unwrap();
    let e = &hub.engine;
    let mut s = commands::new_character(e, ContentRuleset::Dnd2024);
    s = pick(e, &s, LEVEL1, "Race", 1, "ID_2024_SPECIES_HUMAN");
    s = pick(e, &s, LEVEL1, "Class", 1, "ID_2024_CLASS_FIGHTER");
    s = pick(e, &s, LEVEL1, "Background", 1, WARDEN);
    let snap = e.derive(&s);
    let scores = "Ability Scores (Ember Warden)";
    let offered = texts(&snap, WARDEN, scores);
    assert_eq!(
        offered.len(),
        7,
        "six +2/+1 assignments and the +1/+1/+1 triple: {offered:?}"
    );
    assert!(
        offered.contains(&"Strength +2, Wisdom +1".to_string()),
        "{offered:?}"
    );
    assert!(offered
        .iter()
        .all(|t| !t.contains("Dexterity") && !t.contains("Charisma")));
    let before: Vec<i32> = snap.abilities.iter().map(|a| a.score).collect();
    s = pick_item(e, &s, WARDEN, scores, "Strength +2, Wisdom +1");
    let snap = e.derive(&s);
    let after: Vec<i32> = snap.abilities.iter().map(|a| a.score).collect();
    assert_eq!(after[0], (before[0] + 2).min(20));
    assert_eq!(after[4], (before[4] + 1).min(20));
    assert_eq!(
        after[2], before[2],
        "Constitution unchanged by the +2/+1 pick"
    );
    // the origin feat with its spell list: Druid cantrips only, no Spell List choice left
    assert!(snap.has_element(MAGIC_INITIATE));
    assert!(snap.slot(MAGIC_INITIATE, "Spell List").is_none());
    let cantrips = snap
        .slot(MAGIC_INITIATE, "Magic Initiate Cantrips")
        .expect("cantrip choice");
    let ids: Vec<&str> = cantrips.options.iter().map(|o| o.id.as_str()).collect();
    assert!(ids.contains(&"ID_2024_SPELL_DRUIDCRAFT"), "{ids:?}");
    assert!(!ids.contains(&"ID_2024_SPELL_FIRE_BOLT"), "{ids:?}");
    // the two skills and the tool
    for skill in ["nature", "survival"] {
        assert!(snap.skill(skill).is_some_and(|x| x.proficient), "{skill}");
    }
    assert!(snap.has_element(CALLIGRAPHER));
    // the package
    let package = snap
        .equipment_packages
        .iter()
        .find(|p| p.package == WARDEN_PACKAGE)
        .expect("the background's package");
    assert_eq!(package.options.len(), 2);
    drop(hub);

    // it opens again in the background form with its fields; an unchanged save writes the same element
    let again = app
        .state
        .homebrew_open_draft(&pack, Some(WARDEN), None)
        .unwrap();
    assert_eq!(
        again.form.fields["abilityScores"],
        json!(["Strength", "Constitution", "Wisdom"])
    );
    assert_eq!(
        again.form.fields["originFeat"]["feat"]["name"],
        json!("Magic Initiate")
    );
    assert_eq!(again.form.children.len(), 1, "the package is its child");
    app.state.homebrew_discard_draft(&again.draft_id).unwrap();
    assert_eq!(app.resave(&pack, WARDEN), bg);
}

#[test]
fn a_new_2014_background_gives_its_skills_language_choice_feature_characteristics_and_variant() {
    let app = App::new();
    let pack = app.pack("Old World", "OW", Ruleset::Dnd2014);
    let v = app
        .state
        .homebrew_new_draft(&pack, "background", None, None, None)
        .unwrap();
    assert_eq!(v.form.fields["edition"], json!("dnd2014"));
    assert_eq!(
        v.form.fields["personalityTraits"],
        json!({"count": 2, "items": []})
    );
    let v = app.child(&pack, "backgroundFeature", &v);
    let feature = v.form.children.last().unwrap().key.clone();
    assert_eq!(v.form.fields["feature"], json!([{"child": feature}]));
    let v = app.child(&pack, "backgroundVariant", &v);
    let variant = v.form.children.last().unwrap().key.clone();
    assert_eq!(v.form.fields["variants"], json!([{"child": variant}]));

    let mut form = v.form.clone();
    set(&mut form, "name", json!("Hermit"));
    set(
        &mut form,
        "skills",
        json!([
            reference("ID_PROFICIENCY_SKILL_MEDICINE", "Proficiency"),
            reference("ID_PROFICIENCY_SKILL_RELIGION", "Proficiency")
        ]),
    );
    set(
        &mut form,
        "choices",
        json!([{"kind": "language", "count": 2, "from": any_of(&["Standard", "Exotic"])}]),
    );
    set(
        &mut form,
        "personalityTraits",
        json!({"count": 2, "items": ["I am quiet.", "I speak in riddles.", "I collect stones."]}),
    );
    set(
        &mut form,
        "ideals",
        json!({"count": 1, "items": ["Knowledge.", "Solitude."]}),
    );
    set(
        &mut form,
        "bonds",
        json!({"count": 1, "items": ["My hermitage."]}),
    );
    set(
        &mut form,
        "flaws",
        json!({"count": 1, "items": ["I trust no one.", "I hoard secrets."]}),
    );
    set_child(&mut form, &feature, "name", json!("Discovery"));
    set_child(&mut form, &variant, "name", json!("Recluse"));
    let v = app.update(&v, &form);
    assert_eq!(errors(&v), Vec::<(String, String)>::new());
    let saved = app.state.homebrew_save_draft(&v.draft_id, &Quiet).unwrap();
    let feature_id = "ID_OW_BACKGROUND_FEATURE_HERMIT_DISCOVERY";
    let variant_id = "ID_OW_BACKGROUND_VARIANT_HERMIT_RECLUSE";
    assert_eq!(saved.element_ids, [HERMIT, feature_id, variant_id]);
    let bg = app.saved(&pack, HERMIT);
    let rules = bg["rules"].as_array().unwrap();
    assert_eq!(
        rules[2],
        json!({"kind": "select", "type": "Language", "name": "Language (Hermit)", "number": 2, "supports": "Standard||Exotic"})
    );
    assert_eq!(
        rules[3],
        json!({"kind": "grant", "id": feature_id, "type": "Background Feature"})
    );
    assert_eq!(
        rules[4],
        json!({"kind": "select", "type": "List", "name": "Personality Trait", "number": 2, "items": [
            {"id": 1, "text": "I am quiet."}, {"id": 2, "text": "I speak in riddles."}, {"id": 3, "text": "I collect stones."}]})
    );
    assert_eq!(rules[8]["supports"], json!(variant_id));
    assert!(
        bg.get("data").is_none(),
        "no 2024 mirrors on a 2014 background"
    );

    // a 2014 character who takes it
    let hub = app.state.hub().unwrap();
    let e = &hub.engine;
    let mut s = commands::new_character(e, ContentRuleset::Dnd2014);
    s = pick(e, &s, LEVEL1, "Background", 1, HERMIT);
    let snap = e.derive(&s);
    for skill in ["medicine", "religion"] {
        assert!(snap.skill(skill).is_some_and(|x| x.proficient), "{skill}");
    }
    assert!(snap.has_element(feature_id), "the feature is granted");
    let language = snap
        .slot(HERMIT, "Language (Hermit)")
        .expect("the language choice");
    assert!(
        language.options.len() > 2,
        "standard and exotic languages offered"
    );
    assert!(
        slot_key(&snap, HERMIT, "Language (Hermit)", 2).is_some(),
        "two picks"
    );
    for (name, n) in [
        ("Personality Trait", 3),
        ("Ideal", 2),
        ("Bond", 1),
        ("Flaw", 2),
    ] {
        assert_eq!(texts(&snap, HERMIT, name).len(), n, "{name}");
    }
    s = pick_item(e, &s, HERMIT, "Ideal", "Solitude.");
    assert!(e
        .derive(&s)
        .slot(HERMIT, "Ideal")
        .unwrap()
        .list_choice
        .is_some());
    let variants = snap
        .slot(HERMIT, "Background Variant (Hermit)")
        .expect("the variant choice");
    assert_eq!(
        variants
            .options
            .iter()
            .map(|o| o.id.as_str())
            .collect::<Vec<_>>(),
        [variant_id]
    );
    assert!(
        variants.chosen.is_none() && !snap.has_element(variant_id),
        "optional, not taken"
    );
    drop(hub);

    assert_eq!(app.resave(&pack, HERMIT), bg);
}

#[test]
fn the_origin_feat_picker_offers_origin_feats_only() {
    let app = App::new();
    let kinds = borealis_authoring::form::descriptors();
    let picker = kinds
        .iter()
        .find(|k| k.id == "background")
        .and_then(|k| k.fields.iter().find(|f| f.key == "originFeat"))
        .and_then(|f| f.picker.clone())
        .expect("the origin feat picks feats");
    assert_eq!(picker.tags, Some(vec!["Origin Feat".to_string()]));
    let page = app
        .state
        .homebrew_reference_options(
            &borealis_lib::homebrew_dto::HomebrewPickerTarget {
                types: picker.types.clone(),
                tags: picker.tags.clone(),
                multiple: None,
            },
            "",
            &borealis_lib::homebrew_dto::HomebrewPickerFilters::default(),
            borealis_lib::homebrew_dto::HomebrewPageRequest {
                offset: 0,
                limit: 100,
            },
            None,
        )
        .unwrap();
    let mut names: Vec<&str> = page.items.iter().map(|r| r.name.as_str()).collect();
    names.sort_unstable();
    assert_eq!(
        names,
        ["Alert", "Magic Initiate", "Savage Attacker", "Skilled"]
    );
}
