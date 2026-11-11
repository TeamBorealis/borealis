//! The background form's codecs (spec 4.3), in the shapes the bundled SRD backgrounds store them:
//!
//! * **2024**: Ability scores (`ability_distribution` with the three abilities' `pool`, `options [[2,1],[1,1,1]]`,
//!   `cap 20`, + `data.ability_scores`); Origin feat (`grant_feat` with `params`, + `data.origin_feat`,
//!   `data.origin_feat_params`); Skills and Tool (`grant`s of `Proficiency`, + `data.skills`, `data.tool`); Starting
//!   equipment (`equipment_package {ref, name}`, + `data.equipment_package`; the package is a child).
//! * **2014**: Skills, Tools and Languages (`grant`s); language and tool choices (`select` with `number` and a tag
//!   filter); the Background Feature (a child, granted: the shared children list); Personality Trait, Ideal, Bond
//!   and Flaw (`select` of type `List`, items numbered 1..n); the equipment note (`equipment`); variants (an optional
//!   `select` of type `Background Variant` offering the variant children by id).
//!
//! A field takes a rule only when the rule is exactly what the field writes for some value (anything else stays a
//! block); 2024 mirrors are written when the element has them or is a new 2024 background (spec 3.3). The fields'
//! rules go through the one placement (`write::merge_rules`) by rank: abilities, feat, skills, tools, languages,
//! choices, the feature, the characteristics, variants, equipment (the SRD order).

use super::background_values::{
    choice_row_of, choice_rule, choice_type, distribution_rule, feat_grant_of, feat_params,
    feat_rule, list_rule, param_key, variant_ids_of, variants_rule, written_list_of, ChoiceRow,
    FeatGrant, WrittenList, DISTRIBUTION, VARIANT_TYPE,
};
use super::class::{package_rule, Raw, Take, ABILITY_CHOICES};
use super::{decode, decode_raw, to_json, Reader};
use crate::expr_build::tag_filter::TagFilterLanguage;
use crate::expr_build::Ability;
use crate::form::child_refs::{self, ChildEntry};
use crate::form::value::RefValue;
use crate::form::write::Out;
use crate::form::{FieldMemo, FormContext, FormProblem, Slots};
use borealis_content::rich::{self, Doc};
use borealis_content::{CustomRule, Equipment, GrantRule, Rule, Ruleset};
use serde_json::{json, Map, Value};

/// One of the four characteristics of a 2014 background.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Characteristic {
    /// "Personality Trait" (the SRD lets the character pick 2).
    PersonalityTrait,
    /// "Ideal".
    Ideal,
    /// "Bond".
    Bond,
    /// "Flaw".
    Flaw,
}

impl Characteristic {
    /// The select's name, as the SRD (and Aurora) store it.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::PersonalityTrait => "Personality Trait",
            Self::Ideal => "Ideal",
            Self::Bond => "Bond",
            Self::Flaw => "Flaw",
        }
    }

    fn index(self) -> u8 {
        match self {
            Self::PersonalityTrait => 0,
            Self::Ideal => 1,
            Self::Bond => 2,
            Self::Flaw => 3,
        }
    }

    /// Plural words for sentences ("personality traits").
    fn plural(self) -> &'static str {
        match self {
            Self::PersonalityTrait => "personality traits",
            Self::Ideal => "ideals",
            Self::Bond => "bonds",
            Self::Flaw => "flaws",
        }
    }
}

/// A background field (`Codec::Background`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackgroundField {
    /// The three abilities of the 2024 `ability_distribution` (+ `data.ability_scores`).
    AbilityScores,
    /// The 2024 origin feat, `grant_feat` with `params` (+ `data.origin_feat`, `data.origin_feat_params`).
    OriginFeat,
    /// Skill proficiency grants (+ `data.skills`).
    Skills,
    /// Tool (and other) proficiency grants (+ `data.tool`).
    Tools,
    /// Language grants.
    Languages,
    /// Language and tool choices: `select`s with `number` and a tag filter.
    Choices,
    /// A characteristic: a `select` of type `List`.
    Characteristic(Characteristic),
    /// The `equipment_package` rule (+ `data.equipment_package`); the package is a child.
    Equipment,
    /// The equipment note (`equipment`).
    EquipmentNote,
    /// The variants: an optional `select` of type `Background Variant` by id; the variants are children.
    Variants,
}

const SKILL_PREFIX: &str = "ID_PROFICIENCY_SKILL_";
const PROFICIENCY: &str = "Proficiency";
const PACKAGE_KIND: &str = "equipment_package";
const MAX_CHOICE: u32 = 10;
const MAX_PICKS: u32 = 10;

impl BackgroundField {
    pub(super) fn bindings(self) -> Vec<String> {
        let v: &[&str] = match self {
            Self::AbilityScores => &["rules:ability_distribution", "data.ability_scores"],
            Self::OriginFeat => &[
                "rules:grant_feat",
                "data.origin_feat",
                "data.origin_feat_params",
            ],
            Self::Skills => &["rules:grant ID_PROFICIENCY_SKILL_*", "data.skills"],
            Self::Tools => &["rules:grant Proficiency", "data.tool"],
            Self::Languages => &["rules:grant Language"],
            Self::Choices => &["rules:select Language", "rules:select Proficiency"],
            Self::Characteristic(_) => &["rules:select List"],
            Self::Equipment => &["rules:equipment_package", "data.equipment_package"],
            Self::EquipmentNote => &["equipment"],
            Self::Variants => &["rules:select Background Variant"],
        };
        v.iter().map(|s| (*s).to_string()).collect()
    }

    pub(super) fn data_keys(self) -> Vec<&'static str> {
        match self {
            Self::AbilityScores => vec!["ability_scores"],
            Self::OriginFeat => vec!["origin_feat", "origin_feat_params"],
            Self::Skills => vec!["skills"],
            Self::Tools => vec!["tool"],
            Self::Equipment => vec!["equipment_package"],
            _ => Vec::new(),
        }
    }

    /// For a children field: its child type (the package, the variants).
    pub(crate) fn child_type(self) -> Option<&'static str> {
        match self {
            Self::Equipment => Some("Equipment Package"),
            Self::Variants => Some(VARIANT_TYPE),
            _ => None,
        }
    }

    /// Order of its new rules among the other fields' (the SRD order; the feature's grant is the children list's 20).
    fn rank(self) -> u8 {
        match self {
            Self::AbilityScores => 1,
            Self::OriginFeat => 2,
            Self::Skills => 3,
            Self::Tools => 4,
            Self::Languages => 5,
            Self::Choices => 6,
            Self::Characteristic(c) => 21 + c.index(),
            Self::Variants => 26,
            Self::Equipment => 30,
            Self::EquipmentNote => 0,
        }
    }
}

/// A grant of `ty` with nothing else on it.
fn plain_grant<'a>(rule: &'a Rule, ty: &str) -> Option<&'a str> {
    let Rule::Grant(g) = rule else { return None };
    let plain = g.ty.as_deref() == Some(ty)
        && *g
            == GrantRule {
                id: g.id.clone(),
                ty: g.ty.clone(),
                ..GrantRule::default()
            };
    plain.then_some(g.id.as_str())
}

fn grant(id: &str, ty: &str) -> Rule {
    Rule::Grant(GrantRule {
        id: id.to_string(),
        ty: Some(ty.to_string()),
        ..GrantRule::default()
    })
}

fn is_skill(id: &str) -> bool {
    id.starts_with(SKILL_PREFIX)
}

fn custom(rule: &Rule) -> Option<&CustomRule> {
    match rule {
        Rule::Custom(c) => Some(c),
        _ => None,
    }
}

/// "Intelligence" for `intelligence`.
fn ability_word(key: &str) -> Option<String> {
    let a = Ability::from_full_name(key)?;
    ABILITY_CHOICES
        .get(a.index())
        .map(|(word, _)| (*word).to_string())
}

/// The pool a stored distribution holds, as ability words in sheet order (`None` when it isn't the field's shape).
fn distribution_words(c: &CustomRule) -> Option<Vec<String>> {
    if c.kind != DISTRIBUTION {
        return None;
    }
    let name = c.fields.get("name")?.as_str()?;
    let pool: Vec<String> = decode_raw(c.fields.get("pool")?)?;
    let words: Vec<String> = pool
        .iter()
        .map(|k| ability_word(k))
        .collect::<Option<_>>()?;
    (Rule::Custom(c.clone()) == distribution_rule(name, &pool_keys(&words)?)).then_some(words)
}

/// Ability words as the pool's keys (`strength`), in sheet order, each once.
fn pool_keys(words: &[String]) -> Option<Vec<String>> {
    let mut abilities: Vec<Ability> = words
        .iter()
        .map(|w| Ability::from_key(w))
        .collect::<Option<_>>()?;
    abilities.sort_by_key(|a| a.index());
    abilities.dedup();
    Some(abilities.iter().map(|a| a.full().to_string()).collect())
}

/// Read one background field.
pub(crate) fn read(field: BackgroundField, r: &mut Reader<'_>, cx: &FormContext<'_>) -> FieldMemo {
    let mut t = Take {
        r,
        raw: Raw::default(),
        slots: Slots::default(),
    };
    let refs = |rules: &[&Rule], ty: &str| -> Value {
        let refs: Vec<RefValue> = rules
            .iter()
            .filter_map(|x| plain_grant(x, ty))
            .map(|id| RefValue::resolve(id, cx.lookup))
            .collect();
        to_json(&refs)
    };
    let value = match field {
        BackgroundField::AbilityScores => {
            let rules = t.rules(|x| custom(x).and_then(distribution_words).is_some(), 1);
            t.data("ability_scores");
            let words = rules
                .first()
                .and_then(|x| custom(x))
                .and_then(distribution_words)
                .unwrap_or_default();
            json!(words)
        }
        BackgroundField::OriginFeat => {
            let rules = t.rules(|x| custom(x).and_then(feat_grant_of).is_some(), 1);
            t.data("origin_feat");
            t.data("origin_feat_params");
            let grant = rules
                .first()
                .and_then(|x| custom(x))
                .and_then(feat_grant_of)
                .map(|(feat, params)| FeatGrant {
                    feat: Some(RefValue::resolve(&feat, cx.lookup)),
                    params,
                })
                .unwrap_or_default();
            to_json(&grant)
        }
        BackgroundField::Skills => {
            let rules = t.rules(
                |x| plain_grant(x, PROFICIENCY).is_some_and(is_skill),
                usize::MAX,
            );
            t.data("skills");
            refs(&rules, PROFICIENCY)
        }
        BackgroundField::Tools => {
            let rules = t.rules(
                |x| plain_grant(x, PROFICIENCY).is_some_and(|id| !is_skill(id)),
                usize::MAX,
            );
            t.data("tool");
            refs(&rules, PROFICIENCY)
        }
        BackgroundField::Languages => {
            let rules = t.rules(|x| plain_grant(x, "Language").is_some(), usize::MAX);
            refs(&rules, "Language")
        }
        BackgroundField::Choices => {
            let rules = t.rules(|x| select(x).and_then(choice_row_of).is_some(), usize::MAX);
            let rows: Vec<ChoiceRow> = rules
                .iter()
                .filter_map(|x| select(x).and_then(choice_row_of))
                .collect();
            to_json(&rows)
        }
        BackgroundField::Characteristic(c) => {
            let name = c.name();
            let rules = t.rules(
                |x| select(x).and_then(|s| written_list_of(s, name)).is_some(),
                1,
            );
            let list = rules
                .first()
                .and_then(|x| select(x))
                .and_then(|s| written_list_of(s, name))
                .unwrap_or(WrittenList {
                    count: 1,
                    items: Vec::new(),
                });
            to_json(&list)
        }
        BackgroundField::Equipment => {
            let rules = t.rules(
                |x| package_rule(x, None).is_some_and(|(_, n)| n.is_some()),
                1,
            );
            t.data("equipment_package");
            let entries: Vec<ChildEntry> = rules
                .iter()
                .filter_map(|x| package_rule(x, None))
                .map(|(id, _)| ChildEntry {
                    level: None,
                    child: None,
                    element: Some(RefValue::resolve(id, cx.lookup)),
                })
                .collect();
            to_json(&entries)
        }
        BackgroundField::EquipmentNote => {
            let e = t.r.e;
            t.r.parts_taken.insert("equipment");
            t.raw.part = e.equipment.as_ref().map(to_json);
            let html = e
                .equipment
                .as_ref()
                .and_then(|q| q.description.as_deref())
                .unwrap_or_default();
            to_json(&rich::parse(html))
        }
        BackgroundField::Variants => {
            let rules = t.rules(|x| select(x).and_then(variant_ids_of).is_some(), 1);
            let entries: Vec<ChildEntry> = rules
                .first()
                .and_then(|x| select(x))
                .and_then(variant_ids_of)
                .unwrap_or_default()
                .iter()
                .map(|id| ChildEntry {
                    level: None,
                    child: None,
                    element: Some(RefValue::resolve(id, cx.lookup)),
                })
                .collect();
            to_json(&entries)
        }
    };
    t.done(value)
}

fn select(rule: &Rule) -> Option<&borealis_content::SelectRule> {
    match rule {
        Rule::Select(s) => Some(s),
        _ => None,
    }
}

/// The background's name as written so far (the Name field writes first).
fn own_name(out: &Out) -> String {
    out.element.name.trim().to_string()
}

fn is_2024(out: &Out) -> bool {
    out.element.ruleset == Some(Ruleset::Dnd2024)
}

/// The name a stored rule had (the choice's persistence key), else `fresh`.
fn stored_name(raw: &Raw, at: usize, fresh: impl FnOnce() -> String) -> String {
    let stored = raw.rules.get(at).and_then(|x| match x {
        Rule::Select(s) => Some(s.name.clone()),
        Rule::Custom(c) => c
            .fields
            .get("name")
            .and_then(Value::as_str)
            .map(str::to_string),
        _ => None,
    });
    stored.unwrap_or_else(fresh)
}

/// Where the 2024 mirror of `key` is written: the element had it, or it is a new 2024 background.
fn mirrored(raw: &Raw, memo: Option<&FieldMemo>, key: &str, out: &Out) -> bool {
    raw.data.contains_key(key) || (memo.is_none() && is_2024(out))
}

/// Write one background field. `memo` is what it was read as (absent for a new background).
pub(crate) fn write(
    field: BackgroundField,
    value: &Value,
    memo: Option<&FieldMemo>,
    path: &str,
    out: &mut Out,
    cx: &FormContext<'_>,
) -> Result<(), FormProblem> {
    let raw = Raw::of(memo);
    let slots = memo.map(|m| m.slots.clone()).unwrap_or_default();
    let rank = field.rank();
    if memo.is_some_and(|m| m.value == *value) {
        for (k, v) in &raw.data {
            out.element.data.insert(k.clone(), v.clone());
        }
        if field == BackgroundField::EquipmentNote {
            out.element.equipment = raw.part.as_ref().and_then(decode_raw);
        }
        // unchanged rules keep their field's rank, so new rules of other fields find their neighbours
        out.field_rules(
            raw.rules.iter().cloned().map(|x| (rank, x)),
            &slots.rules,
            path,
        );
        return Ok(());
    }
    let rules: Vec<Rule> = match field {
        BackgroundField::AbilityScores => write_abilities(value, &raw, memo, path, out)?,
        BackgroundField::OriginFeat => write_feat(value, &raw, memo, path, out, cx)?,
        BackgroundField::Skills | BackgroundField::Tools | BackgroundField::Languages => {
            write_grants(field, value, &raw, memo, path, out, cx)?
        }
        BackgroundField::Choices => write_choices(value, &raw, path, out)?,
        BackgroundField::Characteristic(c) => write_list(c, value, &raw, path)?,
        BackgroundField::Equipment => write_equipment(value, &raw, memo, path, out)?,
        BackgroundField::EquipmentNote => {
            let doc: Doc = decode(value, path)?;
            let html = rich::render(&doc.normalized());
            let old: Equipment = raw.part.as_ref().and_then(decode_raw).unwrap_or_default();
            let note = Equipment {
                description: (!html.trim().is_empty()).then_some(html),
                ..old
            };
            out.element.equipment = (note != Equipment::default()).then_some(note);
            Vec::new()
        }
        BackgroundField::Variants => {
            let entries: Vec<ChildEntry> = decode(value, path)?;
            let ids = entries
                .iter()
                .enumerate()
                .map(|(i, e)| child_refs::entry_id(e, out, &format!("{path}.{i}"), "variant"))
                .collect::<Result<Vec<_>, _>>()?;
            if ids.is_empty() {
                Vec::new()
            } else {
                let name = stored_name(&raw, 0, || {
                    format!("Background Variant ({})", own_name(out))
                });
                vec![variants_rule(&name, &ids)]
            }
        }
    };
    out.field_rules(rules.into_iter().map(|x| (rank, x)), &slots.rules, path);
    Ok(())
}

fn write_abilities(
    value: &Value,
    raw: &Raw,
    memo: Option<&FieldMemo>,
    path: &str,
    out: &mut Out,
) -> Result<Vec<Rule>, FormProblem> {
    let words: Vec<String> = decode(value, path)?;
    let keys =
        pool_keys(&words).ok_or_else(|| FormProblem::new(path, "Pick abilities from the list."))?;
    if keys.is_empty() && !is_2024(out) {
        return Ok(Vec::new());
    }
    if keys.len() != 3 {
        return Err(FormProblem::new(path, "Pick three abilities."));
    }
    if mirrored(raw, memo, "ability_scores", out) {
        out.element
            .data
            .insert("ability_scores".into(), json!(keys));
    }
    let name = stored_name(raw, 0, || format!("Ability Scores ({})", own_name(out)));
    Ok(vec![distribution_rule(&name, &keys)])
}

fn write_feat(
    value: &Value,
    raw: &Raw,
    memo: Option<&FieldMemo>,
    path: &str,
    out: &mut Out,
    cx: &FormContext<'_>,
) -> Result<Vec<Rule>, FormProblem> {
    let grant: FeatGrant = decode(value, path)?;
    let Some(feat) = grant
        .feat
        .as_ref()
        .map(|f| f.id.trim())
        .filter(|id| !id.is_empty())
    else {
        return if is_2024(out) {
            Err(FormProblem::new(path, "Choose the origin feat."))
        } else {
            Ok(Vec::new())
        };
    };
    let params: std::collections::BTreeMap<String, String> = grant
        .params
        .iter()
        .map(|(k, v)| (param_key(k), v.trim().to_string()))
        .filter(|(k, v)| !k.is_empty() && !v.is_empty())
        .collect();
    if let Some(loaded) = cx.lookup.loaded(feat) {
        check_params(&loaded.name, &feat_params(loaded), &params, path)?;
    }
    if mirrored(raw, memo, "origin_feat", out) {
        out.element.data.insert("origin_feat".into(), json!(feat));
    }
    if mirrored(raw, memo, "origin_feat_params", out) {
        out.element
            .data
            .insert("origin_feat_params".into(), json!(params));
    }
    Ok(vec![feat_rule(feat, &params)])
}

/// Each fixed choice names one of the feat's choices and one of its options.
fn check_params(
    feat: &str,
    choices: &[(String, Vec<String>)],
    params: &std::collections::BTreeMap<String, String>,
    path: &str,
) -> Result<(), FormProblem> {
    for (key, v) in params {
        let Some((name, options)) = choices.iter().find(|(n, _)| param_key(n) == *key) else {
            return Err(FormProblem::new(
                path,
                format!("{feat} has no choice called “{key}”. Clear it, or choose another feat."),
            ));
        };
        if !options.is_empty() && !options.iter().any(|o| o.eq_ignore_ascii_case(v)) {
            return Err(FormProblem::new(
                path,
                format!(
                    "Pick a {} that {feat} offers: {}.",
                    name.to_lowercase(),
                    words(options)
                ),
            ));
        }
    }
    Ok(())
}

/// "A, B or C".
fn words(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} or {last}", rest.join(", ")),
    }
}

fn write_grants(
    field: BackgroundField,
    value: &Value,
    raw: &Raw,
    memo: Option<&FieldMemo>,
    path: &str,
    out: &mut Out,
    cx: &FormContext<'_>,
) -> Result<Vec<Rule>, FormProblem> {
    let refs: Vec<RefValue> = decode(value, path)?;
    let refs: Vec<&RefValue> = refs.iter().filter(|r| !r.id.trim().is_empty()).collect();
    let new = is_2024(out);
    match field {
        BackgroundField::Skills if new && refs.len() != 2 => {
            return Err(FormProblem::new(path, "Pick two skills."));
        }
        BackgroundField::Tools if new && refs.len() > 1 => {
            return Err(FormProblem::new(
                path,
                "A 2024 background gives one tool proficiency. Remove the others.",
            ));
        }
        _ => {}
    }
    // the mirrors hold names: the picked one's, else the loaded element's
    let shown = |r: &RefValue| {
        let loaded = || cx.lookup.find(r.id.trim()).map(|i| i.name);
        let name = Some(r.name.trim().to_string())
            .filter(|n| !n.is_empty())
            .or_else(loaded)
            .unwrap_or_else(|| r.id.trim().to_string());
        tool_name(&name)
    };
    match field {
        BackgroundField::Skills if !refs.is_empty() && mirrored(raw, memo, "skills", out) => {
            let names: Vec<String> = refs.iter().map(|r| shown(r)).collect();
            out.element.data.insert("skills".into(), json!(names));
        }
        BackgroundField::Tools if !refs.is_empty() && mirrored(raw, memo, "tool", out) => {
            out.element
                .data
                .insert("tool".into(), json!(shown(refs[0])));
        }
        _ => {}
    }
    let ty = if field == BackgroundField::Languages {
        "Language"
    } else {
        PROFICIENCY
    };
    Ok(refs.iter().map(|r| grant(r.id.trim(), ty)).collect())
}

/// A proficiency's name as the 2024 mirrors write it: "Tool Proficiency (Thieves’ tools)" is "Thieves’ tools".
fn tool_name(name: &str) -> String {
    name.strip_prefix("Tool Proficiency (")
        .and_then(|n| n.strip_suffix(')'))
        .unwrap_or(name)
        .to_string()
}

fn write_choices(
    value: &Value,
    raw: &Raw,
    path: &str,
    out: &Out,
) -> Result<Vec<Rule>, FormProblem> {
    let rows: Vec<ChoiceRow> = decode(value, path)?;
    // the names rows have (stored ones are the characters' choices' keys) come first; new rows get free ones
    let mut names: Vec<String> = Vec::new();
    for (i, row) in rows.iter().enumerate() {
        let n = row.name.trim();
        if n.is_empty() {
            continue;
        }
        if names.iter().any(|x| x == n) {
            return Err(FormProblem::new(
                format!("{path}.{i}"),
                "Two choices have the same name. Characters' choices are kept by it: remove one.",
            ));
        }
        names.push(n.to_string());
    }
    let mut rules = Vec::new();
    for (i, row) in rows.iter().enumerate() {
        let at = format!("{path}.{i}");
        let ty = choice_type(&row.kind)
            .ok_or_else(|| FormProblem::new(&at, "Choose languages or tools."))?;
        if row.count == 0 || row.count > MAX_CHOICE {
            return Err(FormProblem::new(
                &at,
                format!("The character chooses from 1 to {MAX_CHOICE}."),
            ));
        }
        let supports = row
            .from
            .as_ref()
            .map(|f| f.text::<TagFilterLanguage>(&()))
            .unwrap_or_default();
        if supports.trim().is_empty() {
            return Err(FormProblem::new(
                &at,
                "Say what the character chooses from: add a tag, like Standard or Gaming Set.",
            ));
        }
        // a stored row keeps its name exactly (its spaces too: it is the persistence key)
        let stored = raw.rules.iter().find_map(|x| match x {
            Rule::Select(s) if s.name.trim() == row.name.trim() => Some(s.name.clone()),
            _ => None,
        });
        let name = match stored {
            Some(n) if !row.name.trim().is_empty() => n,
            _ if !row.name.trim().is_empty() => row.name.trim().to_string(),
            _ => {
                let word = if ty == "Language" {
                    "Language"
                } else {
                    "Tool Proficiency"
                };
                let n = unique(&format!("{word} ({})", own_name(out)), &names);
                names.push(n.clone());
                n
            }
        };
        rules.push(choice_rule(ty, &name, row.count, supports.trim()));
    }
    Ok(rules)
}

/// `base`, or `base 2`, `base 3`... when taken.
fn unique(base: &str, taken: &[String]) -> String {
    (1..)
        .map(|n| {
            if n == 1 {
                base.to_string()
            } else {
                format!("{base} {n}")
            }
        })
        .find(|n| !taken.contains(n))
        .unwrap_or_default()
}

fn write_list(
    c: Characteristic,
    value: &Value,
    raw: &Raw,
    path: &str,
) -> Result<Vec<Rule>, FormProblem> {
    let list: WrittenList = decode(value, path)?;
    let items: Vec<String> = list
        .items
        .iter()
        .map(|t| rich::normalize_plain_text(t))
        .filter(|t| !t.is_empty())
        .collect();
    if items.is_empty() {
        return Ok(Vec::new());
    }
    if list.count == 0 || list.count > MAX_PICKS {
        return Err(FormProblem::new(
            path,
            format!("The character picks from 1 to {MAX_PICKS}."),
        ));
    }
    if list.count as usize > items.len() {
        return Err(FormProblem::new(
            path,
            format!(
                "Write at least {} {}: the character picks {}.",
                list.count,
                c.plural(),
                list.count
            ),
        ));
    }
    let name = stored_name(raw, 0, || c.name().to_string());
    Ok(vec![list_rule(&name, list.count, &items)])
}

fn write_equipment(
    value: &Value,
    raw: &Raw,
    memo: Option<&FieldMemo>,
    path: &str,
    out: &mut Out,
) -> Result<Vec<Rule>, FormProblem> {
    let entries: Vec<ChildEntry> = decode(value, path)?;
    let Some(entry) = entries.first() else {
        return Ok(Vec::new());
    };
    let id = child_refs::entry_id(entry, out, &format!("{path}.0"), "package")?;
    let name = stored_name(raw, 0, || format!("Starting Equipment ({})", own_name(out)));
    if mirrored(raw, memo, "equipment_package", out) {
        out.element
            .data
            .insert("equipment_package".into(), json!(id));
    }
    let mut fields = Map::new();
    fields.insert("ref".into(), json!(id));
    fields.insert("name".into(), json!(name));
    Ok(vec![Rule::Custom(CustomRule {
        kind: PACKAGE_KIND.into(),
        fields,
    })])
}

#[cfg(test)]
#[path = "background_tests.rs"]
mod tests;
