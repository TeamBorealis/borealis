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
    AbsentExpect, CaseFile, CountExpect, ElementExpect, ElementFilter, Head, ImportReport,
    JsonCheck, Level, Mech, MentionExpect, PackEmptyExpect, ReportExpect, REPORT_FORMAT,
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
    // a price keeps its unit in an attribute: "1500" + currency "gp" shows as "1500 gp"
    t.extend(
        e.setters
            .iter()
            .map(|s| match s.attributes.get("currency") {
                Some(c) => format!("{} {c}", s.value),
                None => s.value.clone(),
            }),
    );
    norm(&plain_text(&t.join(" \n ")))
}

/// Descriptions are HTML: drop the markup so a phrase split by inline tags
/// (`<i>glow orb</i> cantrip`) reads as the plain text a player sees. Block tags become
/// a space, inline tags vanish, the common entities are decoded.
fn plain_text(html: &str) -> String {
    const BLOCK: [&str; 14] = [
        "p", "br", "li", "ul", "ol", "div", "tr", "td", "th", "table", "h1", "h2", "h3", "h4",
    ];
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(open) = rest.find('<') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let Some(close) = after.find('>') else {
            // not a tag: a literal "<" (as in "HP < 10")
            out.push('<');
            rest = after;
            continue;
        };
        let tag = &after[..close];
        let name = tag
            .trim_start_matches('/')
            .split(|c: char| !c.is_ascii_alphanumeric())
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        let is_tag = !name.is_empty() && name.starts_with(|c: char| c.is_ascii_alphabetic());
        if !is_tag {
            out.push('<');
            rest = after;
            continue;
        }
        if BLOCK.contains(&name.as_str()) {
            out.push(' ');
        }
        rest = &after[close + 1..];
    }
    out.push_str(rest);
    out.replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
}

/// Number words as digits ("six hours" = "6 hours"): the same information, written either way.
fn digits(text: &str) -> String {
    const WORDS: [&str; 20] = [
        "one",
        "two",
        "three",
        "four",
        "five",
        "six",
        "seven",
        "eight",
        "nine",
        "ten",
        "eleven",
        "twelve",
        "thirteen",
        "fourteen",
        "fifteen",
        "sixteen",
        "seventeen",
        "eighteen",
        "nineteen",
        "twenty",
    ];
    text.split(' ')
        .map(|w| {
            let bare = w.trim_matches(|c: char| !c.is_ascii_alphanumeric());
            match WORDS.iter().position(|x| *x == bare) {
                Some(i) => w.replace(bare, &(i + 1).to_string()),
                None => w.to_string(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Does the (normalised) text contain the token, ignoring spacing ("1500gp" = "1500 gp",
/// "Wood/Metal" = "Wood / Metal") and number words ("six hours" = "6 hours")?
fn text_has(text: &str, token: &str) -> bool {
    let t = norm(token);
    text.contains(&t) || squash(text).contains(&squash(&t)) || digits(text).contains(&digits(&t))
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
        .chain(
            r.errors
                .iter()
                .chain(&r.normalised)
                .chain(&r.warnings)
                .filter(|i| !ids.contains(i.element.as_str()))
                .map(|i| format!("{}: unknown element", i.element)),
        )
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

/// The value of `"setter"` or `"setter@attribute"`.
fn setter_value<'a>(e: &'a Element, key: &str) -> Result<&'a str, String> {
    let (name, attr) = match key.split_once('@') {
        Some((n, a)) => (n, Some(a)),
        None => (key, None),
    };
    let Some(s) = e.setter(name) else {
        return Err(format!("no setter {name}"));
    };
    Ok(match attr {
        Some(a) => s.attribute(a).unwrap_or(""),
        None => s.value.as_str(),
    })
}

fn squash(v: &str) -> String {
    norm(v).replace(' ', "")
}

/// Exact (normalised) compare; `want` may list `A|B` alternatives.
fn setter_ok(e: &Element, key: &str, want: &str) -> Result<(), String> {
    let got = setter_value(e, key)?;
    if want.split('|').any(|w| squash(got) == squash(w)) {
        Ok(())
    } else {
        Err(format!("{key} = {got:?}, expected {want:?}"))
    }
}

fn setter_contains(e: &Element, key: &str, token: &str) -> Result<(), String> {
    let got = setter_value(e, key)?;
    if norm(got).contains(&norm(token)) {
        Ok(())
    } else {
        Err(format!("{key} = {got:?} lacks {token:?}"))
    }
}

/// Is `n` a `supports` tag of `e`, or the name of the element a tag names?
fn supports_name(l: &Loaded, e: &Element, n: &str) -> bool {
    e.supports.iter().any(|tag| {
        name_matches(n, tag)
            || l.store
                .element(tag)
                .is_some_and(|t| name_matches(n, &t.name))
    })
}

/// A `rules_include` entry: an object (JSON subset of one rule) or an array of such objects,
/// any of which may match. A wanted `resource` with a `recharge` list is also met by a
/// `grant_spell` whose `free_cast` recharges on one of those rests (a spell cast without a
/// slot once per rest is that spell's own limited use, not a separate resource).
fn rule_included(want: &Value, rules: &[Value]) -> bool {
    match want {
        Value::Array(alts) => alts.iter().any(|a| rule_included(a, rules)),
        w => {
            rules.iter().any(|r| subset(w, r))
                || free_cast_alternatives(w)
                    .iter()
                    .any(|alt| rules.iter().any(|r| subset(alt, r)))
        }
    }
}

/// The `grant_spell` free-cast forms of a wanted per-rest `resource` rule: one per listed rest.
fn free_cast_alternatives(want: &Value) -> Vec<Value> {
    if want.get("kind").and_then(Value::as_str) != Some("resource") {
        return Vec::new();
    }
    let Some(rests) = want.get("recharge").and_then(Value::as_array) else {
        return Vec::new();
    };
    rests
        .iter()
        .filter(|r| r.is_string())
        .map(|r| serde_json::json!({"kind": "grant_spell", "free_cast": {"recharge": r}}))
        .collect()
}

/// The element and, when `follow`, every pack element it grants (transitively), in grant order.
fn granted_closure<'a>(l: &'a Loaded, e: &'a Element, follow: bool) -> Vec<&'a Element> {
    let mut seen = vec![e];
    if !follow {
        return seen;
    }
    let mut next = 0;
    while next < seen.len() {
        let ids: Vec<String> = seen[next]
            .all_rules()
            .filter_map(|r| serde_json::to_value(r).ok())
            .filter(|r| r.get("kind").and_then(Value::as_str) == Some("grant"))
            .filter_map(|r| r.get("id").and_then(Value::as_str).map(str::to_string))
            .collect();
        next += 1;
        for id in ids {
            if let Some(g) = l.elements.iter().find(|x| x.id == id) {
                if !seen.iter().any(|x| x.id == g.id) {
                    seen.push(g);
                }
            }
        }
    }
    seen
}

/// Does the JSON check hold on this serialized element?
fn json_check_errors(json: &Value, j: &JsonCheck) -> Vec<String> {
    let Some(v) = json.pointer(&j.pointer) else {
        return vec![format!("{} missing", j.pointer)];
    };
    let mut errs = Vec::new();
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
    if !j.contains_any.is_empty() && !j.contains_any.iter().any(|t| text.contains(&norm(t))) {
        errs.push(format!("{} has none of {:?}", j.pointer, j.contains_any));
    }
    errs
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
    for (k, v) in &x.setters_contain {
        if let Err(m) = setter_contains(e, k, v) {
            errs.push(m);
        }
    }
    for id in &x.supports_ids {
        if !e.has_support(id) {
            errs.push(format!("does not support {id}"));
        }
    }
    for n in &x.supports_names {
        if !supports_name(l, e, n) {
            errs.push(format!("no supports tag named {n:?}"));
        }
    }
    let closure = granted_closure(l, e, x.follow_grants);
    let jsons: Vec<Value> = closure
        .iter()
        .map(|c| serde_json::to_value(c).map_err(|err| err.to_string()))
        .collect::<Result<_, _>>()?;
    let rules: Vec<Value> = closure
        .iter()
        .flat_map(|c| c.all_rules())
        .filter_map(|r| serde_json::to_value(r).ok())
        .collect();
    for want in &x.rules_include {
        if !rule_included(want, &rules) {
            errs.push(format!("no rule like {want}"));
        }
    }
    for j in &x.json {
        // `/rules` of a closure is every rule of every element in it; any other pointer holds
        // when some element of the closure satisfies it.
        let all_rules = (j.pointer == "/rules" && x.follow_grants).then(|| {
            let mut m = jsons[0].clone();
            m["rules"] = Value::Array(rules.clone());
            m
        });
        let per = all_rules
            .iter()
            .chain(jsons.iter())
            .map(|v| json_check_errors(v, j));
        let mut best: Option<Vec<String>> = None;
        for r in per {
            if r.is_empty() {
                best = Some(r);
                break;
            }
            best.get_or_insert(r);
        }
        errs.extend(best.unwrap_or_default());
    }
    let text = element_text(e);
    for t in &x.text_contains {
        if !text_has(&text, t) {
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
        x.text.iter().all(|w| text_has(&t, w))
    });
    if hit {
        Ok(())
    } else {
        Err(format!("no element mentions {:?}", x.text))
    }
}

fn filter_matches(l: &Loaded, f: &ElementFilter, e: &Element) -> bool {
    f.ty.as_deref().map_or(true, |t| name_matches(t, &e.ty))
        && f.setters.iter().all(|(k, v)| setter_ok(e, k, v).is_ok())
        && f.supports_names.iter().all(|n| supports_name(l, e, n))
}

fn check_count(l: &Loaded, x: &CountExpect) -> Result<(), String> {
    let n = l
        .elements
        .iter()
        .filter(|e| filter_matches(l, &x.filter, e))
        .count();
    let bad = x.equals.is_some_and(|w| n != w)
        || x.min.is_some_and(|m| n < m)
        || x.max.is_some_and(|m| n > m);
    if bad {
        Err(format!(
            "{n} elements match, expected{}{}{}",
            x.equals.map(|w| format!(" = {w}")).unwrap_or_default(),
            x.min.map(|w| format!(" >= {w}")).unwrap_or_default(),
            x.max.map(|w| format!(" <= {w}")).unwrap_or_default()
        ))
    } else {
        Ok(())
    }
}

fn check_pack_empty(l: &Loaded, x: &PackEmptyExpect) -> Result<(), String> {
    let found: Vec<String> = l
        .elements
        .iter()
        .filter(|e| {
            x.except_types
                .as_deref()
                .map_or(true, |t| !name_matches(t, &e.ty))
        })
        .map(|e| format!("{} ({})", e.name, e.ty))
        .collect();
    if found.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "{} unexpected element(s): {:?}",
            found.len(),
            &found[..found.len().min(8)]
        ))
    }
}

fn check_absent(l: &Loaded, x: &AbsentExpect) -> Result<(), String> {
    let found: Vec<&str> = l
        .elements
        .iter()
        .filter(|e| {
            name_matches(&x.name, &e.name)
                && x.ty.as_deref().map_or(true, |t| name_matches(t, &e.ty))
        })
        .map(|e| e.id.as_str())
        .collect();
    if found.is_empty() {
        Ok(())
    } else {
        Err(format!("unexpected element(s) {found:?}"))
    }
}

fn check_report(l: &Loaded, x: &ReportExpect) -> Result<(), String> {
    let Some(r) = &l.report else {
        return Err("no report.json".into());
    };
    let entries: Vec<_> = r
        .entries
        .iter()
        .filter(|e| name_matches(&x.source, &e.source))
        .collect();
    if entries.is_empty() {
        return Err(format!("no report entry for {:?}", x.source));
    }
    match &x.outcome {
        Some(o) if !entries.iter().any(|e| name_matches(o, &e.outcome)) => Err(format!(
            "outcome {:?}, expected {o:?}",
            entries
                .iter()
                .map(|e| e.outcome.as_str())
                .collect::<Vec<_>>()
        )),
        _ => Ok(()),
    }
}

/// Rows of the `elements`, `mentions`, `counts`, `absent` and `report` expectations.
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
    out.extend(
        case.counts
            .iter()
            .map(|x| row(&x.head, x.head.id.clone(), check_count(l, x))),
    );
    out.extend(
        case.pack_empty
            .iter()
            .map(|x| row(&x.head, x.head.id.clone(), check_pack_empty(l, x))),
    );
    out.extend(
        case.absent
            .iter()
            .map(|x| row(&x.head, x.head.id.clone(), check_absent(l, x))),
    );
    out.extend(
        case.report
            .iter()
            .map(|x| row(&x.head, x.head.id.clone(), check_report(l, x))),
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inline_tags_do_not_split_a_phrase() {
        let t = norm(&plain_text("<p>You know the <i>glow orb</i> cantrip.</p>"));
        assert!(text_has(&t, "glow orb cantrip"));
    }

    #[test]
    fn block_tags_separate_words_and_entities_decode() {
        let t = norm(&plain_text(
            "<ul><li>fire</li><li>ice &amp; frost</li></ul>",
        ));
        assert_eq!(t, "fire ice & frost");
    }

    #[test]
    fn a_per_rest_free_cast_meets_a_wanted_resource() {
        let want = serde_json::json!({"kind": "resource", "recharge": ["long_rest"]});
        let cast = |rest: &str| {
            serde_json::json!({"kind": "grant_spell", "spell": "ID_X", "mode": "always_prepared",
                "free_cast": {"uses": 1, "recharge": rest}})
        };
        assert!(rule_included(&want, &[cast("long_rest")]));
        assert!(!rule_included(&want, &[cast("short_rest")]));
        let plain = serde_json::json!({"kind": "grant_spell", "spell": "ID_X"});
        assert!(!rule_included(&want, &[plain]));
        let real = serde_json::json!({"kind": "resource", "recharge": ["long_rest"]});
        assert!(rule_included(&want, &[real]));
        // only a resource want is widened
        let other = serde_json::json!({"kind": "grant", "recharge": ["long_rest"]});
        assert!(!rule_included(&other, &[cast("long_rest")]));
    }

    #[test]
    fn a_literal_less_than_sign_survives() {
        assert_eq!(plain_text("HP < 10 and 3<4"), "HP < 10 and 3<4");
    }

    #[test]
    fn number_words_match_digits_both_ways() {
        assert!(text_has("spend at least 6 hours resting", "six hours"));
        assert!(text_has("spend at least six hours resting", "6 hours"));
        assert!(!text_has("spend at least 7 hours resting", "six hours"));
    }
}
