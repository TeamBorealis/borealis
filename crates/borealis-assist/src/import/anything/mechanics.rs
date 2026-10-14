//! Mechanics the model left in the text: a sentence shaped like a rule ("your Armor Class
//! equals 13 + your Dexterity modifier", "your Strength score increases by 2", "you have a
//! flying speed of 30 feet", "a +1 bonus to AC") on an element whose rules (and the rules of the
//! import elements it grants) have nothing of that kind. Each finding asks the repair loop for
//! ONE repair; what survives it stays a review note (the text may be right to keep it).

use serde_json::Value;

use super::sweep::SweepIssue;
use super::types::{ElementStatus, ImportDraft, Role};

const ABILITIES: [&str; 6] = [
    "strength",
    "dexterity",
    "constitution",
    "intelligence",
    "wisdom",
    "charisma",
];

/// Types whose text is lore, a spell or a stat block, never character rules.
const SKIP_TYPES: [&str; 9] = [
    "Spell",
    "Monster",
    "Rules Glossary Entry",
    "Condition",
    "Class",
    "Archetype",
    "Weapon Property",
    "Language",
    "Grants",
];

/// The longest quote of a sentence in a finding.
const QUOTE_CHARS: usize = 120;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shape {
    ArmorClass,
    Ability,
    Speed,
    Bonus,
}

impl Shape {
    fn hint(self) -> &'static str {
        match self {
            Self::ArmorClass => "an armor class formula is `stat` rules like the bundled Unarmored Defense: \
ac:<name>:base = the number, ac:<name>:dexterity = dexterity:modifier, ac:<name> = each of those, and \
{\"kind\":\"stat\",\"name\":\"ac:calculation\",\"value\":\"ac:<name>\",\"bonus\":\"calculation\",\"equipped\":\"[armor:none]\"}",
            Self::Ability => "a fixed increase is a `stat` rule per ability ({\"kind\":\"stat\",\"name\":\"strength\",\"value\":\"2\"}); \
a choice is an `ability_increase`",
            Self::Speed => "a lasting speed is a `stat` rule (a species' walking speed: name \"innate speed\", value \"30\", \
bonus \"base\"; another mode: name \"speed:fly\" / \"speed:swim\" / \"speed:climb\", value \"30\" or \"speed\"); \
a speed you gain for a while is a `movement_mode` rule",
            Self::Bonus => "a lasting bonus is a `stat` rule (e.g. name \"ac\", value \"1\"; a magic weapon or armor \
uses the enhancement shape of the example)",
        }
    }
}

fn str_at<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}

fn plain(html: &str) -> String {
    let mut out = String::new();
    let mut tag = false;
    for c in html.chars() {
        match c {
            '<' => tag = true,
            '>' => {
                tag = false;
                out.push(' ');
            }
            _ if !tag => out.push(c),
            _ => {}
        }
    }
    out.replace(['\u{2019}', '\u{2018}'], "'")
        .replace('\u{2212}', "-")
}

fn sentences(text: &str) -> Vec<String> {
    text.split(['.', ';', '\n'])
        .map(|s| s.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|s| !s.is_empty())
        .collect()
}

/// `marker` is followed by a positive number ("speed of 30", not "speed is 0").
fn has_number_after(s: &str, marker: &str) -> bool {
    s.match_indices(marker).any(|(i, _)| {
        let digits: String = s[i + marker.len()..]
            .trim_start()
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        digits.parse::<u64>().is_ok_and(|n| n > 0)
    })
}

/// The rule shapes a lowercase sentence asks for.
fn shapes_of(s: &str) -> Vec<Shape> {
    let mut out = Vec::new();
    if !(s.contains("you") || s.contains("your")) {
        return out;
    }
    let ac = [
        "your ac ",
        "your armor class",
        "base ac",
        "base armor class",
    ]
    .iter()
    .any(|w| s.contains(w));
    if ac && s.contains("+ your") && s.chars().any(|c| c.is_ascii_digit()) {
        out.push(Shape::ArmorClass);
    }
    let ability = ABILITIES.iter().any(|a| {
        s.contains(&format!("{a} score increases by"))
            || s.contains(&format!("{a} score increase by"))
            || has_number_after(s, &format!("{a} +"))
    });
    if ability && !s.contains("check") && !s.contains("saving throw") {
        out.push(Shape::Ability);
    }
    let modes = [
        "fly",
        "flying",
        "swim",
        "swimming",
        "climb",
        "climbing",
        "burrow",
        "burrowing",
        "walking",
        "base",
    ];
    let speed = modes.iter().any(|m| {
        has_number_after(s, &format!("{m} speed of"))
            || s.contains(&format!("{m} speed equal to"))
            || has_number_after(s, &format!("{m} speed is"))
    }) || has_number_after(s, "your speed is")
        || has_number_after(s, "your speed increases by")
        || has_number_after(s, "you have a speed of");
    if speed {
        out.push(Shape::Speed);
    }
    let targets = [
        "ac",
        "armor class",
        "attack rolls",
        "attack and damage",
        "damage rolls",
        "saving throws",
        "spell attack",
        "spell save",
        "initiative",
    ];
    let bonus = s
        .split("bonus to ")
        .skip(1)
        .any(|after| targets.iter().any(|t| after.starts_with(t)))
        && has_number_after(s, " +");
    if bonus {
        out.push(Shape::Bonus);
    }
    out
}

/// Does any rule (of `rules`) implement `shape`?
fn modelled(shape: Shape, rules: &[&Value]) -> bool {
    rules.iter().any(|r| {
        let kind = str_at(r, "kind");
        let name = str_at(r, "name").to_lowercase();
        match shape {
            Shape::ArmorClass => kind == "stat" && name.starts_with("ac"),
            Shape::Ability => {
                (kind == "stat" && ABILITIES.iter().any(|a| name == *a))
                    || kind == "ability_increase"
                    || (kind == "select" && str_at(r, "type").contains("Ability"))
            }
            Shape::Speed => (kind == "stat" && name.contains("speed")) || kind == "movement_mode",
            Shape::Bonus => matches!(kind, "stat" | "attack_bonus" | "skill_bonus"),
        }
    })
}

fn rules_of(el: &Value) -> Vec<&Value> {
    ["/rules", "/multiclass/rules"]
        .iter()
        .filter_map(|p| el.pointer(p).and_then(Value::as_array))
        .flatten()
        .collect()
}

/// Findings for `el`, whose granted import elements are `granted` (their rules count too: a
/// species keeps its traits' text but the traits carry the rules).
pub(crate) fn unmodelled(el: &Value, granted: &[&Value]) -> Vec<String> {
    if SKIP_TYPES.contains(&str_at(el, "type")) {
        return Vec::new();
    }
    let enhanced = el
        .get("setters")
        .and_then(Value::as_array)
        .is_some_and(|s| s.iter().any(|x| str_at(x, "name") == "enhancement"));
    let mut rules = rules_of(el);
    rules.extend(granted.iter().flat_map(|g| rules_of(g)));
    // A shape a granted element's own text states is that element's finding, not this one's.
    let delegated: Vec<Shape> = granted
        .iter()
        .flat_map(|g| sentences(&plain(str_at(g, "description"))))
        .flat_map(|s| shapes_of(&s.to_lowercase()))
        .collect();
    let mut out: Vec<String> = Vec::new();
    let mut seen: Vec<Shape> = Vec::new();
    for s in sentences(&plain(str_at(el, "description"))) {
        for shape in shapes_of(&s.to_lowercase()) {
            let skip = seen.contains(&shape)
                || delegated.contains(&shape)
                || modelled(shape, &rules)
                || (shape == Shape::Bonus && enhanced);
            if skip {
                continue;
            }
            seen.push(shape);
            let quote: String = s.chars().take(QUOTE_CHARS).collect();
            out.push(format!(
                "the text says \"{quote}\" but no rule implements it: {}",
                shape.hint()
            ));
        }
    }
    out
}

/// The import elements whose rules count for `id`: the ones it grants and its planned parts
/// and options (their text is in its description, their rules are their own).
fn related<'a>(draft: &'a ImportDraft, id: &str, el: &Value) -> Vec<&'a Value> {
    let granted: Vec<&str> = rules_of(el)
        .into_iter()
        .filter(|r| str_at(r, "kind") == "grant")
        .map(|r| str_at(r, "id"))
        .collect();
    draft
        .elements
        .iter()
        .filter(|e| !e.json.is_null())
        .filter(|e| {
            granted.contains(&e.id.as_str())
                || draft
                    .plan
                    .elements
                    .iter()
                    .any(|p| p.id == e.id && p.parent.as_deref() == Some(id))
        })
        .map(|e| &e.json)
        .collect()
}

/// [`unmodelled`] findings of every generated element, as repairable issues.
pub(crate) fn sweep(draft: &ImportDraft) -> Vec<SweepIssue> {
    draft
        .elements
        .iter()
        .filter(|e| !e.json.is_null() && e.status != ElementStatus::Failed && e.role != Role::Text)
        .flat_map(|e| {
            unmodelled(&e.json, &related(draft, &e.id, &e.json))
                .into_iter()
                .map(|message| SweepIssue {
                    level: 0,
                    element_id: Some(e.id.clone()),
                    message,
                })
        })
        .collect()
}

/// After the repair: what is still only text is a review note on its element.
pub(crate) fn note_leftovers(draft: &mut ImportDraft) {
    for i in sweep(draft) {
        let Some(e) = i
            .element_id
            .as_deref()
            .and_then(|id| draft.elements.iter_mut().find(|e| e.id == id))
        else {
            continue;
        };
        let msg = format!("left as text after a repair: {}", i.message);
        if e.issues.contains(&msg) {
            continue;
        }
        e.issues.push(msg);
        if matches!(e.status, ElementStatus::Ok | ElementStatus::Repaired) {
            e.status = ElementStatus::NeedsReview;
        }
    }
}

/// `requires attunement by a/an ...` printed in the text and the `attunement` setter's
/// `addition` describe the same restriction: the setter gets the printed words when it has
/// none, the text gets the setter's when it lacks them. Returns a note per change.
pub(crate) fn attunement_by(el: &mut Value) -> Vec<String> {
    let mut notes = Vec::new();
    let text = plain(str_at(el, "description"));
    let lower = text.to_lowercase();
    let printed = lower.find("attunement by ").map(|i| {
        let start = i + "attunement ".len();
        let end = lower[start..]
            .find([')', '.', ';', '\n'])
            .map_or(lower.len(), |e| start + e);
        text[start..end].trim().to_string()
    });
    let Some(setters) = el.get_mut("setters").and_then(Value::as_array_mut) else {
        return notes;
    };
    let Some(att) = setters
        .iter_mut()
        .find(|s| str_at(s, "name") == "attunement" && str_at(s, "value") == "true")
    else {
        return notes;
    };
    let addition = att
        .pointer("/attributes/addition")
        .and_then(Value::as_str)
        .map(str::to_string);
    match (printed, addition) {
        (Some(p), None) => {
            att["attributes"] = serde_json::json!({"addition": p});
            notes.push(format!("attunement setter: \"{p}\" (as printed)"));
        }
        (None, Some(a)) if a.to_lowercase().starts_with("by ") => {
            let missing = a
                .to_lowercase()
                .split(|c: char| !c.is_alphanumeric())
                .filter(|w| w.len() > 3)
                .any(|w| !lower.contains(w));
            if missing {
                let line = format!("<p><em>Requires attunement {a}.</em></p>");
                let d = str_at(el, "description").to_string();
                el["description"] = Value::String(format!("{d}{line}"));
                notes.push(format!(
                    "description: \"Requires attunement {a}\" (from the attunement setter)"
                ));
            }
        }
        _ => {}
    }
    notes
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn rule_shaped_sentences_without_rules_are_found() {
        let trait_ = json!({"type": "Racial Trait", "name": "Bark Hide",
            "description": "<p>When you aren't wearing armor, your Armor Class equals 12 + your \
Dexterity modifier. You can use a shield.</p>", "rules": []});
        let f = unmodelled(&trait_, &[]);
        assert_eq!(f.len(), 1, "{f:?}");
        assert!(f[0].contains("ac:calculation"));
        let done = json!({"type": "Racial Trait", "description": trait_["description"],
            "rules": [{"kind": "stat", "name": "ac:calculation", "value": "ac:bark"}]});
        assert!(unmodelled(&done, &[]).is_empty());

        let race = json!({"type": "Race", "description": "<p>Your Strength score increases by 2. \
You have a walking speed of 30 feet.</p>", "rules": [{"kind": "grant", "id": "ID_T_TRAIT"}]});
        assert_eq!(unmodelled(&race, &[]).len(), 2);
        // The trait it grants carries the rules: nothing to repair.
        let part = json!({"rules": [{"kind": "stat", "name": "strength", "value": "2"},
            {"kind": "stat", "name": "innate speed", "value": "30", "bonus": "base"}]});
        assert!(unmodelled(&race, &[&part]).is_empty());
        // A granted trait that states the formula itself is the one to repair.
        let armor = json!({"type": "Racial Trait", "rules": [],
            "description": "<p>Your AC equals 12 + your Dexterity modifier.</p>"});
        let scaled = json!({"type": "Race", "rules": [],
            "description": "<p>Unarmored, your AC equals 12 + your Dexterity modifier.</p>"});
        assert!(unmodelled(&scaled, &[&armor]).is_empty());
        assert_eq!(unmodelled(&armor, &[]).len(), 1);

        let item = json!({"type": "Magic Item", "description": "<p>While you wear these boots you \
have a swimming speed of 40 feet and a +1 bonus to AC.</p>", "rules": []});
        assert_eq!(unmodelled(&item, &[]).len(), 2);
        // Lore, checks and other creatures are not rules of the character.
        for quiet in [
            "<p>The giant has a speed of 40 feet.</p>",
            "<p>You add your Strength +2 to checks made to climb.</p>",
            "<p>Mudlings prize a plan above all.</p>",
            "<p>You can't do this while your speed is 0.</p>",
        ] {
            let el = json!({"type": "Racial Trait", "description": quiet});
            assert!(unmodelled(&el, &[]).is_empty(), "{quiet}");
        }
        let spell = json!({"type": "Spell", "description": "<p>Your AC equals 13 + your Dexterity modifier.</p>"});
        assert!(unmodelled(&spell, &[]).is_empty());
    }

    #[test]
    fn attunement_restrictions_are_in_both_the_setter_and_the_text() {
        let mut quiet = json!({"description": "<p>A staff of reeds.</p>", "setters": [
            {"name": "attunement", "value": "true", "attributes": {"addition": "by a druid or ranger"}}]});
        assert_eq!(attunement_by(&mut quiet).len(), 1);
        assert!(quiet["description"]
            .as_str()
            .unwrap()
            .ends_with("<p><em>Requires attunement by a druid or ranger.</em></p>"));
        assert!(attunement_by(&mut quiet).is_empty(), "idempotent");
        let mut printed = json!({"description": "<p>Wand (requires attunement by a spellcaster)</p>",
            "setters": [{"name": "attunement", "value": "true"}]});
        assert_eq!(attunement_by(&mut printed).len(), 1);
        assert_eq!(
            printed["setters"][0]["attributes"]["addition"],
            "by a spellcaster"
        );
    }
}
