//! Custom rules of generated elements against the engine's rule-kind registry
//! (`borealis_content::rule_kinds::KNOWN_KINDS`, the single source of truth): the catalogue the
//! generation prompt shows, the validation that feeds the repair loop, the deterministic
//! normalisation of `resource` rules and, as a last resort, the removal of what the engine would
//! ignore (always reported, never silent).

use std::sync::OnceLock;

use borealis_content::rule_kinds::{check_custom, KNOWN_KINDS};
use borealis_content::rules::{CustomRule, Rule};
use serde_json::{json, Map, Value};

/// `max.type` values `resource` understands.
const MAX_TYPES: [&str; 4] = ["fixed", "stat", "table", "formula"];
/// `recharge` tokens used by content (`short_rest`/`long_rest` drive rests).
const RECHARGE_TOKENS: [&str; 5] = [
    "short_rest",
    "long_rest",
    "turn_start",
    "initiative",
    "none",
];
/// Words a free-text rest description may consist of.
const REST_WORDS: [&str; 10] = [
    "short", "long", "rest", "rests", "or", "and", "a", "finish", "after", "you",
];

/// Shape hints for the kinds generated content needs most. Kept in sync with the registry by
/// `shape_hints_name_registered_kinds_and_fields`.
const SHAPES: &[(&str, &[&str], &str)] = &[
    (
        "resource",
        &["id", "name", "max", "recharge"],
        "resource needs `id` (the lowercase name, e.g. \"rage\"), `name`, `max` as \
{\"type\":\"fixed\",\"value\":2} | {\"type\":\"stat\",\"stat\":\"proficiency\"} | {\"type\":\"table\",\"key\":\"<progression key>\"} \
| {\"type\":\"formula\",\"expr\":\"...\"}, and `recharge` as a list such as [\"short_rest\",\"long_rest\"] or [\"long_rest\"].",
    ),
    (
        "resource_cost",
        &["resource", "cost"],
        "resource_cost spends uses: {\"kind\":\"resource_cost\",\"resource\":\"<resource id>\",\"cost\":1}.",
    ),
    (
        "save_dc",
        &["id", "formula"],
        "save_dc names a DC: {\"kind\":\"save_dc\",\"id\":\"<name>\",\"formula\":\"8 + proficiency + dexterity\"}.",
    ),
];

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
describe mechanics no kind covers in the description instead.\n{}\n{}",
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

fn shape_problems(c: &CustomRule) -> Vec<String> {
    let f = &c.fields;
    let mut out = Vec::new();
    match c.kind.as_str() {
        "resource" => {
            if !f
                .get("id")
                .and_then(Value::as_str)
                .is_some_and(|s| !s.is_empty())
            {
                out.push("has no `id` (the engine skips resources without one)".to_string());
            }
            match f.get("max") {
                None => out.push("has no `max`".to_string()),
                Some(m) if !max_ok(m) => out.push(format!(
                    "`max` {m} is not {{\"type\": one of {MAX_TYPES:?}, ...}}"
                )),
                Some(_) => {}
            }
            if let Some(r) = f.get("recharge").filter(|r| !recharge_ok(r)) {
                out.push(format!(
                    "`recharge` {r} is not a list of {RECHARGE_TOKENS:?}"
                ));
            }
        }
        "resource_cost" | "resource_regain" if f.get("resource").is_none() => {
            out.push("has no `resource` id".to_string());
        }
        _ => {}
    }
    out
}

fn max_ok(m: &Value) -> bool {
    m.get("type")
        .and_then(Value::as_str)
        .is_some_and(|t| MAX_TYPES.contains(&t))
}

fn recharge_ok(r: &Value) -> bool {
    r.as_array().is_some_and(|a| {
        a.iter()
            .all(|t| t.as_str().is_some_and(|t| RECHARGE_TOKENS.contains(&t)))
    })
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
        }
    }
    out
}

/// Lowercase name with single spaces: the resource id convention of the bundled content.
pub(crate) fn resource_id(name: &str) -> String {
    name.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// `recharge` written as words ("short rest", "a short or long rest") as engine tokens, or None
/// when the text is anything else.
fn rest_tokens(text: &str) -> Option<Vec<&'static str>> {
    let lower = text.to_lowercase();
    let words: Vec<&str> = lower
        .split(|c: char| !c.is_ascii_alphabetic())
        .filter(|w| !w.is_empty())
        .collect();
    if words.is_empty() || !words.iter().all(|w| REST_WORDS.contains(w)) {
        return None;
    }
    let mut out = Vec::new();
    if words.contains(&"short") {
        out.push("short_rest");
    }
    if words.contains(&"long") {
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

fn normalise_max(m: &Value) -> Option<Value> {
    let n = match m {
        Value::Number(n) => n.as_i64()?,
        Value::String(s) => s.trim().parse::<i64>().ok()?,
        _ => return None,
    };
    (n >= 0).then(|| json!({"type": "fixed", "value": n}))
}

fn normalise_resource(rule: &mut Map<String, Value>) -> Vec<String> {
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
    if let Some(new) = rule.get("max").and_then(normalise_max) {
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

/// Unambiguous `resource` fixes (id from the name, bare-number `max`, rest names as tokens).
/// Returns one message per change.
pub(crate) fn normalise_resources(el: &mut Value) -> Vec<String> {
    let mut out = Vec::new();
    for_each_rule(el, |r| {
        if r.get("kind").and_then(Value::as_str) == Some("resource") {
            out.extend(normalise_resource(r));
        }
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
mod tests {
    use super::*;
    use borealis_content::rule_kinds::spec;

    #[test]
    fn catalogue_lists_every_registered_kind() {
        let c = catalogue();
        for s in KNOWN_KINDS {
            assert!(c.contains(&format!("\n{}", s.kind)), "{} missing", s.kind);
        }
        assert!(c.contains("resource*: id, name, max"));
    }

    #[test]
    fn shape_hints_name_registered_kinds_and_fields() {
        for (kind, fields, _) in SHAPES {
            let s = spec(kind).unwrap_or_else(|| panic!("{kind} not registered"));
            for f in *fields {
                assert!(s.reads(f), "{kind}.{f} is not read");
            }
        }
    }

    #[test]
    fn invented_kinds_fields_and_broken_resources_are_problems() {
        let el = json!({"rules": [
            {"kind": "grant", "id": "ID_X", "type": "Proficiency"},
            {"kind": "strike_bonus", "amount": 2},
            {"kind": "resource", "name": "Ember", "max": 3, "requires": "x"},
            {"kind": "resource", "id": "ember", "name": "Ember", "max": {"type": "fixed", "value": 3},
             "recharge": ["long_rest"]}
        ]});
        let p = rule_problems(&el);
        assert_eq!(p.len(), 4, "{p:?}");
        assert!(p[0].contains("strike_bonus"));
        assert!(p.iter().any(|m| m.contains("\"requires\"")));
        assert!(p.iter().any(|m| m.contains("no `id`")));
        assert!(p.iter().any(|m| m.contains("`max` 3")));
    }

    #[test]
    fn resources_are_normalised_only_when_unambiguous() {
        let mut el = json!({"rules": [
            {"kind": "resource", "name": "Ember  Coals", "max": 2, "recharge": "Short Rest"},
            {"kind": "resource", "name": "Odd", "max": "lots", "recharge": "dawn"},
            {"kind": "resource_cost", "resource": "EMBER COALS", "cost": 1}
        ]});
        let mut notes = normalise_resources(&mut el);
        let known = resources_of(&el);
        notes.extend(normalise_references(&mut el, &known));
        let r = &el["rules"];
        assert_eq!(r[0]["id"], "ember coals");
        assert_eq!(r[0]["max"], json!({"type": "fixed", "value": 2}));
        assert_eq!(r[0]["recharge"], json!(["short_rest", "long_rest"]));
        assert_eq!(r[1]["id"], "odd");
        assert_eq!(r[1]["max"], "lots");
        assert_eq!(r[1]["recharge"], "dawn");
        assert_eq!(r[2]["resource"], "ember coals");
        assert_eq!(notes.len(), 5, "{notes:?}");
        assert_eq!(rule_problems(&el).len(), 2, "{:?}", rule_problems(&el));
    }

    #[test]
    fn strip_removes_and_reports_everything_the_engine_ignores() {
        let mut el = json!({"rules": [
            {"kind": "strike_bonus", "amount": 2},
            {"kind": "resource", "id": "ember", "name": "Ember", "max": {"type": "fixed", "value": 1},
             "requires": "x"},
            {"kind": "resource", "name": "Nameless", "max": "lots"},
            {"kind": "stat", "name": "speed", "value": "5"}
        ]});
        let notes = strip_invalid(&mut el);
        assert_eq!(notes.len(), 3, "{notes:?}");
        let rules = el["rules"].as_array().unwrap();
        assert_eq!(rules.len(), 2);
        assert!(rules[0].get("requires").is_none());
        assert!(rule_problems(&el).is_empty());
    }
}
