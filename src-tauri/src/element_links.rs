//! The description pane's linked elements: the other elements an element grants, offers, creates
//! or names, each with its own text, so choosing an option shows what it gives (an item an option
//! creates, a spell it grants, a condition it applies, a creature it calls) without leaving the
//! pick. Only the element data decides what is linked: a rule that names an element id, or an
//! element id in the description's markup. A `<div element="ID"/>` / `<p element="ID"/>`
//! reference the description pane already injects (borealis_engine::inject) is not linked again.

use crate::compendium::store_description_html;
use crate::dto_ext::ElementLink;
use crate::element_grants::has_grant_text;
use crate::elements::summary;
use borealis_content::{ContentStore, Element, Rule};
use serde_json::Value;
use std::cell::RefCell;

/// Content types that are one line of the character (a proficiency, a language, a size, a sense), not a thing to read:
/// the Grants list names them, and their text only repeats the name.
const ONE_LINE_TYPES: [&str; 8] = [
    "Proficiency",
    "Language",
    "Size",
    "Vision",
    "Skill",
    "Grants",
    "Source",
    "List",
];

/// Most linked elements one description lists (a domain's spell list is ten; the rest is noise).
pub(crate) const MAX_LINKS: usize = 24;

/// Why an element is linked, as the UI labels it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Via {
    /// A `grant` rule, or a `grant_feat` (the Grants list names both).
    Grant,
    /// Any other rule that names the element (`grant_spell`, a condition it applies, ...).
    Rule,
    /// A `select` rule's default, list or `supports` ids.
    Select,
    /// An element id in the description markup.
    Description,
}

impl Via {
    fn as_str(self) -> &'static str {
        match self {
            Self::Grant => "grant",
            Self::Rule => "rule",
            Self::Select => "select",
            Self::Description => "description",
        }
    }
}

/// The ids `el` links to, in order (rules first, then the description), each once, without
/// `el` itself. Ids that name no installed element are left out, and so are the description's
/// references whose text the description already shows inline.
pub(crate) fn link_ids<'a>(store: &'a ContentStore, el: &Element) -> Vec<(&'a Element, Via)> {
    let mut out: Vec<(&Element, Via)> = Vec::new();
    let mut add = |id: &str, via: Via| {
        let id = id.trim();
        if id.is_empty() || id == el.id {
            return;
        }
        if let Some(target) = store.element(id) {
            if target.id != el.id && !out.iter().any(|(t, _)| t.id == target.id) {
                out.push((target, via));
            }
        }
    };
    for rule in &el.rules {
        match rule {
            Rule::Grant(g) => add(&g.id, Via::Grant),
            Rule::Select(s) => {
                if let Some(id) = &s.default_id {
                    add(id, Via::Select);
                }
                for id in s.supports.iter().flat_map(|e| expression_ids(e)) {
                    add(id, Via::Select);
                }
            }
            Rule::Custom(c) => {
                let via = if c.kind == "grant_feat" {
                    Via::Grant
                } else {
                    Via::Rule
                };
                for v in c.fields.values() {
                    strings(v, &mut |s| {
                        for part in s.split('|') {
                            add(part, via);
                        }
                    });
                }
            }
            Rule::Unknown(u) => {
                for v in u.attributes.values() {
                    add(v, Via::Rule);
                }
            }
            Rule::Stat(_) => {}
        }
    }
    let injected = injected_ids(store, &el.description);
    for id in markup_ids(&el.description) {
        if !injected.iter().any(|i| i == id.trim()) {
            add(id, Via::Description);
        }
    }
    out
}

/// The linked elements of `el` the pane shows: those with text of their own, except a granted
/// trait, feature or feat whose text the Grants list already shows and one-line types.
pub(crate) fn links(store: &ContentStore, el: &Element) -> Vec<ElementLink> {
    link_ids(store, el)
        .into_iter()
        .filter(|(t, via)| !(*via == Via::Grant && has_grant_text(t)))
        .filter(|(t, _)| !ONE_LINE_TYPES.contains(&t.ty.as_str()))
        .filter_map(|(t, via)| {
            let html = store_description_html(store, t);
            (!html.trim().is_empty()).then(|| ElementLink {
                summary: summary(store, t),
                content_type: t.ty.clone(),
                via: via.as_str().to_string(),
                description_html: html,
            })
        })
        .take(MAX_LINKS)
        .collect()
}

/// Every string inside a rule field (nested objects and arrays included).
fn strings(v: &Value, f: &mut impl FnMut(&str)) {
    match v {
        Value::String(s) => f(s),
        Value::Array(a) => a.iter().for_each(|x| strings(x, f)),
        Value::Object(o) => o.values().for_each(|x| strings(x, f)),
        _ => {}
    }
}

/// The id-shaped words of a `supports` expression (`ID_A|ID_B`, `!ID_C`, `(ID_D&ID_E)`). Plain
/// tags (`Magic Item`, `Common`) come out too; they name no element, so the caller drops them.
fn expression_ids(expr: &str) -> impl Iterator<Item = &str> {
    expr.split(['|', '&', ',', '!', '(', ')'])
        .map(str::trim)
        .filter(|s| !s.is_empty() && !s.contains(' '))
}

/// The element ids a description's markup names: `data-element="ID"` (the importers' links) and
/// `element="ID"` (the legacy description markup).
fn markup_ids(html: &str) -> Vec<&str> {
    let mut out = Vec::new();
    for attr in ["data-element=", " element="] {
        let mut rest = html;
        while let Some(i) = rest.find(attr) {
            rest = &rest[i + attr.len()..];
            let Some(quote) = rest.chars().next().filter(|c| *c == '"' || *c == '\'') else {
                continue;
            };
            let body = &rest[1..];
            if let Some(end) = body.find(quote) {
                out.push(&body[..end]);
                rest = &body[end..];
            }
        }
    }
    out
}

/// The ids of the `<div element>` / `<p element>` references in `html` that
/// [`borealis_engine::inject::inject_references`] replaces with the named element's text when
/// the description is shown (see `store_description_html`).
fn injected_ids(store: &ContentStore, html: &str) -> Vec<String> {
    let seen = RefCell::new(Vec::new());
    let source = |id: &str| {
        let target = store.element(id)?;
        seen.borrow_mut().push(id.to_string());
        Some((target.name.clone(), String::new()))
    };
    let _ = borealis_engine::inject::inject_references(html, &source);
    seen.into_inner()
}

#[cfg(test)]
mod tests {
    use super::*;
    use borealis_content::{
        ContentFile, ContentStoreBuilder, CustomRule, GrantRule, PackOrigin, SelectRule,
    };
    use serde_json::json;

    fn el(id: &str, ty: &str, description: &str, rules: Vec<Rule>) -> Element {
        let mut e = Element::new(id, id.trim_start_matches("ID_"), ty, "Core");
        e.description = description.into();
        e.rules = rules;
        e
    }

    fn store(elements: Vec<Element>) -> ContentStore {
        ContentStoreBuilder::new()
            .add_file("t.json", PackOrigin::Custom, ContentFile::new(elements))
            .build()
    }

    fn grant(id: &str) -> Rule {
        Rule::Grant(GrantRule {
            id: id.into(),
            ..GrantRule::default()
        })
    }

    fn ids(store: &ContentStore, id: &str) -> Vec<String> {
        links(store, store.element(id).unwrap())
            .into_iter()
            .map(|l| format!("{}:{}", l.summary.id, l.via))
            .collect()
    }

    #[test]
    fn an_option_that_creates_an_item_links_the_item() {
        let s = store(vec![
            el(
                "ID_OPT",
                "Option",
                "<p>Creates a bag.</p>",
                vec![grant("ID_BAG")],
            ),
            el(
                "ID_BAG",
                "Magic Item",
                "<p>A bag that holds much.</p>",
                vec![],
            ),
        ]);
        let found = links(&s, s.element("ID_OPT").unwrap());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].summary.id, "ID_BAG");
        assert_eq!(found[0].content_type, "Magic Item");
        assert!(found[0].description_html.contains("holds much"));
    }

    #[test]
    fn selects_custom_rules_and_markup_link_named_ids_once_in_order() {
        let select = Rule::Select(SelectRule {
            ty: "Spell".into(),
            name: "Pick".into(),
            default_id: Some("ID_SPELL_A".into()),
            supports: Some("ID_SPELL_B|Common".into()),
            ..SelectRule::default()
        });
        let custom = Rule::Custom(CustomRule {
            kind: "applies_condition".into(),
            fields: json!({ "condition": "ID_COND", "also": ["ID_SPELL_A"] })
                .as_object()
                .cloned()
                .unwrap(),
        });
        let s = store(vec![
            el(
                "ID_SRC",
                "Class Feature",
                r#"<p>See <a class="ref" data-element="ID_BEAST">the beast</a>, <div element='ID_SRC'/>, <span element="ID_NOTE">n</span> and <a data-element="ID_MISSING">x</a>.</p><div element="ID_INLINE"/><p element="ID_RUN_IN"/>"#,
                vec![select, custom],
            ),
            el("ID_SPELL_A", "Spell", "<p>a</p>", vec![]),
            el("ID_SPELL_B", "Spell", "<p>b</p>", vec![]),
            el("ID_COND", "Condition", "<p>c</p>", vec![]),
            el("ID_BEAST", "Companion", "<p>d</p>", vec![]),
            el("ID_NOTE", "Condition", "<p>n</p>", vec![]),
            el("ID_INLINE", "Racial Trait", "<p>i</p>", vec![]),
            el("ID_RUN_IN", "Racial Trait", "<p>r</p>", vec![]),
        ]);
        assert_eq!(
            ids(&s, "ID_SRC"),
            [
                "ID_SPELL_A:select",
                "ID_SPELL_B:select",
                "ID_COND:rule",
                "ID_BEAST:description",
                "ID_NOTE:description"
            ],
            "itself, unknown ids and the injected div/p references are left out"
        );
    }

    #[test]
    fn granted_features_one_line_types_and_textless_elements_are_not_linked() {
        let s = store(vec![
            el(
                "ID_CLASS",
                "Class",
                "",
                vec![
                    grant("ID_FEATURE"),
                    grant("ID_PROF"),
                    grant("ID_LANG"),
                    grant("ID_SPELL"),
                ],
            ),
            el("ID_FEATURE", "Class Feature", "<p>f</p>", vec![]),
            el("ID_PROF", "Proficiency", "<p>Proficiency text.</p>", vec![]),
            el("ID_LANG", "Language", "", vec![]),
            el("ID_SPELL", "Spell", "<p>s</p>", vec![]),
        ]);
        assert_eq!(ids(&s, "ID_CLASS"), ["ID_SPELL:grant"]);
    }

    #[test]
    fn a_long_list_stops_at_the_cap() {
        let mut all: Vec<Element> = (0..MAX_LINKS + 5)
            .map(|i| el(&format!("ID_S{i}"), "Spell", "<p>s</p>", vec![]))
            .collect();
        let rules = all.iter().map(|e| grant(&e.id)).collect();
        all.push(el("ID_LIST", "Archetype Feature", "", rules));
        let s = store(all);
        assert_eq!(ids(&s, "ID_LIST").len(), MAX_LINKS);
    }

    #[test]
    fn markup_ids_read_both_quote_styles() {
        assert_eq!(
            markup_ids(r#"<a data-element="ID_A">a</a><div element='ID_B'></div><b element=x>"#),
            ["ID_A", "ID_B"]
        );
    }

    #[test]
    fn injected_ids_are_the_div_and_p_references_of_installed_elements() {
        let s = store(vec![
            el("ID_A", "Racial Trait", "<p>a</p>", vec![]),
            el("ID_B", "Racial Trait", "<p>b</p>", vec![]),
            el("ID_C", "Racial Trait", "<p>c</p>", vec![]),
        ]);
        assert_eq!(
            injected_ids(
                &s,
                r#"<div element="ID_A"/><p element='ID_B'>x</p><span element="ID_C">c</span><a data-element="ID_C">c</a><div element="ID_MISSING"/>"#
            ),
            ["ID_A", "ID_B"],
            "a span, a data-element link and an unknown id are not injected"
        );
    }
}
