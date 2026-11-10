//! Spec 18.2 (identity) for WB-04: every element of every bundled pack (`app/content`, all seven packs) goes through
//! its creator form and comes back byte for byte:
//!
//! * `form_to_elements(element_to_form(e)) == [e]`, compared as values and as serialized JSON;
//! * the same after the wire round trip (the form the UI receives, serialized, read back and merged with the
//!   backend's copy) and after the draft-file round trip (the full form serialized and read back);
//! * every class with its granted class features as children, and every feat, background and subclass with theirs;
//! * canonical writers: with the stored bytes forgotten (as if every field had been edited), each form still writes
//!   an element, and reading that element gives the same field values (a fixed point).
//!
//! Exceptions would be listed with their reason in `EXCEPTIONS` (T6: an asserted, reviewed list); it is empty. The
//! counts are printed (`--nocapture`) and written to `target/form_identity.txt`.

use borealis_authoring::expr_build::condition::ConditionContext;
use borealis_authoring::form::{
    element_group_to_form, element_to_form, form_to_element, form_to_elements, kind_for_type,
    merge_ui, to_ui, FormContext, FormState, RawBlockCodec, RefInfo,
};
use borealis_content::{ContentFile, Element, PackFileKind, PackManifest, Rule};
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

/// The form with its stored bytes forgotten (the id kept): every field writes canonically.
fn forgetful(form: &FormState) -> FormState {
    let mut f = form.clone();
    f.memo.fields.clear();
    f
}

#[test]
fn every_bundled_element_round_trips_through_its_form() {
    let c = corpus();
    let codec = RawBlockCodec;
    let cx = FormContext {
        conditions: ConditionContext::new(&c.classes),
        lookup: &c.lookup,
        blocks: &codec,
    };
    let mut failures = Vec::new();
    let mut by_kind: BTreeMap<&str, (usize, usize, usize)> = BTreeMap::new();
    let mut by_pack: BTreeMap<&str, usize> = BTreeMap::new();
    let mut kept_labels: BTreeMap<String, usize> = BTreeMap::new();
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
                let again = element_to_form(&canonical, &cx);
                if again.fields != form.fields {
                    failures.push(format!(
                        "{pack} {}: canonical text reads differently\n  was {:?}\n  now {:?}",
                        e.id, form.fields, again.fields
                    ));
                }
            }
            Err(p) => failures.push(format!("{pack} {}: canonical write fails {p:?}", e.id)),
        }
        let entry = by_kind.entry(kind_for_type(&e.ty).id).or_default();
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

/// Children: each element's granted elements of its child types, in rule order.
fn child_types(ty: &str) -> &'static [&'static str] {
    match ty {
        "Class" => &["Class Feature"],
        "Archetype" => &["Archetype Feature"],
        "Feat" => &["Feat Feature"],
        "Background" => &["Background Feature"],
        "Race" | "Sub Race" | "Species" => &["Racial Trait"],
        _ => &[],
    }
}

#[test]
fn parents_round_trip_with_their_children() {
    let c = corpus();
    let codec = RawBlockCodec;
    let cx = FormContext {
        conditions: ConditionContext::new(&c.classes),
        lookup: &c.lookup,
        blocks: &codec,
    };
    let by_id: HashMap<&str, &Element> =
        c.elements.iter().map(|(_, e)| (e.id.as_str(), e)).collect();
    let mut groups = 0;
    let mut children = 0;
    for (_, parent) in &c.elements {
        let types = child_types(&parent.ty);
        let mut kids: Vec<&Element> = Vec::new();
        for rule in &parent.rules {
            if let Rule::Grant(g) = rule {
                if let Some(child) = by_id
                    .get(g.id.as_str())
                    .filter(|e| types.contains(&e.ty.as_str()))
                {
                    if !kids.iter().any(|k| k.id == child.id) {
                        kids.push(child);
                    }
                }
            }
        }
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
