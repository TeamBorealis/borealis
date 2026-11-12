//! P2a (homebrew creator form and children items, 1.5.0) through the real `AppState` on a temporary data folder with
//! the bundled content:
//!
//! * a species' racial trait made inside the species, and the trait's lineage options made inside the trait, reach a
//!   new character: the trait is given, its choice offers both options, and the option picked gives its rule;
//! * a background variant's own feature, made inside the variant, reaches a 2014 character who takes the variant;
//! * class options made from the class's "Lets the player choose" block (the same calls the block dialog makes) are
//!   offered by that choice to a new character, and the one picked gives its rule;
//! * a renamed class keeps offering its saved subclass (the subclass takes the class's new tag);
//! * a saved class feature's characters are found before it is removed from the class.
//!
//! Each saved parent opens again with its children inside it, and an unchanged save writes the same elements.

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

const LEVEL1: &str = "ID_LEVEL_1";
const DARKVISION: &str = "ID_VISION_DARKVISION";

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

    fn new_draft(&self, pack: &str, kind: &str) -> HomebrewDraftView {
        self.state
            .homebrew_new_draft(pack, kind, None, None, None)
            .unwrap()
    }

    /// A child of `kind` made inside the draft `parent` (`parent_key`: inside that child form of it).
    fn child(
        &self,
        pack: &str,
        kind: &str,
        parent: &HomebrewDraftView,
        parent_key: Option<&str>,
    ) -> HomebrewDraftView {
        self.state
            .homebrew_new_draft_in(pack, kind, None, Some(&parent.draft_id), parent_key, None)
            .unwrap()
    }

    fn update(&self, v: &HomebrewDraftView, form: &FormState) -> HomebrewDraftView {
        self.state.homebrew_update_draft(&v.draft_id, form).unwrap()
    }

    fn save(&self, v: &HomebrewDraftView) -> Vec<String> {
        assert_eq!(errors(v), Vec::<(String, String)>::new());
        self.state
            .homebrew_save_draft(&v.draft_id, &Quiet)
            .unwrap()
            .element_ids
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

    /// Open the saved element again, save it unchanged, and give back what was written for each of `ids`.
    fn resave(&self, pack: &str, id: &str, ids: &[&str]) -> Vec<Value> {
        let again = self
            .state
            .homebrew_open_draft(pack, Some(id), None)
            .unwrap();
        let touched = self.update(&again, &again.form);
        self.save(&touched);
        ids.iter().map(|i| self.saved(pack, i)).collect()
    }
}

fn errors(v: &HomebrewDraftView) -> Vec<(String, String)> {
    v.problems
        .iter()
        .filter(|p| p.severity == "error")
        .map(|p| (p.path.clone().unwrap_or_default(), p.message.clone()))
        .collect()
}

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

/// A rule block that gives Darkvision (the block dialog's values).
fn gives_darkvision(form: &mut FormState, key: &str) {
    let f = form_mut(form, key);
    let block = serde_json::from_value(json!({
        "key": "u1", "kind": "grant", "sentence": "",
        "fields": {"id": {"id": DARKVISION, "name": "Darkvision", "type": "Vision"}, "type": "Vision"}
    }))
    .unwrap();
    f.blocks.push(block);
}

/// The last child key of the form `key` (the child the backend just made there).
fn last_child(form: &FormState, key: &str) -> String {
    fn find<'f>(f: &'f FormState, key: &str) -> Option<&'f FormState> {
        if f.key == key {
            return Some(f);
        }
        f.children.iter().find_map(|c| find(c, key))
    }
    find(form, key)
        .and_then(|f| f.children.last())
        .map(|c| c.key.clone())
        .unwrap_or_else(|| panic!("a child of {key}"))
}

fn pick(e: &Engine, s: &CharacterState, owner: &str, name: &str, id: &str) -> CharacterState {
    let snap = e.derive(s);
    let key = slot_key(&snap, owner, name, 1).unwrap_or_else(|| panic!("no slot {owner}/{name}"));
    commands::select(e, s, &key, Choice::Element(id.into()))
        .unwrap()
        .state
}

fn offered(snap: &CharacterSnapshot, owner: &str, name: &str) -> Vec<String> {
    snap.slot(owner, name)
        .unwrap_or_else(|| panic!("the choice {owner}/{name}"))
        .options
        .iter()
        .map(|o| o.id.clone())
        .collect()
}

fn sees_in_the_dark(snap: &CharacterSnapshot) -> bool {
    snap.senses.iter().any(|x| x.name == "darkvision")
}

/// A one-tag "From" filter, as the block dialog writes it.
fn from_tag(tag: &str) -> Value {
    json!({"value": {"shape": "model", "model": {"specific": false, "root": {"node": "row", "row": {"row": "tag", "tag": tag}}}}})
}

#[test]
fn a_species_trait_and_its_lineage_options_made_inside_reach_a_new_character() {
    let app = App::new();
    let pack = app.pack("Ember Isles", "EI", Ruleset::Dnd2024);
    let v = app.new_draft(&pack, "species");
    // a racial trait made inside the species is listed in its Traits field
    let v = app.child(&pack, "racialTrait", &v, None);
    let lineage = last_child(&v.form, "e");
    assert_eq!(v.form.fields["traits"], json!([{"child": lineage}]));
    // two lineage options made inside the trait are listed in the trait's Lineage options field
    let v = app.child(&pack, "speciesTraitOption", &v, Some(&lineage));
    let ash = last_child(&v.form, &lineage);
    let v = app.child(&pack, "speciesTraitOption", &v, Some(&lineage));
    let cinder = last_child(&v.form, &lineage);
    assert_eq!(
        v.form.children[0].fields["lineage"],
        json!([{"child": ash}, {"child": cinder}])
    );

    let mut form = v.form.clone();
    set(&mut form, "e", "name", json!("Emberkin"));
    set(&mut form, &lineage, "name", json!("Ember Lineage"));
    set(&mut form, &ash, "name", json!("Ash"));
    set(&mut form, &cinder, "name", json!("Cinder"));
    gives_darkvision(&mut form, &ash);
    let v = app.update(&v, &form);
    let ids = app.save(&v);
    let species_id = "ID_EI_SPECIES_EMBERKIN";
    let trait_id = "ID_EI_TRAIT_EMBERKIN_EMBER_LINEAGE";
    let ash_id = "ID_EI_TRAIT_OPTION_EMBER_LINEAGE_ASH";
    let cinder_id = "ID_EI_TRAIT_OPTION_EMBER_LINEAGE_CINDER";
    assert_eq!(ids, [species_id, trait_id, ash_id, cinder_id]);

    let species = app.saved(&pack, species_id);
    assert_eq!(
        species["rules"].as_array().unwrap().last().unwrap(),
        &json!({"kind": "grant", "id": trait_id, "type": "Racial Trait"})
    );
    let lineage_el = app.saved(&pack, trait_id);
    assert_eq!(
        lineage_el["rules"],
        json!([{"kind": "select", "type": "Species Trait Option", "name": "Ember Lineage",
            "supports": format!("{ash_id}|{cinder_id}")}])
    );

    // a new character who takes the species gets the trait, is offered both options, and the one picked applies
    let hub = app.state.hub().unwrap();
    let e = &hub.engine;
    let mut s = commands::new_character(e, ContentRuleset::Dnd2024);
    s = pick(e, &s, LEVEL1, "Race", species_id);
    let snap = e.derive(&s);
    assert!(snap.has_element(trait_id), "the trait is given");
    assert_eq!(
        offered(&snap, trait_id, "Ember Lineage"),
        [ash_id, cinder_id]
    );
    assert!(!sees_in_the_dark(&snap));
    s = pick(e, &s, trait_id, "Ember Lineage", ash_id);
    let snap = e.derive(&s);
    assert!(snap.has_element(ash_id));
    assert!(sees_in_the_dark(&snap), "the option's rule applies");
    drop(hub);

    // it opens again with the trait inside it and the options inside the trait; an unchanged save is identical
    let again = app
        .state
        .homebrew_open_draft(&pack, Some(species_id), None)
        .unwrap();
    assert_eq!(again.form.children.len(), 1);
    let t = &again.form.children[0];
    assert_eq!(t.children.len(), 2, "the options are the trait's children");
    assert_eq!(
        t.fields["lineage"],
        json!([{"child": t.children[0].key}, {"child": t.children[1].key}])
    );
    assert!(t.blocks.is_empty(), "the choice is the field's");
    app.state.homebrew_discard_draft(&again.draft_id).unwrap();
    let before: Vec<Value> = [species_id, trait_id, ash_id, cinder_id]
        .iter()
        .map(|i| app.saved(&pack, i))
        .collect();
    assert_eq!(
        app.resave(
            &pack,
            species_id,
            &[species_id, trait_id, ash_id, cinder_id]
        ),
        before
    );
}

#[test]
fn a_background_variant_s_own_feature_made_inside_it_reaches_a_2014_character() {
    let app = App::new();
    let pack = app.pack("Old World", "OW", Ruleset::Dnd2014);
    let v = app.new_draft(&pack, "background");
    let v = app.child(&pack, "backgroundVariant", &v, None);
    let variant = last_child(&v.form, "e");
    // the variant's own feature, made inside the variant
    let v = app.child(&pack, "backgroundFeature", &v, Some(&variant));
    let feature = last_child(&v.form, &variant);
    assert_eq!(
        v.form.children[0].fields["feature"],
        json!([{"child": feature}])
    );
    assert_eq!(
        v.form.fields["feature"],
        json!([]),
        "not the background's own"
    );

    let mut form = v.form.clone();
    set(&mut form, "e", "name", json!("Wanderer"));
    set(&mut form, &variant, "name", json!("Night Walker"));
    set(&mut form, &feature, "name", json!("Night Eyes"));
    gives_darkvision(&mut form, &feature);
    let v = app.update(&v, &form);
    let ids = app.save(&v);
    let bg_id = "ID_OW_BACKGROUND_WANDERER";
    let variant_id = "ID_OW_BACKGROUND_VARIANT_WANDERER_NIGHT_WALKER";
    let feature_id = "ID_OW_BACKGROUND_FEATURE_NIGHT_WALKER_NIGHT_EYES";
    assert_eq!(ids, [bg_id, variant_id, feature_id]);
    let variant_el = app.saved(&pack, variant_id);
    assert_eq!(
        variant_el["rules"],
        json!([{"kind": "grant", "id": feature_id, "type": "Background Feature"}])
    );
    assert_eq!(variant_el["ruleset"], "dnd2014");
    assert_eq!(
        app.saved(&pack, feature_id)["ruleset"],
        "dnd2014",
        "the nested child takes the background's edition"
    );

    // a 2014 character takes the background, then the variant: the variant's feature and its rule apply
    let hub = app.state.hub().unwrap();
    let e = &hub.engine;
    let mut s = commands::new_character(e, ContentRuleset::Dnd2014);
    s = pick(e, &s, LEVEL1, "Background", bg_id);
    let snap = e.derive(&s);
    assert!(!snap.has_element(feature_id));
    let choice = "Background Variant (Wanderer)";
    assert_eq!(offered(&snap, bg_id, choice), [variant_id]);
    s = pick(e, &s, bg_id, choice, variant_id);
    let snap = e.derive(&s);
    assert!(snap.has_element(feature_id), "the variant's feature");
    assert!(sees_in_the_dark(&snap));
    drop(hub);

    let again = app
        .state
        .homebrew_open_draft(&pack, Some(bg_id), None)
        .unwrap();
    assert_eq!(again.form.children[0].children.len(), 1);
    app.state.homebrew_discard_draft(&again.draft_id).unwrap();
    let all = [bg_id, variant_id, feature_id];
    let before: Vec<Value> = all.iter().map(|i| app.saved(&pack, i)).collect();
    assert_eq!(app.resave(&pack, bg_id, &all), before);
}

/// A new 2024 class "<name>" with a d10 hit die, saved; its draft's view.
fn new_class(app: &App, pack: &str, name: &str) -> HomebrewDraftView {
    let v = app.new_draft(pack, "class");
    let form = v
        .form
        .with_field("name", json!(name))
        .with_field("hitDie", json!("d10"))
        .with_field(
            "skills",
            json!({"count": 1, "skills": [
                {"id": "ID_PROFICIENCY_SKILL_ATHLETICS", "name": "Athletics", "type": "Proficiency"},
                {"id": "ID_PROFICIENCY_SKILL_PERCEPTION", "name": "Perception", "type": "Proficiency"}]}),
        );
    app.update(&v, &form)
}

#[test]
fn class_options_made_from_the_class_s_choice_block_are_offered_to_a_new_character() {
    let app = App::new();
    let pack = app.pack("Ember Isles", "EI", Ruleset::Dnd2024);
    let group = "Ember Style";
    let v = new_class(&app, &pack, "Ember Knight");
    // "Lets the player choose" a class option of the group (the block dialog's values)
    let mut form = v.form.clone();
    form.blocks.push(
        serde_json::from_value(json!({
            "key": "u1", "kind": "select", "sentence": "",
            "fields": {"type": "Option", "name": group, "supports": from_tag(group)}
        }))
        .unwrap(),
    );
    let v = app.update(&v, &form);
    let class_id = app.save(&v)[0].clone();
    assert_eq!(class_id, "ID_EI_CLASS_EMBER_KNIGHT");

    // the dialog's "Add option": a class option of the group, named, with the class's name and edition, saved
    let mut option_ids = Vec::new();
    for name in ["Ash Guard", "Cinder Step"] {
        let o = app.new_draft(&pack, "classOption");
        let mut f = o
            .form
            .with_field("name", json!(name))
            .with_field("group", json!(group))
            .with_field("className", json!("Ember Knight"))
            .with_field("edition", json!("dnd2024"));
        if name == "Ash Guard" {
            gives_darkvision(&mut f, "e");
        }
        let o = app.update(&o, &f);
        option_ids.push(app.save(&o)[0].clone());
    }
    assert_eq!(
        option_ids,
        ["ID_EI_OPTION_ASH_GUARD", "ID_EI_OPTION_CINDER_STEP"]
    );
    let option = app.saved(&pack, &option_ids[0]);
    assert_eq!(option["supports"], json!([group]));
    assert_eq!(option["data"]["class_name"], "Ember Knight");

    let hub = app.state.hub().unwrap();
    let e = &hub.engine;
    let mut s = commands::new_character(e, ContentRuleset::Dnd2024);
    s = pick(e, &s, LEVEL1, "Class", &class_id);
    let snap = e.derive(&s);
    assert_eq!(offered(&snap, &class_id, group), option_ids);
    s = pick(e, &s, &class_id, group, &option_ids[0]);
    let snap = e.derive(&s);
    assert!(snap.has_element(&option_ids[0]));
    assert!(sees_in_the_dark(&snap), "the option's rule applies");
}

#[test]
fn a_renamed_class_keeps_offering_its_saved_subclass() {
    let app = App::new();
    let pack = app.pack("Ember Isles", "EI", Ruleset::Dnd2024);
    let v = new_class(&app, &pack, "Ember Knight");
    let class_id = app.save(&v)[0].clone();

    // a subclass of it, saved
    let sub = app.new_draft(&pack, "subclass");
    let form = sub
        .form
        .with_field("name", json!("Order of Ash"))
        .with_field(
            "parentClass",
            json!({"id": class_id, "name": "Ember Knight", "type": "Class"}),
        );
    let sub = app.update(&sub, &form);
    assert_eq!(
        sub.form.fields["subclassTag"],
        json!("Ember Knight Subclass")
    );
    let sub_id = app.save(&sub)[0].clone();
    assert_eq!(
        app.saved(&pack, &sub_id)["supports"],
        json!(["Ember Knight Subclass"])
    );

    // the class renamed and saved: its choice offers "Cinder Knight Subclass" now, and the subclass follows it
    let again = app
        .state
        .homebrew_open_draft(&pack, Some(&class_id), None)
        .unwrap();
    let renamed = again.form.with_field("name", json!("Cinder Knight"));
    let again = app.update(&again, &renamed);
    app.save(&again);
    let subclass = app.saved(&pack, &sub_id);
    assert_eq!(subclass["supports"], json!(["Cinder Knight Subclass"]));
    assert_eq!(subclass["data"]["class_name"], "Cinder Knight");
    assert_eq!(subclass["data"]["class"], json!(class_id), "same class id");

    // a new character in the renamed class is offered the subclass at the subclass level
    let hub = app.state.hub().unwrap();
    let e = &hub.engine;
    let mut s = commands::new_character(e, ContentRuleset::Dnd2024);
    s = pick(e, &s, LEVEL1, "Class", &class_id);
    for _ in 1..3 {
        s = commands::level_up_main(e, &s, None).unwrap().state;
    }
    let snap = e.derive(&s);
    let choice = snap
        .slots
        .iter()
        .find(|x| x.options.iter().any(|o| o.id == sub_id))
        .expect("the renamed class's subclass choice offers the saved subclass");
    assert_eq!(choice.name, "Subclass");
    drop(hub);

    // the subclass opens with the class's new tag
    let opened = app
        .state
        .homebrew_open_draft(&pack, Some(&sub_id), None)
        .unwrap();
    assert_eq!(
        opened.form.fields["subclassTag"],
        json!("Cinder Knight Subclass")
    );
}

#[test]
fn the_class_form_finds_the_characters_that_use_a_saved_feature_before_it_goes() {
    let app = App::new();
    let pack = app.pack("Ember Isles", "EI", Ruleset::Dnd2024);
    let v = new_class(&app, &pack, "Ember Knight");
    let v = app.child(&pack, "classFeature", &v, None);
    let key = last_child(&v.form, "e");
    let mut form = v.form.clone();
    set(&mut form, &key, "name", json!("Ember Strike"));
    let v = app.update(&v, &form);
    let ids = app.save(&v);
    let feature_id = ids
        .iter()
        .find(|i| i.contains("EMBER_STRIKE"))
        .expect("the feature saved")
        .clone();

    // a character on this device that took the feature
    let characters = app.state.characters_dir();
    std::fs::create_dir_all(&characters).unwrap();
    std::fs::write(
        characters.join("a1b2c3d4.json"),
        json!({"meta": {"name": "Ash Vale"}, "elements": [feature_id]}).to_string(),
    )
    .unwrap();

    let opened = app
        .state
        .homebrew_open_draft(&pack, Some(&ids[0]), None)
        .unwrap();
    let saved_key = opened
        .form
        .children
        .iter()
        .find(|c| c.fields.get("name") == Some(&json!("Ember Strike")))
        .expect("the feature inside the class")
        .key
        .clone();
    let used = app
        .state
        .homebrew_child_use(&opened.draft_id, &saved_key)
        .unwrap();
    assert_eq!(
        used.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(),
        ["Ash Vale"]
    );
    // a feature the draft made is used by no one; neither is a key the draft doesn't have
    let fresh = app.child(&pack, "classFeature", &opened, None);
    let new_key = last_child(&fresh.form, "e");
    assert!(app
        .state
        .homebrew_child_use(&fresh.draft_id, &new_key)
        .unwrap()
        .is_empty());
    assert!(app
        .state
        .homebrew_child_use(&fresh.draft_id, "zz")
        .unwrap()
        .is_empty());
}

/// Hy610: a deep copy of the SRD Elf offers its copied lineage options by id; an option made inside the copied trait
/// joins that choice through the trait's Lineage options field, nothing to add by hand.
#[test]
fn an_option_made_inside_a_copied_lineage_trait_joins_its_choice() {
    let app = App::new();
    let pack = app.pack("Ember Isles", "EI", Ruleset::Dnd2024);
    let v = app
        .state
        .homebrew_new_draft(
            &pack,
            "species",
            Some("ID_2024_SPECIES_ELF"),
            None,
            Some(true),
        )
        .unwrap();
    let lineage = v
        .form
        .children
        .iter()
        .find(|c| c.fields.get("name") == Some(&json!("Elven Lineage")))
        .expect("the copied Elven Lineage")
        .key
        .clone();
    let v = app.child(&pack, "speciesTraitOption", &v, Some(&lineage));
    let moon = last_child(&v.form, &lineage);
    let mut form = v.form.clone();
    set(&mut form, &moon, "name", json!("Moon Elf"));
    let v = app.update(&v, &form);
    let entries = form_mut(&mut v.form.clone(), &lineage).fields["lineage"].clone();
    assert_eq!(entries.as_array().unwrap().len(), 4, "{entries}");
    app.save(&v);
    let trait_el = app.saved(&pack, "ID_EI_TRAIT_ELF_COPY_ELVEN_LINEAGE");
    let offered: Vec<String> = trait_el["rules"][0]["supports"]
        .as_str()
        .unwrap()
        .split('|')
        .map(str::to_string)
        .collect();
    assert_eq!(offered.len(), 4, "{offered:?}");
    assert!(
        offered.contains(&"ID_EI_TRAIT_OPTION_ELVEN_LINEAGE_MOON_ELF".to_string()),
        "{offered:?}"
    );
}
