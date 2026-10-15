//! Mechanics the model left in the text: a sentence shaped like a rule ("your Armor Class
//! equals 13 + your Dexterity modifier", "your Strength score increases by 2", "you have a
//! flying speed of 30 feet", "a +1 bonus to AC") on an element whose rules (and the rules of the
//! import elements it grants) have nothing of that kind. Each finding asks the repair loop for
//! ONE repair; what survives it stays a review note (the text may be right to keep it).

use serde_json::Value;

use super::sweep::SweepIssue;
use super::types::{DraftElement, ElementStatus, ImportDraft, Role};

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

/// Frequency words before "per <period>" ("once per long rest", "three times per day").
const FREQUENCIES: [&str; 3] = ["once per ", "twice per ", "times per "];
/// Periods of a limited use.
const PERIODS: [&str; 5] = [
    "long rest",
    "short rest",
    "short or long rest",
    "long or short rest",
    "day",
];
/// Words that make an "equal to your <ability> modifier" sentence about a roll, damage,
/// healing, a bonus, a duration or a number of uses, not a count of things you have.
const NOT_A_COUNT: [&str; 18] = [
    "damage",
    "hit point",
    "bonus",
    "roll",
    "saving throw",
    " dc",
    "times",
    "number of",
    "check",
    "attack",
    "feet",
    "minute",
    "hour",
    "round",
    "add ",
    "level",
    "uses",
    "temporary",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shape {
    ArmorClass,
    Ability,
    Speed,
    Bonus,
    LimitedUse,
    ModifierCount,
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
uses the enhancement shape of the example; a magic armor's or shield's own +N to AC is {\"kind\":\"stat\",\
\"name\":\"ac:armored:enhancement\",\"value\":\"N\",\"bonus\":\"enhancement\"}, a shield's bonus \
\"enhancement:shield\")",
            Self::LimitedUse => "a limited use is a `resource` rule named as the feature ({\"kind\":\"resource\",\
\"id\":\"<feature name, lowercase>\",\"name\":\"<Feature name>\",\"max\":{\"type\":\"fixed\",\"value\":1},\
\"recharge\":[\"long_rest\"]}; \"per day\" is [\"long_rest\"], \"short or long rest\" [\"short_rest\",\"long_rest\"]), \
next to the grant it limits; a spell cast without a slot a limited number of times is instead its `grant_spell` with \
\"free_cast\":{\"uses\":1,\"recharge\":\"long_rest\"}",
            Self::ModifierCount => "a count of things tied to an ability modifier (\"one companion per point of your \
Charisma modifier\") is a `stat` counter named for what it counts ({\"kind\":\"stat\",\"name\":\"<counted things, \
e.g. companions>\",\"value\":\"charisma:modifier\"}; twice the modifier is two such rules)",
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

/// "once per long rest", "three times per day": a use the text limits.
fn limited_use(s: &str) -> bool {
    FREQUENCIES.iter().any(|f| {
        s.match_indices(f)
            .any(|(i, _)| PERIODS.iter().any(|p| s[i + f.len()..].starts_with(p)))
    })
}

/// "one companion per point of your Charisma modifier", "squires equal to twice your
/// Charisma modifier": a count of things the text ties to an ability modifier.
fn modifier_count(s: &str) -> bool {
    ABILITIES.iter().any(|a| {
        let modifier = format!("{a} modifier");
        s.match_indices(&modifier).any(|(i, _)| {
            let before = &s[..i];
            let gap = |marker: &str| {
                before
                    .rfind(marker)
                    .filter(|p| !before[..*p].ends_with(char::is_alphabetic))
                    .map(|p| {
                        before[p + marker.len()..]
                            .split_whitespace()
                            .collect::<Vec<_>>()
                    })
            };
            let per = gap("per ").is_some_and(|g| g.iter().all(|w| PER_FILLERS.contains(w)));
            let equal = gap("equal to ").is_some_and(|g| {
                g.len() <= EQUAL_GAP_WORDS
                    && g.iter()
                        .all(|w| w.chars().all(|c| c.is_alphabetic() || c == '\''))
            }) && !NOT_A_COUNT.iter().any(|w| s.contains(w));
            per || equal
        })
    })
}

/// Words between "per" and "<ability> modifier" ("per point of your Charisma modifier").
const PER_FILLERS: [&str; 7] = ["point", "of", "your", "the", "their", "his", "her"];
/// Most words between "equal to" and "<ability> modifier" ("twice the character's").
const EQUAL_GAP_WORDS: usize = 4;

/// The rule shapes a lowercase sentence asks for.
fn shapes_of(s: &str) -> Vec<Shape> {
    let mut out = Vec::new();
    if modifier_count(s) {
        out.push(Shape::ModifierCount);
    }
    if !(s.contains("you") || s.contains("your")) {
        return out;
    }
    if limited_use(s) {
        out.push(Shape::LimitedUse);
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
            Shape::LimitedUse => {
                // A resource, or uses with a recharge anywhere in the rule (a spell's free_cast).
                let text = r.to_string();
                matches!(kind, "resource" | "resource_cost")
                    || text.contains("\"uses\"")
                    || text.contains("\"recharge\"")
            }
            Shape::ModifierCount => {
                matches!(kind, "stat" | "resource") && r.to_string().contains(":modifier")
            }
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

/// What the sheet will not compute for one element.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ElementMechanics {
    /// Mechanics its own text states that no rule implements (the sheet shows them as text).
    pub text_only: Vec<String>,
    /// Mechanics its source passage states that neither its rules nor its text carry.
    pub gaps: Vec<String>,
}

/// [`ElementMechanics`] of `e`, whose source passage is `passage` (none when unlocated).
pub(crate) fn element_mechanics(
    draft: &ImportDraft,
    e: &DraftElement,
    passage: Option<&str>,
) -> ElementMechanics {
    if e.json.is_null() || e.role == Role::Text || SKIP_TYPES.contains(&str_at(&e.json, "type")) {
        return ElementMechanics::default();
    }
    let granted = related(draft, &e.id, &e.json);
    ElementMechanics {
        text_only: unmodelled(&e.json, &granted),
        gaps: passage.map_or_else(Vec::new, |p| passage_gaps(&e.json, &granted, p)),
    }
}

/// Element types of a species root: every species has a walking speed.
const SPECIES_TYPES: [&str; 2] = ["Race", "Species"];

/// A species root whose rules (and its traits' rules) set no speed at all: its passage was most
/// likely cut short (an image page not read, a lead-in with the traits missing).
pub(crate) fn species_without_speed(draft: &ImportDraft, e: &DraftElement) -> bool {
    if e.json.is_null() || !SPECIES_TYPES.contains(&str_at(&e.json, "type")) {
        return false;
    }
    let granted = related(draft, &e.id, &e.json);
    let mut rules = rules_of(&e.json);
    rules.extend(granted.iter().flat_map(|g| rules_of(g)));
    !rules
        .iter()
        .any(|r| str_at(r, "kind") == "stat" && str_at(r, "name").contains("speed"))
}

/// Shapes the passage states that the element neither models nor keeps in its (or a granted
/// element's) text: those are lost, not merely text only.
fn passage_gaps(el: &Value, granted: &[&Value], passage: &str) -> Vec<String> {
    let enhanced = el
        .get("setters")
        .and_then(Value::as_array)
        .is_some_and(|s| s.iter().any(|x| str_at(x, "name") == "enhancement"));
    let mut rules = rules_of(el);
    rules.extend(granted.iter().flat_map(|g| rules_of(g)));
    let carried: Vec<Shape> = std::iter::once(el)
        .chain(granted.iter().copied())
        .flat_map(|g| sentences(&plain(str_at(g, "description"))))
        .flat_map(|s| shapes_of(&s.to_lowercase()))
        .collect();
    let mut out = Vec::new();
    let mut seen: Vec<Shape> = Vec::new();
    for s in sentences(passage) {
        for shape in shapes_of(&s.to_lowercase()) {
            if seen.contains(&shape)
                || carried.contains(&shape)
                || modelled(shape, &rules)
                || (shape == Shape::Bonus && enhanced)
            {
                continue;
            }
            seen.push(shape);
            let quote: String = s.chars().take(QUOTE_CHARS).collect();
            out.push(format!(
                "the source says \"{quote}\" but this element has no rule or text for it"
            ));
        }
    }
    out
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

/// A feature whose text sets a save against "your <class> spell save DC" names that DC: the
/// sheet shows it under the feature (bundled shape `{"kind":"save_dc","formula":"spell save DC
/// (<Class>)"}`). Added when the element has no `save_dc` rule and its class is known (the
/// element's `data.class`, printed before "spell save DC" or the text says "your spell save
/// DC"). Returns a note when it changed.
pub(crate) fn caster_save_dc(el: &mut Value) -> Option<String> {
    if SKIP_TYPES.contains(&str_at(el, "type")) {
        return None;
    }
    let class = el
        .pointer("/data/class")
        .and_then(Value::as_str)?
        .trim()
        .to_string();
    let lower = plain(str_at(el, "description")).to_lowercase();
    let own = format!("your {} spell save dc", class.to_lowercase());
    if class.is_empty() || !(lower.contains(&own) || lower.contains("your spell save dc")) {
        return None;
    }
    if rules_of(el).iter().any(|r| str_at(r, "kind") == "save_dc") {
        return None;
    }
    let id = str_at(el, "name").trim().to_lowercase();
    if id.is_empty() {
        return None;
    }
    let formula = format!("spell save DC ({class})");
    let rule = serde_json::json!({"kind": "save_dc", "id": id, "formula": formula});
    let obj = el.as_object_mut()?;
    let rules = obj
        .entry("rules")
        .or_insert_with(|| Value::Array(Vec::new()))
        .as_array_mut()?;
    rules.push(rule);
    Some(format!(
        "save_dc {id:?} = {formula} (the text sets a save against it)"
    ))
}

/// `requires attunement by a/an ...` printed in the text and the `attunement` setter's
/// `addition` describe the same restriction: the setter gets the printed words when it has
/// none, the text gets the setter's when it lacks them. An item's rule conditions that only
/// restate that the item is equipped or attuned are removed first
/// ([`super::rule_terms::item_self_conditions`]; this is the item fix both generation and
/// finalize run, before the armor enhancement rewrite). Returns a note per change.
pub(crate) fn attunement_by(el: &mut Value) -> Vec<String> {
    let mut notes = super::rule_terms::item_self_conditions(el);
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
    fn limited_uses_and_modifier_counts_without_rules_are_found() {
        let trait_ = json!({"type": "Racial Trait", "name": "Moth Call", "description": "<p>Once per \
long rest, you can cast a special version of the <em>Animal Friendship</em> spell.</p>",
            "rules": [{"kind": "grant", "type": "Spell", "id": "ID_X_SPELL"}]});
        let f = unmodelled(&trait_, &[]);
        assert_eq!(f.len(), 1, "{f:?}");
        assert!(f[0].contains("\"kind\":\"resource\""), "{f:?}");
        let mut done = trait_.clone();
        done["rules"] = json!([{"kind": "resource", "id": "moth call", "name": "Moth Call",
            "max": {"type": "fixed", "value": 1}, "recharge": ["long_rest"]}]);
        assert!(unmodelled(&done, &[]).is_empty());
        let daily = json!({"type": "Magic Item", "rules": [], "description":
            "<p>Three times per day you may cast a spell from it.</p>"});
        assert_eq!(unmodelled(&daily, &[]).len(), 1);
        let free = json!({"type": "Magic Item", "description": daily["description"],
            "rules": [{"kind": "spell_free_cast", "select": "x", "uses": 3, "recharge": ["long_rest"]}]});
        assert!(unmodelled(&free, &[]).is_empty());
        let nested = json!({"type": "Magic Item", "description": daily["description"],
            "rules": [{"kind": "grant_spell", "spell": "x",
                "free_cast": {"uses": 3, "recharge": "long_rest"}}]});
        assert!(unmodelled(&nested, &[]).is_empty());

        for text in [
            "<p>Heroes can keep one hound per point of their Charisma modifier.</p>",
            "<p>Heroes may keep one hound or squire per charisma modifier, i.e. three at +3.</p>",
            "<p>Heroes can take extra squires equal to twice the hero's Charisma modifier.</p>",
        ] {
            let feat = json!({"type": "Feat", "name": "Retinue", "description": text});
            let f = unmodelled(&feat, &[]);
            assert_eq!(f.len(), 1, "{text}: {f:?}");
            assert!(f[0].contains("charisma:modifier"), "{f:?}");
            let counted = json!({"type": "Feat", "description": text,
                "rules": [{"kind": "stat", "name": "squires", "value": "charisma:modifier"}]});
            assert!(unmodelled(&counted, &[]).is_empty(), "{text}");
        }
        // Rolls, healing, uses and formulas are not counts.
        for quiet in [
            "you gain a bonus to the roll equal to your charisma modifier",
            "you regain hit points equal to your wisdom modifier",
            "you can do this a number of times equal to your wisdom modifier",
            "a range equal to 10 + your dexterity modifier",
            "a proper charisma modifier",
        ] {
            assert!(!modifier_count(quiet), "{quiet}");
        }
        // Once per turn is no limited use.
        let turn = json!({"type": "Feat", "rules": [],
            "description": "<p>Once per turn, you can deal an extra 1d6 damage.</p>"});
        assert!(unmodelled(&turn, &[]).is_empty());
    }

    #[test]
    fn a_save_against_the_class_spell_save_dc_names_that_dc() {
        let mut el = json!({"type": "Archetype Feature", "name": "Mind Spike",
            "data": {"class": "Wayfinder", "level": 6},
            "description": "<p>The target must make a Wisdom saving throw against your wayfinder \
spell save DC.</p>", "rules": [{"kind": "resource", "id": "mind spike", "name": "Mind Spike"}]});
        assert!(caster_save_dc(&mut el).is_some());
        assert_eq!(
            el["rules"][1],
            json!({"kind": "save_dc", "id": "mind spike", "formula": "spell save DC (Wayfinder)"})
        );
        assert!(caster_save_dc(&mut el).is_none(), "idempotent");
        // No class, a spell, or a DC the text sets otherwise: nothing to name.
        for quiet in [
            json!({"type": "Racial Trait", "name": "A", "description": "<p>against your spell save DC</p>"}),
            json!({"type": "Spell", "name": "B", "data": {"class": "Wayfinder"},
                "description": "<p>against your spell save DC</p>"}),
            json!({"type": "Class Feature", "name": "C", "data": {"class": "Wayfinder"},
                "description": "<p><strong>Spell save DC</strong> = 8 + your proficiency bonus</p>"}),
        ] {
            let mut q = quiet.clone();
            assert!(caster_save_dc(&mut q).is_none(), "{quiet}");
        }
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
