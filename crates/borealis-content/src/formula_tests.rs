//! The formula grammar: hand-written cases, the canonical text round trip, and agreement with the evaluator the engine
//! used before the parser moved here (kept below as the oracle, unchanged apart from its context) on every string in
//! the bundled packs' rules and on generated text.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::Value;

use super::*;

/// A character for the tests: proficiency, six modifiers, class levels.
struct Cx {
    prof: i32,
    mods: [i32; 6],
    levels: BTreeMap<&'static str, i32>,
}

const ABILITIES: [&str; 6] = [
    "strength",
    "dexterity",
    "constitution",
    "intelligence",
    "wisdom",
    "charisma",
];
const ABBR: [&str; 6] = ["str", "dex", "con", "int", "wis", "cha"];

fn ability_index(key: &str) -> Option<usize> {
    let k = key.trim().to_lowercase();
    ABILITIES
        .iter()
        .position(|a| *a == k)
        .or_else(|| ABBR.iter().position(|a| *a == k))
}

impl FormulaContext for Cx {
    fn proficiency(&self) -> i32 {
        self.prof
    }
    fn ability_modifier(&self, ability: &str) -> Option<i32> {
        ability_index(ability).map(|i| self.mods[i])
    }
    fn class_level(&self, class: &str) -> i32 {
        self.levels.get(class).copied().unwrap_or(0)
    }
}

fn contexts() -> Vec<Cx> {
    vec![
        Cx {
            prof: 2,
            mods: [3, 2, 1, 0, -1, 4],
            levels: BTreeMap::from([("cleric", 3), ("paladin", 5)]),
        },
        Cx {
            prof: 6,
            mods: [-1, 5, 0, 2, 3, -2],
            levels: BTreeMap::from([("druid", 17), ("monk", 20), ("cleric", 1)]),
        },
        Cx {
            prof: 0,
            mods: [0; 6],
            levels: BTreeMap::new(),
        },
    ]
}

// ------------------------------------------------------------------ the oracle: the engine's evaluator up to 1.4.x

fn old_split_top(expr: &str, ops: &[char]) -> Option<(String, char, String)> {
    let mut depth = 0i32;
    let chars: Vec<(usize, char)> = expr.char_indices().collect();
    for (i, ch) in chars.iter().rev() {
        match ch {
            ')' => depth += 1,
            '(' => depth -= 1,
            c if depth == 0 && ops.contains(c) && *i > 0 => {
                return Some((
                    expr[..*i].trim().to_string(),
                    *c,
                    expr[*i + c.len_utf8()..].trim().to_string(),
                ));
            }
            _ => {}
        }
    }
    None
}

fn old_split_commas(inner: &str) -> Vec<&str> {
    let mut depth = 0i32;
    let mut start = 0;
    let mut out = Vec::new();
    for (i, ch) in inner.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                out.push(&inner[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(&inner[start..]);
    out
}

fn old_eval(expr: &str, cx: &Cx) -> Option<i32> {
    let e = expr.trim();
    if let Some((l, '/', r)) = e
        .strip_prefix("ceil(")
        .and_then(|x| x.strip_suffix(')'))
        .and_then(|x| old_split_top(x, &['/']))
    {
        let (a, b) = (old_eval(&l, cx)?, old_eval(&r, cx)?);
        return (b > 0).then(|| (a + b - 1) / b);
    }
    if let Some((l, op, r)) = old_split_top(e, &['+', '-']) {
        let (a, b) = (old_eval(&l, cx)?, old_eval(&r, cx)?);
        return Some(if op == '+' { a + b } else { a - b });
    }
    if let Some((l, op, r)) = old_split_top(e, &['*', '/']) {
        let (a, b) = (old_eval(&l, cx)?, old_eval(&r, cx)?);
        return if op == '*' {
            Some(a * b)
        } else {
            a.checked_div(b)
        };
    }
    for f in ["max", "min", "floor"] {
        if let Some(inner) = e
            .strip_prefix(f)
            .and_then(|r| r.trim_start().strip_prefix('('))
            .and_then(|r| r.strip_suffix(')'))
        {
            let vals: Option<Vec<i32>> = old_split_commas(inner)
                .into_iter()
                .map(|p| old_eval(p, cx))
                .collect();
            let vals = vals?;
            return match f {
                "max" => vals.into_iter().max(),
                "min" => vals.into_iter().min(),
                _ => vals.first().copied(),
            };
        }
    }
    if let Some(inner) = e.strip_prefix('(').and_then(|r| r.strip_suffix(')')) {
        return old_eval(inner, cx);
    }
    if let Ok(n) = e.parse::<i32>() {
        return Some(n);
    }
    let lower = e.to_lowercase();
    if lower == "proficiency" || lower == "proficiency bonus" {
        return Some(cx.prof);
    }
    if let Some(a) = lower.strip_suffix(":modifier") {
        let i = ability_index(a)?;
        return Some(cx.mods[i]);
    }
    if let Some(class) = lower.strip_suffix(" level") {
        return Some(cx.levels.get(class.trim()).copied().unwrap_or(0));
    }
    None
}

// ------------------------------------------------------------------ helpers

fn eval(text: &str, cx: &Cx) -> Option<i32> {
    parse(text).ok().and_then(|f| f.eval(cx))
}

/// The parser and the old evaluator agree on `text` in every context, and a parsed formula renders to text that
/// parses back to itself.
fn assert_agrees(text: &str) {
    for cx in contexts() {
        assert_eq!(eval(text, &cx), old_eval(text, &cx), "{text:?}");
    }
    if let Ok(f) = parse(text) {
        let shown = f.to_string();
        assert_eq!(
            parse(&shown).as_ref(),
            Ok(&f),
            "{text:?} renders as {shown:?}"
        );
    }
}

/// Every string anywhere in the custom rules of the bundled packs.
fn bundled_rule_strings() -> Vec<String> {
    fn walk(v: &Value, out: &mut Vec<String>) {
        match v {
            Value::String(s) => out.push(s.clone()),
            Value::Array(a) => a.iter().for_each(|x| walk(x, out)),
            Value::Object(m) => m.values().for_each(|x| walk(x, out)),
            _ => {}
        }
    }
    fn files(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                files(&p, out);
            } else if p.extension().is_some_and(|x| x == "json") {
                out.push(p);
            }
        }
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let mut paths = Vec::new();
    files(&root, &mut paths);
    let mut out = Vec::new();
    for p in paths {
        let Ok(text) = std::fs::read_to_string(&p) else {
            continue;
        };
        let Ok(doc) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        for el in doc["elements"].as_array().into_iter().flatten() {
            for rule in el["rules"].as_array().into_iter().flatten() {
                walk(rule, &mut out);
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Deterministic text from a small formula alphabet (no external generator).
fn generated(count: usize) -> Vec<String> {
    const PARTS: [&str; 24] = [
        "1",
        "2",
        "-3",
        "+4",
        "0",
        " ",
        "+",
        "-",
        "*",
        "/",
        "(",
        ")",
        ",",
        "max(",
        "min(",
        "floor(",
        "ceil(",
        "proficiency",
        "wisdom:modifier",
        "WIS:modifier",
        "cleric level",
        "druid level",
        "proficiency bonus",
        "x",
    ];
    let mut seed: u64 = 0x5eed_f0a1;
    let mut next = move || {
        seed = seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (seed >> 33) as usize
    };
    (0..count)
        .map(|_| {
            let len = 1 + next() % 9;
            (0..len).map(|_| PARTS[next() % PARTS.len()]).collect()
        })
        .collect()
}

// ------------------------------------------------------------------ tests

#[test]
fn reads_the_shapes_content_uses() {
    let cx = &contexts()[0];
    assert_eq!(eval("5*cleric level", cx), Some(15));
    assert_eq!(eval("max(1, charisma:modifier)", cx), Some(4));
    assert_eq!(eval("ceil(paladin level / 2)", cx), Some(3));
    assert_eq!(eval("proficiency + 2", cx), Some(4));
    assert_eq!(eval("Proficiency Bonus", cx), Some(2));
    assert_eq!(eval("8 + proficiency + wis:modifier", cx), Some(9));
    assert_eq!(eval("floor(7, 9)", cx), Some(7));
    assert_eq!(eval("(1 + 2) * 3", cx), Some(9));
    assert_eq!(eval("10 - 3 - 2", cx), Some(5), "left-associative");
    assert_eq!(eval("-2 + 3", cx), Some(1), "a leading sign is not a split");
    assert_eq!(eval("wizard level", cx), Some(0), "no levels in the class");
}

#[test]
fn evaluation_without_a_value() {
    let cx = &contexts()[0];
    assert_eq!(eval("4 / 0", cx), None);
    assert_eq!(eval("ceil(4 / 0)", cx), None);
    assert_eq!(eval("luck:modifier", cx), None, "unknown ability");
    assert!(
        parse("luck:modifier").is_ok(),
        "the parser does not know abilities"
    );
}

#[test]
fn unreadable_text_names_the_piece() {
    assert_eq!(
        parse("2 + banana"),
        Err(FormulaError {
            part: "banana".into()
        })
    );
    assert!(parse("").is_err());
    assert!(
        parse("3 * -2").is_err(),
        "a sign after an operator splits there"
    );
    // The engine reads a text that starts with `ceil(` and ends with `)` as one ceil: here its "class" is
    // `2) + ceil(paladin`, so it divides by 0 and has no value.
    let text = "ceil(cleric level / 2) + ceil(paladin level / 2)";
    assert!(parse(text).is_ok());
    assert_eq!(eval(text, &contexts()[0]), None);
}

#[test]
fn renders_canonical_text() {
    let shown = |t: &str| parse(t).unwrap().to_string();
    assert_eq!(shown("5*Cleric Level"), "5 * cleric level");
    assert_eq!(shown("max(1,CHA:modifier)"), "max(1, cha:modifier)");
    assert_eq!(shown("ceil(druid level/2)"), "ceil(druid level / 2)");
    assert_eq!(shown("(1+2)*3"), "(1 + 2) * 3");
    assert_eq!(shown("proficiency bonus"), "proficiency");
    let built = Formula::Binary {
        op: BinaryOp::Sub,
        left: Box::new(Formula::Number(1)),
        right: Box::new(Formula::Binary {
            op: BinaryOp::Add,
            left: Box::new(Formula::Number(2)),
            right: Box::new(Formula::Number(3)),
        }),
    };
    assert_eq!(built.to_string(), "1 - (2 + 3)");
    assert_eq!(eval(&built.to_string(), &contexts()[0]), Some(-4));
    let ceil = Formula::CeilDiv(
        Box::new(Formula::Number(9)),
        Box::new(Formula::Binary {
            op: BinaryOp::Div,
            left: Box::new(Formula::Number(4)),
            right: Box::new(Formula::Number(2)),
        }),
    );
    assert_eq!(ceil.to_string(), "ceil(9 / (4 / 2))");
}

#[test]
fn agrees_with_the_old_evaluator_on_hand_written_edge_cases() {
    for t in [
        "",
        " 3 ",
        "+5",
        "-5",
        "--5",
        "3 - -2",
        "2*-1",
        "max()",
        "max(,)",
        "max (2, 3)",
        "maximum(2)",
        "min(3, x)",
        "floor()",
        "ceil(5)",
        "ceil(5)/2",
        "ceil(7 / 2) + 1",
        "ceil(a level / 2) + (1)",
        "((2))",
        "(2",
        "2)",
        ")(",
        "2 + (3 * (4 - 1))",
        "Wisdom :modifier",
        " level",
        "level",
        "half-orc level",
        "proficiency level",
        "x level:modifier",
        "2147483647",
        "-2147483648",
        "99999999999",
        "6 / 4",
        "-7 / 2",
        "ceil(-3 / 2)",
        "1/0 + 1",
    ] {
        assert_agrees(t);
    }
}

#[test]
fn agrees_with_the_old_evaluator_on_every_bundled_rule_string() {
    let strings = bundled_rule_strings();
    assert!(
        strings.len() > 1000,
        "bundled packs found: {}",
        strings.len()
    );
    let mut formulas = 0;
    for s in &strings {
        assert_agrees(s);
        formulas += usize::from(parse(s).is_ok());
    }
    assert!(
        formulas > 50,
        "formulas among the bundled strings: {formulas}"
    );
}

#[test]
fn agrees_with_the_old_evaluator_on_generated_text() {
    for t in generated(20_000) {
        // Overflow panics in debug builds in both evaluators alike; keep the numbers small.
        assert_agrees(&t);
    }
}
