//! The class form's codecs (spec 4.5), in the shape the bundled SRD classes store them (2014 and 2024):
//!
//! * **Hit die**: setter `hd` (+ the 2024 mirrors `data.hit_die`, `data.fixed_hp_per_level`).
//! * **Primary abilities**: `data.primary_abilities` (the summary line is the shared setter `short`).
//! * **Saving throws**, **armor**, **weapon** and **other proficiencies**: `grant`s of `Proficiency` elements with the
//!   requirement "first class only" (`!<multiclass id>`, exactly the SRD pattern) (+ `data.saving_throws`).
//! * **Skills**: the `select` of type `Proficiency` named `"Skill Proficiency (<Class>)"`, `number` N, `supports` the
//!   id range of the skills (`ID_A|ID_B`), first class only. The 2014 SRD's tag form (`Skill,Fighter`) is kept as is.
//! * **Class table**: the counters per level, `data.progression` (the engine reads them for `table` maximums).
//! * **Starting equipment**: the `equipment_package` rule (first class only) and its package, a child of the class.
//! * **Multiclassing** (`class_mc.rs`): the `multiclass` section.
//!
//! A field takes a rule only when the rule is exactly what the field writes for some value; anything else stays a
//! block. Unchanged fields write their stored bytes back; edited ones write canonically.

use super::super::child_refs::{self, ChildEntry};
use super::super::value::RefValue;
use super::super::write::Out;
use super::super::{FieldMemo, FormContext, FormProblem, Slots};
use super::{class_mc, decode, edited_setter, to_json, Reader};
use crate::expr_build::Ability;
use borealis_content::{CustomRule, Element, GrantRule, Rule, SelectRule, Setter};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

/// The proficiencies a class grants, by kind (read by id: armor and weapon proficiencies have their own prefixes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProficiencyGroup {
    /// `ID_PROFICIENCY_ARMOR_*`.
    Armor,
    /// `ID_PROFICIENCY_WEAPON_*`.
    Weapons,
    /// Tools and every other proficiency.
    Other,
}

/// A class field (`Codec::Class`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClassField {
    /// Setter `hd` (+ `data.hit_die`, `data.fixed_hp_per_level`).
    HitDie,
    /// `data.primary_abilities`.
    PrimaryAbilities,
    /// Saving throw grants, first class only (+ `data.saving_throws`).
    SavingThrows,
    /// Proficiency grants of a group, first class only.
    Proficiencies(ProficiencyGroup),
    /// The skill choice, first class only.
    Skills,
    /// `data.progression`: the class table's counters.
    Table,
    /// The `equipment_package` rule, first class only.
    Equipment,
    /// The `multiclass` section: on or off, and what no other field edits.
    Multiclass,
    /// `multiclass.requirements` (+ `data.multiclass.requirements`).
    McRequirements,
    /// `multiclass.prerequisite` (+ `data.multiclass.prerequisite`).
    McPrerequisite,
    /// `multiclass.rules`: the proficiency grants (and the internal multiclass grant).
    McProficiencies,
    /// `multiclass.setters` `multiclass proficiencies`.
    McProficiencyText,
}

const HD: &str = "hd";
const SAVE_PREFIX: &str = "ID_PROFICIENCY_SAVINGTHROW_";
const PROFICIENCY: &str = "Proficiency";
const SKILL_LABEL: &str = "Skill Proficiency";
const PACKAGE_KIND: &str = "equipment_package";
const HIT_DICE: [&str; 4] = ["d6", "d8", "d10", "d12"];

/// Order of a new class's rules, as the SRD writes them: 2024 saves, skills, weapons, armor, other; 2014 armor,
/// weapons, other, saves, skills; then equipment and the features.
fn rank(field: ClassField, out: &Out) -> u8 {
    let new = out.element.ruleset == Some(borealis_content::Ruleset::Dnd2024);
    let order: &[ClassField] = if new {
        &[
            ClassField::SavingThrows,
            ClassField::Skills,
            ClassField::Proficiencies(ProficiencyGroup::Weapons),
            ClassField::Proficiencies(ProficiencyGroup::Armor),
            ClassField::Proficiencies(ProficiencyGroup::Other),
        ]
    } else {
        &[
            ClassField::Proficiencies(ProficiencyGroup::Armor),
            ClassField::Proficiencies(ProficiencyGroup::Weapons),
            ClassField::Proficiencies(ProficiencyGroup::Other),
            ClassField::SavingThrows,
            ClassField::Skills,
        ]
    };
    order
        .iter()
        .position(|f| *f == field)
        .map_or(RANK_EQUIPMENT, |i| {
            u8::try_from(i + 1).unwrap_or(RANK_EQUIPMENT)
        })
}
const RANK_EQUIPMENT: u8 = 6;

/// The skill choice of a class (`HomebrewSkillChoice`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillChoice {
    /// How many the character chooses.
    pub count: u32,
    /// The skills to choose from.
    #[serde(default)]
    pub skills: Vec<RefValue>,
    /// A tag filter kept from the original instead of a list (`Skill,Fighter`); empty for a list.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tagged: String,
}

/// One counter column of the class table (`HomebrewTableColumn`): `data.progression.<key>`, a value per level.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableColumn {
    /// The key a `table` maximum names (`rage_damage`); written from the label for a new column.
    #[serde(default)]
    pub key: String,
    /// Column heading.
    #[serde(default)]
    pub label: String,
    /// Level 1 first: numbers or text (dice like `1d6`).
    #[serde(default)]
    pub values: Vec<Value>,
}

impl ClassField {
    /// The model locations, as the generated-data view shows them.
    pub(crate) fn bindings(self) -> Vec<String> {
        let v: &[&str] = match self {
            Self::HitDie => &["setter:hd", "data.hit_die", "data.fixed_hp_per_level"],
            Self::PrimaryAbilities => &["data.primary_abilities"],
            Self::SavingThrows => &[
                "rules:grant ID_PROFICIENCY_SAVINGTHROW_*",
                "data.saving_throws",
            ],
            Self::Proficiencies(ProficiencyGroup::Armor) => &["rules:grant ID_PROFICIENCY_ARMOR_*"],
            Self::Proficiencies(ProficiencyGroup::Weapons) => {
                &["rules:grant ID_PROFICIENCY_WEAPON_*"]
            }
            Self::Proficiencies(ProficiencyGroup::Other) => &["rules:grant Proficiency"],
            Self::Skills => &["rules:select Proficiency (Skill Proficiency)"],
            Self::Table => &["data.progression"],
            Self::Equipment => &["rules:equipment_package"],
            Self::Multiclass => &["multiclass"],
            Self::McRequirements => &["multiclass.requirements", "data.multiclass.requirements"],
            Self::McPrerequisite => &["multiclass.prerequisite", "data.multiclass.prerequisite"],
            Self::McProficiencies => &["multiclass.rules"],
            Self::McProficiencyText => &["multiclass.setters:multiclass proficiencies"],
        };
        v.iter().map(|s| (*s).to_string()).collect()
    }

    /// The setter names it takes, lower case.
    pub(crate) fn setter_names(self) -> Vec<&'static str> {
        match self {
            Self::HitDie => vec![HD],
            _ => Vec::new(),
        }
    }

    /// The `data` keys it takes.
    pub(crate) fn data_keys(self) -> Vec<&'static str> {
        match self {
            Self::HitDie => vec!["hit_die", "fixed_hp_per_level"],
            Self::PrimaryAbilities => vec!["primary_abilities"],
            Self::SavingThrows => vec!["saving_throws"],
            Self::Table => vec!["progression"],
            Self::McRequirements => vec!["multiclass"],
            _ => Vec::new(),
        }
    }

    /// A children field: the starting-equipment package (not levelled).
    pub(crate) fn child_list(self) -> Option<bool> {
        (self == Self::Equipment).then_some(false)
    }
}

/// What a field took, in the order of its slots.
#[derive(Debug, Default, Serialize, Deserialize)]
pub(super) struct Raw {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(super) setters: Vec<Setter>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub(super) data: BTreeMap<String, Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(super) rules: Vec<Rule>,
    /// The "first class only" requirement the rules were read with.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) req: Option<String>,
    /// A part of the multiclass section (`class_mc.rs`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) part: Option<Value>,
}

impl Raw {
    pub(super) fn of(memo: Option<&FieldMemo>) -> Self {
        memo.and_then(|m| serde_json::from_value(m.raw.clone()).ok())
            .unwrap_or_default()
    }
}

/// "First class only" as the stored element writes it: `!<multiclass id>` (none without a multiclass section).
pub(crate) fn first_class_only(e: &Element) -> Option<String> {
    e.multiclass.as_ref().map(|m| format!("!{}", m.id))
}

fn proficiency_grant<'a>(rule: &'a Rule, req: Option<&str>) -> Option<&'a str> {
    let Rule::Grant(g) = rule else { return None };
    let plain = g.ty.as_deref() == Some(PROFICIENCY)
        && g.requirements.as_deref() == req
        && g.level.is_none()
        && g.spellcasting.is_none()
        && g.prepared.is_none()
        && g.setters.is_empty()
        && g.extra.is_empty();
    plain.then_some(g.id.as_str())
}

fn save_ability(id: &str) -> Option<Ability> {
    id.strip_prefix(SAVE_PREFIX)
        .and_then(Ability::from_full_name)
}

fn in_group(id: &str, group: ProficiencyGroup) -> bool {
    match group {
        ProficiencyGroup::Armor => id.starts_with("ID_PROFICIENCY_ARMOR_"),
        ProficiencyGroup::Weapons => id.starts_with("ID_PROFICIENCY_WEAPON_"),
        ProficiencyGroup::Other => save_ability(id).is_none(),
    }
}

/// "Strength" for an ability.
pub(super) fn ability_word(a: Ability) -> String {
    let full = a.full();
    let mut w = full.to_string();
    w[..1].make_ascii_uppercase();
    w
}

fn grant(id: &str, req: Option<&str>) -> Rule {
    Rule::Grant(GrantRule {
        id: id.to_string(),
        ty: Some(PROFICIENCY.into()),
        requirements: req.map(str::to_string),
        ..GrantRule::default()
    })
}

/// Reading one class field: takes parts of the element and remembers them.
pub(super) struct Take<'r, 'e> {
    pub(super) r: &'r mut Reader<'e>,
    pub(super) raw: Raw,
    pub(super) slots: Slots,
}

impl<'e> Take<'_, 'e> {
    fn setter(&mut self, name: &str) -> Option<&'e Setter> {
        let (i, s) = self.r.take_setter(name)?;
        self.raw.setters.push(s.clone());
        self.slots.setters.push(i);
        Some(s)
    }

    fn data(&mut self, key: &str) -> Option<&'e Value> {
        let v = self.r.take_data(key)?;
        self.raw.data.insert(key.to_string(), v.clone());
        Some(v)
    }

    /// Take every untaken rule `want` accepts, in order (at most `limit`).
    fn rules(&mut self, want: impl Fn(&Rule) -> bool, limit: usize) -> Vec<&'e Rule> {
        let e = self.r.e;
        let mut out = Vec::new();
        for (i, rule) in e.rules.iter().enumerate() {
            if out.len() < limit && !self.r.rules_taken[i] && want(rule) {
                self.r.rules_taken[i] = true;
                self.raw.rules.push(rule.clone());
                self.slots.rules.push(i);
                out.push(rule);
            }
        }
        out
    }

    pub(super) fn done(self, value: Value) -> FieldMemo {
        FieldMemo {
            value,
            raw: to_json(&self.raw),
            slots: self.slots,
        }
    }
}

fn number_of(v: Option<&Value>) -> Option<i64> {
    v.and_then(Value::as_i64)
}

/// Read one class field.
pub(crate) fn read(field: ClassField, r: &mut Reader<'_>, cx: &FormContext<'_>) -> FieldMemo {
    let e = r.e;
    let req = first_class_only(e);
    let mut t = Take {
        r,
        raw: Raw {
            req: req.clone(),
            ..Raw::default()
        },
        slots: Slots::default(),
    };
    let value = match field {
        ClassField::HitDie => {
            let s = t.setter(HD);
            let n = t.data("hit_die");
            t.data("fixed_hp_per_level");
            json!(s.map_or_else(
                || number_of(n).map(|n| format!("d{n}")).unwrap_or_default(),
                |s| s.value.trim().to_lowercase()
            ))
        }
        ClassField::PrimaryAbilities => {
            let d = t.data("primary_abilities");
            let words: Vec<String> = d
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(|a| Ability::from_key(a).map_or_else(|| a.to_string(), ability_word))
                .collect();
            json!(words)
        }
        ClassField::SavingThrows => {
            let rules = t.rules(
                |x| {
                    proficiency_grant(x, req.as_deref())
                        .and_then(save_ability)
                        .is_some()
                },
                usize::MAX,
            );
            t.data("saving_throws");
            let words: Vec<String> = rules
                .iter()
                .filter_map(|x| proficiency_grant(x, req.as_deref()))
                .filter_map(save_ability)
                .map(ability_word)
                .collect();
            json!(words)
        }
        ClassField::Proficiencies(group) => {
            let rules = t.rules(
                |x| proficiency_grant(x, req.as_deref()).is_some_and(|id| in_group(id, group)),
                usize::MAX,
            );
            let refs: Vec<RefValue> = rules
                .iter()
                .filter_map(|x| proficiency_grant(x, req.as_deref()))
                .map(|id| RefValue::resolve(id, cx.lookup))
                .collect();
            to_json(&refs)
        }
        ClassField::Skills => {
            let rules = t.rules(|x| skill_select(x, req.as_deref()).is_some(), 1);
            let choice = rules
                .first()
                .and_then(|x| skill_select(x, req.as_deref()))
                .map(|s| skill_choice(s, cx))
                .unwrap_or_default();
            to_json(&choice)
        }
        ClassField::Table => {
            let tabular = e
                .data
                .get("progression")
                .and_then(Value::as_object)
                .is_some_and(|m| m.values().all(Value::is_array));
            let columns = tabular
                .then(|| t.data("progression"))
                .flatten()
                .and_then(Value::as_object)
                .map(|m| {
                    m.iter()
                        .map(|(k, v)| TableColumn {
                            key: k.clone(),
                            label: column_label(k),
                            values: v.as_array().cloned().unwrap_or_default(),
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            to_json(&columns)
        }
        ClassField::Equipment => {
            let rules = t.rules(|x| package_rule(x, req.as_deref()).is_some(), 1);
            let entries: Vec<ChildEntry> = rules
                .iter()
                .filter_map(|x| package_rule(x, req.as_deref()))
                .map(|(id, _)| ChildEntry {
                    level: None,
                    child: None,
                    element: Some(RefValue::resolve(id, cx.lookup)),
                })
                .collect();
            to_json(&entries)
        }
        ClassField::Multiclass
        | ClassField::McRequirements
        | ClassField::McPrerequisite
        | ClassField::McProficiencies
        | ClassField::McProficiencyText => return class_mc::read(field, t, cx),
    };
    t.done(value)
}

/// A column heading from its key: `second_wind` is "Second wind".
fn column_label(key: &str) -> String {
    let words = key.replace(['_', '-'], " ");
    let mut c = words.chars();
    c.next()
        .map(|f| f.to_uppercase().chain(c).collect())
        .unwrap_or_default()
}

/// The key a new column gets from its label ("Rage damage" is `rage_damage`).
fn column_key(label: &str) -> String {
    let mut out = String::new();
    for ch in label.trim().to_lowercase().chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
        } else if !out.ends_with('_') && !out.is_empty() {
            out.push('_');
        }
    }
    out.trim_end_matches('_').to_string()
}

fn skill_select<'a>(rule: &'a Rule, req: Option<&str>) -> Option<&'a SelectRule> {
    let Rule::Select(s) = rule else { return None };
    let bare = SelectRule {
        ty: s.ty.clone(),
        name: s.name.clone(),
        number: s.number,
        requirements: s.requirements.clone(),
        supports: s.supports.clone(),
        ..SelectRule::default()
    };
    let plain = *s == bare
        && s.ty == PROFICIENCY
        && s.requirements.as_deref() == req
        && s.name.starts_with(SKILL_LABEL)
        && s.supports.as_deref().is_some_and(|t| !t.trim().is_empty());
    plain.then_some(s)
}

fn is_id_range(text: &str) -> bool {
    text.split('|').all(|t| t.trim().starts_with("ID_"))
}

fn skill_choice(s: &SelectRule, cx: &FormContext<'_>) -> SkillChoice {
    let supports = s.supports.clone().unwrap_or_default();
    let count = s.number.unwrap_or(1);
    if is_id_range(&supports) {
        SkillChoice {
            count,
            skills: supports
                .split('|')
                .map(|id| RefValue::resolve(id.trim(), cx.lookup))
                .collect(),
            tagged: String::new(),
        }
    } else {
        SkillChoice {
            count,
            skills: Vec::new(),
            tagged: supports,
        }
    }
}

fn package_rule<'a>(rule: &'a Rule, req: Option<&str>) -> Option<(&'a str, Option<&'a str>)> {
    let Rule::Custom(c) = rule else { return None };
    if c.kind != PACKAGE_KIND {
        return None;
    }
    let known = c
        .fields
        .keys()
        .all(|k| matches!(k.as_str(), "ref" | "name" | "requirements"));
    let id = c.fields.get("ref").and_then(Value::as_str)?;
    let name = c.fields.get("name").map(Value::as_str);
    let r = c.fields.get("requirements").map(Value::as_str);
    let same_req = match r {
        None => req.is_none(),
        Some(t) => t == req,
    };
    (known && same_req && name.is_none_or(|n| n.is_some())).then(|| (id, name.flatten()))
}

/// The class's name as written so far (the Name field writes first).
pub(super) fn class_name(out: &Out) -> String {
    out.element.name.trim().to_string()
}

/// Write one class field. `memo` is what it was read as (absent for a new class).
pub(crate) fn write(
    field: ClassField,
    value: &Value,
    memo: Option<&FieldMemo>,
    path: &str,
    out: &mut Out,
    cx: &FormContext<'_>,
) -> Result<(), FormProblem> {
    let raw = Raw::of(memo);
    // rules read with "first class only" change when the class gets or loses its multiclass section
    let same_gate = raw.req.is_some() == out.first_class_only.is_some();
    let unchanged = memo.is_some_and(|m| m.value == *value) && same_gate;
    if matches!(
        field,
        ClassField::Multiclass
            | ClassField::McRequirements
            | ClassField::McPrerequisite
            | ClassField::McProficiencies
            | ClassField::McProficiencyText
    ) {
        return class_mc::write(field, value, memo, unchanged, path, out, cx);
    }
    let slots = memo.map(|m| m.slots.clone()).unwrap_or_default();
    if unchanged {
        out.setters(raw.setters.iter().cloned(), &slots.setters);
        for (k, v) in &raw.data {
            out.element.data.insert(k.clone(), v.clone());
        }
        out.field_rules(
            raw.rules.iter().cloned().map(|x| (0, x)),
            &slots.rules,
            path,
        );
        return Ok(());
    }
    let req = out.first_class_only.clone();
    let req = req.as_deref();
    match field {
        ClassField::HitDie => {
            let t = decode::<String>(value, path)?.trim().to_lowercase();
            if t.is_empty() {
                return Ok(());
            }
            if !HIT_DICE.contains(&t.as_str()) {
                return Err(FormProblem::new(
                    path,
                    "Pick a hit die: d6, d8, d10 or d12.",
                ));
            }
            let n: i64 = t[1..].parse().unwrap_or(0);
            out.setters(
                Some(edited_setter(
                    HD,
                    t.clone(),
                    raw.setters.first().map(to_json).as_ref(),
                )),
                &slots.setters,
            );
            if raw.data.contains_key("hit_die") {
                out.element.data.insert("hit_die".into(), json!(n));
            }
            if raw.data.contains_key("fixed_hp_per_level") {
                out.element
                    .data
                    .insert("fixed_hp_per_level".into(), json!(n / 2 + 1));
            }
        }
        ClassField::PrimaryAbilities => {
            let words: Vec<String> = decode(value, path)?;
            let keys = abilities(&words, path)?;
            if !keys.is_empty() {
                out.element
                    .data
                    .insert("primary_abilities".into(), json!(keys));
            }
        }
        ClassField::SavingThrows => {
            let words: Vec<String> = decode(value, path)?;
            let keys = abilities(&words, path)?;
            let rules = keys.iter().map(|k| {
                (
                    rank(field, out),
                    grant(&format!("{SAVE_PREFIX}{}", k.to_uppercase()), req),
                )
            });
            out.field_rules(rules.collect::<Vec<_>>(), &slots.rules, path);
            if raw.data.contains_key("saving_throws") {
                out.element.data.insert("saving_throws".into(), json!(keys));
            }
        }
        ClassField::Proficiencies(_) => {
            let refs: Vec<RefValue> = decode(value, path)?;
            let rank = rank(field, out);
            let rules: Vec<(u8, Rule)> = refs
                .iter()
                .filter(|r| !r.id.trim().is_empty())
                .map(|r| (rank, grant(r.id.trim(), req)))
                .collect();
            out.field_rules(rules, &slots.rules, path);
        }
        ClassField::Skills => write_skills(value, &raw, &slots, path, out, req)?,
        ClassField::Table => write_table(value, path, out)?,
        ClassField::Equipment => {
            let entries: Vec<ChildEntry> = decode(value, path)?;
            let mut rules = Vec::new();
            for (i, entry) in entries.iter().enumerate().take(1) {
                let id = child_refs::entry_id(entry, out, &format!("{path}.{i}"), "package")?;
                let stored_name = raw
                    .rules
                    .first()
                    .and_then(|x| package_rule(x, raw.req.as_deref()))
                    .and_then(|(_, n)| n);
                let name = stored_name.map_or_else(
                    || format!("Starting Equipment ({})", class_name(out)),
                    str::to_string,
                );
                let mut fields = Map::new();
                if let Some(r) = req {
                    fields.insert("requirements".into(), json!(r));
                }
                fields.insert("ref".into(), json!(id));
                fields.insert("name".into(), json!(name));
                rules.push((
                    RANK_EQUIPMENT,
                    Rule::Custom(CustomRule {
                        kind: PACKAGE_KIND.into(),
                        fields,
                    }),
                ));
            }
            out.field_rules(rules, &slots.rules, path);
        }
        _ => unreachable!("multiclass fields are written in class_mc"),
    }
    Ok(())
}

/// Ability names ("Strength") as the `data` keys write them (`strength`), each once.
fn abilities(words: &[String], path: &str) -> Result<Vec<String>, FormProblem> {
    let mut out: Vec<String> = Vec::new();
    for w in words {
        let a = Ability::from_key(w)
            .ok_or_else(|| FormProblem::new(path, "Pick abilities from the list."))?;
        let k = a.full().to_string();
        if !out.contains(&k) {
            out.push(k);
        }
    }
    Ok(out)
}

fn write_skills(
    value: &Value,
    raw: &Raw,
    slots: &Slots,
    path: &str,
    out: &mut Out,
    req: Option<&str>,
) -> Result<(), FormProblem> {
    let choice: SkillChoice = decode(value, path)?;
    let tagged = choice.tagged.trim();
    let ids: Vec<&str> = choice
        .skills
        .iter()
        .map(|s| s.id.trim())
        .filter(|s| !s.is_empty())
        .collect();
    if tagged.is_empty() && ids.is_empty() {
        return if choice.count == 0 {
            Ok(())
        } else {
            Err(FormProblem::new(
                path,
                "Pick the skills the character chooses from.",
            ))
        };
    }
    if choice.count == 0 || choice.count > 18 {
        return Err(FormProblem::new(
            path,
            "The character chooses from 1 to 18 skills.",
        ));
    }
    if tagged.is_empty() && (choice.count as usize) > ids.len() {
        return Err(FormProblem::new(
            path,
            "Offer at least as many skills as the character chooses.",
        ));
    }
    let stored_name = raw.rules.first().and_then(|x| match x {
        Rule::Select(s) => Some(s.name.clone()),
        _ => None,
    });
    let name = stored_name.unwrap_or_else(|| format!("{SKILL_LABEL} ({})", class_name(out)));
    let rule = Rule::Select(SelectRule {
        ty: PROFICIENCY.into(),
        name,
        number: Some(choice.count),
        requirements: req.map(str::to_string),
        supports: Some(if tagged.is_empty() {
            ids.join("|")
        } else {
            tagged.to_string()
        }),
        ..SelectRule::default()
    });
    out.field_rules(
        vec![(rank(ClassField::Skills, out), rule)],
        &slots.rules,
        path,
    );
    Ok(())
}

fn write_table(value: &Value, path: &str, out: &mut Out) -> Result<(), FormProblem> {
    let columns: Vec<TableColumn> = decode(value, path)?;
    let mut map = Map::new();
    for (i, c) in columns.iter().enumerate() {
        let at = format!("{path}.{i}");
        let key = if c.key.trim().is_empty() {
            column_key(&c.label)
        } else {
            c.key.trim().to_string()
        };
        if key.is_empty() {
            return Err(FormProblem::new(at, "Give the column a heading."));
        }
        if map.contains_key(&key) {
            return Err(FormProblem::new(
                at,
                "Two columns have the same heading. Rename one.",
            ));
        }
        if c.values.len() > 20 {
            return Err(FormProblem::new(at, "A class table has 20 levels."));
        }
        let mut values: Vec<Value> = c.values.clone();
        for v in &values {
            if !(v.is_number() || v.is_string() || v.is_null()) {
                return Err(FormProblem::new(
                    at,
                    "Write a number or dice, like 2 or 1d6, in each level.",
                ));
            }
        }
        values.resize(20, json!(0));
        map.insert(key, Value::Array(values));
    }
    if !map.is_empty() {
        out.element
            .data
            .insert("progression".into(), Value::Object(map));
    }
    Ok(())
}

/// The ability names a class's saving throws field takes (for the UI's closed chips).
pub(crate) const ABILITY_CHOICES: &[(&str, &str)] = &[
    ("Strength", "Strength"),
    ("Dexterity", "Dexterity"),
    ("Constitution", "Constitution"),
    ("Intelligence", "Intelligence"),
    ("Wisdom", "Wisdom"),
    ("Charisma", "Charisma"),
];

#[cfg(test)]
#[path = "class_tests.rs"]
mod tests;
