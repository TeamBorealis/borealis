//! A spellcasting node that is no section, on hand-written cases: a 2014 trait's casting ability and a 2014 subclass
//! feature's expanded spell list read into their fields, write back byte for byte, and are written from the fields
//! alone (the node's name from the parent) when made new. A node of another shape stays preserved.

use crate::expr_build::condition::ConditionContext;
use crate::form::{
    element_group_to_form, element_to_form, form_to_element, form_to_elements, CatalogBlockCodec,
    FormContext, FormState, Memo, NoLookup,
};
use borealis_content::Element;
use serde_json::{json, Value};

fn with_cx<T>(f: impl FnOnce(&FormContext<'_>) -> T) -> T {
    let codec = CatalogBlockCodec;
    let cx = FormContext {
        conditions: ConditionContext::new(["Warlock"]),
        lookup: &NoLookup,
        blocks: &codec,
    };
    f(&cx)
}

fn el(v: Value) -> Element {
    serde_json::from_value(v).expect("element")
}

/// The form with its stored text and kept parts forgotten, the ids kept: as if every field were typed.
fn fresh(form: &FormState) -> FormState {
    let mut f = form.clone();
    f.memo = Memo {
        id: form.memo.id.clone(),
        ..Memo::default()
    };
    f.preserved.clear();
    f.children = form.children.iter().map(fresh).collect();
    f
}

fn race() -> Element {
    el(
        json!({"id": "ID_X_SUBRACE_STAR_ELF", "name": "Star Elf", "type": "Sub Race", "source": "X",
        "supports": ["Elf"],
        "rules": [{"kind": "grant", "type": "Racial Trait", "id": "ID_X_RACIAL_TRAIT_STAR_CANTRIP"}]}),
    )
}

fn cantrip_trait() -> Element {
    el(
        json!({"id": "ID_X_RACIAL_TRAIT_STAR_CANTRIP", "name": "Cantrip", "type": "Racial Trait", "source": "X",
        "spellcasting": {"name": "Star Elf", "ability": "Intelligence"},
        "rules": [{"kind": "select", "type": "Spell", "name": "Cantrip, Star Elf",
            "supports": "Wizard,0", "spellcasting": "Star Elf"}]}),
    )
}

fn subclass() -> Element {
    el(
        json!({"id": "ID_X_ARCHETYPE_EMBER", "name": "The Ember", "type": "Archetype", "source": "X",
        "supports": ["Otherworldly Patron"],
        "rules": [{"kind": "grant", "type": "Archetype Feature", "id": "ID_X_ARCHETYPE_FEATURE_EMBER_LIST", "level": 1}]}),
    )
}

fn expanded_list() -> Element {
    el(
        json!({"id": "ID_X_ARCHETYPE_FEATURE_EMBER_LIST", "name": "Expanded Spell List", "type": "Archetype Feature",
        "source": "X",
        "spellcasting": {"name": "Warlock", "extend": true,
            "extends": [{"supports": "ID_PHB_SPELL_BURNING_HANDS"}, {"supports": "ID_PHB_SPELL_COMMAND"}]}}),
    )
}

#[test]
fn a_trait_casting_ability_reads_writes_back_and_is_made_from_its_field() {
    with_cx(|cx| {
        let t = cantrip_trait();
        let form = element_to_form(&t, cx);
        assert_eq!(form.fields["castingAbility"], json!("Intelligence"));
        assert!(form.preserved.is_empty(), "{:?}", form.preserved);
        assert_eq!(form_to_element(&form, cx).unwrap(), t);
        // made new inside its race: the name is the race's
        let group = element_group_to_form(&race(), &[&t], cx);
        let made = form_to_elements(&fresh(&group), cx).unwrap();
        assert_eq!(made[1].spellcasting, t.spellcasting);
        // edited
        let wis = form.with_field("castingAbility", json!("wisdom"));
        let e = form_to_element(&wis, cx).unwrap();
        let sc = e.spellcasting.unwrap();
        assert_eq!(sc.ability.as_deref(), Some("Wisdom"));
        assert_eq!(
            sc.name.as_deref(),
            Some("Star Elf"),
            "the stored name stays"
        );
        // cleared: no node
        let off = form.with_field("castingAbility", json!(""));
        assert!(form_to_element(&off, cx).unwrap().spellcasting.is_none());
        let bad = form.with_field("castingAbility", json!("Luck"));
        assert_eq!(
            form_to_element(&bad, cx).unwrap_err()[0].path,
            "fields.castingAbility"
        );
    });
}

#[test]
fn an_expanded_spell_list_reads_writes_back_and_is_made_from_its_field() {
    with_cx(|cx| {
        let f = expanded_list();
        let form = element_to_form(&f, cx);
        let ids: Vec<&str> = form.fields["expandedSpells"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|r| r["id"].as_str())
            .collect();
        assert_eq!(ids, ["ID_PHB_SPELL_BURNING_HANDS", "ID_PHB_SPELL_COMMAND"]);
        assert!(form.preserved.is_empty(), "{:?}", form.preserved);
        assert_eq!(form_to_element(&form, cx).unwrap(), f);
        // made new inside its subclass of a Warlock: the name is the class's
        let mut group = element_group_to_form(&subclass(), &[&f], cx);
        group.fields.insert("className".into(), json!("Warlock"));
        let made = form_to_elements(&fresh(&group), cx).unwrap();
        let sc = made[1].spellcasting.clone().unwrap();
        assert_eq!(sc.extend, Some(true));
        assert_eq!(sc.extends, f.spellcasting.as_ref().unwrap().extends);
        // emptied: no node
        let none = form.with_field("expandedSpells", json!([]));
        assert!(form_to_element(&none, cx).unwrap().spellcasting.is_none());
    });
}

#[test]
fn a_node_of_another_shape_stays_preserved() {
    with_cx(|cx| {
        let mut t = cantrip_trait();
        t.spellcasting.as_mut().unwrap().prepare = Some(true);
        let form = element_to_form(&t, cx);
        assert_eq!(form.fields["castingAbility"], json!(""));
        assert!(form.preserved.iter().any(|p| p.key == "spellcasting"));
        assert_eq!(form_to_element(&form, cx).unwrap(), t);
    });
}
