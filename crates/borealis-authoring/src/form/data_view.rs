//! The DATA sub-tab (spec 12.1, 13.14): the elements a form writes as an indented `key: value` tree, read-only.
//! Strings are their characters (never escaped), a description is HTML the UI renders (its "Show HTML" toggle shows
//! the fragment as text), expressions are their text, and each rule carries the sentence of the block (or the name
//! of the field) that writes it. Deterministic, no AI (G7).

use super::kinds::kind;
use super::write::rule_origins;
use super::{FormContext, FormState};
use borealis_content::Element;
use serde::Serialize;
use serde_json::{Map, Value};

/// What a tree line holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DataKind {
    /// Keys and values below it.
    Object,
    /// Items below it (keys are their positions, from 1).
    List,
    /// A string, shown as its characters.
    Text,
    /// An HTML fragment (a description), shown rendered.
    Html,
    /// An expression the engine reads (requirements, a supports filter, an equipped test).
    Expr,
    /// A number.
    Number,
    /// `true` or `false`.
    Flag,
    /// No value.
    Empty,
}

/// One line of the tree (`HomebrewDataNode` on the wire).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DataNode {
    /// The key (an object key, a list position from 1, or a setter's name).
    pub key: String,
    /// What the line holds.
    pub kind: DataKind,
    /// The value as text (leaves only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// What it means in plain words: a rule's block sentence, a prerequisite's sentence.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sentence: Option<String>,
    /// The lines below it.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<DataNode>,
}

impl DataNode {
    fn leaf(key: String, kind: DataKind, text: impl Into<String>) -> Self {
        Self {
            key,
            kind,
            text: Some(text.into()),
            sentence: None,
            children: Vec::new(),
        }
    }
}

/// The order of an element's keys as the content file writes them (`Element`'s field order).
const ELEMENT_KEYS: &[&str] = &[
    "id",
    "name",
    "type",
    "source",
    "ruleset",
    "supports",
    "prerequisite",
    "requirements",
    "description",
    "sheet",
    "compendium",
    "setters",
    "rules",
    "spellcasting",
    "equipment",
    "source_ref",
    "multiclass",
    "extract",
    "maps_to",
    "renamed_from",
    "legacy_alias",
    "profile_overrides",
    "data",
    "extra",
];

const EXPR_KEYS: &[&str] = &["requirements", "supports", "equipped"];

fn node(key: String, v: &Value) -> DataNode {
    match v {
        Value::String(s) if key == "description" => DataNode::leaf(key, DataKind::Html, s.clone()),
        Value::String(s) if EXPR_KEYS.contains(&key.as_str()) => {
            DataNode::leaf(key, DataKind::Expr, s.clone())
        }
        Value::String(s) => DataNode::leaf(key, DataKind::Text, s.clone()),
        Value::Number(n) => DataNode::leaf(key, DataKind::Number, n.to_string()),
        Value::Bool(b) => DataNode::leaf(key, DataKind::Flag, b.to_string()),
        Value::Null => DataNode::leaf(key, DataKind::Empty, ""),
        Value::Array(items) => DataNode {
            key,
            kind: DataKind::List,
            text: None,
            sentence: None,
            children: items
                .iter()
                .enumerate()
                .map(|(i, x)| node((i + 1).to_string(), x))
                .collect(),
        },
        Value::Object(o) => DataNode {
            key,
            kind: DataKind::Object,
            text: None,
            sentence: None,
            children: object(o),
        },
    }
}

fn object(o: &Map<String, Value>) -> Vec<DataNode> {
    o.iter().map(|(k, v)| node(k.clone(), v)).collect()
}

/// A setter as one line: its name as the key, its value as the text (its attributes below it).
fn setter(v: &Value) -> DataNode {
    let name = v["name"].as_str().unwrap_or_default().to_string();
    let mut line = DataNode::leaf(
        name,
        DataKind::Text,
        v["value"].as_str().unwrap_or_default(),
    );
    if let Value::Object(o) = v {
        line.children = o
            .iter()
            .filter(|(k, _)| !matches!(k.as_str(), "name" | "value"))
            .map(|(k, x)| node(k.clone(), x))
            .collect();
    }
    line
}

/// What wrote rule `origin` (`blocks.<i>` or `fields.<key>`), in plain words.
fn origin_sentence(form: &FormState, origin: &str) -> Option<String> {
    if let Some(i) = origin
        .strip_prefix("blocks.")
        .and_then(|i| i.parse::<usize>().ok())
    {
        return form
            .blocks
            .get(i)
            .map(|b| b.sentence.trim().to_string())
            .filter(|s| !s.is_empty());
    }
    let key = origin.strip_prefix("fields.")?;
    let label = kind(&form.kind)?.field(key)?.label;
    Some(format!("Written by the {label} field."))
}

/// The tree of one element written by `form`.
fn element_tree(form: Option<&FormState>, el: &Element, cx: &FormContext<'_>) -> DataNode {
    let Ok(Value::Object(obj)) = serde_json::to_value(el) else {
        return DataNode::leaf(el.name.clone(), DataKind::Text, "");
    };
    let origins = form.and_then(|f| rule_origins(f, cx)).unwrap_or_default();
    let mut lines = Vec::new();
    for key in ELEMENT_KEYS
        .iter()
        .copied()
        .filter(|k| obj.contains_key(*k))
    {
        let v = &obj[key];
        let mut line = match key {
            "setters" => DataNode {
                key: key.into(),
                kind: DataKind::List,
                text: None,
                sentence: None,
                children: v
                    .as_array()
                    .map(|a| a.iter().map(setter).collect())
                    .unwrap_or_default(),
            },
            // an element's own tags are a list of tags, not an expression
            "supports" => DataNode {
                key: key.into(),
                kind: DataKind::List,
                text: None,
                sentence: None,
                children: v
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .enumerate()
                            .map(|(i, t)| {
                                DataNode::leaf(
                                    (i + 1).to_string(),
                                    DataKind::Text,
                                    t.as_str().unwrap_or_default(),
                                )
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
            },
            _ => node(key.to_string(), v),
        };
        if key == "requirements" && !el.prerequisite.trim().is_empty() {
            line.sentence = Some(el.prerequisite.trim().to_string());
        }
        if key == "rules" {
            for (rule, origin) in line.children.iter_mut().zip(&origins) {
                if let Some(f) = form {
                    rule.sentence = origin_sentence(f, origin);
                }
            }
        }
        lines.push(line);
    }
    // keys a later content format adds, after the known ones
    lines.extend(
        obj.iter()
            .filter(|(k, _)| !ELEMENT_KEYS.contains(&k.as_str()))
            .map(|(k, v)| node(k.clone(), v)),
    );
    DataNode {
        key: el.name.clone(),
        kind: DataKind::Object,
        text: None,
        sentence: None,
        children: lines,
    }
}

/// The DATA tree of the elements `form` writes (`form_to_elements` order: the element, then its children depth
/// first), one top line per element named by the element's name.
#[must_use]
pub fn data_tree(form: &FormState, elements: &[Element], cx: &FormContext<'_>) -> Vec<DataNode> {
    let forms = form.flatten();
    elements
        .iter()
        .enumerate()
        .map(|(i, el)| element_tree(forms.get(i).copied(), el, cx))
        .collect()
}

/// The elements as the content file writes them (`ContentFile::to_json_pretty`'s writer): the "Copy as JSON" text.
#[must_use]
pub fn data_json(elements: &[Element]) -> String {
    let mut text = match elements {
        [one] => serde_json::to_string_pretty(one),
        many => serde_json::to_string_pretty(many),
    }
    .unwrap_or_default();
    text.push('\n');
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expr_build::condition::ConditionContext;
    use crate::form::{element_to_form, CatalogBlockCodec, NoLookup};
    use serde_json::json;

    #[test]
    fn the_tree_follows_the_file_with_sentences_beside_the_rules() {
        let codec = CatalogBlockCodec;
        let cx = FormContext {
            conditions: ConditionContext::new(["Wizard"]),
            lookup: &NoLookup,
            blocks: &codec,
        };
        let el: Element = serde_json::from_value(json!({"id": "ID_HB_FEAT_GRIT", "name": "Grit \"Iron\"", "type": "Feat",
            "source": "HB", "supports": ["General"], "requirements": "[str:13]", "prerequisite": "Strength 13",
            "description": "<p>Tough & <b>gritty</b>.</p>",
            "setters": [{"name": "short", "value": "Tough."}],
            "rules": [{"kind": "stat", "name": "hp", "value": "2"}]}))
        .expect("element");
        let form = element_to_form(&el, &cx);
        let tree = data_tree(&form, std::slice::from_ref(&el), &cx);
        assert_eq!(tree.len(), 1);
        let keys: Vec<&str> = tree[0].children.iter().map(|n| n.key.as_str()).collect();
        assert_eq!(
            keys,
            [
                "id",
                "name",
                "type",
                "source",
                "supports",
                "prerequisite",
                "requirements",
                "description",
                "setters",
                "rules"
            ]
        );
        let line = |k: &str| tree[0].children.iter().find(|n| n.key == k).expect(k);
        assert_eq!(
            line("name").text.as_deref(),
            Some("Grit \"Iron\""),
            "characters, never escaped"
        );
        assert_eq!(line("description").kind, DataKind::Html);
        assert_eq!(line("requirements").kind, DataKind::Expr);
        assert_eq!(
            line("requirements").sentence.as_deref(),
            Some("Strength 13")
        );
        assert_eq!(line("setters").children[0].key, "short");
        assert_eq!(line("setters").children[0].text.as_deref(), Some("Tough."));
        let rule = &line("rules").children[0];
        assert_eq!(
            rule.sentence.as_deref(),
            Some(form.blocks[0].sentence.as_str())
        );
        assert!(!form.blocks[0].sentence.is_empty());
        let json = data_json(std::slice::from_ref(&el));
        let back: Element = serde_json::from_str(&json).expect("json reads back");
        assert_eq!(back, el);
        assert!(json.ends_with("}\n"));
    }
}
