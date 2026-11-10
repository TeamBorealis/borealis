//! Spec 6.2, 6.6 and 18.1 (WB-05): the block catalog against the engine.
//!
//! * Registry diff: the catalog has one block per custom rule kind of the engine registry
//!   (`borealis_engine::custom_rules::KNOWN_KINDS`) and no other; each block offers exactly the fields the engine reads
//!   (gate fields as "From level" / "Only if"), never an unmodelled one, requires every registry-required field, and
//!   offers exactly the registry's closed value sets. A new engine kind or field fails here until the catalog has it.
//! * The core blocks cover every `Rule` variant and every field of the typed rules (exhaustive destructuring: a new
//!   field fails to compile here).
//! * Every rule of every bundled pack fits its block (only unmodelled and kept-for-round-trip fields don't), and has a
//!   plain sentence with no id, token or expression in it.
//! * A sample of every block, built from its controls, loads into the engine without a rule diagnostic.

use borealis_authoring::blocks::{
    self, block, catalog, fits, misfits, sentence, value_fits, BlockField, BlockSpec, Control,
    SentenceContext, CORE_KINDS, TOGGLE_EFFECT_KINDS,
};
use borealis_authoring::expr_build::formula::{dialect_of, FormulaDialect, FORMULA_FIELDS};
use borealis_content::file::{ContentFile, PackOrigin};
use borealis_content::rules::{GrantRule, Rule, SelectRule, StatRule};
use borealis_content::{ContentStoreBuilder, Element, Ruleset};
use borealis_engine::custom_rules::{check_custom, spec, KNOWN_KINDS};
use borealis_engine::universe::Universe;
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

fn content_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content")
}

fn json_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).expect("content dir").flatten() {
        let p = e.path();
        if p.is_dir() {
            json_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "json") {
            out.push(p);
        }
    }
}

/// Every element (raw JSON) of every bundled pack.
fn bundled_elements() -> &'static [Value] {
    static ALL: std::sync::OnceLock<Vec<Value>> = std::sync::OnceLock::new();
    ALL.get_or_init(|| {
        let mut paths = Vec::new();
        json_files(&content_root(), &mut paths);
        paths.sort();
        let mut out = Vec::new();
        for p in paths {
            let doc: Value =
                serde_json::from_str(&std::fs::read_to_string(&p).expect("read")).expect("json");
            out.extend(doc["elements"].as_array().into_iter().flatten().cloned());
        }
        assert!(out.len() > 3000, "the bundled packs are there");
        out
    })
}

/// Every rule of every bundled element (element rules, multiclass rules, nested lists), with the element's id.
fn bundled_rules() -> Vec<(String, Value)> {
    fn walk(owner: &str, v: &Value, out: &mut Vec<(String, Value)>) {
        match v {
            Value::Object(m) => {
                if let Some(Value::Array(rules)) = m.get("rules") {
                    out.extend(rules.iter().map(|r| (owner.to_string(), r.clone())));
                }
                for (k, x) in m {
                    if k != "rules" {
                        walk(owner, x, out);
                    }
                }
            }
            Value::Array(a) => a.iter().for_each(|x| walk(owner, x, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    for e in bundled_elements() {
        walk(e["id"].as_str().unwrap_or("?"), e, &mut out);
    }
    out
}

/// The bundled elements by id, for sentences.
struct Bundled {
    names: BTreeMap<String, (String, String)>,
    classes: Vec<String>,
}

impl Bundled {
    fn load() -> Self {
        let mut names = BTreeMap::new();
        let mut classes = Vec::new();
        for e in bundled_elements() {
            let (Some(id), Some(name), Some(ty)) =
                (e["id"].as_str(), e["name"].as_str(), e["type"].as_str())
            else {
                continue;
            };
            names.insert(id.to_string(), (name.to_string(), ty.to_string()));
            if ty == "Class" {
                classes.push(name.to_string());
                // The store synthesizes a Multiclass element from the class's `multiclass` node.
                if let Some(mc) = e["multiclass"]["id"].as_str() {
                    names.insert(mc.to_string(), (name.to_string(), "Multiclass".to_string()));
                }
            }
        }
        Self { names, classes }
    }
}

impl SentenceContext for Bundled {
    fn element(&self, id: &str) -> Option<(String, String)> {
        self.names.get(id).cloned()
    }
    fn class_names(&self) -> Vec<String> {
        self.classes.clone()
    }
}

fn custom_blocks() -> Vec<&'static BlockSpec> {
    catalog()
        .iter()
        .filter(|b| !CORE_KINDS.contains(&b.kind) && !b.effect_only())
        .collect()
}

const GATES: [&str; 2] = ["level", "requirements"];

#[test]
fn every_engine_kind_has_a_block_and_every_block_an_engine_kind() {
    let engine: BTreeSet<&str> = KNOWN_KINDS.iter().map(|s| s.kind).collect();
    let catalog_kinds: BTreeSet<&str> = custom_blocks().iter().map(|b| b.kind).collect();
    let missing: Vec<_> = engine.difference(&catalog_kinds).collect();
    let extra: Vec<_> = catalog_kinds.difference(&engine).collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "engine kinds without a block: {missing:?}; blocks without an engine kind: {extra:?}"
    );
    assert_eq!(
        KNOWN_KINDS.len(),
        69,
        "spec 6.2 names 69 kinds; update the spec with the registry"
    );
    assert_eq!(
        custom_blocks().len(),
        KNOWN_KINDS.len(),
        "one block per kind"
    );
    let all: BTreeSet<&str> = catalog().iter().map(|b| b.kind).collect();
    assert_eq!(all.len(), catalog().len(), "block kinds are unique");
    for k in CORE_KINDS {
        assert!(block(k).is_some(), "core block {k}");
    }
}

/// `path` of every field (`max.expr`, `free_cast.uses`, `plus[]`), with the field.
fn field_paths(
    fields: &'static [BlockField],
    prefix: &str,
    out: &mut Vec<(String, &'static BlockField)>,
) {
    for f in fields {
        let path = format!("{prefix}{}", f.key);
        match f.control {
            Control::Group(parts) => field_paths(parts, &format!("{path}."), out),
            Control::Variant(variants) => {
                for v in variants {
                    field_paths(v.fields, &format!("{path}."), out);
                }
            }
            Control::Formulas(_) => out.push((format!("{path}[]"), f)),
            _ => {}
        }
        out.push((path, f));
    }
}

#[test]
fn every_block_offers_exactly_the_fields_the_engine_reads() {
    let mut problems = Vec::new();
    for b in custom_blocks() {
        let s = spec(b.kind).expect("registry entry");
        let offered: BTreeSet<&str> = b.fields.iter().map(|f| f.key).collect();
        let read: BTreeSet<&str> = s
            .fields
            .iter()
            .copied()
            .filter(|f| !GATES.contains(f))
            .collect();
        if offered != read {
            problems.push(format!(
                "{}: offers {:?}, the engine reads {:?}",
                b.kind,
                offered.difference(&read).collect::<Vec<_>>(),
                read.difference(&offered).collect::<Vec<_>>()
            ));
        }
        for f in b.fields {
            if !s.reads(f.key) {
                problems.push(format!("{}.{} is not read by the engine", b.kind, f.key));
            }
            if s.unmodelled.iter().any(|(u, _)| *u == f.key) {
                problems.push(format!(
                    "{}.{} is unmodelled and must not be offered",
                    b.kind, f.key
                ));
            }
        }
        if b.level_gated != s.reads("level") {
            problems.push(format!(
                "{}: From level {} but the engine {}",
                b.kind,
                b.level_gated,
                s.reads("level")
            ));
        }
        if b.only_if != s.reads("requirements") {
            problems.push(format!(
                "{}: Only if {} but the engine {}",
                b.kind,
                b.only_if,
                s.reads("requirements")
            ));
        }
        for r in s.required {
            if !b.field(r).is_some_and(|f| f.required) {
                problems.push(format!(
                    "{}.{r} is required by the engine but not by the dialog",
                    b.kind
                ));
            }
        }
        let mut paths = Vec::new();
        field_paths(b.fields, "", &mut paths);
        for (path, allowed) in s.enums {
            let mut allowed: Vec<&str> = allowed.to_vec();
            allowed.sort_unstable();
            let (key, sub) = match path.strip_suffix("[]") {
                Some(list) => (list, None),
                None => match path.split_once('.') {
                    Some((k, s)) => (k, Some(s)),
                    None => (*path, None),
                },
            };
            let offered: Option<Vec<&str>> = b.field(key).and_then(|f| match (&f.control, sub) {
                (
                    Control::Combo {
                        choices,
                        custom: false,
                    }
                    | Control::Chips {
                        choices,
                        custom: false,
                    },
                    None,
                ) => Some(choices.iter().map(|c| c.value).collect()),
                (Control::Variant(v), Some("type")) => Some(v.iter().map(|x| x.value).collect()),
                _ => None,
            });
            let offered = offered.map(|mut o| {
                o.sort_unstable();
                o
            });
            if offered.as_deref() != Some(allowed.as_slice()) {
                problems.push(format!(
                    "{}.{path}: offers {offered:?}, the engine allows {allowed:?}",
                    b.kind
                ));
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn formula_fields_use_the_dialect_of_their_reader() {
    let mut problems = Vec::new();
    let mut seen = BTreeSet::new();
    for b in custom_blocks() {
        let mut paths = Vec::new();
        field_paths(b.fields, "", &mut paths);
        for (path, f) in paths {
            let d = match f.control {
                Control::Formula(d) => d,
                Control::Formulas(d) if path.ends_with("[]") => d,
                _ => continue,
            };
            seen.insert((b.kind, path.clone()));
            if dialect_of(b.kind, &path) != Some(d) {
                problems.push(format!(
                    "{}.{path}: {d:?}, the reader is {:?}",
                    b.kind,
                    dialect_of(b.kind, &path)
                ));
            }
        }
    }
    for (kind, field, _) in FORMULA_FIELDS.iter().filter(|(k, _, _)| *k != "element") {
        if !seen.contains(&(*kind, field.to_string())) {
            problems.push(format!(
                "formula field {kind}.{field} has no formula control"
            ));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn free_text_the_engine_shows_verbatim_is_labelled_a_note() {
    for b in custom_blocks() {
        let s = spec(b.kind).expect("registry entry");
        if !s.reader.starts_with("notes_d") {
            continue;
        }
        for f in b.fields {
            if matches!(f.control, Control::Text | Control::Multiline)
                && !["name", "id"].contains(&f.key)
            {
                assert!(
                    f.note,
                    "{}.{} is shown verbatim: mark it a note",
                    b.kind, f.key
                );
            }
        }
    }
    for b in catalog() {
        for f in b.fields {
            if f.note {
                assert!(
                    f.help.is_none(),
                    "{}.{}: a note's help is the note label ({})",
                    b.kind,
                    f.key,
                    blocks::NOTE_HELP
                );
            }
        }
    }
}

#[test]
fn toggle_effects_are_the_kinds_the_toggle_readers_read() {
    for k in TOGGLE_EFFECT_KINDS {
        assert!(block(k).is_some(), "effect block {k}");
    }
    assert!(TOGGLE_EFFECT_KINDS.contains(&borealis_engine::speed_toggles::SPEED_MULTIPLY));
    let toggle = block("toggle").expect("toggle");
    assert_eq!(
        toggle.field("effects").map(|f| f.control),
        Some(Control::Blocks(&TOGGLE_EFFECT_KINDS))
    );
    for (_, r) in bundled_rules() {
        if r["kind"] == "toggle" {
            for e in r["effects"].as_array().into_iter().flatten() {
                let k = e["kind"].as_str().unwrap_or("");
                assert!(
                    TOGGLE_EFFECT_KINDS.contains(&k),
                    "bundled toggle effect {k}"
                );
            }
        }
    }
}

/// The typed rule fields the core blocks don't offer, with the reason (spec 3.4: kept as is).
const CORE_NOT_OFFERED: &[(&str, &str, &str)] = &[
    (
        "grant",
        "setters",
        "setters under a grant: kept as is (expert data view)",
    ),
    ("grant", "extra", "unknown XML parts: kept as is"),
    ("select", "items", "rows belong to the written-list block"),
    (
        "select",
        "default_behaviour",
        "kept for round trip; Borealis does not use it",
    ),
    (
        "select",
        "existing",
        "kept for round trip; Borealis does not use it",
    ),
    (
        "select",
        "stat",
        "kept for round trip; Borealis does not use it",
    ),
    ("select", "setters", "setters under a select: kept as is"),
    ("select", "extra", "unknown XML parts: kept as is"),
    ("list", "supports", "a written list offers its own rows"),
    (
        "list",
        "optional",
        "not used by written lists in the bundled packs",
    ),
    (
        "list",
        "default",
        "a written list has no element to suggest",
    ),
    ("list", "spellcasting", "a written list holds no spells"),
    ("list", "prepared", "a written list holds no spells"),
    ("list", "allow_replace", "a written list holds no spells"),
    (
        "list",
        "default_behaviour",
        "kept for round trip; Borealis does not use it",
    ),
    (
        "list",
        "existing",
        "kept for round trip; Borealis does not use it",
    ),
    (
        "list",
        "stat",
        "kept for round trip; Borealis does not use it",
    ),
    ("list", "setters", "setters under a select: kept as is"),
    ("list", "extra", "unknown XML parts: kept as is"),
    ("stat", "not_equipped", "parsed, unused by the engine"),
    ("stat", "minimum", "parsed, unused by the engine"),
    ("stat", "merge", "debugging only"),
    ("stat", "setters", "setters under a stat: kept as is"),
    ("stat", "extra", "unknown XML parts: kept as is"),
];

/// The serde names of every field of the typed rules. Destructured without `..`: a new field fails to compile here.
fn typed_fields() -> [(&'static str, Vec<&'static str>); 3] {
    let GrantRule {
        id: _,
        ty: _,
        level: _,
        requirements: _,
        spellcasting: _,
        prepared: _,
        setters: _,
        extra: _,
    } = GrantRule::default();
    let SelectRule {
        ty: _,
        name: _,
        number: _,
        level: _,
        requirements: _,
        supports: _,
        optional: _,
        default_id: _,
        spellcasting: _,
        prepared: _,
        allow_replace: _,
        default_behaviour: _,
        existing: _,
        stat: _,
        items: _,
        setters: _,
        extra: _,
    } = SelectRule::default();
    let StatRule {
        name: _,
        value: _,
        bonus: _,
        level: _,
        requirements: _,
        equipped: _,
        not_equipped: _,
        inline: _,
        maximum: _,
        minimum: _,
        alt: _,
        merge: _,
        setters: _,
        extra: _,
    } = StatRule::default();
    [
        (
            "grant",
            vec![
                "id",
                "type",
                "level",
                "requirements",
                "spellcasting",
                "prepared",
                "setters",
                "extra",
            ],
        ),
        (
            "select",
            vec![
                "type",
                "name",
                "number",
                "level",
                "requirements",
                "supports",
                "optional",
                "default",
                "spellcasting",
                "prepared",
                "allow_replace",
                "default_behaviour",
                "existing",
                "stat",
                "items",
                "setters",
                "extra",
            ],
        ),
        (
            "stat",
            vec![
                "name",
                "value",
                "bonus",
                "level",
                "requirements",
                "equipped",
                "not_equipped",
                "inline",
                "maximum",
                "minimum",
                "alt",
                "merge",
                "setters",
                "extra",
            ],
        ),
    ]
}

/// Every `Rule` variant has a block or is kept as is. A new variant fails to compile here.
fn variant_block(rule: &Rule) -> Option<&'static str> {
    match rule {
        Rule::Grant(_) => Some("grant"),
        Rule::Select(s) if s.ty == "List" => Some("list"),
        Rule::Select(_) => Some("select"),
        Rule::Stat(_) => Some("stat"),
        Rule::Custom(c) => block(&c.kind).map(|b| b.kind),
        Rule::Unknown(_) => None,
    }
}

#[test]
fn core_blocks_cover_every_typed_rule_field() {
    let mut problems = Vec::new();
    for (rule, fields) in typed_fields() {
        let kinds: &[&str] = if rule == "select" {
            &["select", "list"]
        } else {
            &[rule]
        };
        for kind in kinds {
            let b = block(kind).expect("core block");
            assert!(
                b.level_gated && b.only_if,
                "{kind} takes From level and Only if"
            );
            let offered: BTreeSet<&str> = b.fields.iter().map(|f| f.key).collect();
            let kept: BTreeSet<&str> = CORE_NOT_OFFERED
                .iter()
                .filter(|(k, _, _)| k == kind)
                .map(|(_, f, _)| *f)
                .collect();
            for f in &fields {
                let n = usize::from(offered.contains(f))
                    + usize::from(kept.contains(f))
                    + usize::from(GATES.contains(f));
                if n != 1 {
                    problems.push(format!(
                        "{kind}.{f}: offered, kept or a gate exactly once (found {n})"
                    ));
                }
            }
            for f in offered.iter().chain(&kept) {
                if !fields.contains(f) {
                    problems.push(format!("{kind}.{f} is no field of the typed rule"));
                }
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
    let list = Rule::Select(SelectRule {
        ty: "List".into(),
        ..SelectRule::default()
    });
    assert_eq!(variant_block(&list), blocks::kind_of(&list));
    assert_eq!(variant_block(&Rule::Unknown(Default::default())), None);
}

/// The fields of a bundled rule its block may leave to the preserved bucket: unmodelled fields and the core fields
/// kept for round trip.
fn may_be_kept(kind: &str, field: &str) -> bool {
    spec(kind).is_some_and(|s| s.unmodelled.iter().any(|(f, _)| *f == field))
        || CORE_NOT_OFFERED
            .iter()
            .any(|(k, f, _)| *k == kind && *f == field)
}

#[test]
fn every_bundled_rule_fits_its_block() {
    let mut problems = Vec::new();
    let mut per_kind: BTreeMap<&str, usize> = BTreeMap::new();
    let mut kept_fields: BTreeMap<String, usize> = BTreeMap::new();
    for (owner, raw) in bundled_rules() {
        let rule: Rule =
            serde_json::from_value(raw.clone()).unwrap_or_else(|e| panic!("{owner}: {e}"));
        let Some(kind) = variant_block(&rule) else {
            assert!(
                matches!(rule, Rule::Unknown(_)),
                "{owner}: a custom kind without a block: {raw}"
            );
            continue;
        };
        *per_kind.entry(kind).or_default() += 1;
        for f in fits(&rule).expect("a block") {
            if may_be_kept(kind, &f) {
                *kept_fields.entry(format!("{kind}.{f}")).or_default() += 1;
            } else {
                problems.push(format!("{owner}: {kind}.{f} = {} does not fit", raw[&f]));
            }
        }
    }
    println!("bundled rules per block: {per_kind:?}");
    println!("kept as is (unmodelled / round trip): {kept_fields:?}");
    assert!(
        problems.is_empty(),
        "{} misfits:\n{}",
        problems.len(),
        problems.join("\n")
    );
}

#[test]
fn every_bundled_rule_reads_as_a_plain_sentence() {
    let cx = Bundled::load();
    let mut problems = Vec::new();
    let mut samples: BTreeMap<String, String> = BTreeMap::new();
    let mut count = 0;
    for (owner, raw) in bundled_rules() {
        let rule: Rule = serde_json::from_value(raw.clone()).expect("rule");
        let s = sentence(&rule, &cx);
        count += 1;
        let bad = [
            "ID_",
            "_",
            "$(",
            "{",
            "}",
            "[",
            "]",
            "a missing element",
            "kept as is",
        ];
        if s.trim().is_empty() || bad.iter().any(|b| s.contains(b)) {
            problems.push(format!("{owner}: {raw} -> {s:?}"));
        }
        let kind = raw["kind"].as_str().unwrap_or("?").to_string();
        samples.entry(kind).or_insert(s);
    }
    for (k, s) in &samples {
        println!("{k}: {s}");
    }
    println!("{count} sentences");
    assert!(
        problems.is_empty(),
        "{} bad sentences:\n{}",
        problems.len(),
        problems.join("\n")
    );
}

/// A value for a control that the engine accepts.
fn sample(f: &BlockField, kind: &str, path: &str) -> Value {
    let choice = |c: &[blocks::Choice]| c.first().map_or("sample", |c| c.value);
    match f.control {
        Control::Auto
        | Control::Text
        | Control::Multiline
        | Control::Name {
            multiple: false, ..
        } => json!("Sample"),
        Control::Dice => json!("1d6"),
        Control::ClassCounter => json!("sample_column"),
        Control::Number { min, max } | Control::NumberOr { min, max, .. } => {
            json!(min.max(1).min(max))
        }
        Control::Numbers { min, .. } => json!([min.max(1)]),
        Control::Check => json!(true),
        Control::Combo { choices, .. } => json!(choice(choices)),
        Control::Chips { choices, .. } => json!([choice(choices)]),
        Control::Reference { .. } => json!("ID_SAMPLE_ELEMENT"),
        Control::References { .. } | Control::Name { multiple: true, .. } => {
            json!(["ID_SAMPLE_ELEMENT"])
        }
        Control::Statistic => json!("speed"),
        Control::StatValue => json!("2"),
        Control::NumberOrStatistic => json!(2),
        Control::Condition => json!("[level:2]"),
        Control::TagFilter => json!("Sample"),
        Control::Equipped => json!("[armor:none]"),
        Control::Formula(d) | Control::Formulas(d) => {
            let text = match d {
                FormulaDialect::SaveDc | FormulaDialect::NoteDc | FormulaDialect::BreathDc => {
                    "8 + proficiency"
                }
                FormulaDialect::ValueOrClassLevel => "dexterity:modifier",
                FormulaDialect::Formula | FormulaDialect::Value => "proficiency",
            };
            if matches!(f.control, Control::Formulas(_)) {
                json!([text])
            } else {
                json!(text)
            }
        }
        Control::Uses => json!(1),
        Control::AbilitySource => json!("wisdom"),
        Control::AbilityChoice => json!(1),
        Control::Splits => json!([[2, 1], [1, 1, 1]]),
        Control::Group(parts) => object(parts, kind, path),
        Control::Rows(parts) => json!([object(parts, kind, path)]),
        Control::Variant(v) => {
            let mut m = object(v[0].fields, kind, path);
            m["type"] = json!(v[0].value);
            m
        }
        Control::Map([key, value]) => {
            let k = match key.control {
                Control::Number { min, .. } => min.max(1).to_string(),
                Control::Combo { choices, .. } => choice(choices).to_string(),
                _ => "Sample".to_string(),
            };
            json!({ k: sample(value, kind, path) })
        }
        Control::Blocks(_) => json!([{"kind": "stat", "name": "speed", "value": "5"}]),
    }
}

fn object(parts: &[BlockField], kind: &str, path: &str) -> Value {
    let m: Map<String, Value> = parts
        .iter()
        .map(|p| {
            (
                p.key.to_string(),
                sample(p, kind, &format!("{path}.{}", p.key)),
            )
        })
        .collect();
    Value::Object(m)
}

fn sample_rule(b: &BlockSpec) -> Value {
    let mut m = Map::new();
    m.insert("kind".into(), json!(b.kind));
    for f in b.fields {
        m.insert(f.key.into(), sample(f, b.kind, f.key));
    }
    if b.level_gated {
        m.insert("level".into(), json!(2));
    }
    if b.only_if {
        m.insert("requirements".into(), json!("[level:2]"));
    }
    if b.kind == "list" {
        m.insert("kind".into(), json!("select"));
        m.insert("type".into(), json!("List"));
        m.insert("items".into(), json!([{"id": 1, "text": "Sample"}]));
    }
    if b.kind == "grant" || b.kind == "select" {
        m.insert("type".into(), json!("Language"));
    }
    Value::Object(m)
}

#[test]
fn a_sample_of_every_block_fits_and_loads_without_rule_diagnostics() {
    let mut rules = Vec::new();
    for b in catalog().iter().filter(|b| !b.effect_only()) {
        let raw = sample_rule(b);
        let rule: Rule = serde_json::from_value(raw.clone())
            .unwrap_or_else(|e| panic!("{}: {e} in {raw}", b.kind));
        let m = raw.as_object().expect("object");
        let mut fields = m.clone();
        fields.remove("kind");
        assert_eq!(
            misfits(b, &fields),
            Vec::<String>::new(),
            "{}: {raw}",
            b.kind
        );
        if let Rule::Custom(c) = &rule {
            assert!(
                check_custom(c).is_empty(),
                "{}: {:?}",
                b.kind,
                check_custom(c)
            );
            let s = spec(&c.kind).expect("spec");
            assert!(
                s.shape_problems(&c.fields).is_empty(),
                "{}: {:?}",
                b.kind,
                s.shape_problems(&c.fields)
            );
        }
        rules.push(rule);
    }
    for b in catalog().iter().filter(|b| b.effect_only()) {
        let mut effect = sample_rule(b).as_object().expect("object").clone();
        effect.remove("kind");
        assert!(misfits(b, &effect).is_empty(), "{}", b.kind);
        let toggle = block("toggle")
            .expect("toggle")
            .field("effects")
            .expect("effects");
        let mut e = Value::Object(effect);
        e["kind"] = json!(b.kind);
        assert!(
            value_fits(&toggle.control, &json!([e])),
            "{} inside a toggle",
            b.kind
        );
    }
    let el: Element = serde_json::from_value(json!({
        "id": "ID_SAMPLE_FEAT",
        "type": "Feat",
        "name": "Sample",
        "source": "Sample",
    }))
    .expect("element");
    let el = Element { rules, ..el };
    let core = content_root().join("core");
    let store = ContentStoreBuilder::new()
        .add_pack_dir(&core)
        .expect("core")
        .add_file(
            "sample.json",
            PackOrigin::Custom,
            ContentFile::new(vec![el]),
        )
        .build();
    for ruleset in [Ruleset::Dnd2014, Ruleset::Dnd2024] {
        let uni = Universe::new(&store, ruleset);
        let loud: Vec<_> = uni
            .diagnostics
            .iter()
            .filter(|d| d.code.starts_with("content.unknown_rule"))
            .map(|d| d.message.clone())
            .collect();
        assert!(loud.is_empty(), "{}", loud.join("\n"));
    }
}

#[test]
fn every_dialog_field_has_a_label_and_no_token_in_it() {
    fn check(kind: &str, fields: &[BlockField]) {
        for f in fields {
            assert!(
                !f.label.is_empty() && !f.label.contains('_'),
                "{kind}.{}",
                f.key
            );
            for p in f.control.parts() {
                check(kind, std::slice::from_ref(p));
            }
            if let Control::Variant(v) = f.control {
                for x in v {
                    assert!(!x.label.contains('_'));
                    check(kind, x.fields);
                }
            }
        }
    }
    for b in catalog() {
        assert!(!b.label.is_empty() && !b.label.contains('_'), "{}", b.kind);
        check(b.kind, b.fields);
    }
}
