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
use borealis_content::{Compendium, Element, Ruleset, Setter, Sheet};
use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use std::collections::BTreeSet;

mod feat;
mod feat_prereq;
mod spell;
pub use feat::FeatField;
pub use spell::{Component, SpellField};

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
    /// A spell field (spec 4.1): its setter and its 2024 mirror, or the spell lists.
    Spell(SpellField),
    /// A feat field (spec 4.2): category, prerequisite, repeatable, in both the 2014 and the 2024 shape.
    Feat(FeatField),
}

const RARITIES: [&str; 3] = ["standard", "exotic", "secret"];

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
                b.push("supports:Standard|Exotic|Secret".into());
                b.push("data.rarity".into());
                b
            }
            Self::Blocks => s("rules"),
            Self::Target => s("id"),
            Self::Spell(f) => f.bindings(),
            Self::Feat(f) => f.bindings(),
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
            _ => false,
        }
    }

    /// The support tags this field owns (the Tags field shows the others).
    #[must_use]
    pub fn owned_tags(self) -> &'static [&'static str] {
        match self {
            Self::LanguageRarity => &["Standard", "Exotic", "Secret"],
            _ => &[],
        }
    }

    /// The setter names this field takes, lower case.
    #[must_use]
    pub fn setter_names(self) -> Vec<&'static str> {
        match self {
            Self::SetterText(n) | Self::SetterFlag(n) | Self::SetterChips(n) => vec![n],
            Self::LanguageRarity => RARITIES.to_vec(),
            Self::Spell(f) => f.setter_names(),
            Self::Feat(f) => f.setter_names(),
            _ => Vec::new(),
        }
    }

    /// The `data` keys this field takes.
    #[must_use]
    pub fn data_keys(self) -> Vec<&'static str> {
        match self {
            Self::DataText(k) => vec![k],
            Self::LanguageRarity => vec!["rarity"],
            Self::Spell(f) => f.data_keys(),
            Self::Feat(f) => f.data_keys(),
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
}

impl<'e> Reader<'e> {
    pub(crate) fn new(e: &'e Element, owned_tags: BTreeSet<&'static str>) -> Self {
        Self {
            e,
            setters_taken: vec![false; e.setters.len()],
            supports_taken: vec![false; e.supports.len()],
            data_taken: BTreeSet::new(),
            owned_tags,
        }
    }

    /// Take the first untaken setter called `name` (any case).
    fn take_setter(&mut self, name: &str) -> Option<(usize, &'e Setter)> {
        let e = self.e;
        let taken = &self.setters_taken;
        let i = (0..e.setters.len())
            .find(|&i| !taken[i] && e.setters[i].name.eq_ignore_ascii_case(name))?;
        self.setters_taken[i] = true;
        Some((i, &e.setters[i]))
    }

    fn take_data(&mut self, key: &str) -> Option<&'e Value> {
        let e = self.e;
        let v = e.data.get(key)?;
        self.data_taken.insert(key.to_string());
        Some(v)
    }

    /// Take every untaken tag `want` accepts, in order.
    fn take_tags(&mut self, want: impl Fn(&str) -> bool) -> (Vec<String>, Vec<usize>) {
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
fn present(v: Option<&Value>) -> Value {
    Value::Array(v.into_iter().cloned().collect())
}

/// The stored value of [`present`], if any.
fn stored(raw: &Value) -> Option<&Value> {
    raw.as_array().and_then(|a| a.first())
}

fn memo(value: Value, raw: Value, slots: Slots) -> FieldMemo {
    FieldMemo { value, raw, slots }
}

fn plain(value: Value, raw: Value) -> FieldMemo {
    memo(value, raw, Slots::default())
}

fn to_json<T: serde::Serialize>(v: &T) -> Value {
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
        Codec::LanguageRarity => read_rarity(r),
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

fn read_rarity(r: &mut Reader<'_>) -> FieldMemo {
    let mut setters = Vec::new();
    let mut slots = Slots::default();
    for name in RARITIES {
        while let Some((i, s)) = r.take_setter(name) {
            setters.push(s.clone());
            slots.setters.push(i);
        }
    }
    let owned = Codec::LanguageRarity.owned_tags();
    let (tags, tag_slots) = r.take_tags(|t| owned.contains(&t));
    slots.supports = tag_slots;
    let data = r.take_data("rarity");
    let value = setters
        .iter()
        .find(|s| s.as_bool() == Some(true))
        .map(|s| s.name.to_lowercase())
        .or_else(|| tags.first().map(|t| t.to_lowercase()))
        .or_else(|| data.and_then(Value::as_str).map(str::to_lowercase))
        .unwrap_or_default();
    let raw = json!({ "setters": setters, "tags": tags, "data": present(data) });
    memo(json!(value), raw, slots)
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

fn decode_raw<T: DeserializeOwned>(raw: &Value) -> Option<T> {
    serde_json::from_value(raw.clone()).ok()
}

fn raw_setters(raw: &Value) -> Vec<Setter> {
    match raw {
        Value::Array(items) => items.iter().filter_map(decode_raw).collect(),
        Value::Null => Vec::new(),
        one => decode_raw(one).into_iter().collect(),
    }
}

/// An edited setter keeps the stored name spelling and attributes.
fn edited_setter(name: &str, value: String, raw: Option<&Value>) -> Setter {
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
    if let Codec::Feat(f) = codec {
        return feat::write(f, value, memo, path, out, cx);
    }
    let unchanged = memo.filter(|m| m.value == *value);
    let raw = memo.map(|m| &m.raw);
    let slots = memo.map(|m| m.slots.clone()).unwrap_or_default();
    if let Some(m) = unchanged {
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
        Codec::LanguageRarity => write_rarity(&text()?, raw, &slots, out, path)?,
        Codec::Spell(f) => spell::write(f, value, memo, path, out)?,
        Codec::Feat(_) => unreachable!("feat fields are written above"),
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

fn write_rarity(
    rarity: &str,
    raw: Option<&Value>,
    slots: &Slots,
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
    let old = raw.map(|r| raw_setters(&r["setters"])).unwrap_or_default();
    let setter = old
        .iter()
        .find(|s| s.name.eq_ignore_ascii_case(&rarity))
        .map_or_else(
            || Setter::new(rarity.clone(), "true"),
            |s| Setter {
                value: "true".into(),
                ..s.clone()
            },
        );
    out.setters(Some(setter), &slots.setters);
    let mut tag = rarity.clone();
    tag[..1].make_ascii_uppercase();
    out.supports(Some(tag), &slots.supports);
    if raw.and_then(|r| stored(&r["data"])).is_some() {
        out.element.data.insert("rarity".into(), json!(rarity));
    }
    Ok(())
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
        Codec::Spell(f) => spell::write_raw(f, m, out),
        Codec::Feat(_) | Codec::Blocks => {}
    }
    Ok(())
}
