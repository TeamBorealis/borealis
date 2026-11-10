//! Item fields (spec 4.9): Item, Weapon, Armor, Magic Item, Item Pack and Equipment Package.
//!
//! Each field takes every model location that holds it (spec 3.3): a weapon's category is its Weapon Category tag in
//! `supports` (what the engine reads) and, on a 2024 weapon, `data.weapon_category` and `data.melee`; an armor's base
//! AC is its `stat ac:armored:armor` rule (or a shield's `ac:shield`) and `data.base_ac`. What a field took is
//! remembered as [`Raw`] with its positions ([`Slots`]): an unchanged field writes it back where it was, an edited
//! field writes canonically, and 2024 mirror keys only when the element had them.
//!
//! Rules a field takes (base AC, stealth disadvantage, a magic armor's enhancement) are not blocks; they go back to
//! their stored positions among the blocks ([`Out::field_rules`]).

use super::codec::{decode, to_json, Reader};
use super::item_value::{ContentsRow, PackageOption};
use super::value::RefValue;
use super::write::Out;
use super::{FieldMemo, FormContext, FormProblem, Lookup, Slots};
use crate::check::is_dice;
use crate::expr_build::tag_filter::{TagFilter, TagFilterLanguage};
use crate::expr_build::ExprValue;
use borealis_content::{Element, Extract, GrantRule, Rule, Setter, StatRule};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;

/// One item field (a [`super::codec::Codec::Item`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemField {
    /// Setter `cost` (a number).
    Cost,
    /// Attribute `currency` of the `cost` setter (gold when it has none).
    Currency,
    /// Setter `weight` written from a number of pounds (`"2 lb."`, attribute `lb`), or a word ("Varies").
    Weight,
    /// An attribute of the setter another field writes (`type` + `addition`), with its 2024 mirror.
    SetterAttr {
        /// The setter.
        setter: &'static str,
        /// The attribute.
        attr: &'static str,
        /// The 2024 `data` mirror.
        mirror: Option<&'static str>,
        /// What to fill in first, when the setter isn't there.
        needs: &'static str,
    },
    /// Setter `rarity` (+ `data.rarity_text`, `data.rarities`).
    Rarity,
    /// A magic item's setter `type` (+ `data.item_type`).
    MagicType,
    /// Setter `attunement` (+ `data.requires_attunement`).
    Attunement,
    /// Setter `strength` (+ `data.strength_requirement`).
    Strength,
    /// Setter `versatile` (+ `data.versatile_damage`).
    Versatile,
    /// Setter `range` (+ `data.range`).
    Range,
    /// Setter `damage` (+ `data.damage.dice`).
    Damage,
    /// The `damage` setter's `type`, the Damage Type tag (+ `data.damage.type`).
    DamageType,
    /// Setter `proficiency`: the Proficiency element.
    Proficiency,
    /// The Weapon Category tag (+ `data.weapon_category`, `data.melee`).
    WeaponCategory,
    /// The Weapon Property tags (+ `data.properties`).
    WeaponProperties,
    /// The Weapon Group tags.
    WeaponGroups,
    /// The Mastery Property tags, setter `mastery` (+ `data.mastery`, `data.mastery_id`, `data.mastery_ids`).
    Mastery,
    /// Setter `armor`, the Armor Group tag (+ `data.armor_category`, `data.dex_modifier`, `data.dex_cap`).
    ArmorType,
    /// Rule `stat ac:armored:armor` (a shield's `ac:shield`) (+ `data.base_ac`).
    BaseAc,
    /// Rule `grant ID_INTERNAL_GRANTS_STEALTH_DISADVANTAGE`, setter `stealth` (+ `data.stealth_disadvantage`).
    Stealth,
    /// Setter `enhancement`, and a magic armor's or shield's AC rule of the same value.
    Enhancement,
    /// A magic item's setter `weapon` or `armor`: the base items it applies to, as a supports expression.
    AppliesTo(&'static str),
    /// `extract.items`: an item pack's contents.
    Contents,
    /// `data.owner`: the class or background an Equipment Package belongs to.
    PackageOwner,
    /// `data.options`: an Equipment Package's lettered options.
    PackageOptions,
}

const STEALTH_GRANT: &str = "ID_INTERNAL_GRANTS_STEALTH_DISADVANTAGE";
const STEALTH_GRANT_TYPE: &str = "Grants";
const AC_ARMOR: &str = "ac:armored:armor";
const AC_SHIELD: &str = "ac:shield";
const AC_ENHANCEMENT: &str = "ac:armored:enhancement";
const COINS: [&str; 5] = ["cp", "sp", "ep", "gp", "pp"];
const ARMOR_TYPES: [&str; 4] = ["Light", "Medium", "Heavy", "Shield"];
/// The Range property is the engine's marker for a range; the 2024 `data.properties` mirror leaves it out.
const RANGE_PROPERTY: &str = "ID_INTERNAL_WEAPON_PROPERTY_RANGE";

/// Order of new field rules (before the blocks): the 2014/2024 SRD order.
const RANK_STEALTH: u8 = 0;
const RANK_AC: u8 = 1;
const RANK_ENHANCEMENT: u8 = 2;

/// Tagged references: the element type and the id prefix used when the tag isn't loaded.
#[derive(Clone, Copy)]
struct TagKind {
    ty: &'static str,
    prefix: &'static str,
}

const WEAPON_CATEGORY: TagKind = TagKind {
    ty: "Weapon Category",
    prefix: "ID_INTERNAL_WEAPON_CATEGORY_",
};
const WEAPON_PROPERTY: TagKind = TagKind {
    ty: "Weapon Property",
    prefix: "ID_INTERNAL_WEAPON_PROPERTY_",
};
const WEAPON_GROUP: TagKind = TagKind {
    ty: "Weapon Group",
    prefix: "ID_INTERNAL_WEAPON_GROUP_",
};
const DAMAGE_TYPE: TagKind = TagKind {
    ty: "Damage Type",
    prefix: "ID_INTERNAL_DAMAGE_TYPE_",
};
const MASTERY: TagKind = TagKind {
    ty: "Mastery Property",
    prefix: "ID_2024_MASTERY_",
};
const ARMOR_GROUP: TagKind = TagKind {
    ty: "Armor Group",
    prefix: "ID_INTERNAL_ARMOR_GROUP_",
};

impl TagKind {
    fn holds(self, tag: &str, lookup: &dyn Lookup) -> bool {
        lookup
            .find(tag)
            .map_or_else(|| tag.starts_with(self.prefix), |i| i.ty == self.ty)
    }
}

impl ItemField {
    /// The model locations, for the generated-data view.
    #[must_use]
    pub fn bindings(self) -> Vec<String> {
        let mut out: Vec<String> = self
            .setter_names()
            .iter()
            .map(|n| format!("setter:{n}"))
            .collect();
        match self {
            Self::Currency => out.push("setter:cost@currency".into()),
            Self::Weight => out.push("setter:weight@lb".into()),
            Self::SetterAttr { setter, attr, .. } => out.push(format!("setter:{setter}@{attr}")),
            Self::DamageType => {
                out.push("setter:damage@type".into());
                out.push("supports:Damage Type".into());
            }
            Self::WeaponCategory => out.push("supports:Weapon Category".into()),
            Self::WeaponProperties => out.push("supports:Weapon Property".into()),
            Self::WeaponGroups => out.push("supports:Weapon Group".into()),
            Self::Mastery => out.push("supports:Mastery Property".into()),
            Self::ArmorType => out.push("supports:Armor Group".into()),
            Self::BaseAc => out.push(format!("rules:stat {AC_ARMOR}|{AC_SHIELD}")),
            Self::Stealth => out.push(format!("rules:grant {STEALTH_GRANT}")),
            Self::Enhancement => out.push(format!("rules:stat {AC_ENHANCEMENT}|{AC_SHIELD}")),
            Self::Contents => out.push("extract.items".into()),
            _ => {}
        }
        out.extend(self.data_keys().iter().map(|k| format!("data.{k}")));
        out
    }

    /// The setter names it takes, lower case.
    #[must_use]
    pub fn setter_names(self) -> Vec<&'static str> {
        match self {
            Self::Cost => vec!["cost"],
            Self::Weight => vec!["weight"],
            Self::Rarity => vec!["rarity"],
            Self::MagicType => vec!["type"],
            Self::Attunement => vec!["attunement"],
            Self::Strength => vec!["strength"],
            Self::Versatile => vec!["versatile"],
            Self::Range => vec!["range"],
            Self::Damage => vec!["damage"],
            Self::Proficiency => vec!["proficiency"],
            Self::Mastery => vec!["mastery"],
            Self::ArmorType => vec!["armor"],
            Self::Stealth => vec!["stealth"],
            Self::Enhancement => vec!["enhancement"],
            Self::AppliesTo(n) => vec![n],
            _ => Vec::new(),
        }
    }

    /// The `data` keys it takes.
    #[must_use]
    pub fn data_keys(self) -> Vec<&'static str> {
        match self {
            Self::SetterAttr {
                mirror: Some(m), ..
            } => vec![m],
            Self::Rarity => vec!["rarity_text", "rarities"],
            Self::MagicType => vec!["item_type"],
            Self::Attunement => vec!["requires_attunement"],
            Self::Strength => vec!["strength_requirement"],
            Self::Versatile => vec!["versatile_damage"],
            Self::Range => vec!["range"],
            Self::Damage => vec!["damage"],
            Self::WeaponCategory => vec!["weapon_category", "melee"],
            Self::WeaponProperties => vec!["properties"],
            Self::Mastery => vec!["mastery", "mastery_id", "mastery_ids"],
            Self::ArmorType => vec!["armor_category", "dex_modifier", "dex_cap"],
            Self::BaseAc => vec!["base_ac"],
            Self::Stealth => vec!["stealth_disadvantage"],
            Self::PackageOwner => vec!["owner"],
            Self::PackageOptions => vec!["options"],
            _ => Vec::new(),
        }
    }
}

/// What a field took, in the order of its [`Slots`].
#[derive(Debug, Default, Serialize, Deserialize)]
struct Raw {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    setters: Vec<Setter>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    tags: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    data: BTreeMap<String, Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    rules: Vec<Rule>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    extract: Option<Extract>,
}

impl Raw {
    fn of(memo: Option<&FieldMemo>) -> Self {
        memo.and_then(|m| serde_json::from_value(m.raw.clone()).ok())
            .unwrap_or_default()
    }

    fn setter(&self) -> Option<&Setter> {
        self.setters.first()
    }

    fn has(&self, key: &str) -> bool {
        self.data.contains_key(key)
    }
}

/// Reading one field: takes parts of the element and remembers them.
struct Take<'r, 'e> {
    r: &'r mut Reader<'e>,
    raw: Raw,
    slots: Slots,
}

impl<'e> Take<'_, 'e> {
    fn e(&self) -> &'e Element {
        self.r.e
    }

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

    /// Take the untaken tags `want` accepts; at most `limit`.
    fn tags(&mut self, want: impl Fn(&str) -> bool, limit: usize) -> Vec<String> {
        let e = self.r.e;
        let mut out = Vec::new();
        for (i, t) in e.supports.iter().enumerate() {
            if out.len() < limit && !self.r.supports_taken[i] && want(t) {
                self.r.supports_taken[i] = true;
                self.raw.tags.push(t.clone());
                self.slots.supports.push(i);
                out.push(t.clone());
            }
        }
        out
    }

    /// Take the first untaken rule `want` accepts.
    fn rule(&mut self, want: impl Fn(&Rule) -> bool) -> Option<&'e Rule> {
        let e = self.r.e;
        let i = (0..e.rules.len()).find(|&i| !self.r.rules_taken[i] && want(&e.rules[i]))?;
        self.r.rules_taken[i] = true;
        self.raw.rules.push(e.rules[i].clone());
        self.slots.rules.push(i);
        Some(&e.rules[i])
    }

    fn done(self, value: Value) -> FieldMemo {
        FieldMemo {
            value,
            raw: to_json(&self.raw),
            slots: self.slots,
        }
    }
}

fn setter_text(s: Option<&Setter>) -> String {
    s.map(|s| s.value.clone()).unwrap_or_default()
}

fn first_setter<'e>(e: &'e Element, name: &str) -> Option<&'e Setter> {
    e.setters.iter().find(|s| s.name.eq_ignore_ascii_case(name))
}

fn data_text(v: Option<&Value>) -> String {
    v.and_then(Value::as_str).unwrap_or_default().to_string()
}

/// A whole number from setter text (`"+2"`, `"15"`), else from a `data` number.
fn number(s: Option<&Setter>, data: Option<&Value>) -> i64 {
    s.and_then(|s| s.value.trim().trim_start_matches('+').parse().ok())
        .or_else(|| data.and_then(Value::as_i64))
        .unwrap_or(0)
}

fn refs(ids: &[String], lookup: &dyn Lookup) -> Value {
    to_json(
        &ids.iter()
            .map(|id| RefValue::resolve(id, lookup))
            .collect::<Vec<_>>(),
    )
}

fn one_ref(id: Option<&String>, lookup: &dyn Lookup) -> Value {
    id.map_or(Value::Null, |id| to_json(&RefValue::resolve(id, lookup)))
}

/// A stat rule `name` = whole number, with only these keys (and `bonus` when given).
fn stat_shape(rule: &Rule, names: &[&str], bonus: Option<&str>) -> Option<i64> {
    let Rule::Stat(s) = rule else { return None };
    let bare = StatRule {
        name: s.name.clone(),
        value: s.value.clone(),
        bonus: bonus.map(str::to_string),
        ..StatRule::default()
    };
    (names.contains(&s.name.as_str()) && *s == bare)
        .then(|| s.value.parse().ok())
        .flatten()
}

fn is_stealth_grant(rule: &Rule) -> bool {
    matches!(rule, Rule::Grant(g) if *g == stealth_grant())
}

fn stealth_grant() -> GrantRule {
    GrantRule {
        id: STEALTH_GRANT.into(),
        ty: Some(STEALTH_GRANT_TYPE.into()),
        ..GrantRule::default()
    }
}

fn stat(name: &str, value: i64, bonus: Option<&str>) -> Rule {
    Rule::Stat(StatRule {
        name: name.into(),
        value: value.to_string(),
        bonus: bonus.map(str::to_string),
        ..StatRule::default()
    })
}

/// Read one item field.
pub(crate) fn read(f: ItemField, r: &mut Reader<'_>, cx: &FormContext<'_>) -> FieldMemo {
    let mut t = Take {
        r,
        raw: Raw::default(),
        slots: Slots::default(),
    };
    let lookup = cx.lookup;
    let value = match f {
        ItemField::Cost => json!(setter_text(t.setter("cost"))),
        ItemField::Currency => json!(
            first_setter(t.e(), "cost").map_or("", |s| s.attribute("currency").unwrap_or("gp"))
        ),
        ItemField::Weight => json!(t.setter("weight").map_or_else(String::new, |s| s
            .attribute("lb")
            .map_or_else(|| s.value.clone(), str::to_string))),
        ItemField::SetterAttr {
            setter,
            attr,
            mirror,
            ..
        } => {
            let from_setter = first_setter(t.e(), setter).and_then(|s| s.attribute(attr));
            let from_data = mirror.and_then(|m| t.data(m));
            json!(from_setter.map_or_else(|| data_text(from_data), str::to_string))
        }
        ItemField::Rarity => {
            let s = t.setter("rarity");
            let text = t.data("rarity_text");
            t.data("rarities");
            json!(s.map_or_else(|| data_text(text), |s| s.value.clone()))
        }
        ItemField::MagicType => {
            let s = t.setter("type");
            let d = t.data("item_type");
            json!(s.map_or_else(|| data_text(d), |s| s.value.clone()))
        }
        ItemField::Attunement => {
            let s = t.setter("attunement");
            let d = t.data("requires_attunement");
            json!(s.map_or_else(
                || d.and_then(Value::as_bool).unwrap_or(false),
                |s| s.as_bool() == Some(true)
            ))
        }
        ItemField::Strength => {
            let s = t.setter("strength");
            let d = t.data("strength_requirement");
            json!(number(s, d))
        }
        ItemField::Versatile => text_with_mirror(&mut t, "versatile", "versatile_damage"),
        ItemField::Range => text_with_mirror(&mut t, "range", "range"),
        ItemField::Damage => {
            let s = t.setter("damage");
            let d = t.data("damage");
            json!(s.map_or_else(
                || data_text(d.and_then(|d| d.get("dice"))),
                |s| s.value.clone()
            ))
        }
        ItemField::DamageType => {
            let tag = t.tags(|x| DAMAGE_TYPE.holds(x, lookup), 1);
            let written = first_setter(t.e(), "damage")
                .and_then(|s| s.attribute("type"))
                .filter(|w| !w.trim().is_empty())
                .map(|w| format!("{}{}", DAMAGE_TYPE.prefix, w.trim().to_uppercase()));
            one_ref(tag.first().or(written.as_ref()), lookup)
        }
        ItemField::Proficiency => {
            let s = t.setter("proficiency");
            let id = s
                .map(|s| s.value.trim().to_string())
                .filter(|i| !i.is_empty());
            one_ref(id.as_ref(), lookup)
        }
        ItemField::WeaponCategory => {
            let tag = t.tags(|x| WEAPON_CATEGORY.holds(x, lookup), 1);
            t.data("weapon_category");
            t.data("melee");
            one_ref(tag.first(), lookup)
        }
        ItemField::WeaponProperties => {
            let tags = t.tags(|x| WEAPON_PROPERTY.holds(x, lookup), usize::MAX);
            t.data("properties");
            refs(&tags, lookup)
        }
        ItemField::WeaponGroups => {
            let tags = t.tags(|x| WEAPON_GROUP.holds(x, lookup), usize::MAX);
            refs(&tags, lookup)
        }
        ItemField::Mastery => read_mastery(&mut t, lookup),
        ItemField::ArmorType => {
            let s = t.setter("armor");
            let tag = t.tags(|x| ARMOR_GROUP.holds(x, lookup), 1);
            let d = t.data("armor_category");
            t.data("dex_modifier");
            t.data("dex_cap");
            let from_tag = tag
                .first()
                .and_then(|x| x.strip_prefix(ARMOR_GROUP.prefix))
                .and_then(|g| ARMOR_TYPES.iter().find(|a| a.eq_ignore_ascii_case(g)));
            json!(s.map_or_else(
                || from_tag.map_or_else(|| data_text(d), |a| (*a).to_string()),
                |s| s.value.clone()
            ))
        }
        ItemField::BaseAc => {
            let rule = t.rule(|x| {
                stat_shape(x, &[AC_ARMOR], None).is_some()
                    || stat_shape(x, &[AC_SHIELD], Some("shield")).is_some()
            });
            let n = rule.and_then(|x| {
                stat_shape(x, &[AC_ARMOR], None)
                    .or_else(|| stat_shape(x, &[AC_SHIELD], Some("shield")))
            });
            let d = t.data("base_ac");
            json!(n.or_else(|| d.and_then(Value::as_i64)).unwrap_or(0))
        }
        ItemField::Stealth => {
            let rule = t.rule(is_stealth_grant);
            let s = t.setter("stealth");
            let d = t.data("stealth_disadvantage");
            json!(
                rule.is_some()
                    || s.is_some_and(|s| s.value.trim().eq_ignore_ascii_case("disadvantage"))
                    || d.and_then(Value::as_bool) == Some(true)
            )
        }
        ItemField::Enhancement => {
            let s = t.setter("enhancement");
            let n = number(s, None);
            if n > 0 {
                t.rule(|x| {
                    stat_shape(x, &[AC_ENHANCEMENT, AC_SHIELD], Some("enhancement")) == Some(n)
                });
            }
            json!(n)
        }
        ItemField::AppliesTo(name) => {
            let s = t.setter(name);
            to_json(&ExprValue::<TagFilter>::read::<TagFilterLanguage>(
                &setter_text(s),
                &(),
            ))
        }
        ItemField::Contents => {
            t.r.parts_taken.insert("extract");
            let x = t.e().extract.clone();
            let rows = ContentsRow::rows(x.as_ref(), lookup);
            t.raw.extract = x;
            to_json(&rows)
        }
        ItemField::PackageOwner => {
            let d = t.data("owner");
            let id = d
                .and_then(Value::as_str)
                .map(str::to_string)
                .filter(|i| !i.is_empty());
            one_ref(id.as_ref(), lookup)
        }
        ItemField::PackageOptions => {
            let d = t.data("options");
            to_json(&PackageOption::read_all(d, lookup))
        }
    };
    t.done(value)
}

fn text_with_mirror(t: &mut Take<'_, '_>, setter: &str, mirror: &str) -> Value {
    let s = t.setter(setter);
    let d = t.data(mirror);
    json!(s.map_or_else(|| data_text(d), |s| s.value.clone()))
}

fn read_mastery(t: &mut Take<'_, '_>, lookup: &dyn Lookup) -> Value {
    let tags = t.tags(|x| MASTERY.holds(x, lookup), usize::MAX);
    while t.setter("mastery").is_some() {}
    t.data("mastery");
    let one = t.data("mastery_id");
    let many = t.data("mastery_ids");
    if !tags.is_empty() {
        return refs(&tags, lookup);
    }
    let ids: Vec<String> = many
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .chain(one)
        .filter_map(Value::as_str)
        .filter(|i| !i.trim().is_empty())
        .map(str::to_string)
        .fold(Vec::new(), |mut acc, id| {
            if !acc.contains(&id) {
                acc.push(id);
            }
            acc
        });
    refs(&ids, lookup)
}

/// Put an unchanged field's stored parts back where they were.
pub(crate) fn write_raw(
    f: ItemField,
    m: &FieldMemo,
    out: &mut Out,
    path: &str,
    cx: &FormContext<'_>,
) -> Result<(), FormProblem> {
    let raw = Raw::of(Some(m));
    // a base AC read from a body armor's rule follows the armor type: a shield's AC is another rule
    if f == ItemField::BaseAc {
        let want = ac_rule_name(out);
        let stored = raw.rules.first().and_then(|x| match x {
            Rule::Stat(s) => Some(s.name.as_str()),
            _ => None,
        });
        if stored.is_some_and(|s| s != want) {
            return write(f, &m.value, Some(m), path, out, cx);
        }
    }
    put_back(&raw, &m.slots, out, path);
    Ok(())
}

fn put_back(raw: &Raw, slots: &Slots, out: &mut Out, path: &str) {
    out.setters(raw.setters.iter().cloned(), &slots.setters);
    out.supports(raw.tags.iter().cloned(), &slots.supports);
    for (k, v) in &raw.data {
        out.element.data.insert(k.clone(), v.clone());
    }
    out.field_rules(
        raw.rules.iter().cloned().map(|x| (0, x)),
        &slots.rules,
        path,
    );
    if raw.extract.is_some() {
        out.element.extract.clone_from(&raw.extract);
    }
}

/// An edited setter: the stored name spelling and attributes, the new value.
fn keep(name: &str, value: impl Into<String>, old: Option<&Setter>) -> Setter {
    match old {
        Some(o) => Setter {
            value: value.into(),
            ..o.clone()
        },
        None => Setter::new(name, value),
    }
}

fn mirror(out: &mut Out, raw: &Raw, key: &str, value: Value) {
    if raw.has(key) {
        out.element.data.insert(key.to_string(), value);
    }
}

fn text_or_null(t: &str) -> Value {
    if t.is_empty() {
        Value::Null
    } else {
        json!(t)
    }
}

fn ac_rule_name(out: &Out) -> &'static str {
    match out.setter_value("armor") {
        Some(a) if a.trim().eq_ignore_ascii_case("shield") => AC_SHIELD,
        _ => AC_ARMOR,
    }
}

fn is_number(t: &str) -> bool {
    t.replace(',', "")
        .parse::<f64>()
        .is_ok_and(|n| n.is_finite() && n >= 0.0)
}

fn is_range(t: &str) -> bool {
    let digits = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit());
    match t.split_once('/') {
        Some((a, b)) => digits(a.trim()) && digits(b.trim()),
        None => digits(t),
    }
}

/// Write an edited item field. `memo` is what it was read as (absent for a new element).
pub(crate) fn write(
    f: ItemField,
    value: &Value,
    memo: Option<&FieldMemo>,
    path: &str,
    out: &mut Out,
    cx: &FormContext<'_>,
) -> Result<(), FormProblem> {
    let raw = Raw::of(memo);
    let slots = memo.map(|m| m.slots.clone()).unwrap_or_default();
    let text = || decode::<String>(value, path).map(|t| t.trim().to_string());
    let problem = |m: &str| Err(FormProblem::new(path, m));
    let one_setter = |out: &mut Out, s: Option<Setter>| out.setters(s, &slots.setters);
    match f {
        ItemField::Cost => {
            let t = text()?;
            if !t.is_empty() && !is_number(&t) {
                return problem("Write the cost as a number, like 15 or 0.5.");
            }
            one_setter(out, (!t.is_empty()).then(|| keep("cost", t, raw.setter())));
        }
        ItemField::Currency => {
            let t = text()?.to_lowercase();
            if !t.is_empty() && !COINS.contains(&t.as_str()) {
                return problem("Pick a coin from the list.");
            }
            if let Some(s) = out.setter_mut("cost") {
                set_attr(s, "currency", &t);
            }
        }
        ItemField::Weight => {
            let t = text()?;
            let s = (!t.is_empty()).then(|| {
                if is_number(&t) {
                    let mut s = keep("weight", format!("{t} lb."), raw.setter());
                    s.attributes.insert("lb".into(), t.clone());
                    s
                } else {
                    let mut s = keep("weight", t.clone(), raw.setter());
                    s.attributes.remove("lb");
                    s
                }
            });
            one_setter(out, s);
        }
        ItemField::SetterAttr {
            setter,
            attr,
            mirror: m,
            needs,
        } => {
            let t = text()?;
            match out.setter_mut(setter) {
                Some(s) => set_attr(s, attr, &t),
                None if !t.is_empty() => return problem(needs),
                None => {}
            }
            if let Some(m) = m {
                mirror(out, &raw, m, text_or_null(&t));
            }
        }
        ItemField::Rarity => {
            let t = text()?;
            one_setter(
                out,
                (!t.is_empty()).then(|| keep("rarity", &t, raw.setter())),
            );
            mirror(out, &raw, "rarity_text", text_or_null(&t));
            let list = if t.is_empty() { json!([]) } else { json!([t]) };
            mirror(out, &raw, "rarities", list);
        }
        ItemField::MagicType => {
            let t = text()?;
            one_setter(out, (!t.is_empty()).then(|| keep("type", &t, raw.setter())));
            mirror(out, &raw, "item_type", text_or_null(&t));
        }
        ItemField::Attunement => {
            let on: bool = decode(value, path)?;
            one_setter(out, on.then(|| keep("attunement", "true", raw.setter())));
            mirror(out, &raw, "requires_attunement", json!(on));
        }
        ItemField::Strength => {
            let n: i64 = decode(value, path)?;
            if !(0..=30).contains(&n) {
                return problem("The Strength needed is a whole number from 0 (none) to 30.");
            }
            one_setter(
                out,
                (n > 0).then(|| keep("strength", n.to_string(), raw.setter())),
            );
            mirror(
                out,
                &raw,
                "strength_requirement",
                if n > 0 { json!(n) } else { Value::Null },
            );
        }
        ItemField::Versatile => {
            let t = text()?;
            if !t.is_empty() && !is_dice(&t) {
                return problem("Write the two-handed damage as dice, like 1d10.");
            }
            one_setter(
                out,
                (!t.is_empty()).then(|| keep("versatile", &t, raw.setter())),
            );
            mirror(out, &raw, "versatile_damage", text_or_null(&t));
        }
        ItemField::Range => {
            let t = text()?;
            if !t.is_empty() && !is_range(&t) {
                return problem("Write the range in feet as normal/long, like 20/60.");
            }
            one_setter(
                out,
                (!t.is_empty()).then(|| keep("range", &t, raw.setter())),
            );
            mirror(out, &raw, "range", text_or_null(&t));
        }
        ItemField::Damage => {
            let t = text()?;
            if !t.is_empty() && !is_dice(&t) {
                return problem("Write the damage as dice, like 1d8 or 2d6.");
            }
            one_setter(
                out,
                (!t.is_empty()).then(|| keep("damage", &t, raw.setter())),
            );
            if raw.has("damage") {
                let ty = raw.data["damage"].get("type").cloned().unwrap_or(json!(""));
                let v = if t.is_empty() {
                    Value::Null
                } else {
                    json!({ "dice": t, "type": ty })
                };
                out.element.data.insert("damage".into(), v);
            }
        }
        ItemField::DamageType => {
            let pick: Option<RefValue> = decode(value, path)?;
            let pick = pick.filter(|p| !p.id.trim().is_empty());
            out.supports(
                pick.iter().map(|p| p.id.trim().to_string()),
                &slots.supports,
            );
            let word = pick.as_ref().map(damage_word).unwrap_or_default();
            if let Some(s) = out.setter_mut("damage") {
                set_attr(s, "type", &word);
            }
            if let Some(d) = out
                .element
                .data
                .get_mut("damage")
                .and_then(Value::as_object_mut)
            {
                d.insert("type".into(), json!(word));
            }
        }
        ItemField::Proficiency => {
            let pick: Option<RefValue> = decode(value, path)?;
            let id = pick
                .map(|p| p.id.trim().to_string())
                .filter(|i| !i.is_empty());
            one_setter(out, id.map(|i| keep("proficiency", i, raw.setter())));
        }
        ItemField::WeaponCategory => {
            let pick: Option<RefValue> = decode(value, path)?;
            let pick = pick.filter(|p| !p.id.trim().is_empty());
            out.supports(
                pick.iter().map(|p| p.id.trim().to_string()),
                &slots.supports,
            );
            let name = pick.as_ref().map(|p| p.name.clone()).unwrap_or_default();
            mirror(out, &raw, "weapon_category", text_or_null(&name));
            let melee = !name.is_empty() && !name.to_lowercase().contains("ranged");
            mirror(out, &raw, "melee", json!(melee));
        }
        ItemField::WeaponProperties => {
            let picks = picked(value, path)?;
            out.supports(picks.iter().map(|p| p.id.clone()), &slots.supports);
            let names: Vec<&str> = picks
                .iter()
                .filter(|p| p.id != RANGE_PROPERTY)
                .map(|p| p.name.as_str())
                .collect();
            mirror(out, &raw, "properties", json!(names));
        }
        ItemField::WeaponGroups => {
            let picks = picked(value, path)?;
            out.supports(picks.iter().map(|p| p.id.clone()), &slots.supports);
        }
        ItemField::Mastery => write_mastery(value, path, &raw, &slots, out)?,
        ItemField::ArmorType => write_armor_type(&text()?, path, &raw, &slots, out)?,
        ItemField::BaseAc => {
            let n: i64 = decode(value, path)?;
            if !(0..=30).contains(&n) {
                return problem("The Armor Class is a whole number from 0 to 30.");
            }
            let rule = (n > 0).then(|| {
                let name = ac_rule_name(out);
                let bonus = (name == AC_SHIELD).then_some("shield");
                (RANK_AC, stat(name, n, bonus))
            });
            out.field_rules(rule, &slots.rules, path);
            mirror(
                out,
                &raw,
                "base_ac",
                if n > 0 { json!(n) } else { Value::Null },
            );
        }
        ItemField::Stealth => {
            let on: bool = decode(value, path)?;
            out.field_rules(
                on.then(|| (RANK_STEALTH, Rule::Grant(stealth_grant()))),
                &slots.rules,
                path,
            );
            let s = if on {
                Some(keep("stealth", "Disadvantage", raw.setter()))
            } else {
                raw.has("stealth_disadvantage")
                    .then(|| keep("stealth", "\u{2014}", raw.setter()))
            };
            one_setter(out, s);
            mirror(out, &raw, "stealth_disadvantage", json!(on));
        }
        ItemField::Enhancement => {
            let n: i64 = decode(value, path)?;
            if !(0..=5).contains(&n) {
                return problem("The bonus is a whole number from 0 (none) to 5.");
            }
            one_setter(
                out,
                (n > 0).then(|| keep("enhancement", n.to_string(), raw.setter())),
            );
            let rule = (n > 0)
                .then(|| enhancement_rule(&raw, out, n))
                .flatten()
                .map(|x| (RANK_ENHANCEMENT, x));
            out.field_rules(rule, &slots.rules, path);
        }
        ItemField::AppliesTo(name) => {
            let v: ExprValue<TagFilter> = decode(value, path)?;
            let t = v.text::<TagFilterLanguage>(&());
            one_setter(
                out,
                (!t.trim().is_empty()).then(|| keep(name, t, raw.setter())),
            );
        }
        ItemField::Contents => {
            let rows: Vec<ContentsRow> = decode(value, path)?;
            let old = raw.extract.unwrap_or_default();
            let x = Extract {
                items: rows.iter().filter_map(ContentsRow::to_item).collect(),
                ..old
            };
            out.element.extract = (x != Extract::default()).then_some(x);
        }
        ItemField::PackageOwner => {
            let pick: Option<RefValue> = decode(value, path)?;
            if let Some(id) = pick
                .map(|p| p.id.trim().to_string())
                .filter(|i| !i.is_empty())
            {
                out.element.data.insert("owner".into(), json!(id));
            }
        }
        ItemField::PackageOptions => {
            let opts: Vec<PackageOption> = decode(value, path)?;
            if opts.iter().any(|o| o.label.trim().is_empty()) {
                return problem("Give every option a letter, like A or B.");
            }
            if !opts.is_empty() || raw.has("options") {
                let v: Vec<Value> = opts.iter().map(PackageOption::write).collect();
                out.element.data.insert("options".into(), Value::Array(v));
            }
        }
    }
    let _ = cx;
    Ok(())
}

fn set_attr(s: &mut Setter, attr: &str, value: &str) {
    if value.is_empty() {
        s.attributes.remove(attr);
    } else {
        s.attributes.insert(attr.to_string(), value.to_string());
    }
}

fn picked(value: &Value, path: &str) -> Result<Vec<RefValue>, FormProblem> {
    let picks: Vec<RefValue> = decode(value, path)?;
    Ok(picks
        .into_iter()
        .filter(|p| !p.id.trim().is_empty())
        .map(|p| RefValue {
            id: p.id.trim().to_string(),
            ..p
        })
        .collect())
}

/// The damage type as the `damage` setter's `type` names it: the element's name in lower case (`piercing`), else
/// the end of its id.
fn damage_word(p: &RefValue) -> String {
    let name = if p.name.trim().is_empty() {
        p.id.strip_prefix(DAMAGE_TYPE.prefix)
            .unwrap_or(&p.id)
            .replace('_', " ")
    } else {
        p.name.trim().to_string()
    };
    name.to_lowercase()
}

fn write_mastery(
    value: &Value,
    path: &str,
    raw: &Raw,
    slots: &Slots,
    out: &mut Out,
) -> Result<(), FormProblem> {
    let picks = picked(value, path)?;
    out.supports(picks.iter().map(|p| p.id.clone()), &slots.supports);
    let names: Vec<String> = picks
        .iter()
        .map(|p| {
            if p.name.trim().is_empty() {
                p.id.strip_prefix(MASTERY.prefix)
                    .unwrap_or(&p.id)
                    .to_string()
            } else {
                p.name.trim().to_string()
            }
            .to_lowercase()
        })
        .collect();
    let joined = names.join(", ");
    out.setters(
        (!joined.is_empty()).then(|| keep("mastery", &joined, raw.setter())),
        &slots.setters,
    );
    mirror(out, raw, "mastery", text_or_null(&joined));
    mirror(
        out,
        raw,
        "mastery_id",
        picks.first().map_or(Value::Null, |p| json!(p.id)),
    );
    let ids: Vec<&str> = picks.iter().map(|p| p.id.as_str()).collect();
    mirror(out, raw, "mastery_ids", json!(ids));
    Ok(())
}

fn write_armor_type(
    t: &str,
    path: &str,
    raw: &Raw,
    slots: &Slots,
    out: &mut Out,
) -> Result<(), FormProblem> {
    let kind = if t.is_empty() {
        None
    } else {
        match ARMOR_TYPES.iter().find(|a| a.eq_ignore_ascii_case(t)) {
            Some(a) => Some(*a),
            None => {
                return Err(FormProblem::new(
                    path,
                    "Pick light, medium or heavy armor, or a shield.",
                ))
            }
        }
    };
    out.setters(kind.map(|k| keep("armor", k, raw.setter())), &slots.setters);
    out.supports(
        kind.map(|k| format!("{}{}", ARMOR_GROUP.prefix, k.to_uppercase())),
        &slots.supports,
    );
    let k = kind.unwrap_or_default();
    mirror(out, raw, "armor_category", text_or_null(k));
    mirror(
        out,
        raw,
        "dex_modifier",
        json!(matches!(k, "Light" | "Medium")),
    );
    mirror(
        out,
        raw,
        "dex_cap",
        if k == "Medium" { json!(2) } else { Value::Null },
    );
    Ok(())
}

/// The AC rule of a magic armor or shield: the stored rule's name with the new value, or a new rule when the item
/// applies to armor (a shield's `ac:shield` when it applies to shields only).
fn enhancement_rule(raw: &Raw, out: &Out, n: i64) -> Option<Rule> {
    let stored = raw.rules.first().and_then(|x| match x {
        Rule::Stat(s) => Some(s.name.clone()),
        _ => None,
    });
    let name = match stored {
        Some(name) => name,
        None => {
            let armor = out.setter_value("armor")?.trim().to_uppercase();
            if armor.is_empty() {
                return None;
            }
            let shield_only = armor.contains("SHIELD")
                && !["LIGHT", "MEDIUM", "HEAVY"]
                    .iter()
                    .any(|g| armor.contains(g));
            if shield_only {
                AC_SHIELD
            } else {
                AC_ENHANCEMENT
            }
            .to_string()
        }
    };
    Some(stat(&name, n, Some("enhancement")))
}
