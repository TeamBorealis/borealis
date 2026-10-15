//! Class proficiency choices in the shape the bundled classes use, read from the class's own
//! proficiency lines ("Skills: Choose two from ...", "Tools: one of ... or ..."): ONE skill
//! `select` of N from the listed skills, ONE tool `select` for an "or" choice of tools, both with
//! the requirements of the class's other starting proficiencies (not taken by a multiclass).
//! Applied by `derived.rs`; every rewrite is reported.

use borealis_content::ContentStore;
use serde_json::{json, Value};

const COUNTS: [&str; 6] = ["one", "two", "three", "four", "five", "six"];
/// Words between a count and "cantrip" that keep it a count ("one additional cantrip").
const QUALIFIERS: [&str; 4] = ["additional", "extra", "bonus", "more"];
/// Words of a class's own cantrip progression (never a bonus cantrip).
const PROGRESSION_WORDS: [&str; 3] = ["cantrips known", "at higher levels", "as shown in the"];
const SKILL_PREFIX: &str = "ID_PROFICIENCY_SKILL_";

fn norm(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn str_at<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}

/// The text after `label:` on the first proficiency line whose label starts with one of `labels`.
fn line_after<'a>(text: &'a str, labels: &[&str]) -> Option<&'a str> {
    text.lines().find_map(|l| {
        let (head, rest) = l.split_once(':')?;
        let h = head.trim().to_lowercase();
        labels
            .iter()
            .any(|p| h.starts_with(p))
            .then_some(rest.trim())
    })
}

fn count_in(text: &str) -> Option<u32> {
    let lower = text.to_lowercase();
    let after = lower.split_once("choose")?.1.trim_start();
    let after = after.strip_prefix("any").map_or(after, str::trim_start);
    let word: String = after
        .chars()
        .take_while(char::is_ascii_alphanumeric)
        .collect();
    word.parse::<u32>().ok().or_else(|| {
        COUNTS
            .iter()
            .position(|c| *c == word)
            .and_then(|i| u32::try_from(i + 1).ok())
    })
}

/// The listed names: after "from" (or the first ':' of a "Choose N:" form), split on commas,
/// "and" and "or".
fn names_in(text: &str) -> Vec<String> {
    let lower = text.to_lowercase();
    let list = lower
        .split_once(" from ")
        .map(|x| x.1)
        .or_else(|| lower.split_once(':').map(|x| x.1))
        .unwrap_or(&lower);
    list.replace(" and ", ",")
        .replace(" or ", ",")
        .split(',')
        .map(|n| n.trim().trim_end_matches('.').trim().to_string())
        .filter(|n| !n.is_empty())
        .collect()
}

fn skill_id(store: &ContentStore, name: &str) -> Option<String> {
    let want = norm(&format!("skill proficiency ({name})"));
    let bare = norm(name);
    store
        .by_type("Proficiency")
        .filter(|s| s.element.id.starts_with(SKILL_PREFIX))
        .find(|s| norm(&s.element.name) == want || norm(&s.element.name) == bare)
        .map(|s| s.element.id.clone())
}

/// Tool proficiency ids an alternative names: one tool, or a whole category ("a gaming set").
fn tool_ids(store: &ContentStore, alt: &str) -> Vec<String> {
    let a = norm(alt);
    let a = a
        .strip_prefix("one ")
        .or_else(|| a.strip_prefix("a "))
        .or_else(|| a.strip_prefix("any "))
        .unwrap_or(&a)
        .trim_end_matches(" of your choice")
        .to_string();
    let single = norm(&format!("tool proficiency ({a})"));
    if let Some(s) = store
        .by_type("Proficiency")
        .find(|s| norm(&s.element.name) == single)
    {
        return vec![s.element.id.clone()];
    }
    let tag = a.trim_end_matches('s');
    store
        .by_type("Proficiency")
        .filter(|s| s.element.supports.iter().any(|t| norm(t) == tag))
        .map(|s| s.element.id.clone())
        .collect()
}

/// The requirements of the class's starting proficiency grants.
fn start_requirements(rules: &[Value]) -> Option<Value> {
    rules
        .iter()
        .find(|r| str_at(r, "type") == "Proficiency" && r.get("requirements").is_some())
        .and_then(|r| r.get("requirements").cloned())
}

fn is_skill_rule(r: &Value) -> bool {
    str_at(r, "type") == "Proficiency"
        && (str_at(r, "id").starts_with(SKILL_PREFIX)
            || str_at(r, "supports").contains(SKILL_PREFIX)
            || str_at(r, "supports").starts_with("Skill"))
}

/// Rewrite the class's skill and tool choices from its text; returns one note per rewrite.
pub(crate) fn class_choices(class: &mut Value, text: &str, store: &ContentStore) -> Vec<String> {
    let mut notes = Vec::new();
    let name = str_at(class, "name").to_string();
    let Some(rules) = class.get_mut("rules").and_then(Value::as_array_mut) else {
        return notes;
    };
    let req = start_requirements(rules);
    if let Some(line) = line_after(text, &["skills", "skill proficiencies"]) {
        if let Some(sel) = skill_select(line, &name, store, req.as_ref()) {
            let current: Vec<&Value> = rules.iter().filter(|r| is_skill_rule(r)).collect();
            let same = current.len() == 1
                && current[0].get("number") == sel.get("number")
                && str_at(current[0], "supports") == str_at(&sel, "supports");
            if !same {
                rules.retain(|r| !is_skill_rule(r));
                notes.push(format!(
                    "skill proficiencies modelled as one choice of {} from the class's list",
                    sel["number"]
                ));
                rules.push(sel);
            }
        }
    }
    if let Some(line) = line_after(text, &["tools", "tool proficiencies"]) {
        if let Some(sel) = tool_select(line, &name, store, req.as_ref()) {
            let ids: Vec<String> = str_at(&sel, "supports")
                .split('|')
                .map(str::to_string)
                .collect();
            let present = rules.iter().any(|r| {
                str_at(r, "kind") == "select" && str_at(r, "supports") == str_at(&sel, "supports")
            });
            if !present {
                rules.retain(|r| {
                    !(str_at(r, "kind") == "grant" && ids.iter().any(|i| i == str_at(r, "id")))
                });
                notes.push(
                    "tool proficiency modelled as one choice between the listed tools".into(),
                );
                rules.push(sel);
            }
        }
    }
    notes
}

fn skill_select(
    line: &str,
    class: &str,
    store: &ContentStore,
    req: Option<&Value>,
) -> Option<Value> {
    let n = count_in(line)?;
    let any = line.to_lowercase().contains("any");
    let supports = if any && !line.to_lowercase().contains(" from ") {
        "Skill".to_string()
    } else {
        let ids: Option<Vec<String>> = names_in(line).iter().map(|s| skill_id(store, s)).collect();
        let ids = ids?;
        if ids.len() < n as usize {
            return None;
        }
        ids.join("|")
    };
    let mut sel = json!({"kind": "select", "type": "Proficiency",
        "name": format!("Skill Proficiency ({class})"), "number": n, "supports": supports});
    if let Some(r) = req {
        sel["requirements"] = r.clone();
    }
    Some(sel)
}

fn tool_select(
    line: &str,
    class: &str,
    store: &ContentStore,
    req: Option<&Value>,
) -> Option<Value> {
    let lower = line.to_lowercase();
    if !(lower.contains(" or ") || lower.contains("one of") || lower.contains("choose")) {
        return None;
    }
    let mut ids: Vec<String> = Vec::new();
    for alt in names_in(&lower.replace("one of", ":")) {
        let found = tool_ids(store, &alt);
        if found.is_empty() {
            return None; // an alternative we cannot resolve: leave the model's modelling
        }
        for f in found {
            if !ids.contains(&f) {
                ids.push(f);
            }
        }
    }
    if ids.len() < 2 {
        return None;
    }
    let mut sel = json!({"kind": "select", "type": "Proficiency",
        "name": format!("Tool Proficiency ({class})"), "supports": ids.join("|")});
    if let Some(r) = req {
        sel["requirements"] = r.clone();
    }
    Some(sel)
}

/// "learn N cantrip(s) ... from the <List> spell list" or "learn N additional <List>
/// cantrip(s)" in a feature with no spell choice: the bundled bonus-cantrip select
/// (`<List>,0`). Returns the select, or None.
pub(crate) fn cantrip_select(feature: &Value, lists: &[String]) -> Option<Value> {
    let rules = feature.get("rules").and_then(Value::as_array);
    if rules.is_some_and(|r| r.iter().any(|x| str_at(x, "type") == "Spell")) {
        return None;
    }
    let text = norm(str_at(feature, "description"));
    // The class's own cantrips ("... more at higher levels, as shown in the Cantrips Known
    // column") come from its caster profile, not a bonus select.
    if PROGRESSION_WORDS.iter().any(|w| text.contains(w)) {
        return None;
    }
    let at = text.find(" cantrip")?;
    let mut before: Vec<&str> = text[..at].split_whitespace().collect();
    // "one additional <List> cantrip": the list named right before the word.
    let named = lists
        .iter()
        .filter(|l| {
            let n = norm(l);
            let words: Vec<&str> = n.split_whitespace().collect();
            !words.is_empty() && before.ends_with(&words)
        })
        .max_by_key(|l| norm(l).split_whitespace().count());
    if let Some(l) = named {
        before.truncate(before.len() - norm(l).split_whitespace().count());
    }
    while before.last().is_some_and(|w| QUALIFIERS.contains(w)) {
        before.pop();
    }
    let count_word = before.last()?;
    let n = count_word.parse::<u32>().ok().or_else(|| {
        COUNTS
            .iter()
            .position(|c| c == count_word)
            .and_then(|i| u32::try_from(i + 1).ok())
    })?;
    if !before
        .iter()
        .rev()
        .take(4)
        .any(|w| matches!(*w, "learn" | "know" | "gain"))
    {
        return None;
    }
    let rest = &text[at..];
    let list = named.or_else(|| {
        lists
            .iter()
            .find(|l| rest.contains(&format!("{} spell list", norm(l))))
    })?;
    let mut sel = json!({"kind": "select", "type": "Spell",
        "name": format!("{list} Cantrip ({})", str_at(feature, "name")),
        "supports": format!("{list},0"),
        "spellcasting": feature.pointer("/data/class").and_then(Value::as_str).unwrap_or(list)});
    if n > 1 {
        sel["number"] = json!(n);
    }
    Some(sel)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bonus_cantrip_is_a_select_but_the_class_progression_is_not() {
        let lists = vec!["Warden".to_string()];
        let bonus = json!({"name": "Grove Lore", "data": {"class": "Warden"},
            "description": "<p>You learn one cantrip of your choice from the Warden spell list.</p>"});
        let sel = cantrip_select(&bonus, &lists).expect("bonus cantrip");
        assert_eq!(sel["supports"], "Warden,0");
        let own = json!({"name": "Cantrips", "data": {"class": "Warden"},
            "description": "<p>You know one cantrip of your choice from the Warden spell list. You learn additional Warden cantrips of your choice at higher levels, as shown in the Cantrips Known column of the Warden table.</p>"});
        assert!(cantrip_select(&own, &lists).is_none());
    }

    #[test]
    fn a_bonus_cantrip_named_by_its_list_before_the_word_is_a_select() {
        let lists = vec!["Warden".to_string(), "Hedge Warden".to_string()];
        let one = json!({"name": "Grove Gift", "data": {"class": "Warden"},
            "description": "<p>When you choose this grove at 2nd level, you learn one additional hedge warden cantrip of your choice.</p>"});
        let sel = cantrip_select(&one, &lists).expect("bonus cantrip");
        assert_eq!(sel["supports"], "Hedge Warden,0");
        assert!(sel.get("number").is_none());
        let two = json!({"name": "Grove Gift", "description": "<p>You learn two extra Warden cantrips of your choice.</p>"});
        let sel = cantrip_select(&two, &lists).expect("bonus cantrips");
        assert_eq!(
            (sel["supports"].as_str(), &sel["number"]),
            (Some("Warden,0"), &json!(2))
        );
        // A named cantrip is no choice.
        let named = json!({"name": "Grove Gift", "description": "<p>You learn the ember lash cantrip.</p>"});
        assert!(cantrip_select(&named, &lists).is_none());
    }
}
