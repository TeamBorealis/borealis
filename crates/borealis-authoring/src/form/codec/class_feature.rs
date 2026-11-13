//! The generated class features (spec 4.5): what a class's Ability Score Improvement, subclass choice and 2024 epic
//! boon features hold, written from a value and the class they belong to, in the SRD's shapes:
//!
//! * **Ability score improvements at levels**: 2024, one `select_feat` per level (`"Ability Score Improvement
//!   (<Class> N)"`, category General, the Ability Score Improvement feat suggested); 2014, one `select` of the class's
//!   improvement options per level (`"Improvement Option (<Class> N)"`, supports `Improvement Option,<Class>,N`: the
//!   engine makes those options for every class by name).
//! * **Subclass choice**: a `select` of type `Archetype` named with its label ("Sacred Oath"); supports
//!   `"<Class> Subclass"` (2024) or the label (2014, also gated at the feature's level). A deep copy of a 2014 class
//!   keeps the 2014 shape with a tag of its own, `"<Class> Subclass"` (P3 Hy6pc: the label is shared with the
//!   original, which would offer the copied subclasses).
//! * **Epic boon**: a `select_feat` of category Epic Boon (`"Epic Boon (<Class>)"`) at the feature's level.
//!
//! The class's name, edition and the feature's level come from the parent ([`super::super::ParentInfo`]); a rule
//! is taken only when it is exactly what the field writes for some class, so an unchanged field writes the same
//! bytes. Without a parent (a feature written on its own), the stored class name and level are used.

use super::super::write::Out;
use super::super::{FieldMemo, FormProblem, Slots};
use super::{decode, to_json, Reader};
use borealis_content::{Rule, Ruleset};
use serde_json::{json, Value};

/// A generated feature's field (`Codec::Feature`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeatureField {
    /// The levels of the ability score improvement choices.
    Asi,
    /// The subclass choice's label.
    Subclass,
    /// The epic boon choice.
    EpicBoon,
}

/// The 2024 Ability Score Improvement feat, suggested at each choice (as the SRD classes do).
const ASI_FEAT: &str = "ID_2024_FEAT_ABILITY_SCORE_IMPROVEMENT";

/// The SRD shape a generated rule is written in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shape {
    /// 2014 (Aurora's).
    Old,
    /// 2014, gated at the feature's level, offering `"<Class> Subclass"` (a copied 2014 class's own tag).
    OldOwn,
    /// 2024 (SRD 5.2.1).
    New,
}

impl FeatureField {
    /// The model locations, as the generated-data view shows them.
    pub(crate) fn bindings(self) -> Vec<String> {
        let v: &[&str] = match self {
            Self::Asi => &[
                "rules:select_feat Ability Score Improvement (<Class> N)",
                "rules:select Improvement Option (<Class> N)",
            ],
            Self::Subclass => &["rules:select Archetype"],
            Self::EpicBoon => &["rules:select_feat Epic Boon (<Class>)"],
        };
        v.iter().map(|s| (*s).to_string()).collect()
    }
}

fn asi_rule(shape: Shape, class: &str, level: u32) -> Rule {
    let v = match shape {
        Shape::New => json!({"kind": "select_feat", "level": level,
            "name": format!("Ability Score Improvement ({class} {level})"), "category": "General", "default": ASI_FEAT}),
        Shape::Old | Shape::OldOwn => json!({"kind": "select", "type": "Class Feature",
            "name": format!("Improvement Option ({class} {level})"), "level": level,
            "supports": format!("Improvement Option,{class},{level}")}),
    };
    serde_json::from_value(v).expect("a valid rule")
}

/// A class name as a tag word: without the characters a tag can't hold (a copy's "Wizard (copy)" offers
/// "Wizard copy Subclass").
pub(crate) fn tag_word(class: &str) -> String {
    class
        .chars()
        .filter(|c| !",|&!()[]{}$".contains(*c))
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn subclass_rule(shape: Shape, class: &str, label: &str, level: u32) -> Rule {
    let v = match shape {
        Shape::New => json!({"kind": "select", "type": "Archetype", "name": label,
            "supports": format!("{} Subclass", tag_word(class))}),
        Shape::Old => {
            json!({"kind": "select", "type": "Archetype", "name": label, "level": level, "supports": label})
        }
        Shape::OldOwn => {
            json!({"kind": "select", "type": "Archetype", "name": label, "level": level,
            "supports": format!("{} Subclass", tag_word(class))})
        }
    };
    serde_json::from_value(v).expect("a valid rule")
}

fn epic_rule(class: &str, level: u32) -> Rule {
    serde_json::from_value(json!({"kind": "select_feat", "level": level,
        "name": format!("Epic Boon ({class})"), "category": "Epic Boon"}))
    .expect("a valid rule")
}

fn same(a: &Rule, b: &Rule) -> bool {
    to_json(a) == to_json(b)
}

fn text_of<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(Value::as_str)
}

fn level_of(v: &Value) -> Option<u32> {
    v.get("level")
        .and_then(Value::as_u64)
        .and_then(|n| u32::try_from(n).ok())
}

/// `"<prefix> (<Class> <n>)"` → (Class, n).
fn class_and_level(name: &str, prefix: &str) -> Option<(String, u32)> {
    let inner = name
        .strip_prefix(prefix)?
        .strip_prefix(" (")?
        .strip_suffix(')')?;
    let (class, n) = inner.rsplit_once(' ')?;
    Some((class.to_string(), n.parse().ok()?))
}

/// What a stored rule is, as this field reads it: (shape, class, level, label).
fn recognise(field: FeatureField, rule: &Rule) -> Option<(Shape, String, u32, String)> {
    let v = to_json(rule);
    let name = text_of(&v, "name")?;
    let found = match field {
        FeatureField::Asi => [
            ("Ability Score Improvement", Shape::New),
            ("Improvement Option", Shape::Old),
        ]
        .into_iter()
        .find_map(|(p, s)| class_and_level(name, p).map(|(c, l)| (s, c, l, String::new()))),
        FeatureField::Subclass => {
            let supports = text_of(&v, "supports")?;
            if supports == name {
                Some((Shape::Old, String::new(), level_of(&v)?, name.to_string()))
            } else {
                let class = supports.strip_suffix(" Subclass")?;
                match level_of(&v) {
                    Some(level) => {
                        Some((Shape::OldOwn, class.to_string(), level, name.to_string()))
                    }
                    None => Some((Shape::New, class.to_string(), 0, name.to_string())),
                }
            }
        }
        FeatureField::EpicBoon => {
            let class = name.strip_prefix("Epic Boon (")?.strip_suffix(')')?;
            Some((Shape::New, class.to_string(), level_of(&v)?, String::new()))
        }
    }?;
    let (shape, class, level, label) = &found;
    let again = match field {
        FeatureField::Asi => asi_rule(*shape, class, *level),
        FeatureField::Subclass => subclass_rule(*shape, class, label, *level),
        FeatureField::EpicBoon => epic_rule(class, *level),
    };
    same(&again, rule).then_some(found)
}

/// Read one generated-feature field.
pub(crate) fn read(field: FeatureField, r: &mut Reader<'_>) -> FieldMemo {
    let e = r.e;
    let mut slots = Vec::new();
    let mut found: Vec<(Shape, String, u32, String)> = Vec::new();
    for (i, rule) in e.rules.iter().enumerate() {
        if r.rules_taken[i] || (field != FeatureField::Asi && !found.is_empty()) {
            continue;
        }
        let Some(f) = recognise(field, rule) else {
            continue;
        };
        let fits = found
            .first()
            .is_none_or(|g| g.0 == f.0 && g.1 == f.1 && !found.iter().any(|x| x.2 == f.2));
        if fits {
            r.rules_taken[i] = true;
            slots.push(i);
            found.push(f);
        }
    }
    let first = found.first();
    let value = match field {
        FeatureField::Asi => json!(found.iter().map(|f| f.2).collect::<Vec<_>>()),
        FeatureField::Subclass => json!(first.map(|f| f.3.clone()).unwrap_or_default()),
        FeatureField::EpicBoon => json!(first.is_some()),
    };
    let raw = json!({
        "class": first.map(|f| f.1.clone()),
        "old": first.map(|f| f.0 != Shape::New),
        "own": first.map(|f| f.0 == Shape::OldOwn),
        "level": first.map(|f| f.2),
    });
    FieldMemo {
        value,
        raw,
        slots: Slots {
            rules: slots,
            ..Slots::default()
        },
    }
}

/// Write one generated-feature field.
pub(crate) fn write(
    field: FeatureField,
    value: &Value,
    memo: Option<&FieldMemo>,
    path: &str,
    out: &mut Out,
) -> Result<(), FormProblem> {
    let raw = memo.map(|m| &m.raw);
    let stored_class = raw
        .and_then(|r| text_of(r, "class"))
        .filter(|c| !c.is_empty());
    let parent = out.parent.clone();
    let class = parent
        .as_ref()
        .map(|p| p.name.clone())
        .filter(|n| !n.is_empty())
        .or_else(|| stored_class.map(str::to_string))
        .unwrap_or_default();
    let old = raw.and_then(|r| r.get("old")).and_then(Value::as_bool);
    let own = raw.and_then(|r| r.get("own")).and_then(Value::as_bool) == Some(true);
    // the stored shape, else the class's edition (a class without one is a 2014 class, as the SRD stores them)
    let edition = parent.as_ref().map_or(out.element.ruleset, |p| p.edition);
    let shape = match old {
        Some(true) if own => Shape::OldOwn,
        Some(true) => Shape::Old,
        Some(false) => Shape::New,
        None if edition == Some(Ruleset::Dnd2024) => Shape::New,
        None => Shape::Old,
    };
    let level = parent
        .as_ref()
        .and_then(|p| p.level)
        .or_else(|| raw.and_then(level_of).filter(|l| *l > 0));
    let slots = memo.map(|m| m.slots.rules.clone()).unwrap_or_default();
    // a feature written on its own with nothing stored (only the canonical-writer check does that) keeps its shape
    let needs_class = || {
        if class.is_empty() && parent.is_some() {
            Err(FormProblem::new(path, "Give the class a name first."))
        } else {
            Ok(())
        }
    };
    let rules: Vec<Rule> = match field {
        FeatureField::Asi => {
            let mut levels: Vec<u32> = decode(value, path)?;
            if levels.iter().any(|l| !(1..=20).contains(l)) {
                return Err(FormProblem::new(
                    path,
                    "Ability score improvements are at levels 1 to 20.",
                ));
            }
            levels.sort_unstable();
            levels.dedup();
            if !levels.is_empty() {
                needs_class()?;
            }
            levels.iter().map(|l| asi_rule(shape, &class, *l)).collect()
        }
        FeatureField::Subclass => {
            let label =
                borealis_content::rich::normalize_plain_text(&decode::<String>(value, path)?);
            if label.is_empty() {
                Vec::new()
            } else {
                if shape != Shape::Old {
                    needs_class()?;
                }
                vec![subclass_rule(shape, &class, &label, level.unwrap_or(3))]
            }
        }
        FeatureField::EpicBoon => {
            if decode::<bool>(value, path)? {
                needs_class()?;
                vec![epic_rule(&class, level.unwrap_or(19))]
            } else {
                Vec::new()
            }
        }
    };
    out.field_rules(rules.into_iter().map(|r| (0, r)), &slots, path);
    Ok(())
}

/// A deep copy of a 2014 class (P3 Hy6pc): its subclass choice, read in the 2014 shape (offering its label), offers the
/// copy's own tag instead (`"<Class> Subclass"`, written from the copy's name). False when `memo` is no 2014 choice.
pub(crate) fn own_subclass_tag(memo: &mut FieldMemo) -> bool {
    let old = memo.raw.get("old").and_then(Value::as_bool) == Some(true);
    if !old || memo.value.as_str().is_none_or(str::is_empty) {
        return false;
    }
    if let Some(raw) = memo.raw.as_object_mut() {
        raw.insert("own".into(), json!(true));
    }
    true
}
