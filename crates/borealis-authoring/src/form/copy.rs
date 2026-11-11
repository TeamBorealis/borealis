//! Copies (spec 11, 12.3): a new element made from a loaded one ("New from copy...", "Copy to homebrew", Duplicate).
//!
//! The copy keeps the original's fields, rules, tags, description and edition, has no id (it gets its own on its first
//! save, spec 5.3), is named "<Name> (copy)", and drops what describes only the original: the 2024 mirror keys no
//! field writes (spec 3.3; the `data` keys the engine reads or a parent rewrites stay), the 2014/2024 counterpart, what
//! it replaces, its earlier type name and its source reference.
//!
//! A **deep copy** copies what the element is made of too, with the ids cleared: a class's features, its starting
//! equipment and (2024 shape) its subclasses with their features; a species' or race's traits and their lineage options;
//! a background's feature, variants and package; a subclass's or feat's own features. Subclasses are separate elements:
//! they travel as the copy's **companions** and are saved with it ([`write_copy`]). After the ids are given, every
//! place a copied element names an original names its copy ([`CopyExtras::links`]), and a lineage choice offers the
//! copied options by id instead of the original's tag ([`CopyTag`]), so the original never offers the copies.
//!
//! Without deep, the copy grants the originals' features. Deterministic, no AI (G7).

use super::read::{element_group_keyed, read_one};
use super::subclass::{class_choice, refresh_subclass, PARENT, SUBCLASS_KIND};
use super::ui::assign_ids;
use super::value::RefValue;
use super::write::form_to_elements;
use super::{FormContext, FormProblem, FormState, Lookup, WithElements};
use crate::check::{Problem, ProblemSeverity};
use crate::ids::IdGenerator;
use crate::pack::{child_types, granted_children, granted_ids, CopiedFrom};
use borealis_content::{Element, Rule, Ruleset};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// The `data` keys a copy keeps although no field writes them: the engine reads them (spec 3.3) or the parent
/// rewrites them when the copy is written (a feature's class and level, a package's owner).
const KEPT_DATA: &[&str] = &[
    "class",
    "level",
    "subclass",
    "owner",
    "prerequisite",
    "repeatable",
    "mastery",
    "mastery_id",
    "base_weapon",
    "weapon_category",
    "melee",
    "save",
    "dc",
    "options",
];

/// Kept parts that describe only the original.
const DROPPED_PARTS: &[&str] = &["renamed_from", "legacy_alias", "source_ref"];

/// The lineage choice's element type (spec 4.4).
const LINEAGE_OPTION: &str = "Species Trait Option";

/// One copied element: the form key of its copy and the original's id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CopyLink {
    /// The copy's form key (in the draft's form or a companion).
    pub key: String,
    /// The original's id.
    pub original: String,
    /// Where the original's text came from (`creator.json`'s `copiedFrom`); the app fills it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<CopiedFrom>,
}

/// A choice the copy makes by tag whose options were copied: the copied choice offers the copies by id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CopyTag {
    /// The form key of the copied element with the choice.
    pub key: String,
    /// The choice's element type.
    #[serde(rename = "type")]
    pub ty: String,
    /// The tag it offered.
    pub tag: String,
    /// The form keys of the copied options.
    pub options: Vec<String>,
}

/// What a copy needs besides its form, kept in the draft until it is saved.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CopyExtras {
    /// Elements saved with the copy (a class's subclasses), keys `s1`, `s1/0`...
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub companions: Vec<FormState>,
    /// Every copied element.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<CopyLink>,
    /// Choices whose options were copied.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<CopyTag>,
}

impl CopyExtras {
    /// True for a draft that is no copy.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.companions.is_empty() && self.links.is_empty() && self.tags.is_empty()
    }

    /// The link of the copy with form key `key`.
    #[must_use]
    pub fn link(&self, key: &str) -> Option<&CopyLink> {
        self.links.iter().find(|l| l.key == key)
    }
}

/// A copy: the draft's form and what it needs besides.
#[derive(Debug, Clone, PartialEq)]
pub struct Copy {
    /// The form (key `e`), named "<Name> (copy)".
    pub form: FormState,
    /// Companions, links and tags.
    pub extras: CopyExtras,
}

/// One element a saved copy writes.
#[derive(Debug, Clone, PartialEq)]
pub struct Written {
    /// The form key it was written from.
    pub key: String,
    /// The creator form (`creator.json`'s `kind`).
    pub kind: String,
    /// The element.
    pub element: Element,
}

/// The form with no ids (new elements), without the parts that describe only the original; children too.
fn strip(form: &mut FormState) {
    form.memo.id = None;
    form.memo.multiclass_id = None;
    form.preserved.retain(|p| {
        let Some(k) = &p.kept else { return true };
        let loc = k.location.as_str();
        match loc.strip_prefix("data.") {
            Some(key) => KEPT_DATA.contains(&key),
            None => !DROPPED_PARTS.contains(&loc),
        }
    });
    if form.fields.contains_key("mapsTo") || form.memo.fields.contains_key("mapsTo") {
        form.fields.insert("mapsTo".into(), Value::Null);
    }
    for c in &mut form.children {
        strip(c);
    }
}

fn copy_name(form: &mut FormState) {
    let name = form
        .fields
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string();
    form.fields
        .insert("name".into(), json!(format!("{name} (copy)")));
}

/// A shallow copy of `el` with form key `key` (a child copied into a parent, spec 3.1): no children, "<Name> (copy)".
#[must_use]
pub fn copy_child(el: &Element, key: &str, cx: &FormContext<'_>) -> FormState {
    let mut form = read_one(el, key.to_string(), cx);
    strip(&mut form);
    copy_name(&mut form);
    form
}

/// The loaded elements `el` gives that its form edits inside it (its children, spec 3.1), in rule order.
fn loaded_children(el: &Element, lookup: &dyn Lookup) -> Vec<Element> {
    let types = child_types(&el.ty);
    let found: Vec<Element> = granted_ids(el)
        .filter_map(|id| lookup.loaded(id))
        .filter(|c| types.contains(&c.ty.as_str()))
        .cloned()
        .collect();
    granted_children(el, &found).into_iter().cloned().collect()
}

/// `el` with its children as a form of key `key`, and a link for each.
fn group(el: &Element, key: &str, cx: &FormContext<'_>, links: &mut Vec<CopyLink>) -> FormState {
    let children = loaded_children(el, cx.lookup);
    let refs: Vec<&Element> = children.iter().collect();
    let form = element_group_keyed(el, &refs, key, cx);
    links.push(link(key, &el.id));
    for (c, e) in form.children.iter().zip(&children) {
        links.push(link(&c.key, &e.id));
    }
    form
}

fn link(key: &str, original: &str) -> CopyLink {
    CopyLink {
        key: key.to_string(),
        original: original.to_string(),
        from: None,
    }
}

/// The one tag a choice offers by (`"Elven Lineage"`), or `None` for an expression or a list of ids.
fn plain_tag(supports: Option<&str>) -> Option<&str> {
    let t = supports?.trim();
    let special = |c: char| ",|&!()[]{}$".contains(c);
    (!t.is_empty() && !t.starts_with("ID_") && !t.contains(special)).then_some(t)
}

fn same_edition(a: Option<Ruleset>, b: Option<Ruleset>) -> bool {
    a.is_none() || b.is_none() || a == b
}

/// The lineage options of a copied trait (spec 4.4): loaded options of the tag its choice offers, as children of the
/// trait's copy, and the tag to turn into their ids.
fn lineage_options(
    trait_form: &mut FormState,
    trait_el: &Element,
    cx: &FormContext<'_>,
    links: &mut Vec<CopyLink>,
    tags: &mut Vec<CopyTag>,
) {
    for rule in &trait_el.rules {
        let Rule::Select(s) = rule else { continue };
        if s.ty != LINEAGE_OPTION {
            continue;
        }
        let Some(tag) = plain_tag(s.supports.as_deref()) else {
            continue;
        };
        if tags.iter().any(|t| t.key == trait_form.key && t.tag == tag) {
            continue;
        }
        let mut keys = Vec::new();
        for o in cx.lookup.loaded_of_type(LINEAGE_OPTION) {
            if !o.supports.iter().any(|t| t == tag) || !same_edition(o.ruleset, trait_el.ruleset) {
                continue;
            }
            let key = format!("{}/{}", trait_form.key, trait_form.children.len());
            trait_form.children.push(read_one(o, key.clone(), cx));
            links.push(link(&key, &o.id));
            keys.push(key);
        }
        if !keys.is_empty() {
            tags.push(CopyTag {
                key: trait_form.key.clone(),
                ty: LINEAGE_OPTION.to_string(),
                tag: tag.to_string(),
                options: keys,
            });
        }
    }
}

/// A class's subclasses, when the copy offers its own (the 2024 shape: "<Class> Subclass", written from the copy's
/// name; a 2014 class offers its subclasses by a label the original shares, so its copy keeps offering the originals).
fn subclasses(class: &Element, cx: &FormContext<'_>, links: &mut Vec<CopyLink>) -> Vec<FormState> {
    let Some(choice) = class_choice(class, cx.lookup) else {
        return Vec::new();
    };
    if !choice.tag.ends_with(" Subclass") {
        return Vec::new();
    }
    let mut out = Vec::new();
    for sub in cx.lookup.loaded_of_type("Archetype") {
        if !sub.supports.contains(&choice.tag) || !same_edition(sub.ruleset, class.ruleset) {
            continue;
        }
        let key = format!("s{}", out.len() + 1);
        out.push(group(sub, &key, cx, links));
    }
    out
}

/// A copy of the loaded element `el` (spec 11): see the module documentation. `deep` copies its children (and a class's
/// subclasses, a species' lineage options) too.
#[must_use]
pub fn copy_element(el: &Element, cx: &FormContext<'_>, deep: bool) -> Copy {
    let mut extras = CopyExtras::default();
    let mut form = if deep {
        group(el, "e", cx, &mut extras.links)
    } else {
        extras.links.push(link("e", &el.id));
        read_one(el, "e".to_string(), cx)
    };
    if deep {
        let children = loaded_children(el, cx.lookup);
        for (c, e) in form.children.iter_mut().zip(&children) {
            if e.ty == "Racial Trait" {
                lineage_options(c, e, cx, &mut extras.links, &mut extras.tags);
            }
        }
        if el.ty == "Class" {
            extras.companions = subclasses(el, cx, &mut extras.links);
        }
    }
    strip(&mut form);
    copy_name(&mut form);
    for c in &mut extras.companions {
        strip(c);
    }
    Copy { form, extras }
}

/// A companion as it is saved with `main` (its ids given): a subclass names the copied class and takes the tag and
/// level the copy's choice offers (spec 4.7).
fn attach(
    companion: &FormState,
    main: &FormState,
    main_elements: &[Element],
    cx: &FormContext<'_>,
    ids: &mut IdGenerator,
) -> FormState {
    let mut c = companion.clone();
    if c.kind == SUBCLASS_KIND {
        if let Some(class) = main.id() {
            let lookup = WithElements {
                base: cx.lookup,
                extra: main_elements,
            };
            let with_class = FormContext {
                conditions: cx.conditions.clone(),
                lookup: &lookup,
                blocks: cx.blocks,
            };
            c.fields
                .insert(PARENT.into(), json!(RefValue::resolve(class, &lookup)));
            c = refresh_subclass(&c, &with_class);
        }
    }
    assign_ids(&c, ids)
}

fn keyed(form: &FormState, elements: Vec<Element>) -> Vec<Written> {
    form.flatten()
        .into_iter()
        .zip(elements)
        .map(|(f, element)| Written {
            key: f.key.clone(),
            kind: f.kind.clone(),
            element,
        })
        .collect()
}

/// Every element a draft saves (spec 10.1, 11): the form's (its ids given, [`assign_ids`]) and its companions' (ids
/// given here), with every name of a copied original turned into its copy's id ([`repoint`]). Without extras this is
/// [`form_to_elements`].
///
/// # Errors
/// The form's problems; a companion's at `companions.<i>.<path>`.
pub fn write_copy(
    form: &FormState,
    extras: &CopyExtras,
    cx: &FormContext<'_>,
    ids: &mut IdGenerator,
) -> Result<Vec<Written>, Vec<FormProblem>> {
    // the companions read the copy as it is saved: its grants already name its own features
    let mut out = repoint(keyed(form, form_to_elements(form, cx)?), extras);
    let main: Vec<Element> = out.iter().map(|w| w.element.clone()).collect();
    let mut problems = Vec::new();
    for (i, c) in extras.companions.iter().enumerate() {
        let c = attach(c, form, &main, cx, ids);
        match form_to_elements(&c, cx) {
            Ok(els) => out.extend(keyed(&c, els)),
            Err(ps) => problems.extend(ps.into_iter().map(|mut p| {
                p.path = format!("companions.{i}.{}", p.path);
                p
            })),
        }
    }
    if !problems.is_empty() {
        return Err(problems);
    }
    Ok(repoint(out, extras))
}

/// The written copies with the originals they name turned into the copies (the copied ids wherever an element names
/// them, outside its own id, name and description) and each copied lineage choice offering the copied options by id
/// (the options lose the original's tag).
#[must_use]
pub fn repoint(mut written: Vec<Written>, extras: &CopyExtras) -> Vec<Written> {
    let id_of = |key: &str, w: &[Written]| {
        w.iter()
            .find(|x| x.key == key)
            .map(|x| x.element.id.clone())
    };
    let map: Vec<(String, String)> = extras
        .links
        .iter()
        .filter_map(|l| id_of(&l.key, &written).map(|id| (l.original.clone(), id)))
        .filter(|(old, new)| old != new)
        .collect();
    for t in &extras.tags {
        let options: Vec<String> = t
            .options
            .iter()
            .filter_map(|k| id_of(k, &written))
            .collect();
        if options.is_empty() {
            continue;
        }
        for w in &mut written {
            if w.key == t.key {
                for r in &mut w.element.rules {
                    if let Rule::Select(s) = r {
                        if s.ty == t.ty && s.supports.as_deref().map(str::trim) == Some(&t.tag) {
                            s.supports = Some(options.join("|"));
                        }
                    }
                }
            } else if t.options.contains(&w.key) {
                w.element.supports.retain(|s| s != &t.tag);
            }
        }
    }
    if map.is_empty() {
        return written;
    }
    for w in &mut written {
        w.element = renamed(&w.element, &map);
    }
    written
}

/// `e` with every whole id token of `map` replaced (outside its id, name and description).
fn renamed(e: &Element, map: &[(String, String)]) -> Element {
    let Ok(Value::Object(mut obj)) = serde_json::to_value(e) else {
        return e.clone();
    };
    for (k, v) in &mut obj {
        if !matches!(k.as_str(), "id" | "name" | "description") {
            replace_ids(v, map);
        }
    }
    serde_json::from_value(Value::Object(obj)).unwrap_or_else(|_| e.clone())
}

fn replace_ids(v: &mut Value, map: &[(String, String)]) {
    match v {
        Value::String(s) => {
            if let Some(t) = replace_tokens(s, map) {
                *s = t;
            }
        }
        Value::Array(a) => a.iter_mut().for_each(|x| replace_ids(x, map)),
        Value::Object(o) => o.values_mut().for_each(|x| replace_ids(x, map)),
        _ => {}
    }
}

/// `s` with each whole token (a run of `A-Z a-z 0-9 _`) that `map` names replaced; `None` when nothing changes.
fn replace_tokens(s: &str, map: &[(String, String)]) -> Option<String> {
    if !s.contains("ID_") {
        return None;
    }
    let word = |c: char| c.is_ascii_alphanumeric() || c == '_';
    let mut out = String::with_capacity(s.len());
    let mut changed = false;
    let mut rest = s;
    while !rest.is_empty() {
        let n = rest.find(|c: char| !word(c)).unwrap_or(rest.len());
        let (token, after) = rest.split_at(n);
        match map.iter().find(|(old, _)| old == token) {
            Some((_, new)) => {
                out.push_str(new);
                changed = true;
            }
            None => out.push_str(token),
        }
        let m = after.find(word).unwrap_or(after.len());
        out.push_str(&after[..m]);
        rest = &after[m..];
    }
    changed.then_some(out)
}

fn edition_of(v: Option<&Value>, pack: Ruleset) -> Ruleset {
    match v.and_then(Value::as_str) {
        Some("dnd2014") => Ruleset::Dnd2014,
        Some("dnd2024") => Ruleset::Dnd2024,
        Some("any") => Ruleset::Any,
        _ => pack,
    }
}

fn year(r: Ruleset) -> &'static str {
    match r {
        Ruleset::Dnd2014 => "2014",
        _ => "2024",
    }
}

/// The edition problems of a draft in a pack of edition `pack` (spec 11): a class read from an element (saved, or a
/// copy) keeps its edition (R4, an error); another element of one edition in a pack of the other is adapted when a
/// character takes it (a warning).
#[must_use]
pub fn edition_problems(form: &FormState, pack: Ruleset) -> Vec<Problem> {
    let now = edition_of(form.fields.get("edition"), pack);
    let path = "fields.edition";
    if form.kind == super::kinds::CLASS_KIND {
        let read = form.memo.fields.get("edition").map(|m| &m.value);
        return match read {
            Some(v) if edition_of(Some(v), pack) != now => vec![Problem::new(
                ProblemSeverity::Error,
                "form.class_edition",
                format!(
                    "A class keeps its edition ({}). Make a new class for the other edition.",
                    year(edition_of(Some(v), pack))
                ),
                path,
                form.id(),
            )],
            _ => Vec::new(),
        };
    }
    let single = matches!(pack, Ruleset::Dnd2014 | Ruleset::Dnd2024);
    if single && matches!(now, Ruleset::Dnd2014 | Ruleset::Dnd2024) && now != pack {
        return vec![Problem::new(
            ProblemSeverity::Warning,
            "form.edition_adapted",
            format!(
                "{} content in a {} pack is adapted when a character takes it.",
                year(now),
                year(pack)
            ),
            path,
            form.id(),
        )];
    }
    Vec::new()
}

#[cfg(test)]
#[path = "copy_tests.rs"]
mod tests;
