//! Custom rules of generated elements against the engine's rule-kind registry
//! (`borealis_content::rule_kinds::KNOWN_KINDS`, the single source of truth): the catalogue the
//! generation prompt shows, the validation that feeds the repair loop, the deterministic
//! normalisation of `resource` rules and, as a last resort, the removal of what the engine would
//! ignore (always reported, never silent).

use std::sync::OnceLock;

use borealis_content::rule_kinds::{
    check_custom, spec, KNOWN_KINDS, RESOURCE_RECHARGE as RECHARGE_TOKENS,
};
use borealis_content::rules::{CustomRule, Rule};
use serde_json::{json, Map, Value};

/// Words a free-text rest description may consist of.
const REST_WORDS: [&str; 18] = [
    "short", "long", "rest", "rests", "or", "and", "a", "finish", "after", "you", "at", "each",
    "every", "per", "the", "next", "once", "until",
];
/// Once-a-day recharges ("at dawn", "daily"): a character's day ends with a long rest, the
/// engine's token for them.
const DAY_WORDS: [&str; 8] = [
    "dawn", "daybreak", "sunrise", "sunset", "dusk", "day", "daily", "days",
];

/// Shape hints for the kinds generated content needs most. Kept in sync with the registry by
/// `shape_hints_name_registered_kinds_and_fields`.
const SHAPES: &[(&str, &[&str], &str)] = &[
    (
        "resource",
        &["id", "name", "max", "recharge"],
        "resource needs `id` (the lowercase name, e.g. \"rage\"), `name`, `max` as \
{\"type\":\"fixed\",\"value\":2} | {\"type\":\"stat\",\"stat\":\"proficiency\"} | {\"type\":\"table\",\"key\":\"<progression key>\"} \
| {\"type\":\"formula\",\"expr\":\"...\"} (expr words: proficiency, <ability>:modifier such as \
dexterity:modifier, \"<class> level\"; a bare ability name counts as 0), and `recharge` as a list such as [\"short_rest\",\"long_rest\"] or [\"long_rest\"] \
(\"at dawn\" or \"daily\" is [\"long_rest\"]). A pool that grows with character level (1 use, 2 at level 3, 3 at level 5) is one \
resource rule per step with the same `id`, each later one with `level` and `replaces` set to that id; name the pool as the source names it.",
    ),
    (
        "resource_cost",
        &["resource", "cost"],
        "resource_cost spends uses: {\"kind\":\"resource_cost\",\"resource\":\"<resource id>\",\"cost\":1}.",
    ),
    (
        "ability_increase",
        &["name", "options", "cap"],
        "ability_increase lets the player choose ability score increases: `options` is a list of OBJECTS, each one way to \
choose, e.g. [{\"abilities\":[\"strength\",\"dexterity\"],\"pick\":1,\"amount\":1}] (pick 1 of those for +1) or \
[{\"abilities\":1,\"amount\":2},{\"abilities\":2,\"amount\":1}] (any one +2, or two different +1); `cap` is the score maximum \
(20). Fixed increases are `stat` rules (\"constitution\" +2), not this kind.",
    ),
    (
        "save_dc",
        &["id", "formula"],
        "save_dc names a DC: {\"kind\":\"save_dc\",\"id\":\"<name>\",\"formula\":\"8 + proficiency + dexterity\"} \
(terms joined by + only: integers, proficiency, an ability name; or \"spell save DC (<class>)\").",
    ),
];

/// How the mechanics that models most often leave in the text are written (bundled shapes).
const STAT_SHAPES: &str = "Write every mechanic the text states as a rule, never as text only: an armor class formula \
(\"your AC equals 13 + your Dexterity modifier\") is `stat` rules like the bundled Unarmored Defense \
(ac:<name>:base = 13, ac:<name>:dexterity = dexterity:modifier, ac:<name> = each of them, then \
{\"kind\":\"stat\",\"name\":\"ac:calculation\",\"value\":\"ac:<name>\",\"bonus\":\"calculation\",\"equipped\":\"[armor:none]\"}); \
a fixed ability score increase is {\"kind\":\"stat\",\"name\":\"strength\",\"value\":\"2\"}; a species' walking speed \
is {\"kind\":\"stat\",\"name\":\"innate speed\",\"value\":\"30\",\"bonus\":\"base\"} and another lasting speed \
{\"kind\":\"stat\",\"name\":\"speed:fly\",\"value\":\"30\"}; a lasting bonus (\"+1 to AC\") is a `stat` rule. \
mystic_arcanum (spells cast once per rest at higher tiers) always comes with ONE Spell `select` per slot, \
{\"kind\":\"select\",\"type\":\"Spell\",\"name\":\"<Feature> (level 6)\",\"level\":11,\"supports\":\"<Class>,6\",\"spellcasting\":\"<Class>\"}. \
In descriptions keep every number, game term and name as printed (\"fly speed of 30 feet\", \"six hours\", the \
classes an item's attunement names).";

/// The compact rule-kind catalogue for prompts, generated from `KNOWN_KINDS`.
pub(crate) fn catalogue() -> &'static str {
    static TEXT: OnceLock<String> = OnceLock::new();
    TEXT.get_or_init(|| {
        let lines: Vec<String> = KNOWN_KINDS
            .iter()
            .map(|s| {
                let gate = if s.gated { "*" } else { "" };
                format!("{}{gate}: {}", s.kind, s.fields.join(", "))
            })
            .collect();
        let shapes: Vec<&str> = SHAPES.iter().map(|(_, _, t)| *t).collect();
        format!(
            "Rule kinds: `grant`, `select` and `stat` as in the examples, plus ONLY these custom kinds with ONLY \
these fields (kinds marked * also accept `level` and `requirements`). Any other kind or field is rejected; \
describe mechanics no kind covers in the description instead.\n{}\n{}\n{STAT_SHAPES}",
            lines.join("\n"),
            shapes.join("\n")
        )
    })
}

fn custom_of(rule: &Value) -> Option<CustomRule> {
    match serde_json::from_value::<Rule>(rule.clone()).ok()? {
        Rule::Custom(c) => Some(c),
        _ => None,
    }
}

fn rule_arrays(el: &Value) -> Vec<&Vec<Value>> {
    [el.get("rules"), el.pointer("/multiclass/rules")]
        .into_iter()
        .flatten()
        .filter_map(Value::as_array)
        .collect()
}

/// Required fields and closed value sets, from the registry (`KindSpec::shape_problems`).
fn shape_problems(c: &CustomRule) -> Vec<String> {
    spec(&c.kind).map_or_else(Vec::new, |s| s.shape_problems(&c.fields))
}

/// Everything in the element's custom rules the engine would ignore or misread, one message per
/// problem (empty when the rules are clean).
pub(crate) fn rule_problems(el: &Value) -> Vec<String> {
    let mut out = Vec::new();
    for rules in rule_arrays(el) {
        for (i, r) in rules.iter().enumerate() {
            let Some(c) = custom_of(r) else { continue };
            let at = format!("rule {} ({:?})", i + 1, c.kind);
            out.extend(check_custom(&c).iter().map(|p| {
                format!("{at}: {p}; use only the kinds and fields of the rule catalogue")
            }));
            out.extend(shape_problems(&c).into_iter().map(|p| format!("{at} {p}")));
            out.extend(
                formula_problems(&c)
                    .into_iter()
                    .chain(dc_problems(&c))
                    .chain(ability_problems(&c))
                    .map(|p| format!("{at}: {p}")),
            );
        }
    }
    out
}

const ABILITY_NAMES: [&str; 6] = [
    "strength",
    "dexterity",
    "constitution",
    "intelligence",
    "wisdom",
    "charisma",
];

fn ability_name(v: &Value) -> Option<String> {
    let t = v.as_str()?.trim().to_lowercase();
    ABILITY_NAMES
        .iter()
        .find(|a| **a == t || a[..3] == t)
        .map(|a| (*a).to_string())
}

/// An `ability_increase` whose options the engine cannot enumerate offers no choice at all.
fn ability_problems(c: &CustomRule) -> Vec<String> {
    if c.kind != "ability_increase" {
        return Vec::new();
    }
    let Some(options) = c.fields.get("options").and_then(Value::as_array) else {
        return vec!["`options` must be a list of choice objects".into()];
    };
    let good = |o: &Value| {
        let n = o.get("abilities");
        o.get("amount").map_or(true, Value::is_i64)
            && (n
                .and_then(Value::as_u64)
                .is_some_and(|k| (1..=6).contains(&k))
                || n.and_then(Value::as_array)
                    .is_some_and(|a| !a.is_empty() && a.iter().all(|x| ability_name(x).is_some())))
    };
    if options.is_empty() || !options.iter().all(good) {
        return vec![format!(
            "`options` {} is not a list of {{\"abilities\": <count or list of ability names>, \"pick\": n, \"amount\": n}} objects: the player would get no choice",
            Value::Array(options.clone())
        )];
    }
    Vec::new()
}

/// `ability_increase` written as a flat list of ability names (one of them, `amount` or a small
/// `cap` as the increase) as the engine's option object. Returns a message when it changed.
fn normalise_ability_increase(rule: &mut Map<String, Value>) -> Option<String> {
    let options = rule.get("options")?.as_array()?;
    if options.is_empty() || !options.iter().all(Value::is_string) {
        return None;
    }
    let abilities: Vec<String> = options.iter().filter_map(ability_name).collect();
    if abilities.len() != options.len() {
        return None;
    }
    let pick = rule.remove("pick").and_then(|v| v.as_u64()).unwrap_or(1);
    let cap = rule.get("cap").and_then(Value::as_i64);
    let amount = match (rule.remove("amount").and_then(|v| v.as_i64()), cap) {
        (Some(a), _) => a,
        // a "cap" of 1-3 is the increase, not the score maximum
        (None, Some(c)) if (1..=3).contains(&c) => {
            rule.insert("cap".into(), json!(20));
            c
        }
        _ => 1,
    };
    rule.insert(
        "options".into(),
        json!([{"abilities": abilities, "pick": pick, "amount": amount}]),
    );
    Some(format!(
        "ability_increase options written as a list of abilities: read as choose {pick} of {} for +{amount}",
        abilities.join("/")
    ))
}

/// A `resource` max formula the engine cannot read: it would evaluate to 0.
fn formula_problems(c: &CustomRule) -> Vec<String> {
    if c.kind != "resource" {
        return Vec::new();
    }
    let Some(max) = c.fields.get("max") else {
        return Vec::new();
    };
    if max.get("type").and_then(Value::as_str) != Some("formula") {
        return Vec::new();
    }
    let expr = max.get("expr").and_then(Value::as_str).unwrap_or("");
    let bad = super::formula::unreadable(expr);
    if bad.is_empty() {
        return Vec::new();
    }
    vec![format!(
        "max.expr {expr:?} uses {}, which the engine cannot read (it counts as 0); write it with only {}",
        bad.iter().map(|b| format!("{b:?}")).collect::<Vec<_>>().join(", "),
        super::formula::VOCABULARY
    )]
}

/// A `save_dc` formula the engine cannot read: the DC would be missing.
fn dc_problems(c: &CustomRule) -> Vec<String> {
    if c.kind != "save_dc" {
        return Vec::new();
    }
    let expr = c
        .fields
        .get("formula")
        .and_then(Value::as_str)
        .unwrap_or("");
    let bad = super::formula::dc_unreadable(expr);
    if bad.is_empty() {
        return Vec::new();
    }
    vec![format!(
        "formula {expr:?} uses {}, which the engine cannot read (the DC would be missing); write it as {}",
        bad.iter().map(|b| format!("{b:?}")).collect::<Vec<_>>().join(", "),
        super::formula::DC_VOCABULARY
    )]
}

/// Unambiguous save-DC terms in the engine's form. Returns a message when it changed.
fn normalise_save_dc(rule: &mut Map<String, Value>) -> Option<String> {
    let expr = rule.get("formula")?.as_str()?.to_string();
    let (new, changes) = super::formula::normalise_dc(&expr);
    if changes.is_empty() {
        return None;
    }
    rule.insert("formula".into(), json!(new));
    Some(format!("save_dc formula {expr:?} read as {new:?}"))
}

/// Lowercase name with single spaces: the resource id convention of the bundled content.
pub(crate) fn resource_id(name: &str) -> String {
    name.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// `recharge` written as words ("short rest", "a short or long rest", "at dawn", "daily") as
/// engine tokens, or None when the text is anything else.
fn rest_tokens(text: &str) -> Option<Vec<&'static str>> {
    let lower = text.to_lowercase();
    let words: Vec<&str> = lower
        .split(|c: char| !c.is_ascii_alphabetic())
        .filter(|w| !w.is_empty())
        .collect();
    let known = |w: &&str| REST_WORDS.contains(w) || DAY_WORDS.contains(w);
    if words.is_empty() || !words.iter().all(known) {
        return None;
    }
    let mut out = Vec::new();
    if words.contains(&"short") {
        out.push("short_rest");
    }
    if words.contains(&"long") || words.iter().any(|w| DAY_WORDS.contains(w)) {
        out.push("long_rest");
    }
    (!out.is_empty()).then_some(out)
}

fn normalise_recharge(r: &Value) -> Option<Value> {
    let items: Vec<&str> = match r {
        Value::String(s) => vec![s.as_str()],
        Value::Array(a) => a.iter().map(|v| v.as_str()).collect::<Option<_>>()?,
        _ => return None,
    };
    let mut tokens: Vec<&str> = Vec::new();
    for t in items {
        let mapped = match RECHARGE_TOKENS.iter().find(|x| **x == t) {
            Some(known) => vec![*known],
            None => rest_tokens(t)?,
        };
        for m in mapped {
            if !tokens.contains(&m) {
                tokens.push(m);
            }
        }
    }
    // A use regained on a short rest is also regained on a long one.
    if tokens.contains(&"short_rest") && !tokens.contains(&"long_rest") {
        tokens.push("long_rest");
    }
    let out = json!(tokens);
    (&out != r).then_some(out)
}

fn normalise_max(m: &Value, class: Option<&str>) -> Option<Value> {
    let n = match m {
        Value::Number(n) => n.as_i64()?,
        Value::String(s) => match s.trim().parse::<i64>() {
            Ok(n) => n,
            Err(_) => return formula_string(s, class),
        },
        Value::Object(o) if o.get("type").and_then(Value::as_str) == Some("formula") => {
            let expr = o.get("expr").and_then(Value::as_str)?;
            let (new, changes) = super::formula::normalise(expr, class);
            if changes.is_empty() {
                return None;
            }
            let mut out = o.clone();
            out.insert("expr".into(), json!(new));
            return Some(Value::Object(out));
        }
        _ => return None,
    };
    (n >= 0).then(|| json!({"type": "fixed", "value": n}))
}

/// A bare string `max` that is (or normalises to) a readable formula.
fn formula_string(s: &str, class: Option<&str>) -> Option<Value> {
    let (expr, _) = super::formula::normalise(s, class);
    super::formula::unreadable(&expr)
        .is_empty()
        .then(|| json!({"type": "formula", "expr": expr}))
}

fn normalise_resource(rule: &mut Map<String, Value>, class: Option<&str>) -> Vec<String> {
    let mut out = Vec::new();
    let name = rule
        .get("name")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_default();
    let label = if name.is_empty() {
        "resource".to_string()
    } else {
        format!("resource {name:?}")
    };
    let has_id = rule
        .get("id")
        .and_then(Value::as_str)
        .is_some_and(|s| !s.trim().is_empty());
    if !has_id && !name.trim().is_empty() {
        let id = resource_id(&name);
        out.push(format!("{label}: missing `id` set to {id:?} from its name"));
        rule.insert("id".into(), json!(id));
    }
    if let Some(new) = rule.get("max").and_then(|m| normalise_max(m, class)) {
        out.push(format!("{label}: `max` {} wrapped as {new}", rule["max"]));
        rule.insert("max".into(), new);
    }
    if let Some(new) = rule.get("recharge").and_then(normalise_recharge) {
        out.push(format!(
            "{label}: `recharge` {} read as {new}",
            rule["recharge"]
        ));
        rule.insert("recharge".into(), new);
    }
    out
}

fn normalise_reference(
    rule: &mut Map<String, Value>,
    known: &[(String, String)],
) -> Option<String> {
    let current = rule.get("resource")?.as_str()?.to_string();
    if known.iter().any(|(id, _)| *id == current) {
        return None;
    }
    let want = resource_id(&current);
    let hits: Vec<&String> = known
        .iter()
        .filter(|(id, name)| resource_id(id) == want || resource_id(name) == want)
        .map(|(id, _)| id)
        .collect();
    let [id] = hits.as_slice() else { return None };
    rule.insert("resource".into(), json!(id));
    Some(format!(
        "{}: `resource` {current:?} pointed at the resource id {id:?}",
        rule.get("kind").and_then(Value::as_str).unwrap_or("rule")
    ))
}

fn for_each_rule(el: &mut Value, mut f: impl FnMut(&mut Map<String, Value>)) {
    for ptr in ["/rules", "/multiclass/rules"] {
        if let Some(rules) = el.pointer_mut(ptr).and_then(Value::as_array_mut) {
            rules
                .iter_mut()
                .filter_map(Value::as_object_mut)
                .for_each(&mut f);
        }
    }
}

/// Unambiguous `resource` fixes (id from the name, bare-number `max`, formula words the engine
/// cannot read such as a bare `dexterity`, rest names as tokens), `ability_increase` options
/// written as a flat list of ability names, lasting `movement_mode` speeds as stats and
/// save-DC shorthands ("Con mod", "PB") as engine terms.
/// Returns one message per change.
pub(crate) fn normalise_resources(el: &mut Value) -> Vec<String> {
    let mut out = Vec::new();
    let class = el
        .pointer("/data/class")
        .and_then(Value::as_str)
        .map(str::to_string);
    for_each_rule(el, |r| match r.get("kind").and_then(Value::as_str) {
        Some("resource") => out.extend(normalise_resource(r, class.as_deref())),
        Some("ability_increase") => out.extend(normalise_ability_increase(r)),
        Some("movement_mode") => out.extend(normalise_movement_mode(r)),
        Some("save_dc") => out.extend(normalise_save_dc(r)),
        _ => {}
    });
    out
}

/// A `movement_mode` with no `duration` and no `action` is a speed the character always has:
/// the engine reads that as a `speed:<mode>` stat (bundled shape), `movement_mode` is a timed,
/// activated speed. Returns a message when it changed.
fn normalise_movement_mode(rule: &mut Map<String, Value>) -> Option<String> {
    if rule.contains_key("duration") || rule.contains_key("action") {
        return None;
    }
    let mode = rule.get("mode")?.as_str()?.trim().to_lowercase();
    if !matches!(mode.as_str(), "climb" | "swim" | "fly" | "burrow") {
        return None;
    }
    let value = match rule.get("speed")? {
        Value::Number(n) => n.as_i64()?.to_string(),
        Value::String(t) => match t.trim().to_lowercase().as_str() {
            "walk" | "walking" | "speed" | "walking speed" => "speed".to_string(),
            other => other.parse::<i64>().ok()?.to_string(),
        },
        _ => return None,
    };
    let mut stat = Map::new();
    stat.insert("kind".into(), json!("stat"));
    stat.insert("name".into(), json!(format!("speed:{mode}")));
    stat.insert("value".into(), json!(value));
    for key in ["level", "requirements"] {
        if let Some(v) = rule.get(key) {
            stat.insert(key.into(), v.clone());
        }
    }
    *rule = stat;
    Some(format!(
        "a lasting {mode} speed (movement_mode without duration or action) became the stat speed:{mode} = {value}"
    ))
}

/// The `ability_increase`, lasting `movement_mode` and `save_dc` parts of [`normalise_resources`]
/// (finalize re-applies them to drafts generated before they existed).
pub(crate) fn normalise_abilities(el: &mut Value) -> Vec<String> {
    let mut out = Vec::new();
    for_each_rule(el, |r| match r.get("kind").and_then(Value::as_str) {
        Some("ability_increase") => out.extend(normalise_ability_increase(r)),
        Some("movement_mode") => out.extend(normalise_movement_mode(r)),
        Some("save_dc") => out.extend(normalise_save_dc(r)),
        _ => {}
    });
    out
}

/// `(id, name)` of every `resource` rule in `el`.
pub(crate) fn resources_of(el: &Value) -> Vec<(String, String)> {
    rule_arrays(el)
        .into_iter()
        .flatten()
        .filter(|r| r.get("kind").and_then(Value::as_str) == Some("resource"))
        .filter_map(|r| {
            let id = r.get("id")?.as_str()?.to_string();
            let name = r
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or(&id)
                .to_string();
            Some((id, name))
        })
        .collect()
}

/// Point `resource_cost`/`resource_regain` at the id of the single resource (of `known`) whose
/// id or name matches case- and space-insensitively.
pub(crate) fn normalise_references(el: &mut Value, known: &[(String, String)]) -> Vec<String> {
    let mut out = Vec::new();
    for_each_rule(el, |r| {
        if matches!(
            r.get("kind").and_then(Value::as_str),
            Some("resource_cost" | "resource_regain")
        ) {
            out.extend(normalise_reference(r, known));
        }
    });
    out
}

/// Remove what the engine would ignore or misread: rules of unknown kinds, unknown fields and
/// resources it cannot use. Returns one message per removal.
pub(crate) fn strip_invalid(el: &mut Value) -> Vec<String> {
    let mut out = Vec::new();
    for ptr in ["/rules", "/multiclass/rules"] {
        let Some(rules) = el.pointer_mut(ptr).and_then(Value::as_array_mut) else {
            continue;
        };
        let kept: Vec<Value> = rules
            .iter()
            .filter_map(|r| strip_rule(r, &mut out))
            .collect();
        *rules = kept;
    }
    out
}

fn strip_rule(rule: &Value, out: &mut Vec<String>) -> Option<Value> {
    let Some(c) = custom_of(rule) else {
        return Some(rule.clone());
    };
    let problems = check_custom(&c);
    if borealis_content::rule_kinds::spec(&c.kind).is_none() {
        out.push(format!("removed rule {:?}: {}", c.kind, problems[0]));
        return None;
    }
    let mut fields = c.fields.clone();
    for p in &problems {
        if let borealis_content::rule_kinds::RuleProblem::UnknownField { field, .. } = p {
            fields.remove(field);
            out.push(format!("removed field {field:?} of rule {:?}: {p}", c.kind));
        }
    }
    let cleaned = CustomRule {
        kind: c.kind.clone(),
        fields,
    };
    let shape = shape_problems(&cleaned);
    if !shape.is_empty() {
        out.push(format!(
            "removed rule {:?}: it {}",
            c.kind,
            shape.join("; ")
        ));
        return None;
    }
    serde_json::to_value(Rule::Custom(cleaned)).ok()
}

#[cfg(test)]
#[path = "rules_tests.rs"]
mod tests;
