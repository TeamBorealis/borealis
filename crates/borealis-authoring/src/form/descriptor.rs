//! The kinds as the UI receives them from `homebrew_kinds` (`HomebrewKindDescriptor`, `HomebrewFieldSpec`).

use super::kinds::{kinds, FieldDef, KindDef};
use borealis_content::Ruleset;
use serde::Serialize;
use serde_json::Value;

/// One choice of a combo field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChoiceSpec {
    /// Stored value.
    pub value: String,
    /// Shown text.
    pub label: String,
}

/// One sub-tab.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TabSpec {
    /// Id.
    pub id: String,
    /// Shown uppercase.
    pub label: String,
}

/// One field (`HomebrewFieldSpec`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldSpec {
    /// Key in the form's fields.
    pub key: String,
    /// Sentence-case label.
    pub label: String,
    /// Sub-tab.
    pub tab: String,
    /// `HomebrewControl`.
    pub control: String,
    /// The validator needs it.
    pub required: bool,
    /// Combo choices.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub choices: Option<Vec<ChoiceSpec>>,
    /// The combo takes custom text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_custom: Option<bool>,
    /// The model locations it writes.
    pub bindings: Vec<String>,
    /// Help under the field.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub help: Option<String>,
    /// Only an expert sees it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expert: Option<bool>,
    /// Shown, not edited.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_only: Option<bool>,
    /// The SectionSecondary heading it is grouped under.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub section: Option<String>,
    /// The value a new element starts with.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,
}

/// One kind (`HomebrewKindDescriptor`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KindDescriptor {
    /// Kind id.
    pub id: String,
    /// "Language".
    pub label: String,
    /// "Languages".
    pub plural_label: String,
    /// New element card line.
    pub description: String,
    /// `main`, `small`, `child`, `expert`.
    pub group: String,
    /// Element types it writes.
    pub element_types: Vec<String>,
    /// Kinds it is edited inside.
    pub parent_kinds: Vec<String>,
    /// Editions.
    pub editions: Vec<Ruleset>,
    /// Sub-tabs.
    pub tabs: Vec<TabSpec>,
    /// Fields.
    pub fields: Vec<FieldSpec>,
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| (*s).to_string()).collect()
}

fn field_spec(f: &FieldDef) -> FieldSpec {
    FieldSpec {
        key: f.key.into(),
        label: f.label.into(),
        tab: f.tab.into(),
        control: f.control.as_str().into(),
        required: f.required,
        choices: (!f.choices.is_empty()).then(|| {
            f.choices
                .iter()
                .map(|(value, label)| ChoiceSpec {
                    value: (*value).into(),
                    label: (*label).into(),
                })
                .collect()
        }),
        allow_custom: f.allow_custom.then_some(true),
        bindings: f.codec.bindings(),
        help: f.help.map(str::to_string),
        expert: f.expert.then_some(true),
        read_only: f.read_only.then_some(true),
        section: f.section.map(str::to_string),
        default: f.default_value(),
    }
}

impl KindDef {
    /// The descriptor the UI renders the form from.
    #[must_use]
    pub fn descriptor(&self) -> KindDescriptor {
        KindDescriptor {
            id: self.id.into(),
            label: self.label.into(),
            plural_label: self.plural.into(),
            description: self.description.into(),
            group: self.group.as_str().into(),
            element_types: strings(self.element_types),
            parent_kinds: strings(self.parent_kinds),
            editions: self.editions.to_vec(),
            tabs: self
                .tabs
                .iter()
                .map(|(id, label)| TabSpec {
                    id: (*id).into(),
                    label: (*label).into(),
                })
                .collect(),
            fields: self.fields.iter().map(field_spec).collect(),
        }
    }
}

/// Every kind's descriptor (`homebrew_kinds`).
#[must_use]
pub fn descriptors() -> Vec<KindDescriptor> {
    kinds().iter().map(KindDef::descriptor).collect()
}
