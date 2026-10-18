//! `_copy` / `_mod` / `_versions` resolution on raw 5etools JSON entries.
//!
//! 5etools lets an entry inherit another entry (`_copy`) and patch it with a list of
//! modifications (`_mod`). Resolution happens on the raw JSON before the typed conversion,
//! so every converter only ever sees fully materialised entries.

use regex::RegexBuilder;
use serde_json::{Map, Value};
use std::collections::{BTreeSet, HashMap};

/// Properties that a copy does *not* inherit unless `_preserve` names them: the copy is a
/// different publication of the content.
const NOT_INHERITED: &[&str] = &[
    "srd",
    "srd52",
    "basicRules",
    "basicRules2024",
    "freeRules2024",
    "page",
    "otherSources",
    "referenceSources",
    "additionalSources",
    "reprintedAs",
    "isReprinted",
    "hasFluff",
    "hasFluffImages",
    "legacy",
    "_versions",
    "alias",
];

/// Identity fields used to look up the base of a `_copy` for a property array.
fn identity_fields(prop: &str) -> &'static [&'static str] {
    match prop {
        "subclass" => &["name", "source", "className", "classSource"],
        "subclassFeature" => &[
            "name",
            "source",
            "className",
            "classSource",
            "subclassShortName",
            "subclassSource",
            "level",
        ],
        "classFeature" => &["name", "source", "className", "classSource", "level"],
        "subrace" => &["name", "source", "raceName", "raceSource"],
        _ => &["name", "source"],
    }
}

fn field_text(value: &Value, field: &str) -> String {
    match value.get(field) {
        Some(Value::String(s)) => s.to_lowercase(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

/// Lookup key of an entry (or of a `_copy` block) for the property array `prop`.
#[must_use]
pub fn identity_key(prop: &str, entry: &Value) -> String {
    identity_fields(prop)
        .iter()
        .map(|f| match (prop, *f) {
            // a generic magic variant keeps its source in `inherits`
            ("magicvariant", "source") if entry.get("source").is_none() => entry
                .get("inherits")
                .map(|i| field_text(i, "source"))
                .unwrap_or_default(),
            _ => field_text(entry, f),
        })
        .collect::<Vec<_>>()
        .join("|")
}

/// Outcome of resolving one property array.
#[derive(Debug, Default)]
pub struct Resolved {
    /// Fully materialised entries (copies resolved, versions expanded), input order kept.
    pub entries: Vec<Value>,
    /// Index into the input of each entry in `entries` (versions share their parent's).
    pub origins: Vec<usize>,
    /// Entries that could not be resolved: `(name|source, reason)`.
    pub failed: Vec<(String, String)>,
    /// Index into the input of each entry in `failed`.
    pub failed_origins: Vec<usize>,
    /// Non-fatal observations (unsupported `_mod` modes and similar).
    pub notes: Vec<String>,
}

struct Resolver<'a> {
    prop: &'a str,
    raw: &'a [Value],
    index: HashMap<String, usize>,
    done: HashMap<usize, Result<Value, String>>,
    stack: BTreeSet<usize>,
    notes: Vec<String>,
}

impl Resolver<'_> {
    fn resolve(&mut self, i: usize) -> Result<Value, String> {
        if let Some(v) = self.done.get(&i) {
            return v.clone();
        }
        if !self.stack.insert(i) {
            return Err("circular _copy".to_string());
        }
        let result = self.resolve_uncached(i);
        self.stack.remove(&i);
        self.done.insert(i, result.clone());
        result
    }

    fn resolve_uncached(&mut self, i: usize) -> Result<Value, String> {
        let entry = self.raw[i].clone();
        let Some(copy) = entry.get("_copy").cloned() else {
            return Ok(entry);
        };
        let base_key = identity_key(self.prop, &copy);
        let Some(&base_idx) = self.index.get(&base_key) else {
            return Err(format!(
                "_copy base {} ({}) not found",
                copy.get("name").and_then(Value::as_str).unwrap_or("?"),
                copy.get("source").and_then(Value::as_str).unwrap_or("?")
            ));
        };
        let base = self.resolve(base_idx)?;
        let mut notes = Vec::new();
        let merged = merge_copy(&base, &entry, &copy, &mut notes);
        self.notes.extend(notes);
        Ok(merged)
    }
}

fn merge_copy(base: &Value, entry: &Value, copy: &Value, notes: &mut Vec<String>) -> Value {
    let preserve: BTreeSet<String> = copy
        .get("_preserve")
        .and_then(Value::as_object)
        .map(|m| m.keys().cloned().collect())
        .unwrap_or_default();
    let mut result = Map::new();
    if let Some(base_map) = base.as_object() {
        for (k, v) in base_map {
            if NOT_INHERITED.contains(&k.as_str()) && !preserve.contains(k) {
                continue;
            }
            result.insert(k.clone(), v.clone());
        }
    }
    if let Some(entry_map) = entry.as_object() {
        for (k, v) in entry_map {
            if k == "_copy" {
                continue;
            }
            // `"prop": null` removes the inherited property (5etools copy semantics)
            if v.is_null() {
                result.remove(k);
            } else {
                result.insert(k.clone(), v.clone());
            }
        }
    }
    let mut merged = Value::Object(result);
    if let Some(mods) = copy.get("_mod") {
        apply_mods(&mut merged, mods, notes);
    }
    if let Value::Object(m) = &mut merged {
        m.remove("_mod");
    }
    merged
}

/// Resolve `_copy` and expand `_versions` for every entry of one property array.
#[must_use]
pub fn resolve_array(prop: &str, raw: &[Value]) -> Resolved {
    let mut index = HashMap::new();
    for (i, e) in raw.iter().enumerate() {
        index.entry(identity_key(prop, e)).or_insert(i);
    }
    let mut resolver = Resolver {
        prop,
        raw,
        index,
        done: HashMap::new(),
        stack: BTreeSet::new(),
        notes: Vec::new(),
    };
    let mut out = Resolved::default();
    for (i, entry) in raw.iter().enumerate() {
        match resolver.resolve(i) {
            Ok(v) => {
                let mut versions = expand_versions(&v, &mut out.notes);
                let trait_name = (prop == "race").then(|| version_trait(&v)).flatten();
                let mut v = strip_versions(v);
                if let Some(t) = trait_name.filter(|_| versions.len() >= 2) {
                    mark_race_versions(&mut v, &mut versions, &t);
                }
                out.entries.push(v);
                out.origins.push(i);
                for version in versions {
                    out.entries.push(version);
                    out.origins.push(i);
                }
            }
            Err(reason) => {
                out.failed.push((display_name(entry), reason));
                out.failed_origins.push(i);
            }
        }
    }
    out.notes.extend(resolver.notes);
    out
}

/// Marker on a race whose `_versions` are one choice: the trait every version replaces.
pub const VERSION_TRAIT: &str = "_borealisVersionTrait";
/// Marker on a race whose `_versions` are one choice: the expanded versions.
pub const VERSIONS: &str = "_borealisVersions";
/// Marker on an expanded version of such a race: `{name, source}` of the race.
pub const VERSION_OF: &str = "_borealisVersionOf";

/// The trait every `_versions` entry of a race replaces or removes first (`Gnomish Lineage`,
/// `Draconic Ancestry`): the versions are the options of that trait. `None` when the versions
/// do not all name the same trait.
fn version_trait(entry: &Value) -> Option<String> {
    let versions = entry.get("_versions")?.as_array()?;
    let mut common: Option<String> = None;
    for version in versions {
        let spec = version.get("_abstract").unwrap_or(version);
        let ops = spec.get("_mod")?.get("entries")?;
        let first = match ops {
            Value::Array(a) => a.first()?,
            other => other,
        };
        let named = match first.get("mode").and_then(Value::as_str)? {
            "replaceArr" => first.get("replace"),
            "removeArr" => first.get("names"),
            _ => None,
        }?;
        let name = match named {
            Value::Array(a) => a.first()?.as_str()?,
            other => other.as_str()?,
        };
        match &common {
            Some(c) if c != name => return None,
            Some(_) => {}
            None => common = Some(name.to_string()),
        }
    }
    common
}

/// A race's versions become the options of `trait_name` on the race (`race.rs`): the race keeps
/// them under [`VERSIONS`]; each stays an entry of its own marked [`VERSION_OF`] (the former
/// species, kept for characters saved with it).
fn mark_race_versions(base: &mut Value, versions: &mut [Value], trait_name: &str) {
    let of = serde_json::json!({
        "name": base.get("name").cloned().unwrap_or(Value::Null),
        "source": base.get("source").cloned().unwrap_or(Value::Null),
    });
    if let Value::Object(m) = base {
        m.insert(VERSION_TRAIT.into(), Value::String(trait_name.to_string()));
        m.insert(VERSIONS.into(), Value::Array(versions.to_vec()));
    }
    for v in versions.iter_mut() {
        if let Value::Object(m) = v {
            m.insert(VERSION_OF.into(), of.clone());
        }
    }
}

fn strip_versions(mut v: Value) -> Value {
    if let Value::Object(m) = &mut v {
        m.remove("_versions");
    }
    v
}

/// `name|source` of an entry for messages.
#[must_use]
pub fn display_name(entry: &Value) -> String {
    let name = entry
        .get("name")
        .and_then(Value::as_str)
        .or_else(|| {
            entry
                .get("_copy")
                .and_then(|c| c.get("name"))
                .and_then(Value::as_str)
        })
        .unwrap_or("?");
    let source = entry.get("source").and_then(Value::as_str).unwrap_or("?");
    format!("{name}|{source}")
}

// ---------------------------------------------------------------------------------------
// _versions
// ---------------------------------------------------------------------------------------

fn substitute_variables(value: &mut Value, vars: &Map<String, Value>) {
    match value {
        Value::String(s) => {
            for (k, v) in vars {
                if let Some(text) = v.as_str() {
                    *s = s.replace(&format!("{{{{{k}}}}}"), text);
                }
            }
        }
        Value::Array(a) => a.iter_mut().for_each(|x| substitute_variables(x, vars)),
        Value::Object(m) => m.values_mut().for_each(|x| substitute_variables(x, vars)),
        _ => {}
    }
}

fn expand_versions(entry: &Value, notes: &mut Vec<String>) -> Vec<Value> {
    let Some(versions) = entry.get("_versions").and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut parent = entry.clone();
    if let Value::Object(m) = &mut parent {
        m.remove("_versions");
    }
    let mut out = Vec::new();
    for version in versions {
        let mut candidates: Vec<Value> = Vec::new();
        if let (Some(abstract_), Some(impls)) = (
            version.get("_abstract"),
            version.get("_implementations").and_then(Value::as_array),
        ) {
            for imp in impls {
                let mut v = abstract_.clone();
                if let Some(vars) = imp.get("_variables").and_then(Value::as_object) {
                    substitute_variables(&mut v, vars);
                }
                if let (Value::Object(vm), Some(im)) = (&mut v, imp.as_object()) {
                    for (k, val) in im {
                        if k != "_variables" {
                            vm.insert(k.clone(), val.clone());
                        }
                    }
                }
                candidates.push(v);
            }
        } else {
            candidates.push(version.clone());
        }
        for cand in candidates {
            let mut merged = parent.clone();
            if let (Value::Object(mm), Some(cm)) = (&mut merged, cand.as_object()) {
                for (k, v) in cm {
                    if k == "_mod" {
                        continue;
                    }
                    if v.is_null() {
                        mm.remove(k);
                    } else {
                        mm.insert(k.clone(), v.clone());
                    }
                }
            }
            if let Some(mods) = cand.get("_mod") {
                apply_mods(&mut merged, mods, notes);
            }
            out.push(merged);
        }
    }
    out
}

// ---------------------------------------------------------------------------------------
// _mod
// ---------------------------------------------------------------------------------------

fn as_list(value: &Value) -> Vec<&Value> {
    match value {
        Value::Array(a) => a.iter().collect(),
        other => vec![other],
    }
}

fn apply_mods(target: &mut Value, mods: &Value, notes: &mut Vec<String>) {
    let Some(map) = mods.as_object() else {
        return;
    };
    for (key, spec) in map {
        for m in as_list(spec) {
            apply_one(target, key, m, notes);
        }
    }
}

fn items_of(m: &Value) -> Vec<Value> {
    match m.get("items") {
        Some(Value::Array(a)) => a.clone(),
        Some(other) => vec![other.clone()],
        None => Vec::new(),
    }
}

fn find_index(arr: &[Value], replace: &Value) -> Option<usize> {
    match replace {
        Value::String(name) => arr.iter().position(|item| match item {
            Value::String(s) => s == name,
            Value::Object(_) => item.get("name").and_then(Value::as_str) == Some(name),
            _ => false,
        }),
        Value::Object(o) => {
            if let Some(i) = o.get("index").and_then(Value::as_u64) {
                usize::try_from(i).ok().filter(|i| *i < arr.len())
            } else if let Some(re) = o.get("regex").and_then(Value::as_str) {
                let re = RegexBuilder::new(re).build().ok()?;
                arr.iter().position(|item| match item {
                    Value::String(s) => re.is_match(s),
                    other => other
                        .get("name")
                        .and_then(Value::as_str)
                        .is_some_and(|s| re.is_match(s)),
                })
            } else {
                None
            }
        }
        _ => None,
    }
}

fn walk_replace(
    value: &mut Value,
    re: &regex::Regex,
    with: &str,
    global: bool,
    props: Option<&[Option<String>]>,
    parent_key: Option<&str>,
) {
    let allowed = |key: Option<&str>| match props {
        None => true,
        Some(list) => list.iter().any(|p| p.as_deref() == key),
    };
    match value {
        Value::String(s) => {
            if allowed(parent_key) {
                let replaced = if global {
                    re.replace_all(s, with).into_owned()
                } else {
                    re.replace(s, with).into_owned()
                };
                *s = replaced;
            }
        }
        Value::Array(a) => {
            for x in a {
                // strings directly inside an array count as "no key" (null in `props`)
                if x.is_string() {
                    walk_replace(
                        x,
                        re,
                        with,
                        global,
                        props,
                        parent_key.filter(|_| props.is_none()),
                    );
                } else {
                    walk_replace(x, re, with, global, props, parent_key);
                }
            }
        }
        Value::Object(m) => {
            for (k, v) in m.iter_mut() {
                walk_replace(v, re, with, global, props, Some(k));
            }
        }
        _ => {}
    }
}

fn build_regex(pattern: &str, flags: &str) -> Option<regex::Regex> {
    RegexBuilder::new(pattern)
        .case_insensitive(flags.contains('i'))
        .multi_line(flags.contains('m'))
        .build()
        .ok()
}

fn set_path(target: &mut Value, path: &str, value: Value) {
    let mut cur = target;
    let parts: Vec<&str> = path.split('.').collect();
    for (i, part) in parts.iter().enumerate() {
        if !cur.is_object() {
            *cur = Value::Object(Map::new());
        }
        let Value::Object(m) = cur else {
            return;
        };
        if i + 1 == parts.len() {
            m.insert((*part).to_string(), value);
            return;
        }
        cur = m
            .entry((*part).to_string())
            .or_insert_with(|| Value::Object(Map::new()));
    }
}

fn apply_one(target: &mut Value, key: &str, m: &Value, notes: &mut Vec<String>) {
    // A dotted key (`inherits.entries`) addresses a nested property: walk to its parent.
    if key != "*" {
        if let Some((head, rest)) = key.split_once('.') {
            if let Value::Object(obj) = target {
                let slot = obj
                    .entry(head.to_string())
                    .or_insert_with(|| Value::Object(Map::new()));
                apply_one(slot, rest, m, notes);
            }
            return;
        }
    }
    // The bare string `"remove"` deletes the property.
    if m.as_str() == Some("remove") {
        if let Value::Object(obj) = target {
            obj.remove(key);
        }
        return;
    }
    let Some(mode) = m.get("mode").and_then(Value::as_str) else {
        if let Some(other) = m.as_str() {
            notes.push(format!("_mod: unsupported mode {other:?} on {key}"));
        }
        return;
    };
    // `*` addresses the whole entity for text rewrites, a property name addresses one property.
    match mode {
        "replaceTxt" => {
            let Some(pattern) = m.get("replace").and_then(Value::as_str) else {
                return;
            };
            let with = m.get("with").and_then(Value::as_str).unwrap_or("");
            let flags = m.get("flags").and_then(Value::as_str).unwrap_or("");
            let Some(re) = build_regex(pattern, flags) else {
                notes.push(format!("replaceTxt: bad regex {pattern:?}"));
                return;
            };
            let props: Option<Vec<Option<String>>> = m
                .get("props")
                .and_then(Value::as_array)
                .map(|a| a.iter().map(|p| p.as_str().map(str::to_string)).collect());
            let global = flags.contains('g');
            if key == "*" {
                walk_replace(target, &re, with, global, props.as_deref(), None);
            } else if let Some(sub) = target.get_mut(key) {
                walk_replace(sub, &re, with, global, props.as_deref(), None);
            }
        }
        "replaceName" => {
            let (Some(pattern), Some(with)) = (
                m.get("replace").and_then(Value::as_str),
                m.get("with").and_then(Value::as_str),
            ) else {
                return;
            };
            let flags = m.get("flags").and_then(Value::as_str).unwrap_or("");
            if let (Some(re), Some(Value::String(name))) =
                (build_regex(pattern, flags), target.get_mut("name"))
            {
                *name = re.replace_all(name, with).into_owned();
            }
        }
        "appendStr" => {
            let Some(text) = m.get("str").and_then(Value::as_str) else {
                return;
            };
            let joiner = m.get("joiner").and_then(Value::as_str).unwrap_or(" ");
            if let Some(Value::String(s)) = target.get_mut(key) {
                s.push_str(joiner);
                s.push_str(text);
            }
        }
        "prependArr" | "appendArr" | "appendIfNotExistsArr" => {
            let items = items_of(m);
            let Value::Object(obj) = target else {
                return;
            };
            let slot = obj
                .entry(key.to_string())
                .or_insert_with(|| Value::Array(Vec::new()));
            if let Value::Array(arr) = slot {
                match mode {
                    "prependArr" => {
                        let mut combined = items;
                        combined.append(arr);
                        *arr = combined;
                    }
                    "appendArr" => arr.extend(items),
                    _ => {
                        for item in items {
                            if !arr.contains(&item) {
                                arr.push(item);
                            }
                        }
                    }
                }
            }
        }
        "replaceArr" | "replaceOrAppendArr" => {
            let items = items_of(m);
            let Some(replace) = m.get("replace") else {
                return;
            };
            let Value::Object(obj) = target else {
                return;
            };
            let slot = obj
                .entry(key.to_string())
                .or_insert_with(|| Value::Array(Vec::new()));
            if let Value::Array(arr) = slot {
                match find_index(arr, replace) {
                    Some(i) => {
                        arr.splice(i..=i, items);
                    }
                    None if mode == "replaceOrAppendArr" => arr.extend(items),
                    None => notes.push(format!("replaceArr: target not found in {key}")),
                }
            }
        }
        "insertArr" => {
            let items = items_of(m);
            let index = m
                .get("index")
                .and_then(Value::as_u64)
                .and_then(|i| usize::try_from(i).ok())
                .unwrap_or(0);
            if let Some(Value::Array(arr)) = target.get_mut(key) {
                let at = index.min(arr.len());
                arr.splice(at..at, items);
            }
        }
        "removeArr" => {
            let Some(Value::Array(arr)) = target.get_mut(key) else {
                return;
            };
            if let Some(names) = m.get("names") {
                let names: Vec<&str> = as_list(names)
                    .into_iter()
                    .filter_map(Value::as_str)
                    .collect();
                arr.retain(|item| {
                    item.get("name")
                        .and_then(Value::as_str)
                        .map_or(true, |n| !names.contains(&n))
                });
            } else if let Some(items) = m.get("items") {
                let items = as_list(items);
                arr.retain(|item| !items.contains(&item));
            }
        }
        "renameArr" => {
            let Some(Value::Array(arr)) = target.get_mut(key) else {
                return;
            };
            for rename in m.get("renames").map(as_list).unwrap_or_default() {
                let (Some(from), Some(to)) = (
                    rename.get("rename").and_then(Value::as_str),
                    rename.get("with").and_then(Value::as_str),
                ) else {
                    continue;
                };
                for item in arr.iter_mut() {
                    if item.get("name").and_then(Value::as_str) == Some(from) {
                        item["name"] = Value::String(to.to_string());
                    }
                }
            }
        }
        "setProp" => {
            let Some(value) = m.get("value") else {
                return;
            };
            match (key, m.get("prop").and_then(Value::as_str)) {
                ("*" | "_", Some(path)) => set_path(target, path, value.clone()),
                (k, Some(path)) => {
                    if let Value::Object(obj) = target {
                        let slot = obj
                            .entry(k.to_string())
                            .or_insert_with(|| Value::Object(Map::new()));
                        set_path(slot, path, value.clone());
                    }
                }
                (k, None) => {
                    if let Value::Object(obj) = target {
                        obj.insert(k.to_string(), value.clone());
                    }
                }
            }
        }
        "scalarAddProp" | "scalarMultProp" => {
            let (Some(prop), Some(by)) = (
                m.get("prop").and_then(Value::as_str),
                m.get("scalar").and_then(Value::as_f64),
            ) else {
                return;
            };
            let Some(slot) = target.get_mut(key).and_then(|v| v.get_mut(prop)) else {
                return;
            };
            if let Some(n) = slot.as_f64() {
                let out = if mode == "scalarAddProp" {
                    n + by
                } else {
                    n * by
                };
                *slot = if out.fract() == 0.0 {
                    Value::from(out as i64)
                } else {
                    Value::from(out)
                };
            }
        }
        other => notes.push(format!("unsupported _mod mode {other:?}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn resolve(prop: &str, raw: Value) -> Resolved {
        resolve_array(prop, raw.as_array().unwrap())
    }

    #[test]
    fn plain_copy_fills_missing_fields_and_drops_flags() {
        let r = resolve(
            "background",
            json!([
                {"name": "Base Job", "source": "AAA", "srd": true, "page": 9,
                 "skillProficiencies": [{"stealth": true}], "entries": ["one", "two"]},
                {"name": "Copy Job", "source": "BBB", "_copy": {"name": "Base Job", "source": "AAA"},
                 "entries": ["own"]}
            ]),
        );
        assert!(r.failed.is_empty());
        let copy = &r.entries[1];
        assert_eq!(copy["name"], "Copy Job");
        assert_eq!(copy["entries"], json!(["own"]));
        assert_eq!(copy["skillProficiencies"], json!([{"stealth": true}]));
        assert!(copy.get("srd").is_none());
        assert!(copy.get("page").is_none());
        assert!(copy.get("_copy").is_none());
    }

    #[test]
    fn preserve_keeps_named_props() {
        let r = resolve(
            "feat",
            json!([
                {"name": "A", "source": "X", "page": 4, "entries": []},
                {"name": "B", "source": "X", "_copy": {"name": "A", "source": "X", "_preserve": {"page": true}}}
            ]),
        );
        assert_eq!(r.entries[1]["page"], 4);
    }

    #[test]
    fn mods_text_arrays_and_props() {
        let r = resolve(
            "background",
            json!([
                {"name": "Base", "source": "S", "entries": [
                    {"name": "Feature: Contact", "type": "entries", "entries": ["a criminal contact"]},
                    "tail"], "tags": ["x"]},
                {"name": "Variant", "source": "S", "_copy": {"name": "Base", "source": "S", "_mod": {
                    "entries": [
                        {"mode": "replaceArr", "replace": "Feature: Contact",
                         "items": {"name": "Feature: Spy Contact", "type": "entries", "entries": ["a spy contact"]}},
                        {"mode": "appendArr", "items": "extra"},
                        {"mode": "prependArr", "items": ["first"]}
                    ],
                    "tags": {"mode": "appendIfNotExistsArr", "items": ["x", "y"]},
                    "*": {"mode": "replaceTxt", "replace": "tail", "with": "end", "flags": "g"}
                }}}
            ]),
        );
        let v = &r.entries[1];
        let entries = v["entries"].as_array().unwrap();
        assert_eq!(entries[0], "first");
        assert_eq!(entries[1]["name"], "Feature: Spy Contact");
        assert_eq!(entries[2], "end");
        assert_eq!(entries[3], "extra");
        assert_eq!(v["tags"], json!(["x", "y"]));
        // base is untouched
        assert_eq!(r.entries[0]["entries"][1], "tail");
    }

    #[test]
    fn more_modes() {
        let r = resolve(
            "item",
            json!([
                {"name": "Orb", "source": "S", "entries": ["a", "b", "c"], "weight": 2,
                 "stats": {"hp": 10}, "tags": ["p", "q"]},
                {"name": "Big Orb", "source": "S", "_copy": {"name": "Orb", "source": "S", "_mod": {
                    "*": [{"mode": "replaceName", "replace": "Orb", "with": "Greater Orb"},
                          {"mode": "setProp", "prop": "rarity", "value": "rare"}],
                    "entries": [{"mode": "insertArr", "index": 1, "items": ["ins"]},
                                {"mode": "removeArr", "items": ["c"]}],
                    "tags": [{"mode": "renameArr", "renames": []}, {"mode": "removeArr", "items": "p"}],
                    "stats": [{"mode": "scalarMultProp", "prop": "hp", "scalar": 2},
                              {"mode": "setProp", "prop": "ac", "value": 15}],
                    "text": {"mode": "appendStr", "str": "x"},
                    "weird": {"mode": "calculateProp", "prop": "a", "formula": "1"}
                }}}
            ]),
        );
        let v = &r.entries[1];
        assert_eq!(v["name"], "Big Greater Orb");
        assert_eq!(v["rarity"], "rare");
        assert_eq!(v["entries"], json!(["a", "ins", "b"]));
        assert_eq!(v["tags"], json!(["q"]));
        assert_eq!(v["stats"], json!({"hp": 20, "ac": 15}));
        assert!(r.notes.iter().any(|n| n.contains("calculateProp")));
    }

    #[test]
    fn missing_base_and_cycles_fail_softly() {
        let r = resolve(
            "feat",
            json!([
                {"name": "Orphan", "source": "S", "_copy": {"name": "Nope", "source": "S"}},
                {"name": "A", "source": "S", "_copy": {"name": "B", "source": "S"}},
                {"name": "B", "source": "S", "_copy": {"name": "A", "source": "S"}},
                {"name": "Fine", "source": "S", "entries": []}
            ]),
        );
        assert_eq!(r.entries.len(), 1);
        assert_eq!(r.failed.len(), 3);
        assert!(r.failed[0].1.contains("not found"));
        assert_eq!(r.failed_origins, vec![0, 1, 2]);
        assert_eq!(r.origins, vec![3]);
        assert!(r.failed.iter().any(|(_, why)| why.contains("circular")));
    }

    #[test]
    fn copy_chains_resolve_in_any_order() {
        let r = resolve(
            "race",
            json!([
                {"name": "C", "source": "S", "_copy": {"name": "B", "source": "S"}},
                {"name": "B", "source": "S", "speed": 30, "_copy": {"name": "A", "source": "S"}},
                {"name": "A", "source": "S", "size": ["M"], "speed": 25}
            ]),
        );
        assert_eq!(r.entries[0]["size"], json!(["M"]));
        assert_eq!(r.entries[0]["speed"], 30);
    }

    #[test]
    fn subclass_features_are_keyed_by_class_and_level() {
        let r = resolve(
            "subclassFeature",
            json!([
                {"name": "Edge", "source": "S", "className": "Rogue", "classSource": "S",
                 "subclassShortName": "Ghost", "subclassSource": "S", "level": 3, "entries": ["orig"]},
                {"name": "Edge", "source": "S", "className": "Rogue", "classSource": "S",
                 "subclassShortName": "Wraith", "subclassSource": "S", "level": 3,
                 "_copy": {"name": "Edge", "source": "S", "className": "Rogue", "classSource": "S",
                           "subclassShortName": "Ghost", "subclassSource": "S", "level": 3}}
            ]),
        );
        assert!(r.failed.is_empty());
        assert_eq!(r.entries[1]["entries"], json!(["orig"]));
        assert_eq!(r.entries[1]["subclassShortName"], "Wraith");
    }

    #[test]
    fn versions_expand_plain_and_abstract() {
        let r = resolve(
            "race",
            json!([{
                "name": "Dragonkin", "source": "S", "srd": true, "entries": ["Breath: {{type}} fire"],
                "resist": ["fire"],
                "_versions": [
                    {"name": "Dragonkin (Red)", "_mod": {"resist": {"mode": "appendArr", "items": "cold"}}},
                    {"_abstract": {"name": "Dragonkin ({{color}})", "_mod": {"entries": {"mode": "replaceTxt", "replace": "fire", "with": "{{dmg}}"}}},
                     "_implementations": [
                        {"_variables": {"color": "Blue", "dmg": "lightning"}},
                        {"_variables": {"color": "White", "dmg": "cold"}}]}
                ]
            }]),
        );
        assert_eq!(r.entries.len(), 4);
        assert!(r.entries[0].get("_versions").is_none());
        assert_eq!(r.entries[1]["name"], "Dragonkin (Red)");
        assert_eq!(r.entries[1]["resist"], json!(["fire", "cold"]));
        assert_eq!(r.entries[2]["name"], "Dragonkin (Blue)");
        assert_eq!(r.entries[2]["entries"][0], "Breath: {{type}} lightning");
        assert_eq!(r.entries[3]["name"], "Dragonkin (White)");
        // versions keep the parent's srd flag (they are part of the same published text)
        assert_eq!(r.entries[3]["srd"], true);
        assert_eq!(r.origins, vec![0, 0, 0, 0]);
    }

    #[test]
    fn null_in_a_copy_or_version_removes_the_inherited_property() {
        let r = resolve(
            "race",
            json!([
                {"name": "Scaled", "source": "S", "resist": ["fire"], "darkvision": 60,
                 "additionalSpells": [{"known": {"1": ["light"]}}],
                 "_versions": [{"name": "Scaled; Plain", "additionalSpells": null}]},
                {"name": "Copied", "source": "T", "_copy": {"name": "Scaled", "source": "S"},
                 "resist": null}
            ]),
        );
        assert!(r.failed.is_empty());
        assert_eq!(r.entries[1]["name"], "Scaled; Plain");
        assert!(r.entries[1].get("additionalSpells").is_none());
        assert_eq!(r.entries[1]["resist"], json!(["fire"]));
        let copied = r.entries.iter().find(|e| e["name"] == "Copied").unwrap();
        assert!(copied.get("resist").is_none());
        assert_eq!(copied["darkvision"], 60);
    }

    #[test]
    fn magic_variant_copies_use_inherited_sources_and_dotted_mods() {
        let r = resolve(
            "magicvariant",
            json!([
                {"name": "Ward Weapon", "type": "GV|AAA", "inherits": {"source": "AAA",
                 "nameSuffix": " of Ward", "rarity": "uncommon", "entries": ["old"]}},
                {"name": "Ward Weapon", "type": "GV|BBB", "inherits": {"source": "BBB",
                 "nameSuffix": " of Ward", "rarity": "rare", "lootTables": ["T"],
                 "entries": ["new"]}},
                {"name": "Keen Ward Weapon", "source": "CCC", "_copy": {
                    "name": "Ward Weapon", "source": "BBB", "_mod": {
                        "inherits.entries": {"mode": "appendArr", "items": "more"},
                        "inherits.namePrefix": {"mode": "setProp", "value": "Keen "},
                        "inherits.lootTables": "remove"}}}
            ]),
        );
        assert!(r.failed.is_empty(), "{:?}", r.failed);
        let keen = &r.entries[2];
        assert_eq!(keen["inherits"]["rarity"], "rare");
        assert_eq!(keen["inherits"]["entries"], json!(["new", "more"]));
        assert_eq!(keen["inherits"]["namePrefix"], "Keen ");
        assert!(keen["inherits"].get("lootTables").is_none());
    }

    #[test]
    fn regex_flags_and_props_filter() {
        let r = resolve(
            "feat",
            json!([
                {"name": "A", "source": "S", "entries": ["Cat sat. cat sat."], "name2": "cat"},
                {"name": "B", "source": "S", "_copy": {"name": "A", "source": "S", "_mod": {
                    "entries": {"mode": "replaceTxt", "replace": "cat", "with": "dog", "flags": "gi"}}}},
                {"name": "C", "source": "S", "_copy": {"name": "A", "source": "S", "_mod": {
                    "entries": {"mode": "replaceTxt", "replace": "cat", "with": "dog", "flags": "i"}}}}
            ]),
        );
        assert_eq!(r.entries[1]["entries"][0], "dog sat. dog sat.");
        assert_eq!(r.entries[2]["entries"][0], "dog sat. cat sat.");
        assert_eq!(r.entries[1]["name2"], "cat");
    }

    #[test]
    fn identity_key_and_display() {
        let e = json!({"name": "X", "source": "Y", "level": 3});
        assert_eq!(identity_key("classFeature", &e), "x|y|||3");
        assert_eq!(display_name(&e), "X|Y");
        assert_eq!(
            display_name(&json!({"_copy": {"name": "Z"}, "source": "Q"})),
            "Z|Q"
        );
    }
}
