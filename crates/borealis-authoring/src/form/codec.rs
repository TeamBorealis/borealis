//! Field codecs: how a field's value is read from its model locations and written back (spec 3.3).
//!
//! Reading records the stored bytes of each location ([`FieldMemo::raw`]) and the positions of the setters and tags
//! the field took. Writing an unchanged value puts those bytes back where they were; an edited value is written in
//! canonical form to every bound location (2024 mirrors only when the element already had them).

use super::value::{MapsToValue, RefValue, SetterRow, SheetValue};
use super::write::Out;
use super::{FieldMemo, FormContext, FormProblem, Slots};
use crate::expr_build::condition::{Condition, ConditionLanguage};
use crate::expr_build::ExprValue;
use borealis_content::rich::{self, normalize_plain_text, Doc};
use borealis_content::{Compendium, Element, Rule, Ruleset, Setter, Sheet};
use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use std::collections::BTreeSet;

mod caster;
pub(crate) mod caster_cx;
mod caster_read;
mod class;
mod class_feature;
mod class_mc;
mod feat;
mod feat_prereq;
mod small;
mod species;
mod spell;
mod subclass;
pub use caster::{CasterField, LevelCounts};
pub(crate) use class::ABILITY_CHOICES;
pub use class::{ClassField, ProficiencyGroup, SkillChoice, TableColumn};
pub use class_feature::FeatureField;
pub use feat::FeatField;
pub use small::SmallField;
pub use species::{Movement, NameKind, Sense, SpeciesField};
pub use spell::{Component, SpellField};
pub use subclass::SubclassField;

/// How one field maps to the element.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Codec {
    /// `name` (text, normalized when edited).
    Name,
    /// `type` (generic form only; other kinds write their own type).
    Type,
    /// `source` (read-only; the pack writes it).
    Source,
    /// `ruleset` (`""` = the pack's).
    Edition,
    /// `supports`, except the tags other fields of the kind own.
    Tags,
    /// `requirements` (condition builder).
    Requirements,
    /// The display `prerequisite`.
    PrerequisiteText,
    /// `description` (rich text).
    Description,
    /// `sheet`.
    Sheet,
    /// `compendium.display` (absent = shown).
    Compendium,
    /// `maps_to`.
    MapsTo,
    /// The value of the first setter with this name.
    SetterText(&'static str),
    /// A setter that is `true` when on and absent when off.
    SetterFlag(&'static str),
    /// A comma-separated setter, as chips.
    SetterChips(&'static str),
    /// Every setter no other field of the kind takes (expert rows).
    OtherSetters,
    /// A text `data` key.
    DataText(&'static str),
    /// A language's rarity: the setters `standard`/`exotic`/`secret`, the tag and `data.rarity`.
    LanguageRarity,
    /// The rule blocks (placement only: blocks live in `FormState::blocks`).
    Blocks,
    /// The element an append adds to (its `id`), as a reference.
    Target,
    /// An item field (spec 4.9; `item_codec`).
    Item(super::item_codec::ItemField),
    /// A spell field (spec 4.1): its setter and its 2024 mirror, or the spell lists.
    Spell(SpellField),
    /// A feat field (spec 4.2): category, prerequisite, repeatable, in both the 2014 and the 2024 shape.
    Feat(FeatField),
    /// A species field (spec 4.4): size, speed, senses, traits, languages, ability increases, the parent race.
    Species(SpeciesField),
    /// The children a parent gives (spec 3.1): `grant`s of elements of type `ty`, each a child form of the draft or a
    /// reference; `levelled` for a class's LEVELS grid (each at a level).
    Children {
        /// The child element type.
        ty: &'static str,
        /// The entries have class levels.
        levelled: bool,
    },
    /// A class field (spec 4.5).
    Class(ClassField),
    /// A generated class feature's field (spec 4.5: ability score improvements, subclass choice, epic boon).
    Feature(FeatureField),
    /// A spellcasting section field (spec 4.6): the `spellcasting` node and the section's rules.
    Caster(CasterField),
    /// A subclass field (spec 4.7: its class, the tag that class offers it by) or a class option's group (4.8).
    Subclass(SubclassField),
    /// A small type's field (spec 4.10): what a proficiency covers, a condition's listing, a rules text's group.
    Small(SmallField),
}

use small::{RARITIES, RARITY_TAGS};

impl Codec {
    /// The model locations this field writes, as shown in the generated-data view (`setter:level`, `data.level`).
    #[must_use]
    pub fn bindings(self) -> Vec<String> {
        let s = |t: &str| vec![t.to_string()];
        match self {
            Self::Name => s("name"),
            Self::Type => s("type"),
            Self::Source => s("source"),
            Self::Edition => s("ruleset"),
            Self::Tags => s("supports"),
            Self::Requirements => s("requirements"),
            Self::PrerequisiteText => s("prerequisite"),
            Self::Description => s("description"),
            Self::Sheet => s("sheet"),
            Self::Compendium => s("compendium.display"),
            Self::MapsTo => s("maps_to"),
            Self::SetterText(n) | Self::SetterFlag(n) | Self::SetterChips(n) => {
                vec![format!("setter:{n}")]
            }
            Self::OtherSetters => s("setters"),
            Self::DataText(k) => vec![format!("data.{k}")],
            Self::LanguageRarity => {
                let mut b: Vec<String> = RARITIES.iter().map(|r| format!("setter:{r}")).collect();
                b.push("supports:Standard|Exotic|Secret|Rare".into());
                b.push("data.rarity".into());
                b
            }
            Self::Blocks => s("rules"),
            Self::Target => s("id"),
            Self::Item(f) => f.bindings(),
            Self::Spell(f) => f.bindings(),
            Self::Feat(f) => f.bindings(),
            Self::Species(f) => f.bindings(),
            Self::Children { ty, levelled } => vec![format!(
                "rules:grant {ty}{}",
                if levelled { " (level)" } else { "" }
            )],
            Self::Class(f) => f.bindings(),
            Self::Feature(f) => f.bindings(),
            Self::Caster(f) => f.bindings(),
            Self::Subclass(f) => f.bindings(),
            Self::Small(f) => f.bindings(),
        }
    }

    /// It edits `requirements` with the condition builder.
    #[must_use]
    pub fn is_condition(self) -> bool {
        matches!(
            self,
            Self::Requirements | Self::Feat(FeatField::Prerequisite)
        )
    }

    /// It edits the display `prerequisite`.
    #[must_use]
    pub fn is_prerequisite_text(self) -> bool {
        matches!(self, Self::PrerequisiteText | Self::Feat(FeatField::Shown))
    }

    /// It holds the element's support tags (the Tags field, or a spell's lists).
    #[must_use]
    pub fn edits_supports(self) -> bool {
        match self {
            Self::Tags => true,
            Self::Spell(f) => f.owns_supports(),
            Self::Subclass(f) => f == SubclassField::OptionGroup,
            _ => false,
        }
    }

    /// The support tags this field owns (the Tags field shows the others).
    #[must_use]
    pub fn owned_tags(self) -> &'static [&'static str] {
        match self {
            Self::LanguageRarity => &RARITY_TAGS,
            Self::Small(f) => f.owned_tags(),
            _ => &[],
        }
    }

    /// The setter names this field takes, lower case.
    #[must_use]
    pub fn setter_names(self) -> Vec<&'static str> {
        match self {
            Self::SetterText(n) | Self::SetterFlag(n) | Self::SetterChips(n) => vec![n],
            Self::LanguageRarity => RARITIES.to_vec(),
            Self::Item(f) => f.setter_names(),
            Self::Spell(f) => f.setter_names(),
            Self::Feat(f) => f.setter_names(),
            Self::Species(f) => f.setter_names(),
            Self::Class(f) => f.setter_names(),
            _ => Vec::new(),
        }
    }

    /// The `data` keys this field takes.
    #[must_use]
    pub fn data_keys(self) -> Vec<&'static str> {
        match self {
            Self::DataText(k) => vec![k],
            Self::LanguageRarity => vec!["rarity"],
            Self::Item(f) => f.data_keys(),
            Self::Spell(f) => f.data_keys(),
            Self::Feat(f) => f.data_keys(),
            Self::Species(f) => f.data_keys(),
            Self::Class(f) => f.data_keys(),
            Self::Subclass(f) => f.data_keys(),
            Self::Small(f) => f.data_keys(),
            _ => Vec::new(),
        }
    }
}

/// What reading needs: the element and which of its setters and tags fields took already.
pub(crate) struct Reader<'e> {
    pub(crate) e: &'e Element,
    pub(crate) setters_taken: Vec<bool>,
    pub(crate) supports_taken: Vec<bool>,
    pub(crate) data_taken: BTreeSet<String>,
    pub(crate) owned_tags: BTreeSet<&'static str>,
    /// Rules a field took (the others become blocks).
    pub(crate) rules_taken: Vec<bool>,
    /// Whole parts a field took (`extract`), so they are not kept as preserved parts.
    pub(crate) parts_taken: BTreeSet<&'static str>,
}

impl<'e> Reader<'e> {
    pub(crate) fn new(e: &'e Element, owned_tags: BTreeSet<&'static str>) -> Self {
        Self {
            e,
            setters_taken: vec![false; e.setters.len()],
            supports_taken: vec![false; e.supports.len()],
            data_taken: BTreeSet::new(),
            owned_tags,
            rules_taken: vec![false; e.rules.len()],
            parts_taken: BTreeSet::new(),
        }
    }

    /// The rules no field took yet, with their positions.
    pub(crate) fn free_rules(&self) -> impl Iterator<Item = (usize, &'e Rule)> + '_ {
        let e = self.e;
        e.rules
            .iter()
            .enumerate()
            .filter(|(i, _)| !self.rules_taken[*i])
    }

    /// Take the rule at `i` (the field records `i` in its [`Slots::rules`]).
    pub(crate) fn take_rule(&mut self, i: usize) -> &'e Rule {
        self.rules_taken[i] = true;
        &self.e.rules[i]
    }

    /// Take the first untaken setter called `name` (any case).
    pub(crate) fn take_setter(&mut self, name: &str) -> Option<(usize, &'e Setter)> {
        let e = self.e;
        let taken = &self.setters_taken;
        let i = (0..e.setters.len())
            .find(|&i| !taken[i] && e.setters[i].name.eq_ignore_ascii_case(name))?;
        self.setters_taken[i] = true;
        Some((i, &e.setters[i]))
    }

    pub(crate) fn take_data(&mut self, key: &str) -> Option<&'e Value> {
        let e = self.e;
        let v = e.data.get(key)?;
        self.data_taken.insert(key.to_string());
        Some(v)
    }

    /// Take every untaken tag `want` accepts, in order.
    pub(crate) fn take_tags(&mut self, want: impl Fn(&str) -> bool) -> (Vec<String>, Vec<usize>) {
        let e = self.e;
        let mut tags = Vec::new();
        let mut slots = Vec::new();
        for (i, t) in e.supports.iter().enumerate() {
            if !self.supports_taken[i] && want(t) {
                self.supports_taken[i] = true;
                tags.push(t.clone());
                slots.push(i);
            }
        }
        (tags, slots)
    }
}

/// A `data` value that may be stored as `null`: `[value]` when stored, `[]` when absent.
pub(crate) fn present(v: Option<&Value>) -> Value {
    Value::Array(v.into_iter().cloned().collect())
}

/// The stored value of [`present`], if any.
pub(crate) fn stored(raw: &Value) -> Option<&Value> {
    raw.as_array().and_then(|a| a.first())
}

fn memo(value: Value, raw: Value, slots: Slots) -> FieldMemo {
    FieldMemo { value, raw, slots }
}

fn plain(value: Value, raw: Value) -> FieldMemo {
    memo(value, raw, Slots::default())
}

pub(crate) fn to_json<T: serde::Serialize>(v: &T) -> Value {
    serde_json::to_value(v).unwrap_or(Value::Null)
}

fn opt_json<T: serde::Serialize>(v: Option<&T>) -> Value {
    v.map_or(Value::Null, to_json)
}

fn setter_memo(
    r: &mut Reader<'_>,
    name: &str,
    value: impl Fn(&Setter) -> Value,
    none: Value,
) -> FieldMemo {
    match r.take_setter(name) {
        Some((i, s)) => memo(
            value(s),
            to_json(s),
            Slots {
                setters: vec![i],
                ..Slots::default()
            },
        ),
        None => plain(none, Value::Null),
    }
}

fn chips(text: &str) -> Vec<String> {
    text.split(',')
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .collect()
}

/// Read one field. `None` for [`Codec::Blocks`] (blocks are read with the rules).
pub(crate) fn read(codec: Codec, r: &mut Reader<'_>, cx: &FormContext<'_>) -> Option<FieldMemo> {
    let e = r.e;
    let text = |t: &str| plain(json!(t), json!(t));
    Some(match codec {
        Codec::Name => text(&e.name),
        Codec::Type => text(&e.ty),
        Codec::Source => text(&e.source),
        Codec::Edition => {
            let v = e.ruleset.map_or("", Ruleset::as_str);
            plain(json!(v), opt_json(e.ruleset.as_ref()))
        }
        Codec::Tags => {
            let owned = r.owned_tags.clone();
            let (tags, slots) = r.take_tags(|t| !owned.contains(t));
            memo(
                json!(tags),
                json!(tags),
                Slots {
                    supports: slots,
                    ..Slots::default()
                },
            )
        }
        Codec::Requirements => plain(
            to_json(&ExprValue::read::<ConditionLanguage>(
                &e.requirements,
                &cx.conditions,
            )),
            json!(e.requirements),
        ),
        Codec::PrerequisiteText => text(&e.prerequisite),
        Codec::Description => plain(to_json(&rich::parse(&e.description)), json!(e.description)),
        Codec::Sheet => {
            let bare = e.sheet.as_ref().map(|s| Sheet {
                extra: Default::default(),
                ..s.clone()
            });
            plain(
                opt_json(e.sheet.as_ref().map(SheetValue::from_sheet).as_ref()),
                opt_json(bare.as_ref()),
            )
        }
        Codec::Compendium => {
            let display = e.compendium.as_ref().and_then(|c| c.display);
            let bare = e.compendium.as_ref().map(|c| Compendium {
                display: c.display,
                ..Compendium::default()
            });
            plain(json!(display.unwrap_or(true)), opt_json(bare.as_ref()))
        }
        Codec::MapsTo => plain(
            opt_json(
                e.maps_to
                    .as_ref()
                    .map(|m| MapsToValue::from_maps_to(m, cx.lookup))
                    .as_ref(),
            ),
            opt_json(e.maps_to.as_ref()),
        ),
        Codec::SetterText(n) => setter_memo(r, n, |s| json!(s.value), json!("")),
        Codec::SetterFlag(n) => {
            setter_memo(r, n, |s| json!(s.as_bool() == Some(true)), json!(false))
        }
        Codec::SetterChips(n) => setter_memo(r, n, |s| json!(chips(&s.value)), json!([])),
        Codec::OtherSetters => read_other_setters(r),
        Codec::DataText(k) => {
            let v = r.take_data(k);
            plain(
                json!(v.and_then(Value::as_str).unwrap_or_default()),
                present(v),
            )
        }
        Codec::LanguageRarity => small::read_rarity(r),
        Codec::Small(f) => small::read(f, r, cx),
        Codec::Item(f) => super::item_codec::read(f, r, cx),
        Codec::Target => plain(
            if e.id.is_empty() {
                Value::Null
            } else {
                to_json(&RefValue::resolve(&e.id, cx.lookup))
            },
            json!(e.id),
        ),
        Codec::Spell(f) => spell::read(f, r),
        Codec::Feat(f) => feat::read(f, r, cx),
        Codec::Species(f) => species::read(f, r, cx),
        Codec::Children { ty, levelled } => super::child_refs::read(ty, levelled, r, cx),
        Codec::Class(f) => class::read(f, r, cx),
        Codec::Feature(f) => class_feature::read(f, r),
        Codec::Caster(f) => caster::read(f, r),
        Codec::Subclass(f) => subclass::read(f, r, cx),
        Codec::Blocks => return None,
    })
}

fn read_other_setters(r: &mut Reader<'_>) -> FieldMemo {
    let mut rows = Vec::new();
    let mut raw = Vec::new();
    let mut slots = Vec::new();
    let e = r.e;
    for (i, s) in e.setters.iter().enumerate() {
        if !r.setters_taken[i] {
            r.setters_taken[i] = true;
            rows.push(SetterRow {
                name: s.name.clone(),
                value: s.value.clone(),
                attributes: s.attributes.clone(),
            });
            raw.push(to_json(s));
            slots.push(i);
        }
    }
    memo(
        to_json(&rows),
        Value::Array(raw),
        Slots {
            setters: slots,
            ..Slots::default()
        },
    )
}

/// Decode a field value of the shape its control holds.
pub(crate) fn decode<T: DeserializeOwned>(value: &Value, path: &str) -> Result<T, FormProblem> {
    serde_json::from_value(value.clone()).map_err(|e| {
        FormProblem::new(
            path,
            format!("This value can't be read ({e}). Undo the last change or enter it again."),
        )
    })
}

pub(crate) fn decode_raw<T: DeserializeOwned>(raw: &Value) -> Option<T> {
    serde_json::from_value(raw.clone()).ok()
}

pub(crate) fn raw_setters(raw: &Value) -> Vec<Setter> {
    match raw {
        Value::Array(items) => items.iter().filter_map(decode_raw).collect(),
        Value::Null => Vec::new(),
        one => decode_raw(one).into_iter().collect(),
    }
}

/// An edited setter keeps the stored name spelling and attributes.
pub(crate) fn edited_setter(name: &str, value: String, raw: Option<&Value>) -> Setter {
    match raw.and_then(decode_raw::<Setter>) {
        Some(old) => Setter { value, ..old },
        None => Setter::new(name, value),
    }
}

/// Write one field into `out`. `memo` is what the field was read as (absent for a new element or a re-created one).
pub(crate) fn write(
    codec: Codec,
    value: &Value,
    memo: Option<&FieldMemo>,
    path: &str,
    out: &mut Out,
    cx: &FormContext<'_>,
) -> Result<(), FormProblem> {
    match codec {
        Codec::Feat(f) => return feat::write(f, value, memo, path, out, cx),
        Codec::Class(f) => return class::write(f, value, memo, path, out, cx),
        Codec::Feature(f) => return class_feature::write(f, value, memo, path, out),
        Codec::Caster(f) => return caster::write(f, value, memo, path, out),
        Codec::Subclass(f) => return subclass::write(f, value, memo, path, out, cx),
        Codec::Children { ty, levelled } => {
            if let Some(m) = memo.filter(|m| m.value == *value) {
                let rules: Vec<borealis_content::Rule> = m
                    .raw
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|r| serde_json::from_value(r.clone()).ok())
                    .collect();
                let rank = super::child_refs::RANK_CHILDREN;
                out.field_rules(rules.into_iter().map(|r| (rank, r)), &m.slots.rules, path);
                return Ok(());
            }
            return super::child_refs::write(ty, levelled, value, memo, path, out);
        }
        _ => {}
    }
    let unchanged = memo.filter(|m| m.value == *value);
    let raw = memo.map(|m| &m.raw);
    let slots = memo.map(|m| m.slots.clone()).unwrap_or_default();
    if let Some(m) = unchanged {
        if let Codec::Item(f) = codec {
            return super::item_codec::write_raw(f, m, out, path, cx);
        }
        return write_raw(codec, m, out, path);
    }
    let text = || decode::<String>(value, path);
    match codec {
        Codec::Name => out.element.name = normalize_plain_text(&text()?),
        Codec::Type => out.element.ty = normalize_plain_text(&text()?),
        Codec::Source => out.element.source = text()?,
        Codec::Edition => out.element.ruleset = edition(&text()?, path)?,
        Codec::Tags => {
            let tags: Vec<String> = decode(value, path)?;
            let tags = tags
                .iter()
                .map(|t| t.trim().to_string())
                .filter(|t| !t.is_empty());
            out.supports(tags, &slots.supports);
        }
        Codec::Requirements => {
            let v: ExprValue<Condition> = decode(value, path)?;
            out.element.requirements = v.text::<ConditionLanguage>(&cx.conditions);
        }
        Codec::PrerequisiteText => out.element.prerequisite = normalize_plain_text(&text()?),
        Codec::Description => {
            let doc: Doc = decode(value, path)?;
            out.element.description = rich::render(&doc.normalized());
        }
        Codec::Sheet => {
            let v: Option<SheetValue> = decode(value, path)?;
            out.element.sheet = v
                .filter(|s| !s.is_blank())
                .map(|s| s.to_sheet(Default::default()));
        }
        Codec::Compendium => {
            let shown: bool = decode(value, path)?;
            out.element.compendium = (!shown).then(|| Compendium {
                display: Some(false),
                ..Compendium::default()
            });
        }
        Codec::MapsTo => {
            let v: Option<MapsToValue> = decode(value, path)?;
            out.element.maps_to = v.and_then(|m| m.to_maps_to());
        }
        Codec::SetterText(n) => {
            let t = text()?.trim().to_string();
            let s = (!t.is_empty()).then(|| edited_setter(n, t, raw));
            out.setters(s, &slots.setters);
        }
        Codec::SetterFlag(n) => {
            let on: bool = decode(value, path)?;
            let s = on.then(|| edited_setter(n, "true".into(), raw));
            out.setters(s, &slots.setters);
        }
        Codec::SetterChips(n) => {
            let items: Vec<String> = decode(value, path)?;
            let t = chips(&items.join(",")).join(", ");
            let s = (!t.is_empty()).then(|| edited_setter(n, t, raw));
            out.setters(s, &slots.setters);
        }
        Codec::OtherSetters => {
            let rows: Vec<SetterRow> = decode(value, path)?;
            let setters = rows
                .into_iter()
                .filter(|r| !r.name.trim().is_empty())
                .map(|r| Setter {
                    name: r.name.trim().to_string(),
                    value: r.value.trim().to_string(),
                    attributes: r.attributes,
                });
            out.setters(setters, &slots.setters);
        }
        Codec::DataText(k) => {
            let t = text()?.trim().to_string();
            if !t.is_empty() {
                out.element.data.insert(k.to_string(), json!(t));
            }
        }
        Codec::LanguageRarity => small::write_rarity(&text()?, memo, out, path)?,
        Codec::Small(f) => small::write(f, value, memo, path, out)?,
        Codec::Item(f) => super::item_codec::write(f, value, memo, path, out, cx)?,
        Codec::Spell(f) => spell::write(f, value, memo, path, out)?,
        Codec::Species(f) => species::write(f, value, memo, path, out)?,
        Codec::Feat(_)
        | Codec::Class(_)
        | Codec::Feature(_)
        | Codec::Caster(_)
        | Codec::Subclass(_)
        | Codec::Children { .. } => {
            unreachable!("written above")
        }
        Codec::Target => {
            let target: Option<RefValue> = decode(value, path)?;
            match target.filter(|t| !t.id.trim().is_empty()) {
                Some(t) => out.element.id = t.id.trim().to_string(),
                None => return Err(FormProblem::new(path, "Choose the element to add to.")),
            }
        }
        Codec::Blocks => {}
    }
    Ok(())
}

fn edition(text: &str, path: &str) -> Result<Option<Ruleset>, FormProblem> {
    match text {
        "" => Ok(None),
        "dnd2014" => Ok(Some(Ruleset::Dnd2014)),
        "dnd2024" => Ok(Some(Ruleset::Dnd2024)),
        "any" => Ok(Some(Ruleset::Any)),
        _ => Err(FormProblem::new(path, "Choose an edition from the list.")),
    }
}

/// Put the stored bytes of an unchanged field back.
fn write_raw(codec: Codec, m: &FieldMemo, out: &mut Out, path: &str) -> Result<(), FormProblem> {
    let raw = &m.raw;
    let bad = || FormProblem::new(path, "The stored value of this field can't be read back.");
    let get = |v: &Value| -> Result<String, FormProblem> {
        v.as_str().map(str::to_string).ok_or_else(bad)
    };
    match codec {
        Codec::Name => out.element.name = get(raw)?,
        Codec::Type => out.element.ty = get(raw)?,
        Codec::Source => out.element.source = get(raw)?,
        Codec::Edition => out.element.ruleset = decode_raw(raw).ok_or_else(bad)?,
        Codec::Tags => {
            let tags: Vec<String> = decode_raw(raw).ok_or_else(bad)?;
            out.supports(tags, &m.slots.supports);
        }
        Codec::Requirements => out.element.requirements = get(raw)?,
        Codec::PrerequisiteText => out.element.prerequisite = get(raw)?,
        Codec::Description => out.element.description = get(raw)?,
        Codec::Sheet => out.element.sheet = decode_raw(raw).ok_or_else(bad)?,
        Codec::Compendium => out.element.compendium = decode_raw(raw).ok_or_else(bad)?,
        Codec::MapsTo => out.element.maps_to = decode_raw(raw).ok_or_else(bad)?,
        Codec::SetterText(_)
        | Codec::SetterFlag(_)
        | Codec::SetterChips(_)
        | Codec::OtherSetters => {
            out.setters(raw_setters(raw), &m.slots.setters);
        }
        Codec::DataText(k) => {
            if let Some(v) = stored(raw) {
                out.element.data.insert(k.to_string(), v.clone());
            }
        }
        Codec::LanguageRarity => {
            out.setters(raw_setters(&raw["setters"]), &m.slots.setters);
            let tags: Vec<String> = decode_raw(&raw["tags"]).unwrap_or_default();
            out.supports(tags, &m.slots.supports);
            if let Some(v) = stored(&raw["data"]) {
                out.element.data.insert("rarity".into(), v.clone());
            }
        }
        Codec::Target => out.element.id = get(raw)?,
        Codec::Small(f) => small::write_raw(f, m, out, path),
        Codec::Spell(f) => spell::write_raw(f, m, out),
        Codec::Species(f) => species::write_raw(f, m, out, path),
        // item fields put their parts back in `item_codec::write_raw` (they need the context)
        Codec::Item(_)
        | Codec::Feat(_)
        | Codec::Class(_)
        | Codec::Feature(_)
        | Codec::Caster(_)
        | Codec::Subclass(_)
        | Codec::Children { .. }
        | Codec::Blocks => {}
    }
    Ok(())
}

/// A class's multiclass id as read and as it is now, when they differ (a copy gets its own id, spec 5.3).
pub(crate) fn class_renamed(form: &super::FormState) -> Option<(String, String)> {
    let new = form.memo.multiclass_id.clone()?;
    let old = form
        .memo
        .fields
        .get("multiclass")
        .and_then(|m| m.raw.get("part"))
        .and_then(|p| p.get("id"))
        .and_then(Value::as_str)?
        .to_string();
    (old != new).then_some((old, new))
}

/// `text` with every whole id `old` written as `new` (ids are `A-Z0-9_` runs).
fn rename_id(text: &str, old: &str, new: &str) -> String {
    let is_id = |c: char| c.is_ascii_alphanumeric() || c == '_';
    let mut out = String::new();
    let mut word = String::new();
    for c in text.chars().chain(std::iter::once('\0')) {
        if is_id(c) {
            word.push(c);
            continue;
        }
        out.push_str(if word == old { new } else { &word });
        word.clear();
        if c != '\0' {
            out.push(c);
        }
    }
    out
}

fn rename_in(v: &mut Value, old: &str, new: &str) {
    match v {
        Value::Object(m) => {
            for (k, x) in m.iter_mut() {
                match x {
                    Value::String(t) if k == "requirements" => *t = rename_id(t, old, new),
                    _ => rename_in(x, old, new),
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(|x| rename_in(x, old, new)),
        _ => {}
    }
}

/// The element with the multiclass id `old` written as `new` in every requirement (its own, its rules' and its
/// multiclass section's): a copied class's "first class only" names its own multiclass section.
pub(crate) fn rename_requirement_ids(e: &mut Element, old: &str, new: &str) {
    let Ok(mut v) = serde_json::to_value(&*e) else {
        return;
    };
    rename_in(&mut v, old, new);
    if let Ok(renamed) = serde_json::from_value(v) {
        *e = renamed;
    }
}
