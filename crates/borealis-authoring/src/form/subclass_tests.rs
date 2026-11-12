//! Subclasses (spec 4.7) and class options (4.8), field by field: a 2024 and a 2014 subclass read from their class
//! (the subclass feature levels derived, never stored) and written back unchanged with their features; a new
//! subclass of a loaded class takes the class's tag and level and its features follow it; a missing class is a plain
//! sentence at the class field; a third caster's section named after the subclass; an SRD invocation and a new
//! option. The bundled ones are in `tests/form_identity.rs`.

use crate::expr_build::condition::ConditionContext;
use crate::form::{
    add_child, assign_ids, element_group_to_form, element_to_form, form_to_elements, kind,
    refresh_blocks, CatalogBlockCodec, FormContext, FormState, Lookup, RefInfo,
};
use crate::ids::IdGenerator;
use borealis_content::{Element, Rule};
use serde_json::{json, Value};

/// A lookup that holds whole elements.
struct Elements(Vec<Element>);

impl Lookup for Elements {
    fn find(&self, id: &str) -> Option<RefInfo> {
        self.loaded(id).map(|e| RefInfo {
            name: e.name.clone(),
            ty: e.ty.clone(),
        })
    }
    fn loaded(&self, id: &str) -> Option<&Element> {
        self.0.iter().find(|e| e.id == id)
    }
    fn loaded_of_type(&self, ty: &str) -> Vec<&Element> {
        self.0.iter().filter(|e| e.ty == ty).collect()
    }
}

fn el(v: Value) -> Element {
    serde_json::from_value(v).expect("element")
}

/// A 2024 class with its subclass choice feature at 3 and a table naming subclass features at 7 and 10, and a 2014
/// class whose choice is gated on its feature, with a placeholder feature at 6.
fn content() -> Elements {
    let mut by_level = vec![json!(""); 20];
    by_level[2] = json!("Knight Subclass");
    by_level[6] = json!("Subclass feature");
    by_level[9] = json!("Subclass feature");
    Elements(vec![
        el(
            json!({"id": "ID_X_CLASS_KNIGHT", "name": "Knight", "type": "Class", "source": "X", "ruleset": "dnd2024",
            "rules": [{"kind": "grant", "id": "ID_X_CLASS_FEATURE_KNIGHT_SUBCLASS", "type": "Class Feature", "level": 3}],
            "data": {"features_by_level": by_level}}),
        ),
        el(
            json!({"id": "ID_X_CLASS_FEATURE_KNIGHT_SUBCLASS", "name": "Knight Subclass", "type": "Class Feature",
            "source": "X", "ruleset": "dnd2024",
            "rules": [{"kind": "select", "type": "Archetype", "name": "Knightly Order", "supports": "Knight Subclass"}]}),
        ),
        el(
            json!({"id": "ID_X_CLASS_WARDEN", "name": "Warden", "type": "Class", "source": "X",
            "rules": [{"kind": "grant", "id": "ID_X_CLASS_FEATURE_WARDEN_VOW", "type": "Class Feature", "level": 2},
                {"kind": "grant", "id": "ID_X_CLASS_FEATURE_WARDEN_VOW_FEATURE", "type": "Class Feature", "level": 6}]}),
        ),
        el(
            json!({"id": "ID_X_CLASS_FEATURE_WARDEN_VOW", "name": "Sacred Vow", "type": "Class Feature", "source": "X",
            "rules": [{"kind": "select", "type": "Archetype", "name": "Sacred Vow", "level": 2, "supports": "Sacred Vow"}]}),
        ),
        el(
            json!({"id": "ID_X_CLASS_FEATURE_WARDEN_VOW_FEATURE", "name": "Sacred Vow Feature", "type": "Class Feature",
            "source": "X"}),
        ),
    ])
}

fn with_cx<T>(f: impl FnOnce(&FormContext<'_>) -> T) -> T {
    let codec = CatalogBlockCodec;
    let lookup = content();
    let cx = FormContext {
        conditions: ConditionContext::new(["Knight", "Warden", "Warlock"]),
        lookup: &lookup,
        blocks: &codec,
    };
    f(&cx)
}

fn ids() -> IdGenerator {
    IdGenerator::new("HB", Vec::<String>::new(), Vec::<String>::new()).expect("generator")
}

/// A 2024 subclass of the Knight in the SRD shape, with its two features.
fn order() -> (Element, Element, Element) {
    let sub = el(json!({
        "id": "ID_X_SUBCLASS_KNIGHT_ROSE", "name": "Order of the Rose", "type": "Archetype", "source": "X",
        "ruleset": "dnd2024", "supports": ["Knight Subclass"],
        "rules": [{"kind": "grant", "id": "ID_X_SUBCLASS_FEATURE_ROSE_THORNS", "type": "Archetype Feature", "level": 3},
            {"kind": "grant", "id": "ID_X_SUBCLASS_FEATURE_ROSE_BLOOM", "type": "Archetype Feature", "level": 7}],
        "data": {"class": "ID_X_CLASS_KNIGHT", "class_name": "Knight", "level_gained": 3}
    }));
    let thorns = el(json!({
        "id": "ID_X_SUBCLASS_FEATURE_ROSE_THORNS", "name": "Thorns", "type": "Archetype Feature", "source": "X",
        "ruleset": "dnd2024", "rules": [{"kind": "critical_range", "min": 19, "applies_to": ["weapon attacks"]}],
        "data": {"class": "Knight", "subclass": "Order of the Rose", "level": 3}
    }));
    let bloom = el(json!({
        "id": "ID_X_SUBCLASS_FEATURE_ROSE_BLOOM", "name": "Bloom", "type": "Archetype Feature", "source": "X",
        "ruleset": "dnd2024", "data": {"class": "Knight", "subclass": "Order of the Rose", "level": 7}
    }));
    (sub, thorns, bloom)
}

fn ref_id(form: &FormState, key: &str) -> Option<String> {
    form.fields[key]["id"].as_str().map(str::to_string)
}

#[test]
fn a_2024_subclass_reads_its_class_and_writes_back_unchanged_with_its_features() {
    with_cx(|cx| {
        let (sub, thorns, bloom) = order();
        let form = element_group_to_form(&sub, &[&thorns, &bloom], cx);
        assert_eq!(form.kind, "subclass");
        assert_eq!(
            ref_id(&form, "parentClass").as_deref(),
            Some("ID_X_CLASS_KNIGHT")
        );
        assert_eq!(form.fields["parentClass"]["name"], "Knight");
        assert_eq!(form.fields["subclassTag"], "Knight Subclass");
        assert_eq!(form.fields["levelGained"], 3);
        // derived from the class: the choice, the table's subclass feature levels, the loaded subclasses (none)
        assert_eq!(form.fields["subclassLevels"], json!([3, 7, 10]));
        assert_eq!(form.fields["tags"], json!([]));
        assert!(form.blocks.is_empty(), "every grant is on the grid");
        assert_eq!(
            form.fields["levels"],
            json!([{"level": 3, "child": "e/0"}, {"level": 7, "child": "e/1"}])
        );
        assert_eq!(form.children[0].kind, "subclassFeature");
        let back = form_to_elements(&form, cx).expect("writes");
        assert_eq!(back, vec![sub, thorns, bloom]);
    });
}

#[test]
fn a_2014_subclass_finds_its_class_by_the_tag_and_keeps_its_shape() {
    with_cx(|cx| {
        let sub = el(json!({
            "id": "ID_X_ARCHETYPE_IRON", "name": "Iron Vow", "type": "Archetype", "source": "X",
            "supports": ["Sacred Vow"],
            "rules": [{"kind": "grant", "id": "ID_X_ARCHETYPE_FEATURE_IRON", "type": "Archetype Feature", "level": 2}]
        }));
        let form = element_to_form(&sub, cx);
        assert_eq!(
            ref_id(&form, "parentClass").as_deref(),
            Some("ID_X_CLASS_WARDEN")
        );
        assert_eq!(form.fields["subclassTag"], "Sacred Vow");
        assert_eq!(form.fields["levelGained"], Value::Null);
        // the choice's level and the "Sacred Vow Feature" placeholder's
        assert_eq!(form.fields["subclassLevels"], json!([2, 6]));
        assert_eq!(form_to_elements(&form, cx).expect("writes"), vec![sub]);
    });
}

/// A new subclass of `class` with one feature at `level`, refreshed as an edit is.
fn new_subclass(class: &str, name: &str, level: u32, cx: &FormContext<'_>) -> FormState {
    let mut form = kind("subclass").unwrap().new_form("e");
    form.fields.insert("name".into(), json!(name));
    form.fields.insert(
        "parentClass".into(),
        json!({"id": class, "name": "", "type": "Class"}),
    );
    let form = refresh_blocks(&form, cx);
    let mut feature = kind("subclassFeature").unwrap().new_form("n1");
    feature.fields.insert("name".into(), json!("First Light"));
    let form = add_child(&form, feature, Some(level)).expect("a subclass takes features");
    assign_ids(&form, &mut ids())
}

#[test]
fn a_new_subclass_takes_its_class_s_choice_and_its_features_follow_it() {
    with_cx(|cx| {
        let form = new_subclass("ID_X_CLASS_KNIGHT", "Order of Dawn", 3, cx);
        assert_eq!(form.fields["subclassTag"], "Knight Subclass");
        assert_eq!(form.fields["levelGained"], 3);
        assert_eq!(form.fields["subclassLevels"], json!([3, 7, 10]));
        let out = form_to_elements(&form, cx).expect("writes");
        let sub = serde_json::to_value(&out[0]).unwrap();
        assert_eq!(sub["type"], "Archetype");
        assert_eq!(sub["supports"], json!(["Knight Subclass"]));
        assert_eq!(
            sub["data"],
            json!({"class": "ID_X_CLASS_KNIGHT", "class_name": "Knight", "level_gained": 3})
        );
        assert_eq!(
            sub["rules"],
            json!([{"kind": "grant", "id": out[1].id, "type": "Archetype Feature", "level": 3}])
        );
        assert!(
            out[1].id.starts_with("ID_HB_SUBCLASS_FEATURE_"),
            "{}",
            out[1].id
        );
        assert_eq!(out[1].ty, "Archetype Feature");
        assert_eq!(
            serde_json::to_value(&out[1].data).unwrap(),
            json!({"class": "Knight", "subclass": "Order of Dawn", "level": 3})
        );
        // renamed, its feature follows; a 2014 class's choice gives its label as the tag
        let renamed = form.with_field("name", json!("Order of Noon"));
        let out = form_to_elements(&renamed, cx).expect("writes");
        assert_eq!(out[1].data["subclass"], "Order of Noon");
        let warden = new_subclass("ID_X_CLASS_WARDEN", "Iron Vow", 2, cx);
        assert_eq!(warden.fields["subclassTag"], "Sacred Vow");
        assert_eq!(warden.fields["levelGained"], 2);
        let out = form_to_elements(&warden, cx).expect("writes");
        assert_eq!(out[0].supports, vec!["Sacred Vow".to_string()]);
    });
}

#[test]
fn an_edit_keeps_the_tag_until_another_class_is_picked() {
    with_cx(|cx| {
        let (sub, thorns, bloom) = order();
        let form = element_group_to_form(&sub, &[&thorns, &bloom], cx);
        // an edit of another field: the stored tag and level stay
        let edited = refresh_blocks(&form.with_field("name", json!("Order of the Thorn")), cx);
        assert_eq!(edited.fields["subclassTag"], "Knight Subclass");
        let out = form_to_elements(&edited, cx).expect("writes");
        assert_eq!(out[0].supports, sub.supports);
        assert_eq!(out[0].data, sub.data);
        assert_eq!(out[1].data["subclass"], "Order of the Thorn");
        // another class: its tag and level
        let moved = refresh_blocks(
            &form.with_field(
                "parentClass",
                json!({"id": "ID_X_CLASS_WARDEN", "name": "Warden", "type": "Class"}),
            ),
            cx,
        );
        assert_eq!(moved.fields["subclassTag"], "Sacred Vow");
        assert_eq!(moved.fields["subclassLevels"], json!([2, 6]));
        let out = form_to_elements(&moved, cx).expect("writes");
        assert_eq!(out[0].supports, vec!["Sacred Vow".to_string()]);
        assert_eq!(out[0].data["class"], "ID_X_CLASS_WARDEN");
        assert_eq!(out[0].data["class_name"], "Warden");
        assert_eq!(out[0].data["level_gained"], 2);
        assert_eq!(out[1].data["class"], "Warden");
    });
}

#[test]
fn a_subclass_without_a_class_says_so_at_the_class_field() {
    with_cx(|cx| {
        let mut form = kind("subclass").unwrap().new_form("e");
        form.fields.insert("name".into(), json!("Lost Order"));
        let form = assign_ids(&refresh_blocks(&form, cx), &mut ids());
        let problems = form_to_elements(&form, cx).expect_err("no class");
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert_eq!(problems[0].path, "fields.parentClass");
        assert!(
            problems[0].message.starts_with("Choose the class"),
            "{problems:?}"
        );
    });
}

#[test]
fn a_third_caster_subclass_feature_counts_its_slots_under_the_subclass() {
    with_cx(|cx| {
        let mut form = kind("subclass").unwrap().new_form("e");
        form.fields.insert("name".into(), json!("Spell Knight"));
        form.fields.insert(
            "parentClass".into(),
            json!({"id": "ID_X_CLASS_KNIGHT", "name": "Knight", "type": "Class"}),
        );
        form.fields.insert("edition".into(), json!("dnd2024"));
        let form = refresh_blocks(&form, cx);
        let mut sc = kind("subclassFeature").unwrap().new_form("n1");
        sc.fields.insert("name".into(), json!("Spellcasting"));
        sc.fields.insert("casts".into(), json!(true));
        sc.fields.insert("ability".into(), json!("Intelligence"));
        sc.fields.insert("casterType".into(), json!("third"));
        sc.fields.insert("spellList".into(), json!("Wizard"));
        let form = assign_ids(&add_child(&form, sc, Some(3)).unwrap(), &mut ids());
        let out = form_to_elements(&form, cx).expect("writes");
        let feature = &out[1];
        let node = feature.spellcasting.as_ref().expect("a section");
        assert_eq!(node.name.as_deref(), Some("Spell Knight"));
        let slot_1: Vec<(String, Option<u32>)> = feature
            .rules
            .iter()
            .filter_map(|r| match r {
                Rule::Stat(s) if s.name == "spell knight:spellcasting:slots:1" => {
                    Some((s.value.clone(), s.level))
                }
                _ => None,
            })
            .collect();
        // a third caster gets its first slots at class level 3
        assert_eq!(
            slot_1.first(),
            Some(&("2".to_string(), Some(3))),
            "{slot_1:?}"
        );
        assert!(feature.rules.iter().any(|r| matches!(r, Rule::Grant(g)
            if g.id == "ID_INTERNAL_GRANT_MULTICLASS_SPELLCASTING_SLOTS_THIRD")));
    });
}

fn invocation() -> Element {
    el(json!({
        "id": "ID_X_OPTION_ELDRITCH_INVOCATION_EMBER_SIGHT", "name": "Ember Sight", "type": "Option", "source": "X",
        "ruleset": "dnd2024", "supports": ["Eldritch Invocation"],
        "description": "<p>You see in the dark.</p>",
        "rules": [{"kind": "grant", "id": "ID_VISION_DARKVISION", "type": "Vision"}],
        "data": {"class_name": "Warlock", "min_warlock_level": 2, "requires_invocation": null, "repeatable": true},
        "prerequisite": "Level 2+ Warlock", "requirements": "[warlock:2]"
    }))
}

#[test]
fn an_srd_invocation_reads_its_group_class_level_and_repeatable() {
    with_cx(|cx| {
        let e = invocation();
        let form = element_to_form(&e, cx);
        assert_eq!(form.kind, "classOption");
        assert_eq!(form.fields["group"], "Eldritch Invocation");
        assert_eq!(form.fields["className"], "Warlock");
        assert_eq!(
            form.fields["repeatable"], false,
            "the engine reads `data.repeatable.allowed`, not the plain boolean"
        );
        assert_eq!(form.fields["tags"], json!([]));
        assert_eq!(form.fields["requirements"]["original"], "[warlock:2]");
        assert_eq!(
            form.fields["requirements"]["value"]["shape"], "model",
            "the condition builder holds the level: {}",
            form.fields["requirements"]
        );
        assert_eq!(form.blocks.len(), 1);
        let assign = |f: &FormState| assign_ids(f, &mut ids());
        assert_eq!(
            form_to_elements(&assign(&form), cx).unwrap(),
            vec![e.clone()]
        );
        // an edit keeps the mirrors no field holds; off is how it reads, so the stored boolean stays
        let off = form
            .with_field("repeatable", json!(false))
            .with_field("group", json!("Pact Secret"));
        let out = form_to_elements(&assign(&off), cx).unwrap();
        assert_eq!(out[0].data["repeatable"], json!(true));
        assert_eq!(out[0].data["min_warlock_level"], 2);
        assert_eq!(out[0].supports, vec!["Pact Secret".to_string()]);
        // turned on, it takes the shape the engine reads
        let on = form.with_field("repeatable", json!(true));
        let out = form_to_elements(&assign(&on), cx).unwrap();
        assert_eq!(out[0].data["repeatable"], json!({"allowed": true}));
    });
}

#[test]
fn a_new_option_writes_its_group_class_level_and_repeatable() {
    with_cx(|cx| {
        let mut form = kind("classOption").unwrap().new_form("e");
        form.fields.insert("name".into(), json!("Ember Sight"));
        form.fields
            .insert("group".into(), json!("Eldritch Invocation"));
        form.fields.insert("className".into(), json!("Warlock"));
        form.fields.insert("repeatable".into(), json!(true));
        let level = crate::expr_build::ExprValue::read::<
            crate::expr_build::condition::ConditionLanguage,
        >("[warlock:2]", &cx.conditions);
        form.fields
            .insert("requirements".into(), serde_json::to_value(level).unwrap());
        let out = form_to_elements(&assign_ids(&form, &mut ids()), cx).expect("writes");
        let e = serde_json::to_value(&out[0]).unwrap();
        assert_eq!(e["id"], "ID_HB_OPTION_EMBER_SIGHT");
        assert_eq!(e["type"], "Option");
        assert_eq!(e["supports"], json!(["Eldritch Invocation"]));
        assert_eq!(e["requirements"], "[warlock:2]");
        assert_eq!(
            e["data"],
            json!({"class_name": "Warlock", "repeatable": {"allowed": true}})
        );
        // a group with an expression's characters is refused at the field
        let bad = form.with_field("group", json!("Pacts, Boons"));
        let problems = form_to_elements(&assign_ids(&bad, &mut ids()), cx).unwrap_err();
        assert_eq!(problems[0].path, "fields.group");
    });
}

/// A class whose subclass choice is on the class itself.
fn class_with_choice(name: &str, label: &str, tag: &str) -> Element {
    el(
        json!({"id": "ID_EI_CLASS_EMBER", "name": name, "type": "Class", "source": "EI", "ruleset": "dnd2024",
        "rules": [{"kind": "select", "type": "Archetype", "name": label, "level": 3, "supports": tag}]}),
    )
}

#[test]
fn a_renamed_class_s_subclasses_take_its_new_tag() {
    let before = [class_with_choice(
        "Ember Knight",
        "Subclass",
        "Ember Knight Subclass",
    )];
    let after = [class_with_choice(
        "Cinder Knight",
        "Subclass",
        "Cinder Knight Subclass",
    )];
    let sub = |id: &str, tag: &str, data: Value| {
        el(
            json!({"id": id, "name": id, "type": "Archetype", "source": "EI", "supports": [tag], "data": data}),
        )
    };
    let pack = [
        // names the class: follows
        sub(
            "ID_A",
            "Ember Knight Subclass",
            json!({"class": "ID_EI_CLASS_EMBER", "class_name": "Ember Knight"}),
        ),
        // no data.class, the 2024 tag the class wrote from its name: follows
        sub("ID_B", "Ember Knight Subclass", json!({})),
        // another class's subclass with the same tag: stays
        sub(
            "ID_C",
            "Ember Knight Subclass",
            json!({"class": "ID_OTHER"}),
        ),
        // another tag: stays
        sub(
            "ID_D",
            "Fighter Subclass",
            json!({"class": "ID_EI_CLASS_EMBER"}),
        ),
    ];
    let followed = super::follow_class(&before, &after, &pack);
    let ids: Vec<&str> = followed.iter().map(|e| e.id.as_str()).collect();
    assert_eq!(ids, ["ID_A", "ID_B"]);
    assert_eq!(followed[0].supports, ["Cinder Knight Subclass"]);
    assert_eq!(followed[0].data["class_name"], "Cinder Knight");
    assert!(!followed[1].data.contains_key("class_name"));
    // unchanged: nothing follows; a 2014 label shared by another class without data.class stays
    assert!(super::follow_class(&before, &before, &pack).is_empty());
    let old = [class_with_choice(
        "Ember Knight",
        "Martial Archetype",
        "Martial Archetype",
    )];
    let new = [class_with_choice(
        "Ember Knight",
        "Ember Order",
        "Ember Order",
    )];
    let shared = [sub("ID_E", "Martial Archetype", json!({}))];
    assert!(super::follow_class(&old, &new, &shared).is_empty());
}
