//! The species form's codecs (spec 4.4), for the 2024 `Species` and the 2014 `Race`, `Sub Race` and `Race Variant`.
//!
//! Most of what a species is lives in its rules, so these fields hold rules (`Out::rules`, placed back in the stored
//! order by the form's layout, `Memo::layout`):
//!
//! * **Size**: one size is a `grant` of `ID_SIZE_<SIZE>`; two or more are a `select_size` (the player chooses).
//! * **Speed**: `stat innate speed` (`innate speed:fly`, `:swim`, `:climb`, `:burrow`) with the value in feet, `bonus base`.
//! * **Darkvision**: the `grant` of `ID_VISION_DARKVISION` (60 feet), plus `stat darkvision:range` for another range.
//!   Blindsight, tremorsense and truesight are `sense` rules.
//! * **Ability score increases** (2014): `stat strength` +2 and so on (a choice is the Ability score increase block).
//! * **Languages** (2014): `grant`s of languages, and the player's choice of standard or exotic languages (`select`).
//! * **Traits**: the `grant` of each Racial Trait.
//!
//! A field takes a stored rule only when the rule is exactly what the field writes for some value, so everything else
//! (a trait from level 5, a speed with a condition) stays a block, and a field read and written again gives the same
//! rules. The 2024 mirrors `data.size`, `data.speed` and `data.traits` are written when the element has them (spec
//! 3.3); `data.creature_type` is the creature type's only location. The parent of a Sub Race or Race Variant is the
//! tag its race's choice offers it by (its first support tag). Random names are `names` setters with a `type`.

use super::super::value::RefValue;
use super::super::write::Out;
use super::super::{FieldMemo, FormContext, FormProblem, Slots};
use super::{chips, decode, decode_raw, memo, plain, present, stored, to_json, Reader};
use borealis_content::{Rule, Setter};
use serde_json::{json, Value};

/// Sizes in order, as `ID_SIZE_<UPPER>` names them.
pub(crate) const SIZES: [&str; 6] = ["Tiny", "Small", "Medium", "Large", "Huge", "Gargantuan"];
/// The highest speed or sense range a field takes, in feet.
const MAX_FEET: i64 = 1000;
/// The range the Darkvision element gives by itself.
const DARKVISION_BASE: i64 = 60;
const DARKVISION: &str = "ID_VISION_DARKVISION";
/// The languages the player's choice offers.
const ANY_LANGUAGE: &str = "Standard||Exotic";
/// The most extra languages a choice gives.
const MAX_LANGUAGES: i64 = 10;
/// Ability score increases go from -10 to +10.
const MAX_INCREASE: i64 = 10;
const NAMES: &str = "names";
/// Characters a tag can't hold (IM15).
const TAG_FORBIDDEN: &[char] = &[',', '|', '&', '!', '(', ')', '[', ']', '{', '}', '$'];

/// A kind of movement (`innate speed`, `innate speed:fly`...).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Movement {
    /// Walking (`innate speed`, mirrored in `data.speed`).
    Walk,
    /// Flying.
    Fly,
    /// Swimming.
    Swim,
    /// Climbing.
    Climb,
    /// Burrowing.
    Burrow,
}

impl Movement {
    fn statistic(self) -> &'static str {
        match self {
            Self::Walk => "innate speed",
            Self::Fly => "innate speed:fly",
            Self::Swim => "innate speed:swim",
            Self::Climb => "innate speed:climb",
            Self::Burrow => "innate speed:burrow",
        }
    }
}

/// A sense with a range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sense {
    /// The Darkvision element (+ `darkvision:range`).
    Darkvision,
    /// A `sense` rule.
    Blindsight,
    /// A `sense` rule.
    Tremorsense,
    /// A `sense` rule.
    Truesight,
}

impl Sense {
    /// The `sense` rule's name (none for darkvision, which is a Vision element).
    fn rule_name(self) -> Option<&'static str> {
        match self {
            Self::Darkvision => None,
            Self::Blindsight => Some("blindsight"),
            Self::Tremorsense => Some("tremorsense"),
            Self::Truesight => Some("truesight"),
        }
    }
}

/// A list of random names (the `type` attribute of a `names` setter).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameKind {
    /// `type="male"`.
    Male,
    /// `type="female"`.
    Female,
    /// `type="family"`.
    Family,
}

impl NameKind {
    fn attribute(self) -> &'static str {
        match self {
            Self::Male => "male",
            Self::Female => "female",
            Self::Family => "family",
        }
    }
}

/// A species field (`Codec::Species`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpeciesField {
    /// `data.creature_type`.
    CreatureType,
    /// A size `grant` or a `select_size`, + `data.size`.
    Size,
    /// `stat innate speed[:mode]`.
    Speed(Movement),
    /// Darkvision (Vision grant + range) or a `sense` rule.
    Sense(Sense),
    /// `grant`s of Racial Traits, + `data.traits`.
    Traits,
    /// `grant`s of languages.
    Languages,
    /// A `select` of standard or exotic languages.
    LanguageChoice,
    /// `stat <ability>` with a fixed increase (the lower-case ability name).
    Ability(&'static str),
    /// The first support tag: what the race's subrace or variant choice offers it by.
    Parent,
    /// A `names` setter of one type.
    Names(NameKind),
}

impl SpeciesField {
    /// The model locations, as the generated-data view shows them.
    pub(crate) fn bindings(self) -> Vec<String> {
        let v: Vec<String> = match self {
            Self::CreatureType => vec!["data.creature_type".into()],
            Self::Size => vec![
                "rule:grant Size".into(),
                "rule:select_size".into(),
                "data.size".into(),
            ],
            Self::Speed(Movement::Walk) => {
                vec!["rule:stat innate speed".into(), "data.speed".into()]
            }
            Self::Speed(m) => vec![format!("rule:stat {}", m.statistic())],
            Self::Sense(Sense::Darkvision) => vec![
                format!("rule:grant {DARKVISION}"),
                "rule:stat darkvision:range".into(),
            ],
            Self::Sense(s) => vec![format!("rule:sense {}", s.rule_name().unwrap_or_default())],
            Self::Traits => vec!["rule:grant Racial Trait".into(), "data.traits".into()],
            Self::Languages => vec!["rule:grant Language".into()],
            Self::LanguageChoice => vec!["rule:select Language".into()],
            Self::Ability(a) => vec![format!("rule:stat {a}")],
            Self::Parent => vec!["supports:<parent>".into()],
            Self::Names(k) => vec![format!("setter:names type={}", k.attribute())],
        };
        v
    }

    /// The setter names it takes, lower case (a problem on any `names` setter points at the first list).
    pub(crate) fn setter_names(self) -> Vec<&'static str> {
        match self {
            Self::Names(NameKind::Male) => vec![NAMES],
            _ => Vec::new(),
        }
    }

    /// The `data` keys it owns.
    pub(crate) fn data_keys(self) -> Vec<&'static str> {
        match self {
            Self::CreatureType => vec!["creature_type"],
            Self::Size => vec!["size"],
            Self::Speed(Movement::Walk) => vec!["speed"],
            Self::Traits => vec!["traits"],
            _ => Vec::new(),
        }
    }

    /// The mirror `data` key it writes when the element has one.
    fn mirror(self) -> Option<&'static str> {
        match self {
            Self::Size => Some("size"),
            Self::Speed(Movement::Walk) => Some("speed"),
            Self::Traits => Some("traits"),
            _ => None,
        }
    }
}

fn rule(v: Value) -> Rule {
    serde_json::from_value(v).expect("a canonical rule reads as a rule")
}

fn same(a: &Rule, b: &Rule) -> bool {
    to_json(a) == to_json(b)
}

fn size_rules(sizes: &[String]) -> Vec<Rule> {
    match sizes {
        [] => Vec::new(),
        [one] => vec![rule(json!({
            "kind": "grant",
            "id": format!("ID_SIZE_{}", one.to_uppercase()),
            "type": "Size",
        }))],
        many => vec![rule(
            json!({ "kind": "select_size", "name": "Size", "options": many }),
        )],
    }
}

fn speed_rule(m: Movement, feet: i64) -> Rule {
    rule(
        json!({ "kind": "stat", "name": m.statistic(), "value": feet.to_string(), "bonus": "base" }),
    )
}

fn sense_rules(s: Sense, feet: i64) -> Vec<Rule> {
    if feet <= 0 {
        return Vec::new();
    }
    match s.rule_name() {
        Some(name) => vec![rule(
            json!({ "kind": "sense", "sense": name, "range": feet }),
        )],
        None => {
            let mut out = vec![grant(DARKVISION, "Vision")];
            if feet != DARKVISION_BASE {
                out.push(rule(json!({
                    "kind": "stat",
                    "name": "darkvision:range",
                    "value": feet.to_string(),
                    "bonus": "base",
                })));
            }
            out
        }
    }
}

fn grant(id: &str, ty: &str) -> Rule {
    rule(json!({ "kind": "grant", "id": id, "type": ty }))
}

fn ability_rule(ability: &str, n: i64) -> Rule {
    rule(json!({ "kind": "stat", "name": ability, "value": n.to_string() }))
}

fn language_choice(name: &str, n: i64) -> Rule {
    let mut v =
        json!({ "kind": "select", "type": "Language", "name": name, "supports": ANY_LANGUAGE });
    if n != 1 {
        v["number"] = json!(n);
    }
    rule(v)
}

/// The element type a granting field grants.
fn granted_type(field: SpeciesField) -> &'static str {
    if field == SpeciesField::Traits {
        "Racial Trait"
    } else {
        "Language"
    }
}

/// The value a stored rule would be read as by `field`, when the field writes exactly that rule for it.
fn one_rule_value(field: SpeciesField, r: &Rule) -> Option<Value> {
    match (field, r) {
        (SpeciesField::Size, Rule::Grant(g)) => {
            let size = SIZES
                .iter()
                .find(|s| g.id == format!("ID_SIZE_{}", s.to_uppercase()))?;
            let v = vec![(*size).to_string()];
            same(r, &size_rules(&v)[0]).then(|| json!(v))
        }
        (SpeciesField::Size, Rule::Custom(c)) if c.kind == "select_size" => {
            let v: Vec<String> = decode_raw(c.fields.get("options")?)?;
            let ordered = ordered_sizes(&v).ok()?;
            (ordered.len() > 1 && same(r, &size_rules(&ordered)[0])).then(|| json!(ordered))
        }
        (SpeciesField::Speed(m), Rule::Stat(s)) if s.name == m.statistic() => {
            let n: i64 = s.value.parse().ok().filter(|n| *n > 0)?;
            same(r, &speed_rule(m, n)).then(|| json!(n))
        }
        (SpeciesField::Sense(sense), Rule::Custom(c)) if sense.rule_name().is_some() => {
            let n = c
                .fields
                .get("range")
                .and_then(Value::as_i64)
                .filter(|n| *n > 0)?;
            same(r, &sense_rules(sense, n)[0]).then(|| json!(n))
        }
        (SpeciesField::Ability(a), Rule::Stat(s)) if s.name == a => {
            let n: i64 = s.value.parse().ok().filter(|n| *n != 0)?;
            same(r, &ability_rule(a, n)).then(|| json!(n))
        }
        (SpeciesField::LanguageChoice, Rule::Select(s))
            if s.ty == "Language" && s.supports.as_deref() == Some(ANY_LANGUAGE) =>
        {
            let n = i64::from(s.number.unwrap_or(1));
            same(r, &language_choice(&s.name, n)).then(|| json!(n))
        }
        _ => None,
    }
}

/// Take the first free rule `field` reads a value from.
fn take_one(field: SpeciesField, r: &mut Reader<'_>) -> Option<(Value, Rule)> {
    let (i, value) = r
        .free_rules()
        .find_map(|(i, rule)| one_rule_value(field, rule).map(|v| (i, v)))?;
    Some((value, r.take_rule(i).clone()))
}

fn rules_memo(value: Value, rules: &[Rule], data: Option<&Value>) -> FieldMemo {
    plain(
        value,
        json!({ "rules": rules.iter().map(to_json).collect::<Vec<_>>(), "data": present(data) }),
    )
}

/// Read one species field.
pub(crate) fn read(field: SpeciesField, r: &mut Reader<'_>, cx: &FormContext<'_>) -> FieldMemo {
    let data = field.mirror().and_then(|k| r.take_data(k));
    match field {
        SpeciesField::CreatureType => {
            let v = r.take_data("creature_type");
            plain(
                json!(v.and_then(Value::as_str).unwrap_or_default()),
                present(v),
            )
        }
        SpeciesField::Size
        | SpeciesField::Speed(_)
        | SpeciesField::Ability(_)
        | SpeciesField::LanguageChoice => {
            let none = if field == SpeciesField::Size {
                json!([])
            } else {
                json!(0)
            };
            match take_one(field, r) {
                Some((v, rule)) => rules_memo(v, &[rule], data),
                None => rules_memo(none, &[], data),
            }
        }
        SpeciesField::Sense(Sense::Darkvision) => read_darkvision(r),
        SpeciesField::Sense(_) => match take_one(field, r) {
            Some((v, rule)) => rules_memo(v, &[rule], None),
            None => rules_memo(json!(0), &[], None),
        },
        SpeciesField::Traits | SpeciesField::Languages => {
            let ty = granted_type(field);
            let found: Vec<(usize, String)> = r
                .free_rules()
                .filter_map(|(i, rule)| match rule {
                    Rule::Grant(g) if same(rule, &grant(&g.id, ty)) => Some((i, g.id.clone())),
                    _ => None,
                })
                .collect();
            let rules: Vec<Rule> = found.iter().map(|(i, _)| r.take_rule(*i).clone()).collect();
            let refs: Vec<RefValue> = found
                .iter()
                .map(|(_, id)| RefValue::resolve(id, cx.lookup))
                .collect();
            rules_memo(to_json(&refs), &rules, data)
        }
        SpeciesField::Parent => {
            let e = r.e;
            match (0..e.supports.len()).find(|&i| !r.supports_taken[i]) {
                Some(i) => {
                    r.supports_taken[i] = true;
                    memo(
                        json!(e.supports[i]),
                        json!(e.supports[i]),
                        Slots {
                            supports: vec![i],
                            ..Slots::default()
                        },
                    )
                }
                None => plain(json!(""), Value::Null),
            }
        }
        SpeciesField::Names(k) => {
            let e = r.e;
            let found = (0..e.setters.len()).find(|&i| {
                let s = &e.setters[i];
                !r.setters_taken[i]
                    && s.name.eq_ignore_ascii_case(NAMES)
                    && s.attribute("type")
                        .is_some_and(|t| t.eq_ignore_ascii_case(k.attribute()))
            });
            match found {
                Some(i) => {
                    r.setters_taken[i] = true;
                    let s = &e.setters[i];
                    memo(
                        json!(chips(&s.value)),
                        to_json(s),
                        Slots {
                            setters: vec![i],
                            ..Slots::default()
                        },
                    )
                }
                None => plain(json!([]), Value::Null),
            }
        }
    }
}

fn read_darkvision(r: &mut Reader<'_>) -> FieldMemo {
    let vision = grant(DARKVISION, "Vision");
    let Some(i) = r
        .free_rules()
        .find(|(_, rule)| same(rule, &vision))
        .map(|(i, _)| i)
    else {
        return rules_memo(json!(0), &[], None);
    };
    let mut rules = vec![r.take_rule(i).clone()];
    let range = r.free_rules().find_map(|(j, rule)| match rule {
        Rule::Stat(s) if s.name == "darkvision:range" => {
            let n: i64 = s.value.parse().ok()?;
            let written = sense_rules(Sense::Darkvision, n);
            (n != DARKVISION_BASE && written.len() == 2 && same(rule, &written[1]))
                .then_some((j, n))
        }
        _ => None,
    });
    let feet = match range {
        Some((j, n)) => {
            rules.push(r.take_rule(j).clone());
            n
        }
        None => DARKVISION_BASE,
    };
    rules_memo(json!(feet), &rules, None)
}

/// The chosen sizes in size order, each once.
fn ordered_sizes(chosen: &[String]) -> Result<Vec<String>, ()> {
    if chosen.iter().any(|c| !SIZES.contains(&c.as_str())) {
        return Err(());
    }
    Ok(SIZES
        .iter()
        .filter(|s| chosen.iter().any(|c| c == *s))
        .map(|s| (*s).to_string())
        .collect())
}

fn number(value: &Value, path: &str, min: i64, max: i64, says: &str) -> Result<i64, FormProblem> {
    let n: i64 = decode(value, path)?;
    if (min..=max).contains(&n) {
        Ok(n)
    } else {
        Err(FormProblem::new(path, says))
    }
}

fn feet(value: &Value, path: &str) -> Result<i64, FormProblem> {
    number(
        value,
        path,
        0,
        MAX_FEET,
        &format!("Write it in feet, from 0 to {MAX_FEET}."),
    )
}

/// The trait ids of the `data.traits` mirror after an edit: the stored list without the traits the field no longer
/// grants (traits other blocks grant stay), then the new ones.
fn traits_mirror(stored_ids: &[String], before: &[String], now: &[String]) -> Vec<String> {
    let mut out: Vec<String> = stored_ids
        .iter()
        .filter(|id| now.contains(id) || !before.contains(id))
        .cloned()
        .collect();
    for id in now {
        if !out.contains(id) {
            out.push(id.clone());
        }
    }
    out
}

fn ref_ids(refs: &[RefValue]) -> Vec<String> {
    let mut ids: Vec<String> = Vec::new();
    for r in refs {
        let id = r.id.trim();
        if !id.is_empty() && !ids.iter().any(|x| x == id) {
            ids.push(id.to_string());
        }
    }
    ids
}

/// Write an edited species field.
pub(crate) fn write(
    field: SpeciesField,
    value: &Value,
    memo: Option<&FieldMemo>,
    path: &str,
    out: &mut Out,
) -> Result<(), FormProblem> {
    let raw = memo.map(|m| &m.raw);
    let slots = memo.map(|m| m.slots.clone()).unwrap_or_default();
    let has_mirror = raw.and_then(|r| stored(&r["data"])).is_some();
    let mirror = |out: &mut Out, v: Value| {
        if let Some(k) = field.mirror() {
            if has_mirror {
                out.element.data.insert(k.to_string(), v);
            }
        }
    };
    match field {
        SpeciesField::CreatureType => {
            let t: String = decode(value, path)?;
            let t = t.trim();
            if !t.is_empty() {
                out.element.data.insert("creature_type".into(), json!(t));
            }
        }
        SpeciesField::Size => {
            let chosen: Vec<String> = decode(value, path)?;
            let sizes = ordered_sizes(&chosen)
                .map_err(|()| FormProblem::new(path, "Choose sizes from the list."))?;
            out.rules(size_rules(&sizes));
            if !sizes.is_empty() {
                mirror(out, json!(sizes));
            }
        }
        SpeciesField::Speed(m) => {
            let n = feet(value, path)?;
            if n > 0 {
                out.rules([speed_rule(m, n)]);
                mirror(out, json!(n));
            }
        }
        SpeciesField::Sense(s) => {
            let n = feet(value, path)?;
            out.rules(sense_rules(s, n));
        }
        SpeciesField::Ability(a) => {
            let n = number(
                value,
                path,
                -MAX_INCREASE,
                MAX_INCREASE,
                &format!("An increase goes from -{MAX_INCREASE} to {MAX_INCREASE}."),
            )?;
            if n != 0 {
                out.rules([ability_rule(a, n)]);
            }
        }
        SpeciesField::LanguageChoice => {
            let n = number(
                value,
                path,
                0,
                MAX_LANGUAGES,
                &format!("Choose from 0 to {MAX_LANGUAGES} languages."),
            )?;
            if n > 0 {
                // the stored label is the choice's persistence key: keep it across renames
                let stored_name = raw
                    .and_then(|r| r["rules"].get(0))
                    .and_then(|r| r["name"].as_str())
                    .map(str::to_string);
                let name = stored_name
                    .unwrap_or_else(|| format!("Language ({})", out.element.name.trim()));
                out.rules([language_choice(&name, n)]);
            }
        }
        SpeciesField::Traits | SpeciesField::Languages => {
            let refs: Vec<RefValue> = decode(value, path)?;
            let ids = ref_ids(&refs);
            let ty = granted_type(field);
            out.rules(ids.iter().map(|id| grant(id, ty)));
            if has_mirror {
                let stored_ids: Vec<String> = raw
                    .and_then(|r| stored(&r["data"]))
                    .and_then(decode_raw)
                    .unwrap_or_default();
                let before: Vec<RefValue> =
                    memo.and_then(|m| decode_raw(&m.value)).unwrap_or_default();
                mirror(
                    out,
                    json!(traits_mirror(&stored_ids, &ref_ids(&before), &ids)),
                );
            }
        }
        SpeciesField::Parent => {
            let t: String = decode(value, path)?;
            let t = t.trim();
            if t.contains(TAG_FORBIDDEN) {
                return Err(FormProblem::new(
                    path,
                    "A tag can't contain , | & ! ( ) [ ] { } or $.",
                ));
            }
            if !t.is_empty() {
                out.supports(Some(t.to_string()), &slots.supports);
            }
        }
        SpeciesField::Names(k) => {
            let items: Vec<String> = decode(value, path)?;
            let t = chips(&items.join(",")).join(", ");
            if !t.is_empty() {
                let setter = match raw.and_then(decode_raw::<Setter>) {
                    Some(old) => Setter { value: t, ..old },
                    None => {
                        let mut s = Setter::new(NAMES, t);
                        s.attributes.insert("type".into(), k.attribute().into());
                        s
                    }
                };
                out.setters(Some(setter), &slots.setters);
            }
        }
    }
    Ok(())
}

/// Put the stored bytes of an unchanged species field back.
pub(crate) fn write_raw(field: SpeciesField, m: &FieldMemo, out: &mut Out) {
    let raw = &m.raw;
    match field {
        SpeciesField::CreatureType => {
            if let Some(v) = stored(raw) {
                out.element.data.insert("creature_type".into(), v.clone());
            }
        }
        SpeciesField::Parent => {
            if let Some(t) = raw.as_str() {
                out.supports(Some(t.to_string()), &m.slots.supports);
            }
        }
        SpeciesField::Names(_) => {
            if let Some(s) = decode_raw::<Setter>(raw) {
                out.setters(Some(s), &m.slots.setters);
            }
        }
        _ => {
            let rules: Vec<Rule> = decode_raw(&raw["rules"]).unwrap_or_default();
            out.rules(rules);
            if let (Some(k), Some(v)) = (field.mirror(), stored(&raw["data"])) {
                out.element.data.insert(k.to_string(), v.clone());
            }
        }
    }
}

#[cfg(test)]
#[path = "species_tests.rs"]
mod tests;
