//! Deterministic spelling fixes of model-written rules the engine reads loosely or not at all:
//! a `roll_advantage` roll written as `skill_check` (the reader's word is `ability_check`), a
//! skill or condition named by its element id, an ability by its abbreviation, and a choice of
//! ability score increase ("your Strength or Dexterity score increases by 1") written as a
//! fixed `stat` on one of the abilities, which the text says is the player's choice; and an
//! item's rule conditions that only restate "while this item is equipped / attuned".

use borealis_engine::item_conds::{is_self_reference, ITEM_TYPES};
use borealis_engine::rule_terms::{ability, roll_kind};
use serde_json::{json, Map, Value};

const ABILITIES: [&str; 6] = [
    "strength",
    "dexterity",
    "constitution",
    "intelligence",
    "wisdom",
    "charisma",
];

/// The ordinary ability score maximum.
const DEFAULT_CAP: i64 = 20;

/// Number words an increase may be written with.
const NUMBER_WORDS: [(&str, i64); 4] = [("one", 1), ("two", 2), ("three", 3), ("four", 4)];

/// The lower-case words after the last `marker` of an id (`ID_X_CONDITION_BLINDED` -> blinded).
fn id_words(raw: &str, marker: &str) -> Option<String> {
    let t = raw.trim();
    if !t.starts_with("ID_") {
        return None;
    }
    let at = t.to_uppercase().rfind(marker)?;
    let w = t[at + marker.len()..].replace('_', " ").to_lowercase();
    let w = w.trim();
    (!w.is_empty()).then(|| w.to_string())
}

fn set_str(r: &mut Map<String, Value>, key: &str, new: String, out: &mut Vec<String>) {
    let Some(old) = r.get(key).and_then(Value::as_str).map(str::to_string) else {
        return;
    };
    if old != new {
        out.push(format!(
            "rule {:?}: `{key}` {old:?} read as {new:?}",
            r.get("kind").and_then(Value::as_str).unwrap_or("")
        ));
        r.insert(key.into(), json!(new));
    }
}

/// `roll_advantage` roll, skill and ability in the words the engine reads.
fn roll_advantage(r: &mut Map<String, Value>, out: &mut Vec<String>) {
    if let Some(roll) = r.get("roll").and_then(Value::as_str) {
        let kind = roll_kind(roll);
        set_str(r, "roll", kind, out);
    }
    if let Some(skill) = r.get("skill").and_then(Value::as_str) {
        if let Some(w) = id_words(skill, "SKILL_") {
            set_str(r, "skill", w, out);
        }
    }
    if let Some(a) = r.get("ability").and_then(Value::as_str) {
        let full = ability(a);
        if ABILITIES.contains(&full.as_str()) {
            set_str(r, "ability", full, out);
        }
    }
}

/// Condition lists (`conditions`) named by element id as condition names.
fn conditions(r: &mut Map<String, Value>, out: &mut Vec<String>) {
    let Some(list) = r.get("conditions").and_then(Value::as_array) else {
        return;
    };
    let named: Vec<Value> = list
        .iter()
        .map(|c| {
            c.as_str()
                .and_then(|s| id_words(s, "CONDITION_"))
                .map_or_else(|| c.clone(), Value::String)
        })
        .collect();
    if &named != list {
        out.push(format!(
            "rule {:?}: conditions {} read as {}",
            r.get("kind").and_then(Value::as_str).unwrap_or(""),
            Value::Array(list.clone()),
            Value::Array(named.clone())
        ));
        r.insert("conditions".into(), Value::Array(named));
    }
}

/// Term fixes of one rule. Returns one message per change.
pub(crate) fn normalise_rule(r: &mut Map<String, Value>) -> Vec<String> {
    let mut out = Vec::new();
    match r.get("kind").and_then(Value::as_str) {
        Some("roll_advantage") => roll_advantage(r, &mut out),
        Some("condition_immunity" | "condition_immunity_while" | "aura_immunity") => {
            conditions(r, &mut out);
        }
        _ => {}
    }
    out
}

/// A number written as digits, a sign or a word ("2", "+2", "two").
fn number(w: &str) -> Option<i64> {
    let w = w.trim_matches(|c: char| !c.is_alphanumeric());
    w.parse::<i64>()
        .ok()
        .or_else(|| NUMBER_WORDS.iter().find(|(n, _)| *n == w).map(|(_, v)| *v))
}

/// The number after "by" in `words` from `from` on.
fn amount_after(words: &[&str], from: usize) -> Option<i64> {
    words[from..]
        .windows(2)
        .find(|p| p[0] == "by")
        .and_then(|p| number(p[1]))
}

/// "to a maximum of 22" in `words`.
fn stated_cap(words: &[&str]) -> Option<i64> {
    words
        .windows(4)
        .find(|w| w[..3] == ["a", "maximum", "of"] || w[..3] == ["maximum", "of", "a"])
        .and_then(|w| number(w[3]))
        .or_else(|| {
            words
                .windows(3)
                .find(|w| w[..2] == ["maximum", "of"])
                .and_then(|w| number(w[2]))
        })
}

/// An ability score choice a sentence states: the pool (empty = any ability), the amount and
/// the score maximum.
#[derive(Debug, PartialEq, Eq)]
struct Choice {
    pool: Vec<String>,
    /// Abilities chosen from any (when the pool is empty).
    count: i64,
    amount: i64,
    cap: i64,
}

/// The abilities of an "A, B, or C" list ending at `end` (the word before "score").
fn listed_before(words: &[&str], end: usize) -> Vec<String> {
    let mut pool = Vec::new();
    let mut saw_or = false;
    for w in words[..end].iter().rev() {
        let w = w.trim_matches(',');
        if ABILITIES.contains(&w) {
            pool.push(w.to_string());
        } else if w == "or" {
            saw_or = true;
        } else if !w.is_empty() {
            break;
        }
    }
    pool.reverse();
    if saw_or && pool.len() > 1 {
        pool
    } else {
        Vec::new()
    }
}

/// Words after an ability list that make it a bonus to rolls of the ability, not to its score.
const ROLL_WORDS: [&str; 10] = [
    "check", "checks", "saving", "save", "saves", "throw", "throws", "attack", "attacks",
    "modifier",
];

/// True when the sentence speaks of raising a score: "increase", a "bonus", or a signed "+N".
fn speaks_of_increase(words: &[&str]) -> bool {
    words
        .iter()
        .any(|w| w.starts_with("increase") || w.starts_with("bonus") || signed(w).is_some())
}

/// A signed amount written "+2".
fn signed(w: &str) -> Option<i64> {
    let w = w.trim_matches(|c: char| !(c.is_alphanumeric() || c == '+'));
    w.strip_prefix('+')?.parse::<i64>().ok().filter(|n| *n > 0)
}

/// The amount a sentence gives from `from` on: "by N", else the first signed "+N" anywhere.
fn amount_in(words: &[&str], from: usize) -> Option<i64> {
    amount_after(words, from).or_else(|| words.iter().find_map(|w| signed(w)))
}

/// The lower-case words of `sentence` with ability abbreviations ("Str", "dex.") spelled out.
fn words_of(sentence: &str) -> Vec<String> {
    sentence
        .split_whitespace()
        .map(|w| {
            let core = w.trim_matches(|c: char| !c.is_alphanumeric());
            match core {
                "str" | "dex" | "con" | "int" | "wis" | "cha" => ability(core).to_string(),
                _ => w.to_string(),
            }
        })
        .collect()
}

/// The choice a lower-case sentence states, if any: "your Strength or Dexterity score increases
/// by 1", "+1 to Strength or Dexterity", "one ability score of your choice increases by 1".
fn choice_in(sentence: &str) -> Option<Choice> {
    let owned = words_of(sentence);
    let words: Vec<&str> = owned.iter().map(String::as_str).collect();
    if !speaks_of_increase(&words) {
        return None;
    }
    let cap = stated_cap(&words).unwrap_or(DEFAULT_CAP);
    for i in 0..words.len() {
        let w = words[i].trim_matches(',');
        let next = words.get(i + 1).map(|n| n.trim_matches(',')).unwrap_or("");
        let ends_list = ABILITIES.contains(&w) && !ABILITIES.contains(&next) && next != "or";
        if !ends_list || ROLL_WORDS.contains(&next) {
            continue;
        }
        let pool = listed_before(&words, i + 1);
        if !pool.is_empty() {
            let amount = amount_in(&words, i)?;
            return Some(Choice {
                pool,
                count: 1,
                amount,
                cap,
            });
        }
    }
    let at = any_ability_at(&words)?;
    Some(Choice {
        pool: Vec::new(),
        count: count_before(&words, at),
        amount: amount_in(&words, at)?,
        cap,
    })
}

/// Where "one / an / another ability score" or "ability score(s) of your choice" ends.
fn any_ability_at(words: &[&str]) -> Option<usize> {
    (1..words.len()).find(|&i| {
        if words[i - 1] != "ability" || !words[i].starts_with("score") {
            return false;
        }
        let before = words.get(i.wrapping_sub(2)).copied().unwrap_or("");
        let choice = words.get(i + 1..i + 4) == Some(&["of", "your", "choice"][..]);
        choice || matches!(before, "one" | "an" | "another" | "other")
    })
}

/// How many abilities "two (other) ability scores" at `at` (the "scores" word) names; 1 if none.
fn count_before(words: &[&str], at: usize) -> i64 {
    words[at.saturating_sub(3)..at]
        .iter()
        .rev()
        .find_map(|w| number(w))
        .filter(|n| (1..=6).contains(n))
        .unwrap_or(1)
}

fn stat_amount(r: &Value) -> Option<i64> {
    match r.get("value")? {
        Value::String(s) => number(s),
        v => v.as_i64(),
    }
}

/// A fixed ability `stat` that stands for the choice of `c`.
fn stands_for(r: &Value, c: &Choice) -> bool {
    let name = r.get("name").and_then(Value::as_str).unwrap_or("");
    r.get("kind").and_then(Value::as_str) == Some("stat")
        && r.get("bonus").is_none()
        && ABILITIES.contains(&name)
        && (c.pool.is_empty() || c.pool.iter().any(|a| a == name))
        && stat_amount(r) == Some(c.amount)
}

/// The `ability_increase` rule of a choice.
fn increase_rule(c: &Choice) -> Value {
    let option = if c.pool.is_empty() {
        json!({"abilities": c.count, "amount": c.amount})
    } else {
        json!({"abilities": c.pool, "pick": 1, "amount": c.amount})
    };
    json!({"kind": "ability_increase", "name": "Ability Score Increase",
        "options": [option], "cap": c.cap})
}

/// The description's ability score increases, read against the model's fixed `stat`s: a capped
/// fixed increase ([`fixed_caps`]) and a choice ([`choices`]). Returns one message per change.
pub(crate) fn ability_choice(el: &mut Value) -> Vec<String> {
    let mut out = fixed_caps(el);
    out.extend(choices(el));
    out
}

/// Phrases saying a fixed increase also raises the score's maximum by the same amount.
const RAISES_MAX: [&str; 4] = [
    "as does your maximum",
    "maximum for that score",
    "maximum for those scores",
    "maximum for the score",
];

/// The fixed ability `stat` rule on `a` with `amount` (no bonus type), if any.
fn fixed_stat(rules: &[Value], a: &str, amount: i64) -> Option<usize> {
    rules.iter().position(|r| {
        r.get("kind").and_then(Value::as_str) == Some("stat")
            && r.get("bonus").is_none()
            && r.get("name").and_then(Value::as_str) == Some(a)
            && stat_amount(r) == Some(amount)
    })
}

/// `r` with another stat `name` / `value` (its other keys, such as a condition, kept).
fn restat(r: &Value, name: String, value: i64, bonus: Option<&str>) -> Value {
    let mut out = r.clone();
    if let Some(o) = out.as_object_mut() {
        o.insert("name".into(), json!(name));
        o.insert("value".into(), json!(value.to_string()));
        if let Some(b) = bonus {
            o.insert("bonus".into(), json!(b));
        }
    }
    out
}

/// A fixed increase the text gives a maximum other than the ordinary one ("your Strength score
/// increases by 2, to a maximum of 22"), written as a plain `stat`, becomes the engine's capped
/// increase: `<ability>:boon` N with its cap in `<ability>:boon:cap` (applied after the ordinary
/// maximum, so the score can pass 20 up to the stated maximum and no further; the shape the
/// engine gives an `ability_increase` with a cap). One that also raises the score's maximum
/// ("..., as does your maximum for that score") gets `<ability>:max` N next to it. A choice is
/// left to [`choices`]. Returns one message per change.
fn fixed_caps(el: &mut Value) -> Vec<String> {
    let text = super::fidelity::plain(el.get("description").and_then(Value::as_str).unwrap_or(""))
        .to_lowercase();
    let Some(rules) = el.get_mut("rules").and_then(Value::as_array_mut) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for sentence in text.split(['.', ';', '\n']) {
        let spaced = sentence.replace(',', " , ");
        if choice_in(&spaced).is_some() || !spaced.contains("increase") {
            continue;
        }
        let owned = words_of(&spaced);
        let words: Vec<&str> = owned.iter().map(String::as_str).collect();
        let Some(amount) = amount_after(&words, 0) else {
            continue;
        };
        let cap = stated_cap(&words).filter(|c| *c != DEFAULT_CAP);
        let raises = RAISES_MAX.iter().any(|p| spaced.contains(p));
        let named: Vec<&str> = ABILITIES
            .iter()
            .copied()
            .filter(|a| words.iter().any(|w| w.trim_matches(',') == *a))
            .collect();
        for a in named {
            let Some(i) = fixed_stat(rules, a, amount) else {
                continue;
            };
            if let Some(m) = cap {
                let base = rules[i].clone();
                rules[i] = restat(&base, format!("{a}:boon"), amount, None);
                rules.insert(
                    i + 1,
                    restat(&base, format!("{a}:boon:cap"), m, Some("base")),
                );
                out.push(format!(
                    "stat {a} +{amount} read as the capped increase the text states ({a}:boon, maximum {m})"
                ));
            } else if raises {
                let max = format!("{a}:max");
                if rules
                    .iter()
                    .any(|r| r.get("name").and_then(Value::as_str) == Some(max.as_str()))
                {
                    continue;
                }
                let raised = restat(&rules[i], max.clone(), amount, None);
                rules.insert(i + 1, raised);
                out.push(format!(
                    "stat {max} +{amount} added: the text raises the score's maximum with it"
                ));
            }
        }
    }
    out
}

/// An ability score choice the description states but the rules fix on one ability (a `stat`)
/// becomes the `ability_increase` the player chooses with. Returns one message per change.
fn choices(el: &mut Value) -> Vec<String> {
    let text = super::fidelity::plain(el.get("description").and_then(Value::as_str).unwrap_or(""))
        .to_lowercase();
    let Some(rules) = el.get_mut("rules").and_then(Value::as_array_mut) else {
        return Vec::new();
    };
    if rules
        .iter()
        .any(|r| r.get("kind").and_then(Value::as_str) == Some("ability_increase"))
    {
        return Vec::new();
    }
    let mut out = Vec::new();
    for sentence in text.split(['.', ';', '\n']) {
        let Some(c) = choice_in(&sentence.replace(',', " , ")) else {
            continue;
        };
        let Some(first) = rules.iter().position(|r| stands_for(r, &c)) else {
            continue;
        };
        let dropped: Vec<String> = rules
            .iter()
            .filter(|r| stands_for(r, &c))
            .filter_map(|r| r.get("name").and_then(Value::as_str).map(str::to_string))
            .collect();
        rules[first] = increase_rule(&c);
        let mut i = first + 1;
        while i < rules.len() {
            if stands_for(&rules[i], &c) {
                rules.remove(i);
            } else {
                i += 1;
            }
        }
        out.push(format!(
            "stat {} +{} read as the ability score choice the text states (ability_increase)",
            dropped.join("/"),
            c.amount
        ));
    }
    out
}

/// An item's rule conditions that only say "while this item is equipped / attuned"
/// (`"equipped": "true"`, `"requirements": "attuned"`, the item's own name) are removed: the
/// item's rules apply only while it is used anyway, and the loose spellings read as false, so the
/// rule would never apply (and a magic armor's `+N` AC would not become its enhancement bonus).
/// Returns one message per removal.
pub(crate) fn item_self_conditions(el: &mut Value) -> Vec<String> {
    let ty = el.get("type").and_then(Value::as_str).unwrap_or("");
    if !ITEM_TYPES.contains(&ty) {
        return Vec::new();
    }
    let id = el
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let name = el
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let Some(rules) = el.get_mut("rules").and_then(Value::as_array_mut) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for r in rules.iter_mut().filter_map(Value::as_object_mut) {
        for key in ["equipped", "requirements"] {
            let own = r
                .get(key)
                .and_then(Value::as_str)
                .is_some_and(|t| is_self_reference(t, &id, &name));
            if own {
                let text = r.remove(key).unwrap_or_default();
                out.push(format!(
                    "rule {:?}: removed `{key}` {text}: the item's rules apply only while it is used",
                    r.get("kind").and_then(Value::as_str).unwrap_or("")
                ));
            }
        }
    }
    out
}

#[cfg(test)]
#[path = "rule_terms_tests.rs"]
mod tests;
