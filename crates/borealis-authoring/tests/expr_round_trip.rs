//! Spec 6.5 and 18.1 (WB-02): every expression in the bundled packs (`app/content`) reads into its builder and is
//! written back byte for byte, or is kept as is (an asserted list). The canonical text of every model means exactly
//! what the original means, read the way the engine reads it, and is a fixed point of the builder.
//!
//! The counts printed by `bundled_expressions_round_trip` are the ones in `specs/handoffs/hc-wb02.md`.

use borealis_authoring::expr_build::condition::{
    ConditionContext, ConditionLanguage, ConditionRow,
};
use borealis_authoring::expr_build::equipped::{EquippedLanguage, EquippedRow};
use borealis_authoring::expr_build::formula::{
    dialect_of, FormulaDialect, FormulaLanguage, FORMULA_FIELDS,
};
use borealis_authoring::expr_build::tag_filter::{TagFilterLanguage, TagRow};
use borealis_authoring::expr_build::{Built, ExprLanguage, ExprValue, Node};
use borealis_content::expr::sanitize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn json_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            json_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "json") {
            out.push(p);
        }
    }
}

/// Every element of every bundled pack.
fn bundled_elements() -> Vec<Value> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let mut paths = Vec::new();
    json_files(&root, &mut paths);
    paths.sort();
    let mut out = Vec::new();
    for p in paths {
        let text = std::fs::read_to_string(&p).expect("read");
        let doc: Value = serde_json::from_str(&text).expect("json");
        out.extend(doc["elements"].as_array().into_iter().flatten().cloned());
    }
    assert!(out.len() > 3000, "the bundled packs are there");
    out
}

/// The expressions of the bundled packs, by builder.
#[derive(Default)]
struct Corpus {
    conditions: Vec<String>,
    tag_filters: Vec<String>,
    equipped: Vec<String>,
    formulas: Vec<(FormulaDialect, String)>,
    classes: Vec<String>,
}

fn str_at<'a>(v: &'a Value, path: &str) -> Vec<&'a str> {
    let mut cur = vec![v];
    for part in path.split('.') {
        let (key, list) = part.strip_suffix("[]").map_or((part, false), |k| (k, true));
        cur = cur
            .into_iter()
            .filter_map(|x| x.get(key))
            .flat_map(|x| match (list, x.as_array()) {
                (true, Some(a)) => a.iter().collect::<Vec<_>>(),
                _ => vec![x],
            })
            .collect();
    }
    cur.into_iter().filter_map(Value::as_str).collect()
}

fn rules_of(v: &Value) -> impl Iterator<Item = &Value> {
    v["rules"].as_array().into_iter().flatten()
}

fn collect_rule(rule: &Value, c: &mut Corpus) {
    let kind = rule["kind"].as_str().unwrap_or("");
    if let Some(r) = rule["requirements"].as_str() {
        c.conditions.push(r.to_string());
    }
    if kind == "select" {
        c.tag_filters
            .extend(rule["supports"].as_str().map(str::to_string));
    }
    if kind == "stat" {
        c.equipped
            .extend(rule["equipped"].as_str().map(str::to_string));
    }
    for (k, path, dialect) in FORMULA_FIELDS {
        if *k == kind {
            for s in str_at(rule, path) {
                c.formulas.push((*dialect, s.to_string()));
            }
        }
    }
    // nested block lists (toggle effects)
    for nested in rule["effects"].as_array().into_iter().flatten() {
        collect_rule(nested, c);
    }
}

fn corpus() -> Corpus {
    let mut c = Corpus::default();
    for el in bundled_elements() {
        if el["type"] == "Class" {
            c.classes.extend(el["name"].as_str().map(str::to_string));
        }
        if let Some(r) = el["requirements"].as_str() {
            c.conditions.push(r.to_string());
        }
        let mc = &el["multiclass"];
        if let Some(r) = mc["requirements"].as_str() {
            c.conditions.push(r.to_string());
        }
        for rule in rules_of(&el).chain(rules_of(mc)) {
            collect_rule(rule, &mut c);
        }
        let sc = &el["spellcasting"];
        c.tag_filters
            .extend(sc["list"]["supports"].as_str().map(str::to_string));
        for e in sc["extends"].as_array().into_iter().flatten() {
            c.tag_filters
                .extend(e["supports"].as_str().map(str::to_string));
        }
        if el["type"] == "Magic Item" {
            for s in el["setters"].as_array().into_iter().flatten() {
                if matches!(s["name"].as_str(), Some("weapon" | "armor")) {
                    c.tag_filters
                        .extend(s["value"].as_str().map(str::to_string));
                }
            }
        }
        c.formulas.extend(el["data"]["dc"].as_str().map(|s| {
            (
                dialect_of("element", "data.dc").expect("data.dc"),
                s.to_string(),
            )
        }));
    }
    c
}

#[derive(Default, Debug)]
struct Counts {
    total: usize,
    distinct: usize,
    canonical: usize,
    sanitized: usize,
    kept_whole: Vec<String>,
    kept_rows: Vec<String>,
}

/// Read, write back, re-render: the guarantees for one language over its corpus.
fn round_trip<L: ExprLanguage>(
    name: &str,
    texts: &[String],
    cx: &L::Context,
    kept_rows: &dyn Fn(&L::Model) -> bool,
) -> Counts
where
    L::Model: std::fmt::Debug,
{
    let mut counts = Counts {
        total: texts.len(),
        ..Counts::default()
    };
    let mut distinct: Vec<&String> = texts.iter().collect();
    distinct.sort();
    distinct.dedup();
    counts.distinct = distinct.len();
    for text in distinct {
        let value = ExprValue::read::<L>(text, cx);
        assert_eq!(
            &value.text::<L>(cx),
            text,
            "{name}: written back byte for byte"
        );
        let model = match &value.value {
            Built::Kept { .. } => {
                counts.kept_whole.push(text.clone());
                continue;
            }
            Built::Model { model } => model,
        };
        assert!(
            L::check(model, cx).is_empty(),
            "{name}: {text:?} reads to a model with problems: {:?}",
            L::check(model, cx)
        );
        let canonical = L::render(model, cx);
        assert!(
            L::means_same(model, &canonical, cx),
            "{name}: {text:?} -> {canonical:?} changes its meaning"
        );
        assert!(
            L::means_same(model, text, cx),
            "{name}: {text:?} means its model"
        );
        let again = L::parse(&canonical, cx);
        assert_eq!(
            again,
            Built::Model {
                model: model.clone()
            },
            "{name}: {canonical:?} is a fixed point"
        );
        assert_eq!(L::render(model, cx), canonical);
        counts.canonical += usize::from(&canonical == text);
        counts.sanitized += usize::from(canonical == sanitize(text));
        if kept_rows(model) {
            counts.kept_rows.push(text.clone());
        }
    }
    eprintln!(
        "{name}: {} expressions ({} distinct): canonical text identical {}, identical to sanitize(text) {}, \
         original spelling kept {}, kept as is {}, with a kept row {}",
        counts.total,
        counts.distinct,
        counts.canonical,
        counts.sanitized,
        counts.distinct - counts.canonical - counts.kept_whole.len(),
        counts.kept_whole.len(),
        counts.kept_rows.len()
    );
    counts
}

fn any_row<R>(node: &Node<R>, kept: &dyn Fn(&R) -> bool) -> bool {
    node.rows().into_iter().any(|(_, r)| kept(r))
}

#[test]
fn bundled_expressions_round_trip() {
    let c = corpus();
    let cx = ConditionContext::new(&c.classes);

    let cond = round_trip::<ConditionLanguage>("conditions", &c.conditions, &cx, &|m| {
        any_row(m, &|r| matches!(r, ConditionRow::Kept { .. }))
    });
    assert!(cond.kept_whole.is_empty(), "{:?}", cond.kept_whole);
    assert!(cond.kept_rows.is_empty(), "{:?}", cond.kept_rows);

    let tags = round_trip::<TagFilterLanguage>("tag filters", &c.tag_filters, &(), &|m| {
        any_row(&m.root, &|r| matches!(r, TagRow::Kept { .. }))
    });
    assert!(tags.kept_whole.is_empty(), "{:?}", tags.kept_whole);
    assert!(tags.kept_rows.is_empty(), "{:?}", tags.kept_rows);

    let eq = round_trip::<EquippedLanguage>("equipped", &c.equipped, &(), &|m| {
        any_row(m, &|r| matches!(r, EquippedRow::Kept { .. }))
    });
    assert!(eq.kept_whole.is_empty(), "{:?}", eq.kept_whole);
    assert!(eq.kept_rows.is_empty(), "{:?}", eq.kept_rows);

    let mut by_dialect: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (d, text) in &c.formulas {
        by_dialect
            .entry(format!("{d:?}"))
            .or_default()
            .push(text.clone());
    }
    let mut kept = Vec::new();
    for d in FormulaDialect::ALL {
        let texts = by_dialect.remove(&format!("{d:?}")).unwrap_or_default();
        let counts =
            round_trip::<FormulaLanguage>(&format!("formulas ({d:?})"), &texts, &d, &|_| false);
        kept.extend(counts.kept_whole);
    }
    assert!(kept.is_empty(), "{kept:?}");
}

/// Every field of [`FORMULA_FIELDS`] that bundled content uses is found by the walk (the table's paths are right).
#[test]
fn formula_fields_reach_bundled_values() {
    let c = corpus();
    let used: Vec<FormulaDialect> = c.formulas.iter().map(|(d, _)| *d).collect();
    for d in FormulaDialect::ALL {
        assert!(used.contains(&d), "no bundled formula in the {d:?} dialect");
    }
    assert!(c.formulas.len() >= 30, "{}", c.formulas.len());
}
