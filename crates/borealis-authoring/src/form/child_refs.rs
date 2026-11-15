//! Children (spec 3.1): elements edited inside their parent's form (a class's features and starting-equipment
//! package, a feat's sub-features), and the parent field that gives them.
//!
//! A **children field** ([`Codec::Children`]) holds a list of [`ChildEntry`]: a child form of this draft (by its form
//! key) or an element of elsewhere (by reference: a copied class still grants the original's features), and for a
//! class the level it is gained at. It takes the parent's `grant` rules of the child type, so the grid and the rules
//! can't disagree. Reading a parent with its children ([`link`]) points the entries at the child forms.
//!
//! Writing a child knows its parent ([`ParentInfo`]): a class feature's owner (`data.class`, which the engine reads
//! for class-table counters) and level (`data.level`) and a package's owner (`data.owner`) follow the class. A child
//! no field of its parent lists would be saved but never given: that is a problem at the child ([`orphans`]).
//!
//! An **offered** children field ([`Codec::Offered`]) gives its children through one `select` that offers them by id
//! (`ID_A|ID_B`): a racial trait's lineage options (spec 4.4), the player picks one. A background's variants are the
//! same shape ([`offer_rule`]).
//!
//! Children nest: a child form has children of its own (a background variant's feature, a trait's lineage options),
//! read with [`super::element_tree_to_form`] and made with [`add_child_in`].
//!
//! Deterministic, no AI (G7).

use super::codec::{decode, to_json, Codec, Reader};
use super::kinds::kind;
use super::value::RefValue;
use super::write::Out;
use super::{FieldMemo, FormContext, FormProblem, FormState, Slots};
use borealis_content::{Element, GrantRule, Rule, Ruleset, SelectRule};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// Order of new grant rules of a children field among the other new field rules (after a class's proficiencies).
pub(crate) const RANK_CHILDREN: u8 = 20;

/// One entry of a children field (`HomebrewChildEntry`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChildEntry {
    /// The class level it is gained at (levelled fields only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<u32>,
    /// The key of a child form of this draft.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub child: Option<String>,
    /// An element that is not edited here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub element: Option<RefValue>,
}

/// What a child's writing knows about its parent.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParentInfo {
    /// The parent's id (its provisional id before the first save).
    pub id: Option<String>,
    /// The parent's name.
    pub name: String,
    /// The parent's own edition (`None` = the pack's).
    pub edition: Option<Ruleset>,
    /// The level the parent gives this child at.
    pub level: Option<u32>,
    /// A class's multiclass id as stored and as written now (a copy's changes).
    pub renamed: Option<(String, String)>,
    /// A subclass's class name (its features' `data.class`, spec 4.7).
    pub class: Option<String>,
}

/// A child form as its parent's fields see it.
#[derive(Debug, Clone)]
pub(crate) struct ChildInfo {
    pub(crate) key: String,
    pub(crate) id: Option<String>,
}

fn grant_of(rule: &Rule, ty: &str, levelled: bool) -> Option<(String, Option<u32>)> {
    let Rule::Grant(g) = rule else { return None };
    let plain = g.ty.as_deref() == Some(ty)
        && g.requirements.is_none()
        && g.spellcasting.is_none()
        && g.prepared.is_none()
        && g.setters.is_empty()
        && g.extra.is_empty()
        && (levelled || g.level.is_none());
    plain.then(|| (g.id.clone(), g.level))
}

/// Read a children field: every grant of the child type that is only a grant (at a level, for a levelled field).
pub(crate) fn read(
    ty: &str,
    levelled: bool,
    r: &mut Reader<'_>,
    cx: &FormContext<'_>,
) -> FieldMemo {
    let e = r.e;
    let mut entries = Vec::new();
    let mut raw = Vec::new();
    let mut slots = Vec::new();
    for (i, rule) in e.rules.iter().enumerate() {
        if r.rules_taken[i] {
            continue;
        }
        if let Some((id, level)) = grant_of(rule, ty, levelled) {
            r.rules_taken[i] = true;
            entries.push(ChildEntry {
                level: levelled.then(|| level.unwrap_or(1)),
                child: None,
                element: Some(RefValue::resolve(&id, cx.lookup)),
            });
            raw.push(to_json(rule));
            slots.push(i);
        }
    }
    FieldMemo {
        value: to_json(&entries),
        raw: Value::Array(raw),
        slots: Slots {
            rules: slots,
            ..Slots::default()
        },
    }
}

/// The element id an entry gives: its child's (by key) or its reference's.
pub(crate) fn entry_id(
    entry: &ChildEntry,
    out: &Out,
    path: &str,
    what: &str,
) -> Result<String, FormProblem> {
    if let Some(key) = &entry.child {
        return match out.children.iter().find(|c| &c.key == key) {
            Some(c) => {
                c.id.clone()
                    .ok_or_else(|| FormProblem::new("id", "This element has no id yet."))
            }
            None => Err(FormProblem::new(
                path,
                format!("This {what} isn't in this element any more. Remove it from the list."),
            )),
        };
    }
    match entry.element.as_ref().filter(|e| !e.id.trim().is_empty()) {
        Some(e) => Ok(e.id.trim().to_string()),
        None => Err(FormProblem::new(path, format!("Choose the {what}."))),
    }
}

/// The `select` of type `ty` that offers the elements `ids` by id (`ID_A|ID_B`); `optional` for a choice the player may
/// leave empty (a background's variants).
pub(crate) fn offer_rule(ty: &str, name: &str, ids: &[String], optional: bool) -> Rule {
    Rule::Select(SelectRule {
        ty: ty.to_string(),
        name: name.to_string(),
        supports: Some(ids.join("|")),
        optional: optional.then_some(true),
        ..SelectRule::default()
    })
}

/// The ids a `select` of type `ty` offers, when it is exactly [`offer_rule`]'s shape.
pub(crate) fn offered_ids(s: &SelectRule, ty: &str, optional: bool) -> Option<Vec<String>> {
    if s.ty != ty {
        return None;
    }
    let ids: Vec<String> = s
        .supports
        .as_deref()?
        .split('|')
        .map(|t| t.trim().to_string())
        .collect();
    let all_ids = ids.iter().all(|t| t.starts_with("ID_"));
    (all_ids && Rule::Select(s.clone()) == offer_rule(ty, &s.name, &ids, optional)).then_some(ids)
}

/// Read an offered children field: the first free `select` of `ty` in [`offer_rule`]'s shape.
pub(crate) fn read_offered(
    ty: &str,
    optional: bool,
    r: &mut Reader<'_>,
    cx: &FormContext<'_>,
) -> FieldMemo {
    let e = r.e;
    let found = e.rules.iter().enumerate().find_map(|(i, rule)| match rule {
        Rule::Select(s) if !r.rules_taken[i] => offered_ids(s, ty, optional).map(|ids| (i, ids)),
        _ => None,
    });
    let Some((i, ids)) = found else {
        return FieldMemo {
            value: json!([]),
            raw: json!([]),
            slots: Slots::default(),
        };
    };
    r.rules_taken[i] = true;
    let entries: Vec<ChildEntry> = ids
        .iter()
        .map(|id| ChildEntry {
            level: None,
            child: None,
            element: Some(RefValue::resolve(id, cx.lookup)),
        })
        .collect();
    FieldMemo {
        value: to_json(&entries),
        raw: json!([to_json(&e.rules[i])]),
        slots: Slots {
            rules: vec![i],
            ..Slots::default()
        },
    }
}

/// Write an offered children field: one `select` offering every entry by id (none for no entry). Its name is the
/// stored one (the choice's persistence key), else the element's own name (a trait "Elven Lineage" offers its options
/// by a choice "Elven Lineage", as the SRD).
pub(crate) fn write_offered(
    ty: &str,
    optional: bool,
    value: &Value,
    memo: Option<&FieldMemo>,
    path: &str,
    out: &mut Out,
) -> Result<(), FormProblem> {
    let slots = memo.map(|m| m.slots.rules.clone()).unwrap_or_default();
    let stored: Vec<Rule> = memo
        .and_then(|m| m.raw.as_array())
        .into_iter()
        .flatten()
        .filter_map(|r| serde_json::from_value(r.clone()).ok())
        .collect();
    if memo.is_some_and(|m| m.value == *value) {
        out.field_rules(stored.into_iter().map(|r| (RANK_CHILDREN, r)), &slots, path);
        return Ok(());
    }
    let entries: Vec<ChildEntry> = decode(value, path)?;
    let ids = entries
        .iter()
        .enumerate()
        .map(|(i, e)| entry_id(e, out, &format!("{path}.{i}"), "option"))
        .collect::<Result<Vec<_>, _>>()?;
    if ids.is_empty() {
        return Ok(());
    }
    let name = match stored.first() {
        Some(Rule::Select(s)) => s.name.clone(),
        _ => {
            let own = borealis_content::rich::normalize_plain_text(&out.element.name);
            if own.trim().is_empty() {
                ty.to_string()
            } else {
                own.trim().to_string()
            }
        }
    };
    out.field_rules(
        [(RANK_CHILDREN, offer_rule(ty, &name, &ids, optional))],
        &slots,
        path,
    );
    Ok(())
}

/// Write a children field: a grant per entry, a stored rule for an entry that has one (same element, same level).
pub(crate) fn write(
    ty: &str,
    levelled: bool,
    value: &Value,
    memo: Option<&FieldMemo>,
    path: &str,
    out: &mut Out,
) -> Result<(), FormProblem> {
    let entries: Vec<ChildEntry> = decode(value, path)?;
    let mut stored: Vec<Option<Rule>> = memo
        .and_then(|m| m.raw.as_array())
        .into_iter()
        .flatten()
        .map(|r| serde_json::from_value(r.clone()).ok())
        .collect();
    let mut rules = Vec::new();
    for (i, entry) in entries.iter().enumerate() {
        let at = format!("{path}.{i}");
        let id = entry_id(entry, out, &at, "feature")?;
        let level = if levelled {
            match entry.level {
                Some(l @ 1..=20) => Some(l),
                _ => return Err(FormProblem::new(at, "Give it a level from 1 to 20.")),
            }
        } else {
            None
        };
        let reuse = stored.iter_mut().find(|s| {
            s.as_ref()
                .and_then(|r| grant_of(r, ty, levelled))
                .is_some_and(|(sid, sl)| sid == id && sl.unwrap_or(1) == level.unwrap_or(1))
        });
        let rule = match reuse.and_then(Option::take) {
            Some(r) => r,
            None => Rule::Grant(GrantRule {
                id,
                ty: Some(ty.to_string()),
                // the 2024 SRD leaves level 1 out; the 2014 SRD writes every level
                level: level.filter(|l| *l > 1 || out.element.ruleset != Some(Ruleset::Dnd2024)),
                ..GrantRule::default()
            }),
        };
        rules.push((RANK_CHILDREN, rule));
    }
    let slots = memo.map(|m| m.slots.rules.clone()).unwrap_or_default();
    out.field_rules(rules, &slots, path);
    Ok(())
}

/// The entries of a children field value (none for a value of another shape).
pub(crate) fn entries(value: Option<&Value>) -> Vec<ChildEntry> {
    value
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default()
}

/// The parent's children fields: (key, levelled).
fn child_fields(form: &FormState) -> Vec<(&'static str, bool)> {
    kind(&form.kind)
        .map(|k| {
            k.fields
                .iter()
                .filter_map(|f| f.codec.child_list().map(|levelled| (f.key, levelled)))
                .collect()
        })
        .unwrap_or_default()
}

fn field_value<'f>(form: &'f FormState, key: &str) -> Option<&'f Value> {
    form.fields
        .get(key)
        .or_else(|| form.memo.fields.get(key).map(|m| &m.value))
}

/// Point the entries of the parent's children fields at the child forms that hold their elements (value and memo
/// alike, so the field is still unchanged).
pub(crate) fn link(form: &mut FormState) {
    let ids: Vec<(String, String)> = form
        .children
        .iter()
        .filter_map(|c| c.id().map(|id| (id.to_string(), c.key.clone())))
        .collect();
    let relink = |v: &Value| -> Value {
        let list: Vec<ChildEntry> = entries(Some(v))
            .into_iter()
            .map(|mut e| {
                let key = e
                    .element
                    .as_ref()
                    .and_then(|r| ids.iter().find(|(id, _)| *id == r.id))
                    .map(|(_, k)| k.clone());
                if key.is_some() {
                    e.child = key;
                    e.element = None;
                }
                e
            })
            .collect();
        json!(list)
    };
    for (key, _) in child_fields(form) {
        if let Some(v) = form.fields.get(key).map(relink) {
            form.fields.insert(key.to_string(), v);
        }
        if let Some(m) = form.memo.fields.get_mut(key) {
            m.value = relink(&m.value);
        }
    }
}

/// What a child of `parent` knows when it is written.
pub(crate) fn parent_info(parent: &FormState, child: &FormState) -> ParentInfo {
    let text = |key: &str| {
        field_value(parent, key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let level = child_fields(parent)
        .into_iter()
        .find_map(|(key, levelled)| {
            entries(field_value(parent, key))
                .into_iter()
                .find(|e| e.child.as_deref() == Some(child.key.as_str()))
                .map(|e| if levelled { e.level } else { None })
        });
    let edition = match text("edition").as_str() {
        "dnd2014" => Some(Ruleset::Dnd2014),
        "dnd2024" => Some(Ruleset::Dnd2024),
        _ => None,
    };
    let class = field_value(parent, super::subclass::PARENT)
        .and_then(|v| v.get("name"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .map(str::to_string);
    ParentInfo {
        id: parent.id().map(str::to_string),
        name: borealis_content::rich::normalize_plain_text(&text("name")),
        edition,
        level: level.flatten(),
        renamed: super::codec::class_renamed(parent),
        class,
    }
}

/// The children no children field of `parent` lists and no rule of it gives (`given`: the ids its written rules give;
/// they would be saved but never given), as problems at them.
pub(crate) fn orphans(parent: &FormState, given: &[String]) -> Vec<FormProblem> {
    let fields = child_fields(parent);
    if fields.is_empty() {
        return Vec::new();
    }
    let listed: Vec<String> = fields
        .iter()
        .flat_map(|(key, _)| entries(field_value(parent, key)))
        .filter_map(|e| e.child)
        .collect();
    let levelled = fields.iter().any(|(_, l)| *l);
    parent
        .children
        .iter()
        .enumerate()
        .filter(|(_, c)| {
            !listed.contains(&c.key) && !c.id().is_some_and(|id| given.iter().any(|g| g == id))
        })
        .map(|(i, c)| {
            let name = field_value(c, "name").and_then(Value::as_str).unwrap_or_default().trim().to_string();
            let name = if name.is_empty() { "This part".to_string() } else { format!("“{name}”") };
            let message = if levelled {
                format!("{name} isn't on any level. Put it on a level in the LEVELS grid, or delete it.")
            } else {
                format!("{name} isn't on the list of what this element gives. Add it there, or delete it.")
            };
            FormProblem::new(format!("children.{i}"), message)
        })
        .collect()
}

/// A child's element after writing: the parent's owner, level and edition where the child keeps them.
pub(crate) fn patch_child(e: &mut Element, child: &FormState, parent: &ParentInfo) {
    let fresh = child.memo.fields.is_empty();
    let mut set = |key: &str, v: Value| {
        if fresh || e.data.contains_key(key) {
            e.data.insert(key.to_string(), v);
        }
    };
    match child.kind.as_str() {
        "classFeature" => {
            if !parent.name.is_empty() {
                set("class", json!(parent.name));
            }
            if let Some(l) = parent.level {
                set("level", json!(l));
            }
        }
        // a subclass's feature: its class's name, the subclass's, the level (the 2024 SRD's `data`)
        super::subclass::SUBCLASS_FEATURE_KIND => {
            if let Some(class) = &parent.class {
                set("class", json!(class));
            }
            if !parent.name.is_empty() {
                set("subclass", json!(parent.name));
            }
            if let Some(l) = parent.level {
                set("level", json!(l));
            }
        }
        "equipmentPackage" => {
            if let Some(id) = &parent.id {
                set("owner", json!(id));
            }
        }
        _ => {}
    }
    if fresh && e.ruleset.is_none() {
        e.ruleset = parent.edition;
    }
    if let Some((old, new)) = &parent.renamed {
        super::codec::rename_requirement_ids(e, old, new);
    }
}

impl Codec {
    /// For a children field: whether its entries have levels.
    #[must_use]
    pub fn child_list(self) -> Option<bool> {
        match self {
            Self::Children { levelled, .. } => Some(levelled),
            Self::Offered { .. } | Self::Species(super::codec::SpeciesField::Traits) => Some(false),
            Self::Class(f) => f.child_list(),
            Self::Background(f) => f.child_type().map(|_| false),
            _ => None,
        }
    }

    /// For a children field: the element type of its children.
    #[must_use]
    pub fn child_type(self) -> Option<&'static str> {
        match self {
            Self::Children { ty, .. } | Self::Offered { ty, .. } => Some(ty),
            Self::Species(super::codec::SpeciesField::Traits) => Some("Racial Trait"),
            Self::Class(super::codec::ClassField::Equipment) => Some("Equipment Package"),
            Self::Background(f) => f.child_type(),
            _ => None,
        }
    }
}

/// A key no child of `parent` has (`n1`, `n2`...).
#[must_use]
pub fn new_child_key(parent: &FormState) -> String {
    (1..)
        .map(|n| format!("n{n}"))
        .find(|k| !parent.flatten().iter().any(|f| &f.key == k))
        .unwrap_or_default()
}

/// `parent` with `child` added (spec 3.1, `homebrew_new_draft {parentId}`): the child form last, and an entry for it
/// in the parent's children field of its type (a class feature at `level`, 1 when not given; a package replaces the
/// one there). `None` when the parent's form has no place for a child of that kind.
#[must_use]
pub fn add_child(parent: &FormState, child: FormState, level: Option<u32>) -> Option<FormState> {
    let ty = kind(&child.kind)?.new_element_type()?;
    let pk = kind(&parent.kind)?;
    if !kind(&child.kind)?.parent_kinds.contains(&pk.id) {
        return None;
    }
    let def = pk
        .fields
        .iter()
        .find(|f| f.codec.child_type() == Some(ty))?;
    let levelled = def.codec.child_list() == Some(true);
    let entry = ChildEntry {
        level: levelled.then(|| level.unwrap_or(1).clamp(1, 20)),
        child: Some(child.key.clone()),
        element: None,
    };
    let mut list = entries(field_value(parent, def.key));
    // one starting-equipment package: a new one replaces the one there
    if def.control == super::kinds::Control::Package {
        list.clear();
    }
    list.push(entry);
    let mut next = parent.clone();
    next.fields.insert(def.key.to_string(), json!(list));
    next.children.push(child);
    Some(next)
}

/// The form with key `key` among `root` and its descendants.
#[must_use]
pub fn child_form<'f>(root: &'f FormState, key: &str) -> Option<&'f FormState> {
    if root.key == key {
        return Some(root);
    }
    root.children.iter().find_map(|c| child_form(c, key))
}

/// `root` with `child` added to its descendant form `parent_key` ([`add_child`] there: a variant's own feature, a
/// trait's lineage option). `None` when no form has that key or it has no place for the child.
#[must_use]
pub fn add_child_in(
    root: &FormState,
    parent_key: &str,
    child: FormState,
    level: Option<u32>,
) -> Option<FormState> {
    if root.key == parent_key {
        return add_child(root, child, level);
    }
    let i = root
        .children
        .iter()
        .position(|c| child_form(c, parent_key).is_some())?;
    let inner = add_child_in(&root.children[i], parent_key, child, level)?;
    let mut next = root.clone();
    next.children[i] = inner;
    Some(next)
}
