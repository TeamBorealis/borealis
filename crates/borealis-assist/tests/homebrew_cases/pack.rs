//! Load a case's `imported/` pack on top of the bundled packs and run the pack-level and
//! element-level expectations.

use std::collections::HashSet;
use std::path::Path;

use borealis_content::file::{ContentFile, PackFileKind, PackManifest};
use borealis_content::{
    rule_kinds, ContentStore, ContentStoreBuilder, Element, IssueCode, Rule, Severity,
};
use serde_json::Value;

use crate::schema::{
    CaseFile, ElementExpect, Head, ImportReport, Level, Mech, MentionExpect, REPORT_FORMAT,
    REPORT_OUTCOMES,
};
use crate::{Row, IMPORTED_DIR};

/// The imported pack, loaded.
pub struct Loaded {
    pub store: ContentStore,
    /// Elements of the imported pack, in file order.
    pub elements: Vec<Element>,
    pub appends: Vec<borealis_content::Append>,
    pub manifest: PackManifest,
    pub report: Option<ImportReport>,
    pub report_error: Option<String>,
    pub provenance: Option<Value>,
}

fn read(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))
}

/// Load the bundled packs plus `case_dir/imported`.
pub fn load(content_root: &Path, case: &CaseFile, case_dir: &Path) -> Result<Loaded, String> {
    let dir = case_dir.join(IMPORTED_DIR);
    if !dir.join("pack.json").is_file() {
        return Err(format!(
            "no imported pack: {} has no pack.json. Run the import and write its pack there \
             (specs/homebrew-cases/README.md)",
            dir.display()
        ));
    }
    let manifest = PackManifest::from_json_str(&read(&dir.join("pack.json"))?)
        .map_err(|e| format!("pack.json: {e}"))?;
    let mut elements = Vec::new();
    let mut appends = Vec::new();
    for f in &manifest.files {
        if f.kind != PackFileKind::Content {
            continue;
        }
        let file = ContentFile::from_json_str(&read(&dir.join(&f.path))?)
            .map_err(|e| format!("{}: {e}", f.path))?;
        elements.extend(file.elements);
        appends.extend(file.appends);
    }
    let mut b = ContentStoreBuilder::new();
    for p in &case.base_packs {
        b = b
            .add_pack_dir(&content_root.join(p))
            .map_err(|e| format!("bundled pack {p}: {e}"))?;
    }
    let store = b
        .add_pack_dir(&dir)
        .map_err(|e| format!("imported pack does not load: {e}"))?
        .build();
    let (report, report_error) = match std::fs::read_to_string(dir.join("report.json")) {
        Err(_) => (None, None),
        Ok(t) => match serde_json::from_str::<ImportReport>(&t) {
            Ok(r) => (Some(r), None),
            Err(e) => (None, Some(format!("report.json: {e}"))),
        },
    };
    let provenance = std::fs::read_to_string(dir.join("import.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok());
    Ok(Loaded {
        store,
        elements,
        appends,
        manifest,
        report,
        report_error,
        provenance,
    })
}

/// Rebuild the store from `loaded.elements` (after a test edited them in memory).
pub fn reload_with(content_root: &Path, case: &CaseFile, loaded: Loaded) -> Result<Loaded, String> {
    let doc = serde_json::json!({
        "format": borealis_content::file::ContentFormat::ID,
        "ruleset": loaded.manifest.rulesets.first(),
        "elements": loaded.elements,
        "appends": loaded.appends,
    });
    let file = ContentFile::from_json_str(&doc.to_string()).map_err(|e| e.to_string())?;
    let mut b = ContentStoreBuilder::new();
    for p in &case.base_packs {
        b = b
            .add_pack_dir(&content_root.join(p))
            .map_err(|e| format!("bundled pack {p}: {e}"))?;
    }
    let store = b
        .add_file(
            format!("{}/content.json", loaded.manifest.id),
            loaded.manifest.origin,
            file,
        )
        .build();
    Ok(Loaded { store, ..loaded })
}

/// Lowercase, straight quotes, single spaces.
pub fn norm(s: &str) -> String {
    let s: String = s
        .chars()
        .map(|c| match c {
            '\u{2018}' | '\u{2019}' | '`' => '\'',
            '\u{201c}' | '\u{201d}' => '"',
            '\u{2013}' | '\u{2014}' => '-',
            c => c.to_ascii_lowercase(),
        })
        .collect();
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Does `actual` match one of the `A|B` alternatives?
pub fn name_matches(alternatives: &str, actual: &str) -> bool {
    let a = norm(actual);
    alternatives.split('|').any(|alt| norm(alt) == a)
}

/// Is `expected` a JSON subset of `actual`?
pub fn subset(expected: &Value, actual: &Value) -> bool {
    match (expected, actual) {
        (Value::Object(e), Value::Object(a)) => e
            .iter()
            .all(|(k, v)| a.get(k).is_some_and(|av| subset(v, av))),
        (Value::Array(e), Value::Array(a)) => e.iter().all(|v| a.iter().any(|av| subset(v, av))),
        (Value::String(e), Value::String(a)) => e == a,
        (Value::Number(e), Value::Number(a)) => e.as_f64() == a.as_f64(),
        _ => expected == actual,
    }
}

/// All text an element shows: name, description, sheet text, setter values.
pub fn element_text(e: &Element) -> String {
    let mut t = vec![
        e.name.clone(),
        e.description.clone(),
        e.prerequisite.clone(),
    ];
    if let Some(s) = &e.sheet {
        t.extend(s.descriptions.iter().map(|d| d.text.clone()));
        t.extend(s.usage.clone());
        t.extend(s.action.clone());
    }
    t.extend(e.setters.iter().map(|s| s.value.clone()));
    norm(&t.join(" \n "))
}

fn has_prose(e: &Element) -> bool {
    let sheet = e
        .sheet
        .as_ref()
        .is_some_and(|s| s.descriptions.iter().any(|d| !d.text.trim().is_empty()));
    !e.description.trim().is_empty() || sheet
}

fn row(head: &Head, id: String, result: Result<(), String>) -> Row {
    Row {
        id,
        level: head.level,
        class: head.class,
        pass: result.is_ok(),
        detail: result.err().unwrap_or_default(),
    }
}

fn builtin(id: &str, level: Level, result: Result<(), String>) -> Row {
    Row {
        id: id.to_string(),
        level,
        class: Mech::Mechanised,
        pass: result.is_ok(),
        detail: result.err().unwrap_or_default(),
    }
}

fn list_or_ok(what: &str, items: Vec<String>) -> Result<(), String> {
    if items.is_empty() {
        return Ok(());
    }
    let n = items.len();
    let shown: Vec<String> = items.into_iter().take(12).collect();
    Err(format!("{n} {what}: {}", shown.join("; ")))
}

/// Pack-level checks every case gets.
pub fn builtin_rows(case: &CaseFile, l: &Loaded) -> Vec<Row> {
    let ids: HashSet<&str> = l.elements.iter().map(|e| e.id.as_str()).collect();
    let mine = |i: &borealis_content::Issue| {
        i.element.as_deref().is_some_and(|id| ids.contains(id))
            || i.file
                .as_deref()
                .is_some_and(|f| f.starts_with(&format!("{}/", l.manifest.id)))
    };
    let issues = l.store.validate();
    let dangling = issues
        .iter()
        .filter(|i| i.code == IssueCode::DanglingReference && mine(i))
        .map(|i| format!("{}: {}", i.element.as_deref().unwrap_or("?"), i.message))
        .collect();
    let errors = issues
        .iter()
        .chain(l.store.diagnostics())
        .filter(|i| i.severity == Severity::Error && mine(i))
        .map(|i| format!("{}: {}", i.element.as_deref().unwrap_or("?"), i.message))
        .collect();
    let dupes = l
        .store
        .diagnostics()
        .iter()
        .filter(|i| {
            matches!(i.code, IssueCode::DuplicateId | IssueCode::ElementReplaced) && mine(i)
        })
        .map(|i| i.message.clone())
        .collect();
    let mut unknown = Vec::new();
    for e in &l.elements {
        let rules = e.all_rules().cloned().collect::<Vec<Rule>>();
        unknown.extend(
            rule_kinds::check_rules(&rules)
                .into_iter()
                .map(|p| format!("{}: {p}", e.id)),
        );
    }
    for a in &l.appends {
        unknown.extend(
            rule_kinds::check_rules(&a.rules)
                .into_iter()
                .map(|p| format!("append {}: {p}", a.id)),
        );
    }
    let unknown_types = l
        .elements
        .iter()
        .filter(|e| e.known_type().is_none())
        .map(|e| format!("{} has unknown type {:?}", e.id, e.ty))
        .collect();
    let mut rows = vec![
        builtin(
            "pack.no_dangling_references",
            Level::Must,
            list_or_ok("dangling references", dangling),
        ),
        builtin(
            "pack.no_validation_errors",
            Level::Must,
            list_or_ok("errors", errors),
        ),
        builtin(
            "pack.no_unknown_rule_kinds_or_fields",
            Level::Must,
            list_or_ok("unknown", unknown),
        ),
        builtin(
            "pack.known_element_types",
            Level::Must,
            list_or_ok("elements", unknown_types),
        ),
        builtin("pack.report_valid", Level::Must, report_valid(l)),
        builtin(
            "pack.no_id_collisions",
            Level::Should,
            list_or_ok("collisions", dupes),
        ),
        builtin("pack.provenance_user_import", Level::Should, provenance(l)),
    ];
    rows.extend(silent_drop_rows(case, l));
    rows
}

fn report_valid(l: &Loaded) -> Result<(), String> {
    if let Some(e) = &l.report_error {
        return Err(e.clone());
    }
    let Some(r) = &l.report else { return Ok(()) };
    if r.format != REPORT_FORMAT {
        return Err(format!(
            "report format {:?}, expected {REPORT_FORMAT:?}",
            r.format
        ));
    }
    let ids: HashSet<&str> = l.elements.iter().map(|e| e.id.as_str()).collect();
    let bad: Vec<String> = r
        .entries
        .iter()
        .filter_map(|e| {
            if !REPORT_OUTCOMES.contains(&e.outcome.as_str()) {
                return Some(format!("{}: outcome {:?}", e.source, e.outcome));
            }
            let missing: Vec<&str> = e
                .elements
                .iter()
                .map(String::as_str)
                .filter(|id| !ids.contains(id))
                .collect();
            (!missing.is_empty()).then(|| format!("{}: unknown elements {missing:?}", e.source))
        })
        .collect();
    list_or_ok("bad report entries", bad)
}

fn provenance(l: &Loaded) -> Result<(), String> {
    let Some(p) = &l.provenance else {
        return Err("no import.json".into());
    };
    if p.get("origin").and_then(Value::as_str) != Some("user-import") {
        return Err(format!("origin is {:?}", p.get("origin")));
    }
    if p.get("redistributable").and_then(Value::as_bool) != Some(false) {
        return Err("redistributable is not false".into());
    }
    Ok(())
}

/// One row per source object: it became an element of that name, has a report entry, or
/// survives as text inside another element.
fn silent_drop_rows(case: &CaseFile, l: &Loaded) -> Vec<Row> {
    let reported = |name: &str| {
        l.report
            .as_ref()
            .is_some_and(|r| r.entries.iter().any(|e| name_matches(name, &e.source)))
    };
    let texts: Vec<String> = l.elements.iter().map(element_text).collect();
    case.source_objects
        .iter()
        .map(|o| {
            let as_element = l.elements.iter().any(|e| {
                name_matches(&o.name, &e.name)
                    && o.ty.as_deref().map_or(true, |t| name_matches(t, &e.ty))
            });
            let as_text = texts
                .iter()
                .any(|t| o.name.split('|').any(|alt| t.contains(&norm(alt))));
            let ok = as_element || reported(&o.name) || as_text;
            let first = o.name.split('|').next().unwrap_or_default();
            Row {
                id: format!("source.{}", norm(first).replace(' ', "_")),
                level: o.level,
                class: o.class,
                pass: ok,
                detail: if ok {
                    String::new()
                } else {
                    format!(
                        "{} ({}) has no element, no report entry and no text in the pack",
                        o.name,
                        o.ty.as_deref().unwrap_or("any type")
                    )
                },
            }
        })
        .collect()
}

/// The pack element an expectation targets.
pub fn find<'a>(l: &'a Loaded, ty: &str, name: &str) -> Option<&'a Element> {
    l.elements
        .iter()
        .find(|e| name_matches(name, &e.name) && name_matches(ty, &e.ty))
}

fn setter_ok(e: &Element, key: &str, want: &str) -> Result<(), String> {
    let (name, attr) = match key.split_once('@') {
        Some((n, a)) => (n, Some(a)),
        None => (key, None),
    };
    let Some(s) = e.setter(name) else {
        return Err(format!("no setter {name}"));
    };
    let got = match attr {
        Some(a) => s.attribute(a).unwrap_or(""),
        None => s.value.as_str(),
    };
    let clean = |v: &str| norm(v).replace(' ', "");
    if clean(got) == clean(want) {
        Ok(())
    } else {
        Err(format!("{key} = {got:?}, expected {want:?}"))
    }
}

fn check_element(l: &Loaded, x: &ElementExpect) -> Result<(), String> {
    let Some(e) = find(l, &x.ty, &x.name) else {
        return Err(format!("no {} element named {:?}", x.ty, x.name));
    };
    let mut errs = Vec::new();
    if x.head.class != Mech::Mechanised && !has_prose(e) {
        errs.push("no description text".to_string());
    }
    for (k, v) in &x.setters {
        if let Err(m) = setter_ok(e, k, v) {
            errs.push(m);
        }
    }
    for id in &x.supports_ids {
        if !e.has_support(id) {
            errs.push(format!("does not support {id}"));
        }
    }
    for n in &x.supports_names {
        let hit = e.supports.iter().any(|tag| {
            name_matches(n, tag)
                || l.store
                    .element(tag)
                    .is_some_and(|t| name_matches(n, &t.name))
        });
        if !hit {
            errs.push(format!("no supports tag named {n:?}"));
        }
    }
    let json = serde_json::to_value(e).map_err(|err| err.to_string())?;
    let rules: Vec<Value> = e
        .all_rules()
        .filter_map(|r| serde_json::to_value(r).ok())
        .collect();
    for want in &x.rules_include {
        if !rules.iter().any(|r| subset(want, r)) {
            errs.push(format!("no rule like {want}"));
        }
    }
    for j in &x.json {
        let Some(v) = json.pointer(&j.pointer) else {
            errs.push(format!("{} missing", j.pointer));
            continue;
        };
        if let Some(eq) = &j.equals {
            if !subset(eq, v) {
                errs.push(format!("{} = {v}, expected {eq}", j.pointer));
            }
        }
        let text = norm(&match v {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        });
        for t in &j.contains {
            if !text.contains(&norm(t)) {
                errs.push(format!("{} lacks {t:?}", j.pointer));
            }
        }
    }
    let text = element_text(e);
    for t in &x.text_contains {
        if !text.contains(&norm(t)) {
            errs.push(format!("text lacks {t:?}"));
        }
    }
    if errs.is_empty() {
        Ok(())
    } else {
        Err(format!("{}: {}", e.id, errs.join("; ")))
    }
}

fn check_mention(l: &Loaded, x: &MentionExpect) -> Result<(), String> {
    let hit = l.elements.iter().any(|e| {
        let t = element_text(e);
        x.text.iter().all(|w| t.contains(&norm(w)))
    });
    if hit {
        Ok(())
    } else {
        Err(format!("no element mentions {:?}", x.text))
    }
}

/// Rows of the `elements` and `mentions` expectations.
pub fn element_rows(case: &CaseFile, l: &Loaded) -> Vec<Row> {
    let mut out: Vec<Row> = case
        .elements
        .iter()
        .map(|x| row(&x.head, x.head.id.clone(), check_element(l, x)))
        .collect();
    out.extend(
        case.mentions
            .iter()
            .map(|x| row(&x.head, x.head.id.clone(), check_mention(l, x))),
    );
    out
}
