//! The live pipeline by hand: validator issues, rule problems and lints as plain sentences at form paths. The
//! bundled corpus is covered by `tests/check_live.rs`.

use super::*;
use crate::form::{element_to_form, CatalogBlockCodec, FormContext};
use borealis_content::file::PackOrigin;
use borealis_content::{ContentStoreBuilder, Element};
use serde_json::{json, Value};

fn element(v: Value) -> Element {
    serde_json::from_value(v).expect("element")
}

fn loaded() -> ContentStore {
    let spell = element(
        json!({"id": "ID_T_SPELL_SPARK", "name": "Spark", "type": "Spell", "source": "T",
        "setters": [{"name": "level", "value": "0"}, {"name": "school", "value": "Evocation"},
            {"name": "time", "value": "1 action"}, {"name": "duration", "value": "Instantaneous"},
            {"name": "range", "value": "60 feet"}]}),
    );
    ContentStoreBuilder::new()
        .add_file("t.json", PackOrigin::Custom, ContentFile::new(vec![spell]))
        .build()
}

fn feat(rules: Value, description: &str) -> Element {
    element(
        json!({"id": "ID_T_FEAT_HOWL", "name": "Howl", "type": "Feat", "source": "T",
        "description": description, "rules": rules}),
    )
}

fn run(
    e: &Element,
    pack: &[PackElement],
) -> Vec<(Option<String>, String, String, ProblemSeverity)> {
    let store = loaded();
    let codec = CatalogBlockCodec;
    let cx = FormContext::from_store(&store, &codec);
    let form = element_to_form(e, &cx);
    let hidden = HashSet::new();
    live_problems(&LiveInput {
        form: &form,
        cx: &cx,
        store: &store,
        file: "mine/content.json",
        hidden: &hidden,
        pack,
    })
    .into_iter()
    .map(|p| (p.path, p.code, p.message, p.severity))
    .collect()
}

#[test]
fn a_clean_element_has_no_problems() {
    let e = feat(
        json!([{"kind": "grant", "id": "ID_T_SPELL_SPARK", "type": "Spell"}]),
        "<p>A howl.</p>",
    );
    assert_eq!(run(&e, &[]), vec![]);
}

#[test]
fn a_missing_reference_is_named_by_the_last_name_the_form_knew() {
    let e = feat(
        json!([{"kind": "grant", "id": "ID_T_FEAT_SHIELD_MASTER", "type": "Feat"}]),
        "<p>A howl.</p>",
    );
    let store = loaded();
    let codec = CatalogBlockCodec;
    let cx = FormContext::from_store(&store, &codec);
    let mut form = element_to_form(&e, &cx);
    form.blocks[0].fields.insert(
        "id".into(),
        json!({"id": "ID_T_FEAT_SHIELD_MASTER", "name": "Shield Master", "type": "Feat"}),
    );
    let hidden = HashSet::new();
    let problems = live_problems(&LiveInput {
        form: &form,
        cx: &cx,
        store: &store,
        file: "mine/content.json",
        hidden: &hidden,
        pack: &[],
    });
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert_eq!(problems[0].code, "dangling_reference");
    assert_eq!(problems[0].path.as_deref(), Some("blocks.0"));
    assert_eq!(problems[0].severity, ProblemSeverity::Warning);
    assert_eq!(
        problems[0].message,
        "\"Shield Master\" isn't loaded any more (its pack was removed or turned off). Pick it again."
    );
    assert_eq!(problems[0].element_id.as_deref(), Some("ID_T_FEAT_HOWL"));
}

#[test]
fn a_rule_the_engine_does_not_read_is_kept_with_a_warning() {
    let e = feat(
        json!([{"kind": "select.ability_distribution", "pool": ["strength"]}]),
        "<p>A howl.</p>",
    );
    assert_eq!(
        run(&e, &[]),
        vec![(
            Some("blocks.0".into()),
            "content.unknown_rule_kind".into(),
            "Kept from the original: Borealis doesn't use this rule.".into(),
            ProblemSeverity::Warning
        )]
    );
}

#[test]
fn lints_name_the_twin_the_paste_leftovers_and_the_missing_pool() {
    let e = feat(
        json!([{"kind": "resource_cost", "resource": "ember points", "cost": 1}]),
        "<p>Fire&amp;amp;ice</p>",
    );
    let twin = PackElement {
        id: "ID_T_FEAT_HOWL_2".into(),
        name: "howl ".into(),
        ty: "Feat".into(),
    };
    let got = run(&e, &[twin]);
    let codes: Vec<(&str, &str)> = got
        .iter()
        .map(|(p, c, ..)| (p.as_deref().unwrap_or(""), c.as_str()))
        .collect();
    assert_eq!(
        codes,
        vec![
            ("fields.name", "lint.duplicate_name"),
            ("fields.description", "lint.escape"),
            ("blocks.0.fields.resource", "lint.unknown_pool"),
        ]
    );
    assert_eq!(
        got[0].2,
        "Another feat in this pack is already called \"Howl\"."
    );
    assert_eq!(
        got[2].2,
        "Nothing defines the points \"ember points\" this costs. Add the pool, or pick one that exists."
    );
}

#[test]
fn an_empty_description_is_a_hint_only() {
    let got = run(&feat(json!([]), ""), &[]);
    assert_eq!(
        got,
        vec![(
            Some("fields.description".into()),
            "lint.empty_description".into(),
            "There is no description yet.".into(),
            ProblemSeverity::Info
        )]
    );
}

#[test]
fn a_missing_required_setter_points_at_the_field_that_holds_it() {
    // a spell (its own form): the form says it once per field, the validator's missing setter is not said again
    let spell = element(
        json!({"id": "ID_T_SPELL_NEW", "name": "New", "type": "Spell", "source": "T",
        "description": "<p>x</p>", "setters": [{"name": "level", "value": "1"}]}),
    );
    let got = run(&spell, &[]);
    let errors: Vec<(&str, &str, &str)> = got
        .iter()
        .filter(|p| p.3 == ProblemSeverity::Error)
        .map(|p| (p.0.as_deref().unwrap_or(""), p.1.as_str(), p.2.as_str()))
        .collect();
    assert_eq!(
        errors,
        [
            ("fields.school", "form.required", "A spell needs a school."),
            (
                "fields.time",
                "form.required",
                "A spell needs a casting time."
            ),
            ("fields.range", "form.required", "A spell needs a range."),
            (
                "fields.duration",
                "form.required",
                "A spell needs a duration."
            ),
        ],
        "{got:?}"
    );
    // a class (its own form since WB-16): the missing hit die points at the Hit die field
    let class = element(
        json!({"id": "ID_T_CLASS_NEW", "name": "New", "type": "Class", "source": "T", "description": "<p>x</p>"}),
    );
    let got = run(&class, &[]);
    let missing: Vec<_> = got
        .iter()
        .filter(|p| p.0.as_deref() == Some("fields.hitDie"))
        .collect();
    assert_eq!(missing.len(), 1, "{got:?}");
    assert_eq!(missing[0].2, "A class needs a hit die.");
    assert_eq!(missing[0].3, ProblemSeverity::Error);
}

#[test]
fn schema_locations_are_read_from_pointers() {
    assert_eq!(
        schema_location("/elements/0/rules/2/kind"),
        Some((0, Some("rules[2]".into())))
    );
    assert_eq!(
        schema_location("/elements/1/data/rarity"),
        Some((1, Some("data.rarity".into())))
    );
    assert_eq!(schema_location("/elements/3"), Some((3, None)));
    assert_eq!(schema_location("/format"), None);
}

#[test]
fn a_subrace_no_race_offers_is_told_at_its_parent_race() {
    let race = element(
        json!({"id": "ID_T_RACE_DWARF", "name": "Dwarf", "type": "Race", "source": "T", "description": "<p>D.</p>",
        "rules": [{"kind": "select", "type": "Sub Race", "name": "Dwarven Subrace", "supports": "Dwarf"}]}),
    );
    let store = ContentStoreBuilder::new()
        .add_file("t.json", PackOrigin::Custom, ContentFile::new(vec![race]))
        .build();
    let codec = CatalogBlockCodec;
    let cx = FormContext::from_store(&store, &codec);
    let hidden = HashSet::new();
    let lints_of = |parent: &str| {
        let sub = element(
            json!({"id": "ID_T_SUBRACE_ASH", "name": "Ash Dwarf", "type": "Sub Race",
            "source": "T", "description": "<p>Ash.</p>", "supports": [parent]}),
        );
        let form = element_to_form(&sub, &cx);
        live_problems(&LiveInput {
            form: &form,
            cx: &cx,
            store: &store,
            file: "mine/content.json",
            hidden: &hidden,
            pack: &[],
        })
        .into_iter()
        .map(|p| (p.path.unwrap_or_default(), p.code, p.message))
        .collect::<Vec<_>>()
    };
    assert_eq!(lints_of("Dwarf"), vec![]);
    assert_eq!(
        lints_of("Dwarrf"),
        vec![(
            "fields.parent".to_string(),
            "lint.not_offered".to_string(),
            "No loaded race's subrace choice offers it. Check the parent race: it is the tag that choice offers, usually the race's name."
                .to_string()
        )]
    );
}

/// The live problems of a spell on `lists`, with a Wizard class and a feat whose choice picks spells by "Shadow".
fn spell_list_lints(lists: &[&str]) -> Vec<(String, String, String, ProblemSeverity)> {
    let wizard = element(
        json!({"id": "ID_T_CLASS_WIZARD", "name": "Wizard", "type": "Class", "source": "T",
        "spellcasting": {"name": "Wizard", "ability": "Intelligence", "list": {"supports": "Wizard"}}}),
    );
    let knight = element(
        json!({"id": "ID_T_CLASS_FEATURE_KNIGHT", "name": "Knight Spellcasting", "type": "Class Feature",
        "source": "T", "spellcasting": {"name": "Knight", "list": {"supports": "(Oracle||Wizard),(Abjuration||Evocation)"}}}),
    );
    let gloom = element(
        json!({"id": "ID_T_FEAT_GLOOM", "name": "Gloom", "type": "Feat", "source": "T",
        "rules": [{"kind": "select", "type": "Spell", "name": "Gloom Spell", "supports": "Shadow,1"}]}),
    );
    let store = ContentStoreBuilder::new()
        .add_file(
            "t.json",
            PackOrigin::Custom,
            ContentFile::new(vec![wizard, knight, gloom]),
        )
        .build();
    let codec = CatalogBlockCodec;
    let cx = FormContext::from_store(&store, &codec);
    let spell = element(
        json!({"id": "ID_T_SPELL_GLIMMER", "name": "Glimmer", "type": "Spell", "source": "T",
        "description": "<p>A glimmer.</p>", "supports": lists,
        "setters": [{"name": "level", "value": "1"}, {"name": "school", "value": "Illusion"},
            {"name": "time", "value": "Action"}, {"name": "duration", "value": "Instantaneous"},
            {"name": "range", "value": "Self"}]}),
    );
    let form = element_to_form(&spell, &cx);
    let hidden = HashSet::new();
    live_problems(&LiveInput {
        form: &form,
        cx: &cx,
        store: &store,
        file: "mine/content.json",
        hidden: &hidden,
        pack: &[],
    })
    .into_iter()
    .filter(|p| p.path.as_deref() == Some("fields.spellLists"))
    .map(|p| (p.path.unwrap_or_default(), p.code, p.message, p.severity))
    .collect()
}

#[test]
fn a_spell_list_nothing_declares_is_a_warning_with_the_near_spelling() {
    assert_eq!(spell_list_lints(&["Wizard"]), vec![], "a class's list");
    assert_eq!(
        spell_list_lints(&["Oracle"]),
        vec![],
        "a list in an expression"
    );
    assert_eq!(
        spell_list_lints(&["Shadow"]),
        vec![],
        "a list a choice picks spells from"
    );
    let at = "fields.spellLists".to_string();
    let code = "lint.unknown_spell_list".to_string();
    assert_eq!(
        spell_list_lints(&["Wizard", "Wizzard"]),
        vec![(
            at.clone(),
            code.clone(),
            "No loaded class or choice uses the spell list \"Wizzard\". Check the spelling: there is a list \"Wizard\"."
                .to_string(),
            ProblemSeverity::Warning
        )]
    );
    assert_eq!(
        spell_list_lints(&["wizard"]),
        vec![(
            at.clone(),
            code.clone(),
            "No loaded class or choice uses the spell list \"wizard\". Lists match by exact spelling: there is a list \"Wizard\"."
                .to_string(),
            ProblemSeverity::Warning
        )]
    );
    assert_eq!(
        spell_list_lints(&["Artificer"]),
        vec![(
            at,
            code,
            "No loaded class or choice uses the spell list \"Artificer\". Check the spelling, or make the class that has it."
                .to_string(),
            ProblemSeverity::Warning
        )]
    );
}

#[test]
fn a_spell_on_no_list_is_a_hint() {
    assert_eq!(
        spell_list_lints(&[]),
        vec![(
            "fields.spellLists".to_string(),
            "lint.no_spell_list".to_string(),
            "This spell is on no spell list, so no class offers it. It can still be given by a feature.".to_string(),
            ProblemSeverity::Info
        )]
    );
}

/// The live problems at a class option's group, with a class whose choice offers "Eldritch Invocation", loaded
/// "Cunning Strike" options no choice offers (a feature uses them), and an option given by its id alone.
fn group_lints(group: &str, id: &str) -> Vec<(String, String, String, ProblemSeverity)> {
    let invocations = element(
        json!({"id": "ID_T_CLASS_FEATURE_INVOCATIONS", "name": "Eldritch Invocations", "type": "Class Feature",
        "source": "T", "rules": [{"kind": "select", "type": "Option", "name": "Eldritch Invocation", "supports": "Eldritch Invocation", "level": 2}]}),
    );
    let strike = element(
        json!({"id": "ID_T_OPTION_POISON", "name": "Poison", "type": "Option", "source": "T", "supports": ["Cunning Strike"]}),
    );
    let sense = element(
        json!({"id": "ID_T_CLASS_FEATURE_SENSE", "name": "Divine Sense", "type": "Class Feature", "source": "T",
        "rules": [{"kind": "grant", "type": "Option", "id": "ID_T_OPTION_GLOW"}]}),
    );
    let store = ContentStoreBuilder::new()
        .add_file(
            "t.json",
            PackOrigin::Custom,
            ContentFile::new(vec![invocations, strike, sense]),
        )
        .build();
    let codec = CatalogBlockCodec;
    let cx = FormContext::from_store(&store, &codec);
    let option = element(
        json!({"id": id, "name": "Ember Gaze", "type": "Option", "source": "T",
        "description": "<p>Your eyes burn.</p>", "supports": [group]}),
    );
    let form = element_to_form(&option, &cx);
    assert_eq!(form.kind, "classOption");
    let hidden = HashSet::new();
    live_problems(&LiveInput {
        form: &form,
        cx: &cx,
        store: &store,
        file: "mine/content.json",
        hidden: &hidden,
        pack: &[],
    })
    .into_iter()
    .filter(|p| p.path.as_deref() == Some("fields.group"))
    .map(|p| (p.path.unwrap_or_default(), p.code, p.message, p.severity))
    .collect()
}

#[test]
fn a_class_option_group_no_choice_offers_is_a_warning() {
    let id = "ID_T_OPTION_EMBER_GAZE";
    assert_eq!(
        group_lints("Eldritch Invocation", id),
        vec![],
        "a choice offers it"
    );
    assert_eq!(
        group_lints("Cunning Strike", id),
        vec![],
        "a group loaded options are in (a feature uses them)"
    );
    assert_eq!(
        group_lints("Glow", "ID_T_OPTION_GLOW"),
        vec![],
        "the option is given by its id"
    );
    let warning = |message: &str| {
        vec![(
            "fields.group".to_string(),
            "lint.group_not_offered".to_string(),
            message.to_string(),
            ProblemSeverity::Warning,
        )]
    };
    assert_eq!(
        group_lints("Eldritch Invocations", id),
        warning("No loaded class's choice offers the group \"Eldritch Invocations\", so no character can take this option. Check the spelling: there is a group \"Eldritch Invocation\".")
    );
    assert_eq!(
        group_lints("Ember Techniques", id),
        warning("No loaded class's choice offers the group \"Ember Techniques\", so no character can take this option. Add a choice of it to a class (Lets the player choose), or pick a group a class offers.")
    );
}
