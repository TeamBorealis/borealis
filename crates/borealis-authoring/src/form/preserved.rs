//! The preserved bucket (spec 3.4): every part of an element no form field edits, written back byte for byte.
//!
//! A part is listed to the user in plain words ("Kept from the original") and can only be removed. On the wire a
//! part is `{key, label}`; the stored bytes ([`KeptPart`]) stay with the backend's copy of the form.

use super::write::Out;
use super::FormProblem;
use borealis_content::extra::UnknownNode;
use borealis_content::{Compendium, Element, Setter, Sheet};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// One part kept from the original.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreservedPart {
    /// Stable key (for Remove).
    pub key: String,
    /// Plain words.
    pub label: String,
    /// The stored bytes; absent on the wire.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kept: Option<KeptPart>,
}

/// Where a preserved part goes back and what it holds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeptPart {
    /// The model location: `setter`, `support`, `data.<key>`, `spellcasting`, `multiclass`, `extract`, `equipment`,
    /// `source_ref`, `renamed_from`, `legacy_alias`, `profile_overrides`, `sheet.extra`, `compendium.extra`,
    /// `extra.attribute.<name>`, `extra.child`.
    pub location: String,
    /// The stored value.
    pub value: Value,
    /// Position in `setters` / `supports` (for `setter` and `support`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slot: Option<usize>,
}

fn part(
    key: String,
    label: String,
    location: &str,
    value: Value,
    slot: Option<usize>,
) -> PreservedPart {
    PreservedPart {
        key,
        label,
        kept: Some(KeptPart {
            location: location.to_string(),
            value,
            slot,
        }),
    }
}

fn to_json<T: Serialize>(v: &T) -> Value {
    serde_json::to_value(v).unwrap_or(Value::Null)
}

/// `features_by_level` as "features by level".
fn words(key: &str) -> String {
    key.replace(['_', '-'], " ")
}

/// What reading left: the element's setters, tags and `data` keys no field took, and its parts no field edits.
pub(crate) struct Residue<'a> {
    pub(crate) e: &'a Element,
    pub(crate) setters: Vec<usize>,
    pub(crate) supports: Vec<usize>,
    pub(crate) data: Vec<&'a str>,
    /// Whole parts a field took (`extract`).
    pub(crate) parts: &'a std::collections::BTreeSet<&'static str>,
}

/// The preserved parts of an element, in a stable order.
pub(crate) fn collect(r: &Residue<'_>) -> Vec<PreservedPart> {
    let e = r.e;
    let mut out = Vec::new();
    for &i in &r.setters {
        let s = &e.setters[i];
        out.push(part(
            format!("setter.{i}"),
            format!("Property “{}” ({})", s.name, s.value),
            "setter",
            to_json(s),
            Some(i),
        ));
    }
    for &i in &r.supports {
        let t = &e.supports[i];
        out.push(part(
            format!("support.{i}"),
            format!("Tag “{t}”"),
            "support",
            json!(t),
            Some(i),
        ));
    }
    for k in &r.data {
        out.push(part(
            format!("data.{k}"),
            format!("Stored detail “{}”", words(k)),
            &format!("data.{k}"),
            e.data[*k].clone(),
            None,
        ));
    }
    whole_parts(e, r.parts, &mut out);
    extras(e, &mut out);
    out
}

fn whole_parts(
    e: &Element,
    taken: &std::collections::BTreeSet<&'static str>,
    out: &mut Vec<PreservedPart>,
) {
    let mut add = |loc: &str, label: &str, value: Value| {
        if !value.is_null() && !taken.contains(loc) {
            out.push(part(loc.to_string(), label.to_string(), loc, value, None));
        }
    };
    add("spellcasting", "Spellcasting", opt(e.spellcasting.as_ref()));
    add("multiclass", "Multiclassing", opt(e.multiclass.as_ref()));
    add("extract", "Pack contents", opt(e.extract.as_ref()));
    add("equipment", "Equipment note", opt(e.equipment.as_ref()));
    add("source_ref", "Source reference", opt(e.source_ref.as_ref()));
    add(
        "renamed_from",
        "What it replaces",
        opt(e.renamed_from.as_ref()),
    );
    add(
        "legacy_alias",
        "Earlier type name",
        opt(e.legacy_alias.as_ref()),
    );
    if !e.profile_overrides.is_empty() {
        add(
            "profile_overrides",
            "Changes per edition",
            to_json(&e.profile_overrides),
        );
    }
    if let Some(s) = e.sheet.as_ref().filter(|s| !s.extra.is_empty()) {
        add("sheet.extra", "Unknown sheet settings", to_json(&s.extra));
    }
    if let Some(c) = e.compendium.as_ref().filter(|c| !c.extra.is_empty()) {
        add(
            "compendium.extra",
            "Unknown Compendium settings",
            to_json(&c.extra),
        );
    }
}

fn opt<T: Serialize>(v: Option<&T>) -> Value {
    v.map_or(Value::Null, to_json)
}

fn extras(e: &Element, out: &mut Vec<PreservedPart>) {
    for (name, value) in &e.extra.attributes {
        let loc = format!("extra.attribute.{name}");
        out.push(part(
            loc.clone(),
            format!("Unknown setting “{name}”"),
            &loc,
            json!(value),
            None,
        ));
    }
    for (i, child) in e.extra.children.iter().enumerate() {
        out.push(part(
            format!("extra.child.{i}"),
            format!("Unknown part “{}”", child.tag),
            "extra.child",
            to_json(child),
            None,
        ));
    }
}

fn read<T: serde::de::DeserializeOwned>(v: &Value, key: &str) -> Result<T, FormProblem> {
    serde_json::from_value(v.clone()).map_err(|_| {
        FormProblem::new(
            format!("preserved.{key}"),
            "A part kept from the original can't be read back. Remove it.",
        )
    })
}

/// Put the preserved parts back (a part without its stored bytes is skipped: the UI never had them).
pub(crate) fn restore(parts: &[PreservedPart], out: &mut Out) -> Result<(), FormProblem> {
    for p in parts {
        let Some(k) = &p.kept else { continue };
        restore_one(&p.key, k, out)?;
    }
    Ok(())
}

fn restore_one(key: &str, k: &KeptPart, out: &mut Out) -> Result<(), FormProblem> {
    let slots: Vec<usize> = k.slot.into_iter().collect();
    match k.location.as_str() {
        "setter" => {
            let s: Setter = read(&k.value, key)?;
            out.setters(Some(s), &slots);
            Ok(())
        }
        "support" => {
            let t: String = read(&k.value, key)?;
            out.supports(Some(t), &slots);
            Ok(())
        }
        _ => restore_part(key, k, &mut out.element),
    }
}

fn restore_part(key: &str, k: &KeptPart, e: &mut Element) -> Result<(), FormProblem> {
    match k.location.as_str() {
        "spellcasting" => e.spellcasting = Some(read(&k.value, key)?),
        "multiclass" => e.multiclass = Some(read(&k.value, key)?),
        "extract" => e.extract = Some(read(&k.value, key)?),
        "equipment" => e.equipment = Some(read(&k.value, key)?),
        "source_ref" => e.source_ref = Some(read(&k.value, key)?),
        "renamed_from" => e.renamed_from = Some(read(&k.value, key)?),
        "legacy_alias" => e.legacy_alias = Some(read(&k.value, key)?),
        "profile_overrides" => e.profile_overrides = read(&k.value, key)?,
        "sheet.extra" => e.sheet.get_or_insert_with(Sheet::default).extra = read(&k.value, key)?,
        "compendium.extra" => {
            e.compendium.get_or_insert_with(Compendium::default).extra = read(&k.value, key)?;
        }
        "extra.child" => {
            let c: UnknownNode = read(&k.value, key)?;
            e.extra.children.push(c);
        }
        loc => {
            if let Some(name) = loc.strip_prefix("extra.attribute.") {
                let v: String = read(&k.value, key)?;
                e.extra.attributes.insert(name.to_string(), v);
            } else if let Some(d) = loc.strip_prefix("data.") {
                e.data.insert(d.to_string(), k.value.clone());
            } else {
                return Err(FormProblem::new(
                    format!("preserved.{key}"),
                    "A part kept from the original has no place in this element. Remove it.",
                ));
            }
        }
    }
    Ok(())
}
