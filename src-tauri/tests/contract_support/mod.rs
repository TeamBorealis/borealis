//! Shared helpers for the contract tests: the generated contract file, a structural shape checker
//! and source parsing for the Tauri invoke handler and command signatures.

#![allow(dead_code)]

use serde_json::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;

fn manifest() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn camel_to_snake(s: &str) -> String {
    let mut out = String::new();
    for ch in s.chars() {
        if ch.is_ascii_uppercase() {
            out.push('_');
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

/// Command names registered in `tauri::generate_handler!`.
pub fn rust_commands() -> Vec<String> {
    let src = std::fs::read_to_string(manifest().join("src/lib.rs")).expect("lib.rs");
    let start = src.find("generate_handler![").expect("handler") + "generate_handler![".len();
    let end = start + src[start..].find("])").expect("end of handler");
    src[start..end]
        .lines()
        .map(str::trim)
        .filter(|l| !l.starts_with('#') && !l.starts_with("//") && l.ends_with(','))
        .map(|l| {
            l.trim_end_matches(',')
                .rsplit("::")
                .next()
                .unwrap()
                .to_string()
        })
        .collect()
}

/// For each `#[tauri::command]`, its invoke parameters (snake_case name, is `Option`).
pub fn rust_params() -> BTreeMap<String, Vec<(String, bool)>> {
    let mut out = BTreeMap::new();
    let mut files: Vec<_> = std::fs::read_dir(manifest().join("src"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("commands") && n.ends_with(".rs"))
        })
        .collect();
    files.sort();
    for f in files {
        let src = std::fs::read_to_string(&f).unwrap();
        let mut rest = src.as_str();
        while let Some(at) = rest.find("#[tauri::command") {
            rest = &rest[at..];
            let fn_at = rest.find(" fn ").expect("fn after attribute");
            let after = &rest[fn_at + " fn ".len()..];
            let name_end = after.find('(').unwrap();
            let name = after[..name_end].trim().to_string();
            let mut depth = 0;
            let mut params = String::new();
            for ch in after[name_end..].chars() {
                match ch {
                    '(' => depth += 1,
                    ')' => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    _ => {}
                }
                if depth > 0 {
                    params.push(ch);
                }
            }
            out.insert(name, split_params(&params[1..]));
            rest = &rest[fn_at + 1..];
        }
    }
    out
}

fn split_params(params: &str) -> Vec<(String, bool)> {
    let mut parts = Vec::new();
    let (mut depth, mut cur) = (0i32, String::new());
    for ch in params.chars() {
        match ch {
            '<' | '(' | '[' => depth += 1,
            '>' | ')' | ']' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(std::mem::take(&mut cur));
                continue;
            }
            _ => {}
        }
        cur.push(ch);
    }
    parts.push(cur);
    parts
        .iter()
        .filter_map(|p| p.split_once(':'))
        .map(|(n, t)| (n.trim().to_string(), t.trim().to_string()))
        .filter(|(_, t)| {
            !(t.starts_with("S<")
                || t.starts_with("A<")
                || t.starts_with("tauri::")
                || t.starts_with("State<")
                || t.starts_with("AppHandle"))
        })
        .map(|(n, t)| (n, t.starts_with("Option<")))
        .collect()
}

/// The generated contract.
pub struct Contract(Value);

impl Contract {
    pub fn load() -> Self {
        let path = manifest().join("../ui/src/api/generated/contract.json");
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
            panic!("{}: {e} (run `npm run gen:contract` in ui)", path.display())
        });
        Self(serde_json::from_str(&text).expect("contract.json"))
    }

    pub fn command_names(&self) -> Vec<String> {
        self.0["commands"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect()
    }

    /// Argument keys (snake_case) and whether each is optional in the TS signature.
    pub fn args(&self, command: &str) -> Vec<(String, bool)> {
        self.0["commands"][command]["args"]
            .as_object()
            .unwrap_or_else(|| panic!("unknown command {command}"))
            .iter()
            .map(|(k, v)| (camel_to_snake(k), v.as_bool().unwrap()))
            .collect()
    }

    /// Check a command's real response against the TS type.
    pub fn response(&self, command: &str, value: &Value) -> Vec<String> {
        let shape = &self.0["commands"][command]["response"];
        assert!(!shape.is_null(), "unknown command {command}");
        let mut errs = Vec::new();
        self.check(shape, value, command, &mut errs);
        errs
    }

    /// Check an event payload against `ApiEvents`.
    pub fn event(&self, event: &str, value: &Value) -> Vec<String> {
        let shape = &self.0["events"][event];
        assert!(!shape.is_null(), "unknown event {event}");
        let mut errs = Vec::new();
        self.check(shape, value, event, &mut errs);
        errs
    }

    /// Check a value against a named TS type (`defs[name]`).
    pub fn def(&self, name: &str, value: &Value) -> Vec<String> {
        let shape = &self.0["defs"][name];
        assert!(!shape.is_null(), "unknown TS type {name}");
        let mut errs = Vec::new();
        self.check(shape, value, name, &mut errs);
        errs
    }

    /// The property names of a named TS type.
    pub fn def_keys(&self, name: &str) -> Vec<String> {
        self.0["defs"][name]["props"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect()
    }

    fn check(&self, shape: &Value, v: &Value, path: &str, errs: &mut Vec<String>) {
        match shape["k"].as_str().unwrap_or("any") {
            "ref" => self.check(
                &self.0["defs"][shape["name"].as_str().unwrap()],
                v,
                path,
                errs,
            ),
            "object" => self.check_object(shape, v, path, errs),
            "array" => match v {
                Value::Array(items) => items
                    .iter()
                    .enumerate()
                    .for_each(|(i, x)| self.check(&shape["t"], x, &format!("{path}[{i}]"), errs)),
                Value::Null => {}
                other => errs.push(format!("{path}: expected array, got {}", kind(other))),
            },
            "record" => match v {
                Value::Object(m) => m
                    .iter()
                    .for_each(|(k, x)| self.check(&shape["t"], x, &format!("{path}.{k}"), errs)),
                Value::Null => {}
                other => errs.push(format!("{path}: expected map, got {}", kind(other))),
            },
            "prim" => {
                if matches!(v, Value::Object(_) | Value::Array(_)) {
                    errs.push(format!("{path}: expected a scalar, got {}", kind(v)));
                }
            }
            "union" => {
                if v.is_null() {
                    return;
                }
                let mut best: Option<Vec<String>> = None;
                for variant in shape["variants"].as_array().unwrap() {
                    let mut e = Vec::new();
                    self.check(variant, v, path, &mut e);
                    if e.is_empty() {
                        return;
                    }
                    if best.as_ref().map_or(true, |b| e.len() < b.len()) {
                        best = Some(e);
                    }
                }
                errs.extend(best.unwrap_or_default());
            }
            _ => {}
        }
    }

    fn check_object(&self, shape: &Value, v: &Value, path: &str, errs: &mut Vec<String>) {
        let Value::Object(map) = v else {
            if !v.is_null() {
                errs.push(format!("{path}: expected object, got {}", kind(v)));
            }
            return;
        };
        let props = shape["props"].as_object().unwrap();
        let open = shape["open"].as_bool().unwrap_or(false);
        for (k, x) in map {
            match props.get(k) {
                Some(p) => self.check(&p["t"], x, &format!("{path}.{k}"), errs),
                None if open => {}
                None => errs.push(format!(
                    "{path}: Rust sends `{k}` which the TS type does not declare"
                )),
            }
        }
        for (k, p) in props {
            if !p["opt"].as_bool().unwrap_or(false) && !map.contains_key(k) {
                errs.push(format!("{path}: TS requires `{k}` but Rust omitted it"));
            }
        }
    }
}

fn kind(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}
