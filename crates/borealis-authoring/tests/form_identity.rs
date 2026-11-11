//! Spec 18.2 (identity) for WB-04: every element of every bundled pack (`app/content`, all seven packs) goes through
//! its creator form and comes back byte for byte:
//!
//! * `form_to_elements(element_to_form(e)) == [e]`, compared as values and as serialized JSON;
//! * the same after the wire round trip (the form the UI receives, serialized, read back and merged with the
//!   backend's copy) and after the draft-file round trip (the full form serialized and read back);
//! * every class with its granted class features as children, and every feat, background and subclass with theirs;
//! * every rule is a block of the block catalog (`CatalogBlockCodec`) with a plain sentence; what a block doesn't
//!   edit is in its preserved bucket;
//! * canonical writers: with all stored text forgotten (the fields' and blocks' memos, and the original text of every
//!   builder value, as if every field and block had been edited), each form still writes an element, and reading
//!   that element gives the same field and block values (a fixed point).
//!
//! Exceptions would be listed with their reason in `EXCEPTIONS` (T6: an asserted, reviewed list); it is empty. The
//! counts are printed (`--nocapture`) and written to `target/form_identity.txt`.

use borealis_authoring::expr_build::condition::ConditionContext;
use borealis_authoring::form::{
    element_group_to_form, element_to_form, form_to_element, form_to_elements, kind_for_element,
    merge_ui, to_ui, CatalogBlockCodec, FormContext, FormState, RefInfo,
};
use borealis_authoring::pack::granted_children;
use borealis_content::{ContentFile, Element, PackFileKind, PackManifest};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

/// Elements that cannot round trip, with the reason. Reviewed: none.
const EXCEPTIONS: &[(&str, &str)] = &[];

fn content_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content")
}

/// (pack, element) for every element of every bundled pack, in pack and file order.
fn bundled() -> Vec<(String, Element)> {
    let mut dirs: Vec<PathBuf> = fs::read_dir(content_dir())
        .expect("content dir")
        .map(|e| e.expect("entry").path())
        .filter(|p| p.join("pack.json").exists())
        .collect();
    dirs.sort();
    let mut out = Vec::new();
    for dir in dirs {
        let pack = dir
            .file_name()
            .expect("name")
            .to_string_lossy()
            .into_owned();
        let manifest = PackManifest::from_json_str(
            &fs::read_to_string(dir.join("pack.json")).expect("pack.json"),
        )
        .expect("manifest");
        for file in manifest
            .files
            .iter()
            .filter(|f| f.kind == PackFileKind::Content)
        {
            let text = fs::read_to_string(dir.join(&file.path)).expect("file");
            let content =
                ContentFile::from_json_str(&text).unwrap_or_else(|e| panic!("{}: {e}", file.path));
            assert!(
                content.appends.is_empty(),
                "bundled appends are covered by unit tests only"
            );
            out.extend(content.elements.into_iter().map(|e| (pack.clone(), e)));
        }
    }
    assert!(
        out.len() >= 4000,
        "the bundled packs are there ({} elements)",
        out.len()
    );
    out
}

struct Corpus {
    elements: Vec<(String, Element)>,
    lookup: HashMap<String, RefInfo>,
    classes: Vec<String>,
}

fn corpus() -> Corpus {
    let elements = bundled();
    let lookup = elements
        .iter()
        .map(|(_, e)| {
            (
                e.id.clone(),
                RefInfo {
                    name: e.name.clone(),
                    ty: e.ty.clone(),
                },
            )
        })
        .collect();
    let classes = elements
        .iter()
        .filter(|(_, e)| e.ty == "Class")
        .map(|(_, e)| e.name.clone())
        .collect();
    Corpus {
        elements,
        lookup,
        classes,
    }
}

fn json(e: &[Element]) -> String {
    serde_json::to_string(e).expect("json")
}

/// The wire trip: what the UI receives, as JSON and back, merged with the backend's copy.
fn through_ui(form: &FormState) -> FormState {
    let wire = serde_json::to_string(&to_ui(form)).expect("ui json");
    let back: FormState = serde_json::from_str(&wire).expect("ui form reads");
    merge_ui(form, &back)
}

/// The draft-file trip.
fn through_file(form: &FormState) -> FormState {
    serde_json::from_str(&serde_json::to_string(form).expect("json")).expect("draft form reads")
}

fn check_identity(
    form: &FormState,
    want: &[Element],
    cx: &FormContext<'_>,
    what: &str,
) -> Result<(), String> {
    let back = form_to_elements(form, cx).map_err(|p| format!("{what}: problems {p:?}"))?;
    if back != want || json(&back) != json(want) {
        return Err(format!(
            "{what}: differs\n  want {}\n  got  {}",
            json(want),
            json(&back)
        ));
    }
    Ok(())
}

/// A builder value (`{value: {shape: ...}, original?}`) without the text it was read from, anywhere in `v`.
fn without_originals(v: &Value) -> Value {
    match v {
        Value::Object(m) => {
            let is_expr = m
                .get("value")
                .and_then(Value::as_object)
                .is_some_and(|x| x.contains_key("shape"));
            Value::Object(
                m.iter()
                    .filter(|(k, _)| !(is_expr && k.as_str() == "original"))
                    .map(|(k, x)| (k.clone(), without_originals(x)))
                    .collect(),
            )
        }
        Value::Array(items) => Value::Array(items.iter().map(without_originals).collect()),
        other => other.clone(),
    }
}

fn fields_without_originals(fields: &BTreeMap<String, Value>) -> BTreeMap<String, Value> {
    fields
        .iter()
        .map(|(k, v)| (k.clone(), without_originals(v)))
        .collect()
}

/// The form with all stored text forgotten (the id kept): every field and block writes canonically.
fn forgetful(form: &FormState) -> FormState {
    let mut f = form.clone();
    f.memo.fields.clear();
    f.fields = fields_without_originals(&f.fields);
    for b in &mut f.blocks {
        b.memo = None;
        b.fields = fields_without_originals(&b.fields);
    }
    f
}

/// Where `a` and `b` differ in JSON shape (object keys, list lengths, value types; strings may differ in spelling):
/// a canonical writer must not turn a number the engine reads into text.
fn shape_difference(a: &Value, b: &Value, path: &str) -> Option<String> {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            if x.keys().ne(y.keys()) {
                return Some(format!("{path}: keys {:?} vs {:?}", x.keys(), y.keys()));
            }
            x.iter()
                .find_map(|(k, v)| shape_difference(v, &y[k], &format!("{path}.{k}")))
        }
        (Value::Array(x), Value::Array(y)) if x.len() == y.len() => x
            .iter()
            .zip(y)
            .enumerate()
            .find_map(|(i, (v, w))| shape_difference(v, w, &format!("{path}[{i}]"))),
        (Value::String(_), Value::String(_)) | (Value::Bool(_), Value::Bool(_)) => None,
        (Value::Number(x), Value::Number(y)) if x.is_i64() == y.is_i64() => None,
        (Value::Null, Value::Null) => None,
        _ => Some(format!("{path}: {a} vs {b}")),
    }
}

/// The shape of a value (keys and value types, strings' spelling aside), for comparing rules in any order.
fn shape_of(v: &Value) -> String {
    match v {
        Value::Object(m) => format!(
            "{{{}}}",
            m.iter()
                .map(|(k, x)| format!("{k}:{}", shape_of(x)))
                .collect::<Vec<_>>()
                .join(",")
        ),
        Value::Array(items) => format!(
            "[{}]",
            items.iter().map(shape_of).collect::<Vec<_>>().join(",")
        ),
        Value::String(_) => "s".into(),
        Value::Bool(_) => "b".into(),
        Value::Number(n) if n.is_i64() => "i".into(),
        Value::Number(_) => "f".into(),
        Value::Null => "n".into(),
    }
}

/// Where the rules of a canonical write differ in shape from the stored ones. A canonical write may put rules in
/// another order (rules that fields hold before the blocks, a block's copies at other levels after it, WB-16), so
/// the rules are compared rule by rule, and else as a set of shapes.
fn rules_shape_difference(a: &Value, b: &Value) -> Option<String> {
    let d = shape_difference(a, b, "rules")?;
    let set = |v: &Value| {
        let mut s: Vec<String> = v.as_array().into_iter().flatten().map(shape_of).collect();
        s.sort();
        s
    };
    (set(a) != set(b)).then_some(d)
}

/// Count the builder values (`{value: {shape}}`) in `v` by shape (`model`, `kept`).
fn count_builders(v: &Value, out: &mut BTreeMap<String, usize>) {
    match v {
        Value::Object(m) => {
            if let Some(shape) = m
                .get("value")
                .and_then(|x| x.get("shape"))
                .and_then(Value::as_str)
            {
                *out.entry(shape.to_string()).or_default() += 1;
                return;
            }
            m.values().for_each(|x| count_builders(x, out));
        }
        Value::Array(items) => items.iter().for_each(|x| count_builders(x, out)),
        _ => {}
    }
}

/// What a form's values are, stored text aside: fields, and each block's kind, values and kept parts.
fn values(form: &FormState) -> Value {
    let blocks: Vec<Value> = form
        .blocks
        .iter()
        .map(|b| {
            let kept: Vec<&str> = b.preserved.iter().map(|p| p.key.as_str()).collect();
            serde_json::json!({
                "kind": b.kind,
                "fields": fields_without_originals(&b.fields),
                "kept": kept,
            })
        })
        .collect();
    serde_json::json!({ "fields": fields_without_originals(&form.fields), "blocks": blocks })
}

#[test]
fn every_bundled_element_round_trips_through_its_form() {
    let c = corpus();
    let codec = CatalogBlockCodec;
    let cx = FormContext {
        conditions: ConditionContext::new(&c.classes),
        lookup: &c.lookup,
        blocks: &codec,
    };
    let mut failures = Vec::new();
    let mut by_kind: BTreeMap<&str, (usize, usize, usize)> = BTreeMap::new();
    let mut by_pack: BTreeMap<&str, usize> = BTreeMap::new();
    let mut kept_labels: BTreeMap<String, usize> = BTreeMap::new();
    let mut block_kinds: BTreeMap<String, usize> = BTreeMap::new();
    let mut builders: BTreeMap<String, usize> = BTreeMap::new();
    for (pack, e) in &c.elements {
        if EXCEPTIONS.iter().any(|(id, _)| *id == e.id) {
            continue;
        }
        let want = std::slice::from_ref(e);
        let form = element_to_form(e, &cx);
        for (what, f) in [
            ("identity", form.clone()),
            ("wire", through_ui(&form)),
            ("draft file", through_file(&form)),
        ] {
            if let Err(m) = check_identity(&f, want, &cx, what) {
                failures.push(format!("{pack} {} ({}): {m}", e.id, e.ty));
            }
        }
        match form_to_element(&forgetful(&form), &cx) {
            Ok(canonical) => {
                let rules = |x: &Element| serde_json::to_value(&x.rules).expect("rules");
                if let Some(d) = rules_shape_difference(&rules(e), &rules(&canonical)) {
                    failures.push(format!(
                        "{pack} {}: canonical rules change shape: {d}",
                        e.id
                    ));
                }
                let again = element_to_form(&canonical, &cx);
                if values(&again) != values(&form) {
                    failures.push(format!(
                        "{pack} {}: canonical text reads differently\n  was {}\n  now {}",
                        e.id,
                        values(&form),
                        values(&again)
                    ));
                }
            }
            Err(p) => failures.push(format!("{pack} {}: canonical write fails {p:?}", e.id)),
        }
        for b in &form.blocks {
            if b.sentence.trim().is_empty() || b.sentence.contains("ID_") {
                failures.push(format!(
                    "{pack} {}: block {} has no plain sentence: {:?}",
                    e.id, b.key, b.sentence
                ));
            }
            *block_kinds.entry(b.kind.clone()).or_default() += 1;
            for v in b.fields.values() {
                count_builders(v, &mut builders);
            }
        }
        let entry = by_kind.entry(kind_for_element(e).id).or_default();
        entry.0 += 1;
        entry.1 += form.blocks.len();
        entry.2 +=
            form.preserved.len() + form.blocks.iter().map(|b| b.preserved.len()).sum::<usize>();
        *by_pack.entry(pack).or_default() += 1;
        for p in form
            .preserved
            .iter()
            .chain(form.blocks.iter().flat_map(|b| b.preserved.iter()))
        {
            let label = p.kept.as_ref().map_or("?", |k| k.location.as_str());
            let label = if form.preserved.iter().any(|q| std::ptr::eq(q, p)) {
                label.to_string()
            } else {
                format!("block {label}")
            };
            let label = label.as_str();
            let label = label.split_once('.').map_or(label, |(head, _)| head);
            *kept_labels.entry(label.to_string()).or_default() += 1;
        }
    }
    let mut report = String::new();
    let total: usize = by_pack.values().sum();
    let _ = writeln!(
        report,
        "form identity: {total} elements, {} failures, {} exceptions",
        failures.len(),
        EXCEPTIONS.len()
    );
    for (pack, n) in &by_pack {
        let _ = writeln!(report, "  pack {pack}: {n}");
    }
    for (kind, (n, blocks, kept)) in &by_kind {
        let _ = writeln!(
            report,
            "  kind {kind}: {n} elements, {blocks} blocks, {kept} preserved parts"
        );
    }
    let _ = writeln!(
        report,
        "  blocks: {} in {} block kinds",
        block_kinds.values().sum::<usize>(),
        block_kinds.len()
    );
    let _ = writeln!(report, "  builder values in blocks by shape: {builders:?}");
    for (kind, n) in &block_kinds {
        let _ = writeln!(report, "  block {kind}: {n}");
    }
    for (loc, n) in &kept_labels {
        let _ = writeln!(report, "  preserved {loc}: {n}");
    }
    println!("{report}");
    let _ = fs::write(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/form_identity.txt"),
        &report,
    );
    assert!(
        failures.is_empty(),
        "{} failures, first 10:\n{}",
        failures.len(),
        failures
            .iter()
            .take(10)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn parents_round_trip_with_their_children() {
    let c = corpus();
    let codec = CatalogBlockCodec;
    let cx = FormContext {
        conditions: ConditionContext::new(&c.classes),
        lookup: &c.lookup,
        blocks: &codec,
    };
    let mut groups = 0;
    let mut children = 0;
    let all: Vec<Element> = c.elements.iter().map(|(_, e)| e.clone()).collect();
    for (_, parent) in &c.elements {
        let kids: Vec<&Element> = granted_children(parent, &all);
        if kids.is_empty() {
            continue;
        }
        let form = element_group_to_form(parent, &kids, &cx);
        let want: Vec<Element> = std::iter::once(parent.clone())
            .chain(kids.iter().map(|k| (*k).clone()))
            .collect();
        check_identity(&form, &want, &cx, "group").unwrap_or_else(|m| panic!("{}: {m}", parent.id));
        check_identity(&through_ui(&form), &want, &cx, "group wire")
            .unwrap_or_else(|m| panic!("{}: {m}", parent.id));
        groups += 1;
        children += kids.len();
    }
    println!("groups: {groups} parents, {children} children");
    assert!(
        groups >= 40 && children >= 400,
        "{groups} groups, {children} children"
    );
}
