//! The small types' own fields (spec 4.10) that bind more than one location:
//!
//! * **Language rarity** (standard, exotic or secret): the setters `standard`/`exotic`/`secret`, the support tags
//!   `Standard`/`Exotic`/`Secret` (2024: `Rare` for exotic, as the 2024 language choices offer `Standard||Rare`) and
//!   the 2024 mirror `data.rarity`. A 2014 Aurora-shaped language holds one `Exotic` setter, `true` or `false`; it
//!   keeps that shape.
//! * **What a proficiency covers**: the plain `grant`s of `Proficiency` elements (a group granting each armor).
//! * **Listed as a condition**: the tag `Condition` and the 2024 mirror `data.tag`.
//! * **A rules text's group**: `data.tag` and the support tag of the same name.

use super::{decode, present, raw_setters, stored, to_json, Reader};
use crate::form::value::RefValue;
use crate::form::write::Out;
use crate::form::{FieldMemo, FormContext, FormProblem, Slots};
use borealis_content::{GrantRule, Rule, Ruleset, Setter};
use serde_json::{json, Value};

/// A small type's field (spec 4.10).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SmallField {
    /// A proficiency group's narrower proficiencies: its plain `grant`s of `Proficiency`.
    Covers,
    /// A condition listed with the conditions: the tag `Condition` and `data.tag`.
    Listed,
    /// A rules glossary entry's group: `data.tag` and the support tag of that name.
    GlossaryGroup,
}

const PROFICIENCY: &str = "Proficiency";
const CONDITION: &str = "Condition";
/// Rank of the covers grants among new field rules (they lead, before the blocks).
const RANK_COVERS: u8 = 0;

pub(super) const RARITIES: [&str; 3] = ["standard", "exotic", "secret"];
/// The support tags a language's rarity owns.
pub(super) const RARITY_TAGS: [&str; 4] = ["Standard", "Exotic", "Secret", "Rare"];
const RARE: &str = "rare";

impl SmallField {
    pub(super) fn bindings(self) -> Vec<String> {
        let b: &[&str] = match self {
            Self::Covers => &["rules:grant Proficiency"],
            Self::Listed => &["supports:Condition", "data.tag"],
            Self::GlossaryGroup => &["data.tag", "supports:<group>"],
        };
        b.iter().map(|s| (*s).to_string()).collect()
    }

    pub(super) fn data_keys(self) -> Vec<&'static str> {
        match self {
            Self::Covers => Vec::new(),
            Self::Listed | Self::GlossaryGroup => vec!["tag"],
        }
    }

    pub(super) fn owned_tags(self) -> &'static [&'static str] {
        match self {
            Self::Listed => &[CONDITION],
            Self::Covers | Self::GlossaryGroup => &[],
        }
    }
}

/// A grant of a `Proficiency` with nothing else on it (no level, condition, setters...).
fn plain_grant(rule: &Rule) -> Option<&str> {
    let Rule::Grant(g) = rule else { return None };
    let plain = g.ty.as_deref() == Some(PROFICIENCY)
        && g.level.is_none()
        && g.requirements.is_none()
        && g.spellcasting.is_none()
        && g.prepared.is_none()
        && g.setters.is_empty()
        && g.extra.is_empty();
    plain.then_some(g.id.as_str())
}

/// Take the first untaken support tag equal to `tag`.
fn take_tag(r: &mut Reader<'_>, tag: &str) -> Option<(String, usize)> {
    let e = r.e;
    let i = (0..e.supports.len()).find(|&i| !r.supports_taken[i] && e.supports[i] == tag)?;
    r.supports_taken[i] = true;
    Some((e.supports[i].clone(), i))
}

pub(super) fn read(field: SmallField, r: &mut Reader<'_>, cx: &FormContext<'_>) -> FieldMemo {
    match field {
        SmallField::Covers => {
            let e = r.e;
            let mut raw = Vec::new();
            let mut refs = Vec::new();
            let mut slots = Slots::default();
            for (i, rule) in e.rules.iter().enumerate() {
                if r.rules_taken[i] {
                    continue;
                }
                if let Some(id) = plain_grant(rule) {
                    r.take_rule(i);
                    refs.push(RefValue::resolve(id, cx.lookup));
                    raw.push(to_json(rule));
                    slots.rules.push(i);
                }
            }
            FieldMemo {
                value: to_json(&refs),
                raw: Value::Array(raw),
                slots,
            }
        }
        SmallField::Listed => {
            let tag = take_tag(r, CONDITION);
            let data = r.take_data("tag");
            let on = tag.is_some() || data.and_then(Value::as_str) == Some(CONDITION);
            tagged_memo(json!(on), tag, data)
        }
        SmallField::GlossaryGroup => {
            let data = r.take_data("tag");
            let group = data.and_then(Value::as_str).unwrap_or_default().trim();
            let tag = (!group.is_empty()).then(|| take_tag(r, group)).flatten();
            tagged_memo(json!(group), tag, data)
        }
    }
}

fn tagged_memo(value: Value, tag: Option<(String, usize)>, data: Option<&Value>) -> FieldMemo {
    let (tags, slots): (Vec<String>, Vec<usize>) = tag.into_iter().unzip();
    FieldMemo {
        value,
        raw: json!({ "tags": tags, "data": present(data) }),
        slots: Slots {
            supports: slots,
            ..Slots::default()
        },
    }
}

/// Put an unchanged field's stored parts back.
pub(super) fn write_raw(field: SmallField, m: &FieldMemo, out: &mut Out, path: &str) {
    match field {
        SmallField::Covers => {
            let rules: Vec<Rule> = m
                .raw
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|r| serde_json::from_value(r.clone()).ok())
                .collect();
            out.field_rules(
                rules.into_iter().map(|r| (RANK_COVERS, r)),
                &m.slots.rules,
                path,
            );
        }
        SmallField::Listed | SmallField::GlossaryGroup => {
            let tags: Vec<String> =
                serde_json::from_value(m.raw["tags"].clone()).unwrap_or_default();
            out.supports(tags, &m.slots.supports);
            if let Some(v) = stored(&m.raw["data"]) {
                out.element.data.insert("tag".into(), v.clone());
            }
        }
    }
}

/// Write an edited field canonically. `memo` is absent for a new element.
pub(super) fn write(
    field: SmallField,
    value: &Value,
    memo: Option<&FieldMemo>,
    path: &str,
    out: &mut Out,
) -> Result<(), FormProblem> {
    let slots = memo.map(|m| m.slots.clone()).unwrap_or_default();
    let stored_data = memo.and_then(|m| stored(&m.raw["data"]));
    let is_2024 = out.element.ruleset == Some(Ruleset::Dnd2024);
    match field {
        SmallField::Covers => {
            let refs: Vec<RefValue> = decode(value, path)?;
            let rules: Vec<(u8, Rule)> = refs
                .iter()
                .map(|r| r.id.trim())
                .filter(|id| !id.is_empty())
                .map(|id| {
                    let grant = GrantRule {
                        id: id.to_string(),
                        ty: Some(PROFICIENCY.into()),
                        ..GrantRule::default()
                    };
                    (RANK_COVERS, Rule::Grant(grant))
                })
                .collect();
            out.field_rules(rules, &slots.rules, path);
        }
        SmallField::Listed => {
            let on: bool = decode(value, path)?;
            if on {
                out.supports(Some(CONDITION.to_string()), &slots.supports);
            }
            let data = if on {
                (stored_data.is_some() || (memo.is_none() && is_2024)).then(|| json!(CONDITION))
            } else {
                // a stored group other than "Condition" is no part of this check
                stored_data
                    .filter(|v| v.as_str() != Some(CONDITION))
                    .cloned()
            };
            if let Some(v) = data {
                out.element.data.insert("tag".into(), v);
            }
        }
        SmallField::GlossaryGroup => {
            let group = decode::<String>(value, path)?.trim().to_string();
            if !group.is_empty() {
                out.supports(Some(group.clone()), &slots.supports);
            }
            let as_data = if group.is_empty() {
                Value::Null
            } else {
                json!(group)
            };
            if stored_data.is_some() || (memo.is_none() && !group.is_empty()) {
                out.element.data.insert("tag".into(), as_data);
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------- language rarity

/// A language's rarity, read from its setters, tags and `data.rarity` (secret before exotic before standard).
pub(super) fn read_rarity(r: &mut Reader<'_>) -> FieldMemo {
    let mut setters = Vec::new();
    let mut slots = Slots::default();
    for name in RARITIES {
        while let Some((i, s)) = r.take_setter(name) {
            setters.push(s.clone());
            slots.setters.push(i);
        }
    }
    let (tags, tag_slots) = r.take_tags(|t| RARITY_TAGS.contains(&t));
    slots.supports = tag_slots;
    let data = r.take_data("rarity");
    let mut said: Vec<String> = setters
        .iter()
        .filter(|s| s.as_bool() == Some(true))
        .map(|s| s.name.to_lowercase())
        .collect();
    said.extend(tags.iter().map(|t| t.to_lowercase()));
    said.extend(data.and_then(Value::as_str).map(str::to_lowercase));
    if aurora_exotic(&setters).is_some_and(|s| s.as_bool() == Some(false)) {
        said.push("standard".into());
    }
    let has = |r: &str| said.iter().any(|s| s == r || (r == "exotic" && s == RARE));
    let value = ["secret", "exotic", "standard"]
        .into_iter()
        .find(|r| has(r))
        .unwrap_or_default();
    let raw = json!({ "setters": setters, "tags": tags, "data": present(data) });
    FieldMemo {
        value: json!(value),
        raw,
        slots,
    }
}

/// The one `Exotic` setter of a 2014 Aurora-shaped language (`true` or `false`), if that is its shape.
fn aurora_exotic(setters: &[Setter]) -> Option<&Setter> {
    match setters {
        [s] if s.name.eq_ignore_ascii_case("exotic") && s.as_bool().is_some() => Some(s),
        _ => None,
    }
}

/// Write an edited rarity to the setters, the tag and (when the language has it, or is new and 2024) `data.rarity`.
pub(super) fn write_rarity(
    rarity: &str,
    memo: Option<&FieldMemo>,
    out: &mut Out,
    path: &str,
) -> Result<(), FormProblem> {
    let rarity = rarity.trim().to_lowercase();
    if rarity.is_empty() {
        return Ok(());
    }
    if !RARITIES.contains(&rarity.as_str()) {
        return Err(FormProblem::new(path, "Choose standard, exotic or secret."));
    }
    let raw = memo.map(|m| &m.raw);
    let slots = memo.map(|m| m.slots.clone()).unwrap_or_default();
    let old = raw.map(|r| raw_setters(&r["setters"])).unwrap_or_default();
    let setter = match aurora_exotic(&old) {
        // the 2014 shape: one Exotic setter, true or false; a secret language keeps it as it was
        Some(s) => Setter {
            value: match rarity.as_str() {
                "exotic" => "true".into(),
                "standard" => "false".into(),
                _ => s.value.clone(),
            },
            ..s.clone()
        },
        None => old
            .iter()
            .find(|s| s.name.eq_ignore_ascii_case(&rarity))
            .map_or_else(
                || Setter::new(rarity.clone(), "true"),
                |s| Setter {
                    value: "true".into(),
                    ..s.clone()
                },
            ),
    };
    out.setters(Some(setter), &slots.setters);
    let old_tags: Vec<String> = raw
        .and_then(|r| serde_json::from_value(r["tags"].clone()).ok())
        .unwrap_or_default();
    let old_data = raw.and_then(|r| stored(&r["data"]));
    let rare = old_tags.iter().any(|t| t == "Rare")
        || old_data.and_then(Value::as_str) == Some(RARE)
        || out.element.ruleset == Some(Ruleset::Dnd2024);
    let word = if rarity == "exotic" && rare {
        RARE.to_string()
    } else {
        rarity.clone()
    };
    let mut tag = word.clone();
    tag[..1].make_ascii_uppercase();
    out.supports(Some(tag), &slots.supports);
    let is_new_2024 = memo.is_none() && out.element.ruleset == Some(Ruleset::Dnd2024);
    if old_data.is_some() || is_new_2024 {
        out.element.data.insert("rarity".into(), json!(word));
    }
    Ok(())
}

#[cfg(test)]
#[path = "small_tests.rs"]
mod tests;
