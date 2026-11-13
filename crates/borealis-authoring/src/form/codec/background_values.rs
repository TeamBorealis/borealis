//! The value shapes of the background form's own controls (spec 4.3) and the rules each writes, in the shape the
//! bundled SRD backgrounds store them:
//!
//! * [`FeatGrant`]: the origin feat, `grant_feat {feat, params}` (a fixed choice of the feat, such as Magic
//!   Initiate's spell list, by the engine's param key `spell_list`);
//! * [`ChoiceRow`]: a language or tool choice, `select` of type `Language` or `Proficiency` with `number` and a tag
//!   filter (`Standard||Exotic`, `Gaming Set`);
//! * [`WrittenList`]: a characteristic (Personality Trait, Ideal, Bond, Flaw), `select` of type `List` with
//!   `items [{id, text}]` numbered 1..n and `number`.
//!
//! Deterministic, no AI (G7).

use crate::expr_build::tag_filter::{TagFilter, TagFilterLanguage};
use crate::expr_build::ExprValue;
use crate::form::value::RefValue;
use borealis_content::expr::{SupportsExpr, SupportsLeaf};
use borealis_content::{CustomRule, Element, ListItem, Rule, SelectRule};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

/// The origin feat of a 2024 background (`HomebrewFeatGrant`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeatGrant {
    /// The feat.
    #[serde(default)]
    pub feat: Option<RefValue>,
    /// The feat's choices the background makes, by param key (`spell_list: Cleric`); the others stay the
    /// character's.
    #[serde(default)]
    pub params: BTreeMap<String, String>,
}

/// One language or tool choice (`HomebrewChoiceRow`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChoiceRow {
    /// `language` or `proficiency` (tools and other proficiencies).
    pub kind: String,
    /// How many the character chooses.
    pub count: u32,
    /// What it offers: the tag filter (`Standard||Exotic`).
    #[serde(default)]
    pub from: Option<ExprValue<TagFilter>>,
    /// The choice's name, its persistence key (kept once saved); empty for a new row, written from the background's
    /// name.
    #[serde(default)]
    pub name: String,
}

/// A written list the character picks from (`HomebrewWrittenList`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WrittenList {
    /// How many the character picks.
    pub count: u32,
    /// The entries, in order (numbered 1..n when written).
    #[serde(default)]
    pub items: Vec<String>,
}

/// The engine's key of a feat's choice (`borealis_engine::custom_rules::param_key`: "Spell List" is `spell_list`).
#[must_use]
pub fn param_key(name: &str) -> String {
    name.trim().to_lowercase().replace([' ', '-'], "_")
}

/// The pseudo picker type of a feat's choices (`homebrew_reference_options`): `["Feat Parameter", <feat id>]` gives
/// the feat's choice names, `["Feat Parameter", <feat id>, <name>]` that choice's options.
pub const FEAT_PARAMETER_PICKER: &str = "Feat Parameter";

/// The words of the pseudo picker [`FEAT_PARAMETER_PICKER`]: the choice names of the feat `feat` (with `param`, that
/// choice's options); none when the feat isn't loaded.
#[must_use]
pub fn feat_parameter_words(
    lookup: &dyn crate::form::Lookup,
    feat: &str,
    param: Option<&str>,
) -> Vec<String> {
    let Some(e) = lookup.loaded(feat) else {
        return Vec::new();
    };
    let choices = feat_params(e);
    match param {
        None => choices.into_iter().map(|(n, _)| n).collect(),
        Some(p) => choices
            .into_iter()
            .find(|(n, _)| param_key(n) == param_key(p))
            .map(|(_, o)| o)
            .unwrap_or_default(),
    }
}

/// A feat's own choices (its `select_param` rules): name and options, in rule order.
#[must_use]
pub fn feat_params(feat: &Element) -> Vec<(String, Vec<String>)> {
    feat.rules
        .iter()
        .filter_map(|r| match r {
            Rule::Custom(c) if c.kind == "select_param" => {
                let name = c.fields.get("name")?.as_str()?.trim().to_string();
                let options = c
                    .fields
                    .get("options")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect();
                (!name.is_empty()).then_some((name, options))
            }
            _ => None,
        })
        .collect()
}

pub(super) const GRANT_FEAT: &str = "grant_feat";
pub(super) const DISTRIBUTION: &str = "ability_distribution";
/// The fixed 2024 shape: +2 and +1, or +1 on each of the three.
const DISTRIBUTION_OPTIONS: [[i64; 3]; 2] = [[2, 1, 0], [1, 1, 1]];
const DISTRIBUTION_CAP: i64 = 20;

/// `ability_distribution {name, pool, options [[2,1],[1,1,1]], cap 20}`.
pub(super) fn distribution_rule(name: &str, pool: &[String]) -> Rule {
    let options: Vec<Vec<i64>> = DISTRIBUTION_OPTIONS
        .iter()
        .map(|o| o.iter().copied().filter(|n| *n > 0).collect())
        .collect();
    let mut fields = Map::new();
    fields.insert("name".into(), json!(name));
    fields.insert("pool".into(), json!(pool));
    fields.insert("options".into(), json!(options));
    fields.insert("cap".into(), json!(DISTRIBUTION_CAP));
    Rule::Custom(CustomRule {
        kind: DISTRIBUTION.into(),
        fields,
    })
}

/// `grant_feat {feat, params}` (`params` always written, `{}` when the background fixes none, as the SRD).
pub(super) fn feat_rule(feat: &str, params: &BTreeMap<String, String>) -> Rule {
    let mut fields = Map::new();
    fields.insert("feat".into(), json!(feat));
    fields.insert("params".into(), json!(params));
    Rule::Custom(CustomRule {
        kind: GRANT_FEAT.into(),
        fields,
    })
}

/// The feat grant a stored `grant_feat` rule holds (`None` for another shape).
pub(super) fn feat_grant_of(c: &CustomRule) -> Option<(String, BTreeMap<String, String>)> {
    if c.kind != GRANT_FEAT || c.fields.keys().any(|k| k != "feat" && k != "params") {
        return None;
    }
    let feat = c.fields.get("feat")?.as_str()?.trim().to_string();
    let params: BTreeMap<String, String> =
        serde_json::from_value(c.fields.get("params")?.clone()).ok()?;
    (!feat.is_empty()).then_some((feat, params))
}

pub(super) const LANGUAGE: &str = "language";
pub(super) const PROFICIENCY: &str = "proficiency";

/// The select type of a choice row's kind.
pub(super) fn choice_type(kind: &str) -> Option<&'static str> {
    match kind {
        LANGUAGE => Some("Language"),
        PROFICIENCY => Some("Proficiency"),
        _ => None,
    }
}

/// The row kind of a select type.
pub(super) fn choice_kind(ty: &str) -> Option<&'static str> {
    match ty {
        "Language" => Some(LANGUAGE),
        "Proficiency" => Some(PROFICIENCY),
        _ => None,
    }
}

/// A select with only these parts (the rest at their defaults).
pub(super) fn plain_select(
    ty: &str,
    name: &str,
    number: Option<u32>,
    supports: Option<String>,
) -> SelectRule {
    SelectRule {
        ty: ty.to_string(),
        name: name.to_string(),
        number,
        supports,
        ..SelectRule::default()
    }
}

/// `number` as the SRD writes it: absent for one pick.
pub(super) fn picks(count: u32) -> Option<u32> {
    (count != 1).then_some(count)
}

/// A choice row's select: its type, name, `number` and tag filter.
pub(super) fn choice_rule(ty: &str, name: &str, count: u32, supports: &str) -> Rule {
    Rule::Select(plain_select(
        ty,
        name,
        picks(count),
        Some(supports.to_string()),
    ))
}

/// The row a plain choice select is (`None` for a select of another shape: an id list, a level, a default...).
pub(super) fn choice_row_of(s: &SelectRule) -> Option<ChoiceRow> {
    let kind = choice_kind(&s.ty)?;
    let supports = s.supports.as_deref().filter(|t| !t.trim().is_empty())?;
    if supports.split('|').all(|t| t.trim().starts_with("ID_")) {
        return None;
    }
    let row = ChoiceRow {
        kind: kind.to_string(),
        count: s.number.unwrap_or(1),
        from: Some(ExprValue::<TagFilter>::read::<TagFilterLanguage>(
            supports,
            &(),
        )),
        name: s.name.clone(),
    };
    let same = Rule::Select(s.clone()) == choice_rule(&s.ty, &s.name, row.count, supports);
    same.then_some(row)
}

/// A characteristic's select: `List`, its name, `number`, the items numbered 1..n.
pub(super) fn list_rule(name: &str, count: u32, items: &[String]) -> Rule {
    let mut s = plain_select("List", name, picks(count), None);
    s.items = items
        .iter()
        .zip(1..)
        .map(|(text, id)| ListItem {
            id,
            text: text.clone(),
            extra: borealis_content::extra::Extra::default(),
        })
        .collect();
    Rule::Select(s)
}

/// The written list a plain characteristic select called `name` is.
pub(super) fn written_list_of(s: &SelectRule, name: &str) -> Option<WrittenList> {
    if s.ty != "List" || s.name != name || s.items.is_empty() {
        return None;
    }
    let list = WrittenList {
        count: s.number.unwrap_or(1),
        items: s.items.iter().map(|i| i.text.clone()).collect(),
    };
    (Rule::Select(s.clone()) == list_rule(name, list.count, &list.items)).then_some(list)
}

pub(super) const VARIANT_TYPE: &str = "Background Variant";

/// The variants choice: an optional `select` of type `Background Variant` offering the variants by id.
pub(super) fn variants_rule(name: &str, ids: &[String]) -> Rule {
    let mut s = plain_select(VARIANT_TYPE, name, None, Some(ids.join("|")));
    s.optional = Some(true);
    Rule::Select(s)
}

/// The variant ids a plain variants select offers.
pub(super) fn variant_ids_of(s: &SelectRule) -> Option<Vec<String>> {
    if s.ty != VARIANT_TYPE {
        return None;
    }
    let ids: Vec<String> = s
        .supports
        .as_deref()?
        .split('|')
        .map(|t| t.trim().to_string())
        .collect();
    let all_ids = ids.iter().all(|t| t.starts_with("ID_"));
    (all_ids && Rule::Select(s.clone()) == variants_rule(&s.name, &ids)).then_some(ids)
}

/// A variants select that offers its variants by tag (an Aurora background: `supports "Variant Criminal"`, or an
/// expression of tags): its expression, when it names no id and the rule is only the select (maybe optional).
pub(super) fn variant_tags_of(s: &SelectRule) -> Option<SupportsExpr> {
    if s.ty != VARIANT_TYPE {
        return None;
    }
    let text = s.supports.as_deref()?;
    let expr = SupportsExpr::parse(text).ok().flatten()?;
    let tags = expr.tags();
    if tags.is_empty() || tags.iter().any(|t| t.starts_with("ID_")) || !expr.dynamics().is_empty() {
        return None;
    }
    let mut plain = plain_select(VARIANT_TYPE, &s.name, None, Some(text.to_string()));
    plain.optional = s.optional;
    (*s == plain).then_some(expr)
}

/// The variant carries the tags `expr` asks for.
pub(super) fn offered_by_tags(expr: &SupportsExpr, variant: &Element) -> bool {
    borealis_engine::eval::eval_bool(expr.root(), &|leaf| match leaf {
        SupportsLeaf::Tag(t) => variant.supports.iter().any(|s| s == t),
        SupportsLeaf::Dynamic(_) => false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn param_keys_are_the_engine_s() {
        assert_eq!(param_key("Spell List"), "spell_list");
        assert_eq!(param_key(" Spellcasting-Ability "), "spellcasting_ability");
    }

    #[test]
    fn the_distribution_is_the_srd_shape() {
        let r = distribution_rule("Ability Scores (X)", &["strength".into()]);
        assert_eq!(
            serde_json::to_value(r).unwrap(),
            json!({"kind": "ability_distribution", "name": "Ability Scores (X)", "pool": ["strength"],
                   "options": [[2, 1], [1, 1, 1]], "cap": 20})
        );
    }
}
