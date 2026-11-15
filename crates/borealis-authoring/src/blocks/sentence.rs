//! One plain sentence per block (spec 6: "Gives proficiency: Stealth", "From level 5: lets the player choose ...").
//! Ids are shown by the element's name, statistics by their label, tokens and expressions in words.

use super::choices::{self, label_of};
use super::words::{self, and_list, cap, element, or_list, token};
use super::{block, BlockField, BlockSpec, Control};
use borealis_content::rules::{GrantRule, Rule, SelectRule, StatRule};
use serde_json::{Map, Value};

/// What a sentence needs from the loaded content.
pub trait SentenceContext {
    /// The name and element type of an element id, when it is loaded (or a draft of the pack).
    fn element(&self, id: &str) -> Option<(String, String)>;
    /// The class names loaded content defines (conditions read `[warlock:2]` as a class level only for these).
    fn class_names(&self) -> Vec<String> {
        Vec::new()
    }
}

/// A context that knows no element: every reference reads "a missing element".
#[derive(Debug, Clone, Copy, Default)]
pub struct NoNames;

impl SentenceContext for NoNames {
    fn element(&self, _id: &str) -> Option<(String, String)> {
        None
    }
}

/// The sentence of a rule.
#[must_use]
pub fn sentence(rule: &Rule, cx: &dyn SentenceContext) -> String {
    let (body, level, requirements) = match rule {
        Rule::Grant(g) => (grant(g, cx), g.level, g.requirements.as_deref()),
        Rule::Select(s) => (select(s, cx), s.level, s.requirements.as_deref()),
        Rule::Stat(s) => (stat(s, cx), s.level, s.requirements.as_deref()),
        Rule::Unknown(_) => (KEPT.to_string(), None, None),
        Rule::Custom(c) => match block(&c.kind) {
            Some(b) => (
                custom(b, &c.fields, cx),
                c.fields
                    .get("level")
                    .and_then(Value::as_u64)
                    .and_then(|n| u32::try_from(n).ok()),
                c.fields.get("requirements").and_then(Value::as_str),
            ),
            None => (KEPT.to_string(), None, None),
        },
    };
    let mut out = match level {
        Some(n) if n > 1 => format!("From level {n}: {}", lower_first(&body)),
        _ => body,
    };
    if let Some(r) = requirements.filter(|r| !r.trim().is_empty()) {
        out.push_str(", only if ");
        out.push_str(&words::condition(cx, r));
    }
    out
}

const KEPT: &str = "Kept as is: a rule this creator does not edit";

/// Lower-cases the first letter unless the first word is a name (two capitals in a row or a capital after it).
fn lower_first(s: &str) -> String {
    let first_word = s.split(' ').next().unwrap_or("");
    let is_name = first_word.chars().skip(1).any(char::is_uppercase);
    if is_name {
        return s.to_string();
    }
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_lowercase().chain(c).collect())
        .unwrap_or_default()
}

fn grant(g: &GrantRule, cx: &dyn SentenceContext) -> String {
    let (name, ty) = cx
        .element(&g.id)
        .unwrap_or_else(|| ("a missing element".to_string(), String::new()));
    let mut out = match ty.as_str() {
        "Proficiency" => format!("Gives proficiency: {name}"),
        "Language" => format!("Gives a language: {name}"),
        "Spell" => format!("Gives the spell {name}"),
        "Size" => format!("Size: {name}"),
        _ => format!("Gives {name}"),
    };
    if g.prepared == Some(true) {
        out.push_str(", always prepared");
    }
    if let Some(c) = g.spellcasting.as_deref().filter(|c| !c.is_empty()) {
        out.push_str(&format!(", as a {c} spell"));
    }
    out
}

fn select(s: &SelectRule, cx: &dyn SentenceContext) -> String {
    let n = s.number.unwrap_or(1);
    let picks = if n > 1 {
        format!(" ({n} picks)")
    } else {
        String::new()
    };
    let mut out = if s.ty == "List" {
        format!(
            "Lets the player pick from a written list: {}{picks}",
            s.name
        )
    } else {
        format!("Lets the player choose: {}{picks}", s.name)
    };
    if s.ty != "List" {
        if let Some(sup) = s.supports.as_deref().filter(|t| !t.trim().is_empty()) {
            out.push_str(&format!(", from {}", words::tag_filter(cx, sup)));
        }
    }
    if s.optional == Some(true) {
        out.push_str(", may stay empty");
    }
    out
}

fn stat(s: &StatRule, cx: &dyn SentenceContext) -> String {
    let mut out = if s.inline == Some(true) {
        format!("Sets {} to a text value", crate::stats::label(&s.name))
    } else {
        format!(
            "Changes {}: {}",
            crate::stats::label(&s.name),
            words::stat_value(&s.value)
        )
    };
    if let Some(e) = s.equipped.as_deref().filter(|e| !e.trim().is_empty()) {
        out.push_str(&format!(", while {}", words::equipped(cx, e)));
    }
    if let Some(m) = s.maximum.as_deref().filter(|m| !m.trim().is_empty()) {
        out.push_str(&format!(
            ", at most {}",
            words::stat_value(m).trim_start_matches('+')
        ));
    }
    out
}

/// The words of one stored value of a field; `None` for an empty value or an unticked box.
fn value_words(f: &BlockField, v: &Value, cx: &dyn SentenceContext) -> Option<String> {
    let text = v.as_str().map(str::trim).filter(|t| !t.is_empty());
    let list = || -> Vec<String> {
        v.as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_else(|| text.map(|t| vec![t.to_string()]).unwrap_or_default())
    };
    Some(match &f.control {
        Control::Auto => return None,
        Control::Check => return (v.as_bool() == Some(true)).then(|| lower_first(f.label)),
        Control::Number { .. } | Control::NumberOr { .. } => match v.as_i64() {
            Some(n) => n.to_string(),
            None => match &f.control {
                Control::NumberOr { words, .. } => {
                    let t = text?;
                    label_of(words, t).map_or_else(|| t.to_string(), str::to_lowercase)
                }
                _ => return None,
            },
        },
        Control::Numbers { .. } => {
            let n: Vec<String> = v.as_array()?.iter().map(ToString::to_string).collect();
            and_list(&n)
        }
        Control::Combo { choices, .. }
        | Control::Skills {
            choices,
            multiple: false,
        } => {
            let t = text?;
            label_of(choices, t).map_or_else(|| token(t), str::to_string)
        }
        Control::Chips { choices, .. }
        | Control::Skills {
            choices,
            multiple: true,
        } => {
            let l: Vec<String> = list()
                .iter()
                .map(|t| label_of(choices, t).map_or_else(|| token(t), str::to_string))
                .collect();
            if l.is_empty() {
                return None;
            }
            and_list(&l)
        }
        Control::Reference { .. } => element(cx, text?),
        Control::References { .. } => {
            let l: Vec<String> = list().iter().map(|id| element(cx, id)).collect();
            if l.is_empty() {
                return None;
            }
            and_list(&l)
        }
        Control::Name { .. } | Control::Text | Control::Multiline | Control::Dice => {
            let l = list();
            if l.is_empty() {
                return None;
            }
            and_list(&l)
        }
        Control::ClassCounter => format!("the class table's {} column", token(text?)),
        Control::Statistic => crate::stats::label(text?),
        Control::StatValue => words::stat_value(text?),
        Control::NumberOrStatistic => match v.as_i64() {
            Some(n) => n.to_string(),
            None => crate::stats::label(v.get("stat")?.as_str()?),
        },
        Control::Condition => words::condition(cx, text?),
        Control::TagFilter => words::tag_filter(cx, text?),
        Control::Equipped => words::equipped(cx, text?),
        Control::Formula(d) => match v.as_i64() {
            Some(n) => n.to_string(),
            None => words::formula(text?, *d),
        },
        Control::Formulas(d) => {
            let l: Vec<String> = list().iter().map(|t| words::formula(t, *d)).collect();
            if l.is_empty() {
                return None;
            }
            l.join(" + ")
        }
        Control::Uses => uses(v)?,
        Control::AbilitySource => match v.get("param").and_then(Value::as_str) {
            Some(p) => format!("the ability chosen for {p}"),
            None => cap(text?),
        },
        Control::AbilityChoice => ability_choice(v)?,
        Control::Splits => splits(v)?,
        Control::Group(parts) => {
            let l: Vec<String> = parts
                .iter()
                .filter_map(|p| value_words(p, v.get(p.key)?, cx))
                .collect();
            if l.is_empty() {
                return None;
            }
            l.join(", ")
        }
        Control::Rows(_) => {
            let n = v.as_array()?.len();
            format!("{n} {}", if n == 1 { "row" } else { "rows" })
        }
        Control::Variant(variants) => {
            let ty = v.get("type")?.as_str()?;
            let x = variants.iter().find(|x| x.value == ty)?;
            let l: Vec<String> = x
                .fields
                .iter()
                .filter_map(|p| value_words(p, v.get(p.key)?, cx))
                .collect();
            l.join(", ")
        }
        Control::Map([key, value]) => {
            let mut entries: Vec<(&String, &Value)> = v.as_object()?.iter().collect();
            if matches!(key.control, Control::Number { .. }) {
                entries.sort_by_key(|(k, _)| k.parse::<i64>().unwrap_or(i64::MAX));
            }
            let l: Vec<String> = entries
                .into_iter()
                .filter_map(|(k, x)| {
                    let kw = match &key.control {
                        Control::Number { .. } => format!("{} {k}", lower_first(key.label)),
                        Control::Combo { choices, .. } => {
                            label_of(choices, k).map_or_else(|| token(k), str::to_string)
                        }
                        _ => k.clone(),
                    };
                    Some(format!("{kw}: {}", value_words(value, x, cx)?))
                })
                .collect();
            if l.is_empty() {
                return None;
            }
            l.join(", ")
        }
        Control::Blocks(_) => {
            let l: Vec<String> = v
                .as_array()?
                .iter()
                .filter_map(|e| serde_json::from_value::<Rule>(e.clone()).ok())
                .map(|r| lower_first(&sentence(&r, cx)))
                .collect();
            if l.is_empty() {
                return None;
            }
            l.join("; ")
        }
    })
}

fn uses(v: &Value) -> Option<String> {
    Some(match v {
        Value::Number(n) => n.to_string(),
        Value::String(s) if s == "proficiency" => "proficiency bonus".to_string(),
        Value::String(s) => format!("{} modifier", cap(s)),
        Value::Object(m) => format!(
            "the class table's {} column",
            token(m.get("key")?.as_str()?)
        ),
        _ => return None,
    })
}

/// "1 free cast per long rest", "free casts equal to the proficiency bonus per long rest".
fn free_cast(v: Option<&Value>) -> Option<String> {
    let v = v?;
    let per = v
        .get("recharge")
        .and_then(Value::as_str)
        .map(|r| {
            format!(
                " per {}",
                label_of(choices::RECHARGE, r).map_or_else(|| token(r), str::to_lowercase)
            )
        })
        .unwrap_or_default();
    Some(match v.get("uses")? {
        Value::Number(n) if n.as_i64() == Some(1) => format!("1 free cast{per}"),
        Value::Number(n) => format!("{n} free casts{per}"),
        u => format!("free casts equal to {}{per}", uses(u)?),
    })
}

fn ability_choice(v: &Value) -> Option<String> {
    Some(match v {
        Value::Number(n) if n.as_i64() == Some(1) => "any ability".to_string(),
        Value::Number(n) => format!("any {n} abilities"),
        Value::Array(a) => {
            let l: Vec<String> = a.iter().filter_map(Value::as_str).map(cap).collect();
            or_list(&l)
        }
        _ => return None,
    })
}

fn splits(v: &Value) -> Option<String> {
    let l: Vec<String> = v
        .as_array()?
        .iter()
        .filter_map(Value::as_array)
        .map(|p| {
            p.iter()
                .map(|n| format!("+{n}"))
                .collect::<Vec<_>>()
                .join("/")
        })
        .collect();
    Some(or_list(&l))
}

struct Fields<'a> {
    b: &'a BlockSpec,
    m: &'a Map<String, Value>,
    cx: &'a dyn SentenceContext,
}

impl Fields<'_> {
    fn words(&self, key: &str) -> Option<String> {
        let f = self.b.field(key)?;
        value_words(f, self.m.get(key)?, self.cx)
    }

    fn text(&self, key: &str) -> Option<&str> {
        self.m
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|t| !t.is_empty())
    }

    fn or(&self, key: &str, fallback: &str) -> String {
        self.words(key).unwrap_or_else(|| fallback.to_string())
    }
}

fn plural(n: &str, one: &str, many: &str) -> String {
    if n == "1" {
        format!("{n} {one}")
    } else {
        format!("{n} {many}")
    }
}

fn custom(b: &BlockSpec, m: &Map<String, Value>, cx: &dyn SentenceContext) -> String {
    let f = Fields { b, m, cx };
    let w = |k: &str| f.words(k);
    let label = b.label;
    let sentence = match b.kind {
        "resource" => {
            let name = f.text("name").or_else(|| f.text("id")).unwrap_or("a pool");
            let mut s = format!("Limited uses: {name}");
            if let Some(max) = w("max").filter(|t| !t.is_empty()) {
                s.push_str(&format!(" ({max})"));
            }
            if let Some(r) = w("recharge") {
                s.push_str(&format!(", regained on {}", r.to_lowercase()));
            }
            s
        }
        "resource_cost" => format!(
            "Costs {} of {}",
            plural(&f.or("cost", "1"), "use", "uses"),
            f.or("resource", "a pool")
        ),
        "resource_regain" => {
            let mut s = format!("Regains uses of {}", f.or("resource", "a pool"));
            if let Some(a) = w("amount") {
                s = format!("Regains {a} of {}", f.or("resource", "a pool"));
            }
            if let Some(on) = w("on") {
                s.push_str(&format!(" on {}", on.to_lowercase()));
            }
            s
        }
        "grant_spell" => {
            let mut s = format!("Gives the spell {}", f.or("spell", "a missing element"));
            if let Some(mode) = w("mode") {
                s.push_str(&format!(" ({})", mode.to_lowercase()));
            }
            if let Some(fc) = free_cast(m.get("free_cast")) {
                s.push_str(&format!(", {fc}"));
            }
            s
        }
        "grant_feat" => format!("Gives the feat {}", f.or("feat", "a missing element")),
        "select_feat" => {
            let mut s = format!("Lets the player choose a feat: {}", f.or("name", "Feat"));
            if let Some(c) = w("category") {
                s.push_str(&format!(" ({c})"));
            }
            s
        }
        "select_param" => format!(
            "Named choice: {} ({})",
            f.or("name", "a choice"),
            m.get("options")
                .and_then(Value::as_array)
                .map(|a| or_list(
                    &a.iter()
                        .filter_map(|x| x.as_str().map(str::to_string))
                        .collect::<Vec<_>>()
                ))
                .unwrap_or_default()
        ),
        "select_size" => format!(
            "Choose a size: {}",
            m.get("options")
                .and_then(Value::as_array)
                .map(|a| or_list(
                    &a.iter()
                        .filter_map(|x| x.as_str().map(str::to_string))
                        .collect::<Vec<_>>()
                ))
                .unwrap_or_default()
        ),
        "ability_increase" => {
            let opts: Vec<String> = m
                .get("options")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|o| {
                    let amount = o.get("amount")?.as_i64()?;
                    let to = ability_choice(o.get("abilities")?)?;
                    Some(match o.get("pick").and_then(Value::as_i64) {
                        Some(p) if p > 1 => format!("+{amount} to {p} of {to}"),
                        _ => format!("+{amount} to {to}"),
                    })
                })
                .collect();
            format!("Ability score increase: {}", or_list(&opts))
        }
        "ability_distribution" => format!(
            "Ability score points: {} among {}",
            f.or("options", "points"),
            f.or("pool", "the abilities")
        ),
        "expertise" => format!(
            "Expertise in {}",
            plural(&f.or("count", "1"), "skill", "skills")
        ),
        "weapon_mastery" => match m
            .get("count")
            .and_then(|c| c.get("value"))
            .and_then(Value::as_i64)
        {
            Some(n) => format!(
                "Weapon mastery: {} ",
                plural(&n.to_string(), "weapon", "weapons")
            ),
            None => format!(
                "Weapon mastery: as many weapons as {}",
                f.or("count", "the class table says")
            ),
        }
        .trim_end()
        .to_string(),
        "equipment_package" => format!(
            "Starting equipment choice: {}",
            f.or("ref", "a missing element")
        ),
        "toggle" => {
            let n = m
                .get("effects")
                .and_then(Value::as_array)
                .map_or(0, Vec::len);
            format!(
                "On/off state: {} ({})",
                f.text("name").or_else(|| f.text("id")).unwrap_or("a state"),
                plural(&n.to_string(), "effect", "effects")
            )
        }
        "condition_immunity" => format!("Immune to {}", f.or("conditions", "conditions")),
        "condition_immunity_while" => format!(
            "Immune to {} while {} is on",
            f.or("conditions", "conditions"),
            f.or("toggle", "a state")
        ),
        "save_advantage" => {
            let mut s = if let Some(c) = w("against_condition") {
                format!("Advantage on saving throws against being {c}")
            } else if let Some(a) = w("against_ability") {
                format!("Advantage on {a} saving throws")
            } else if let Some(a) = w("ability") {
                format!("Advantage on {a} saving throws")
            } else {
                "Advantage on saving throws".to_string()
            };
            if let Some(c) = w("context") {
                s.push_str(&format!(" to {c}"));
            }
            if let Some(u) = w("unless_condition") {
                s.push_str(&format!(", unless {}", u.to_lowercase()));
            }
            s
        }
        "roll_advantage" => {
            let roll = f.text("roll").unwrap_or("ability_check");
            let target = w("skill").or_else(|| w("ability"));
            match (roll, target) {
                ("initiative", _) => "Advantage on initiative rolls".to_string(),
                ("spell_attack", _) => match w("spell_class") {
                    Some(c) => format!("Advantage on spell attack rolls ({c} spells)"),
                    None => "Advantage on spell attack rolls".to_string(),
                },
                ("saving_throw", Some(t)) => format!("Advantage on {t} saving throws"),
                (_, Some(t)) => format!("Advantage on {t} checks"),
                (r, None) => format!(
                    "Advantage on {}",
                    label_of(choices::ROLLS, r).map_or_else(|| token(r), str::to_lowercase)
                ),
            }
        }
        "sense" => {
            let mut s = format!("Special sense: {}", f.or("sense", "a sense"));
            if let Some(r) = w("range") {
                s.push_str(&format!(" {r} feet"));
            }
            s
        }
        "movement_mode" => {
            let mode = f.or("mode", "a").to_lowercase();
            match w("speed") {
                Some(sp) if sp.parse::<i64>().is_ok() => {
                    format!("Gives a {mode} speed of {sp} feet")
                }
                Some(sp) => format!("Gives a {mode} speed {sp}"),
                None => format!("Gives a {mode} speed"),
            }
        }
        "attack_bonus" => format!(
            "Attack bonus with {}: {}",
            f.or("weapon", "any weapon").to_lowercase(),
            f.or("value", "0")
        ),
        "critical_range" => format!("Critical hit on a roll of {}-20", f.or("min", "20")),
        "skill_bonus" => format!(
            "Bonus to {}: {}",
            f.or("skills", "skills"),
            f.or("value", "0")
        ),
        "damage_reduction" => {
            let amount = [w("dice"), w("plus")]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" + ");
            format!(
                "Reduces {} damage by {}",
                f.or("damage_types", "all").to_lowercase(),
                if amount.is_empty() {
                    "an amount".to_string()
                } else {
                    amount
                }
            )
        }
        "extra_damage" => {
            let dice = w("dice").or_else(|| w("dice_table")).or_else(|| {
                Some(format!(
                    "{}{}",
                    w("dice_count")?,
                    f.text("die").unwrap_or("")
                ))
            });
            let mut s = format!("Extra damage: {}", f.text("name").unwrap_or("extra damage"));
            let detail = [dice, f.text("damage_type").map(str::to_string)]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" ");
            if !detail.is_empty() {
                s.push_str(&format!(" ({detail})"));
            }
            if let Some(o) = w("once_per") {
                s.push_str(&format!(", once per {}", o.to_lowercase()));
            }
            s
        }
        "healing" => {
            let amount = [w("dice"), w("plus")]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" + ");
            let mut s = format!("Healing: {}", f.text("name").unwrap_or("healing"));
            if !amount.is_empty() {
                s.push_str(&format!(" ({amount})"));
            }
            s
        }
        "save_dc" => format!(
            "Save DC of {}: {}",
            f.or("id", "a feature"),
            f.or("formula", "a formula")
        ),
        "spell_list_extend" => format!(
            "The {} spell list also offers the {} lists",
            f.or("spell_class", "class's"),
            f.or("lists", "other")
        ),
        "aura" => match w("radius_feet") {
            Some(r) => format!("Aura of {r} feet"),
            None => "Aura".to_string(),
        },
        _ => generic(&f),
    };
    if sentence.is_empty() {
        label.to_string()
    } else {
        sentence
    }
}

/// The block label and up to three of its values ("Spellbook (spells at first: 6; spells added per level: 2)").
fn generic(f: &Fields<'_>) -> String {
    let details: Vec<String> =
        f.b.fields
            .iter()
            .filter(|x| !x.expert)
            .filter_map(|x| {
                let v = value_words(x, f.m.get(x.key)?, f.cx)?;
                Some(match x.control {
                    Control::Check => v,
                    _ => format!("{}: {v}", lower_first(x.label)),
                })
            })
            .take(3)
            .collect();
    if details.is_empty() {
        f.b.label.to_string()
    } else {
        format!("{} ({})", f.b.label, details.join("; "))
    }
}
