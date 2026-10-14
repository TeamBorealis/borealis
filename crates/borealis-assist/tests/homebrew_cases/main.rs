//! Homebrew acceptance cases: judge an "import anything" result against expectations written
//! before the import ran.
//!
//! Real cases are third-party material and never live in this repository. Point
//! `BOREALIS_HOMEBREW_CASES` at a directory of case folders (`<case>/expected.json` plus
//! `<case>/imported/`, the pack the import wrote) and run
//! `cargo test -p borealis-assist --test homebrew_cases -- --nocapture`.
//! Unset, the env test is skipped; the synthetic case below always runs.

mod character;
mod pack;
mod schema;

use std::io::Write as _;
use std::path::{Path, PathBuf};

use borealis_engine::Engine;
use serde_json::Value;

use schema::{CaseFile, Level, Mech, SCHEMA};

/// Env var naming the directory of case folders.
pub const ENV: &str = "BOREALIS_HOMEBREW_CASES";
/// Folder (inside a case) holding the imported pack.
pub const IMPORTED_DIR: &str = "imported";

/// One evaluated expectation.
#[derive(Debug, Clone)]
pub struct Row {
    pub id: String,
    pub level: Level,
    pub class: Mech,
    pub pass: bool,
    pub detail: String,
}

/// The result of one case.
#[derive(Debug)]
pub struct CaseReport {
    pub case: String,
    /// Set when the case could not be evaluated at all (it fails).
    pub fatal: Option<String>,
    pub rows: Vec<Row>,
}

impl CaseReport {
    fn failed_musts(&self) -> Vec<&Row> {
        self.rows
            .iter()
            .filter(|r| !r.pass && r.level == Level::Must)
            .collect()
    }

    pub fn passed(&self) -> bool {
        self.fatal.is_none() && self.failed_musts().is_empty()
    }

    /// The per-expectation table.
    pub fn table(&self) -> String {
        let mut out = format!("== case {} ==\n", self.case);
        if let Some(f) = &self.fatal {
            out.push_str(&format!("FAIL  (case)  {f}\n"));
            return out;
        }
        for r in &self.rows {
            let verdict = if r.pass { "PASS" } else { "FAIL" };
            let level = match r.level {
                Level::Must => "must",
                Level::Should => "should",
            };
            out.push_str(&format!(
                "{verdict}  {level:<6} {:<5} {}",
                r.class.tag(),
                r.id
            ));
            if !r.pass {
                out.push_str(&format!("  -- {}", r.detail));
            }
            out.push('\n');
        }
        let count = |f: &dyn Fn(&Row) -> bool| self.rows.iter().filter(|r| f(r)).count();
        out.push_str(&format!(
            "summary: {} rows, must {}/{} pass, should {}/{} pass\n",
            self.rows.len(),
            count(&|r| r.pass && r.level == Level::Must),
            count(&|r| r.level == Level::Must),
            count(&|r| r.pass && r.level == Level::Should),
            count(&|r| r.level == Level::Should),
        ));
        out
    }
}

/// Bundled content root (`app/content`).
pub fn content_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content")
}

const HEAD_KEYS: [&str; 4] = ["id", "level", "class", "note"];

fn char_check_keys(check: &str) -> Option<&'static [&'static str]> {
    Some(match check {
        "clean" => &[],
        "hit_points" | "proficiency_bonus" => &["value"],
        "save" => &["ability", "proficient", "total"],
        "skill" => &["skill", "proficient", "expertise", "total"],
        "initiative" => &["total", "advantage"],
        "features" => &["present", "absent"],
        "granted" => &["ids", "absent_ids"],
        "resource" => &["name", "max"],
        "save_dc" => &["name", "value"],
        "choice" => &[
            "name_contains",
            "type",
            "owner",
            "options_include_ids",
            "options_include_names",
            "count",
            "options",
            "min_options",
        ],
        "attack" => &["equip", "attack", "damage", "range", "proficient"],
        _ => return None,
    })
}

const ELEMENT_KEYS: [&str; 8] = [
    "type",
    "name",
    "setters",
    "supports_names",
    "supports_ids",
    "rules_include",
    "json",
    "text_contains",
];

fn unknown_keys(v: &Value, allowed: &[&str], at: &str, out: &mut Vec<String>) {
    if let Some(o) = v.as_object() {
        for k in o.keys() {
            if !HEAD_KEYS.contains(&k.as_str()) && !allowed.contains(&k.as_str()) {
                out.push(format!("{at}: unknown key {k:?}"));
            }
        }
    }
}

/// serde `flatten` cannot deny unknown fields, so typos are caught here.
fn lint_keys(raw: &Value) -> Vec<String> {
    let mut out = Vec::new();
    let list = |k: &str| {
        raw.get(k)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    };
    for (i, e) in list("elements").iter().enumerate() {
        unknown_keys(e, &ELEMENT_KEYS, &format!("elements[{i}]"), &mut out);
    }
    for (i, e) in list("mentions").iter().enumerate() {
        unknown_keys(e, &["text"], &format!("mentions[{i}]"), &mut out);
    }
    for c in list("characters") {
        let cid = c
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("?")
            .to_string();
        let exp = c
            .get("expect")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        for (i, x) in exp.iter().enumerate() {
            let check = x.get("check").and_then(Value::as_str).unwrap_or("");
            match char_check_keys(check) {
                Some(keys) => {
                    let mut allowed = keys.to_vec();
                    allowed.push("check");
                    unknown_keys(x, &allowed, &format!("{cid}.expect[{i}]"), &mut out);
                }
                None => out.push(format!("{cid}.expect[{i}]: unknown check {check:?}")),
            }
        }
    }
    out
}

/// Read and validate `expected.json`.
pub fn read_case(path: &Path) -> Result<CaseFile, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let raw: Value = serde_json::from_str(&text).map_err(|e| format!("expected.json: {e}"))?;
    let lint = lint_keys(&raw);
    if !lint.is_empty() {
        return Err(format!("expected.json: {}", lint.join("; ")));
    }
    let case: CaseFile = serde_json::from_value(raw).map_err(|e| format!("expected.json: {e}"))?;
    if case.schema != SCHEMA {
        return Err(format!(
            "expected.json schema {}, this harness reads {SCHEMA}",
            case.schema
        ));
    }
    let mut seen = std::collections::HashSet::new();
    let ids = case
        .elements
        .iter()
        .map(|x| x.head.id.clone())
        .chain(case.mentions.iter().map(|x| x.head.id.clone()))
        .chain(case.characters.iter().flat_map(|c| {
            c.expect
                .iter()
                .map(move |x| format!("{}/{}", c.id, x.head.id))
        }));
    for id in ids {
        if !seen.insert(id.clone()) {
            return Err(format!("expected.json: duplicate expectation id {id}"));
        }
    }
    Ok(case)
}

/// Evaluate one case folder.
pub fn run_case(case_dir: &Path) -> CaseReport {
    let name = case_dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let fatal = |m: String| CaseReport {
        case: name.clone(),
        fatal: Some(m),
        rows: Vec::new(),
    };
    let case = match read_case(&case_dir.join("expected.json")) {
        Ok(c) => c,
        Err(e) => return fatal(e),
    };
    let loaded = match pack::load(&content_root(), &case, case_dir) {
        Ok(l) => l,
        Err(e) => return fatal(e),
    };
    run_loaded(&name, &case, &loaded)
}

/// Evaluate a case whose pack is already loaded.
pub fn run_loaded(name: &str, case: &CaseFile, loaded: &pack::Loaded) -> CaseReport {
    let mut rows = pack::builtin_rows(case, loaded);
    rows.extend(pack::element_rows(case, loaded));
    let engine = Engine::new(&loaded.store);
    for c in &case.characters {
        rows.extend(character::rows(&engine, loaded, case.ruleset, c));
    }
    CaseReport {
        case: name.to_string(),
        fatal: None,
        rows,
    }
}

/// Every case folder (a directory holding `expected.json`) under `root`, sorted.
pub fn case_dirs(root: &Path) -> Result<Vec<PathBuf>, String> {
    let rd = std::fs::read_dir(root).map_err(|e| format!("{}: {e}", root.display()))?;
    let mut dirs: Vec<PathBuf> = rd
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.join("expected.json").is_file())
        .collect();
    dirs.sort();
    Ok(dirs)
}

fn loud(text: &str) {
    // Written to the process stderr directly so libtest does not capture it.
    let _ = writeln!(std::io::stderr(), "{text}");
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/homebrew_cases")
        .join(name)
}

#[test]
fn homebrew_cases_from_env() {
    let Some(root) = std::env::var_os(ENV) else {
        loud(&format!(
            "\n**** homebrew_cases: SKIPPED (local-only cases) -- set {ENV} to a directory of \
             case folders to run them ****\n"
        ));
        return;
    };
    let dirs = case_dirs(Path::new(&root)).unwrap_or_else(|e| panic!("{ENV}: {e}"));
    assert!(
        !dirs.is_empty(),
        "{ENV}: no case folder with an expected.json"
    );
    let mut failed = Vec::new();
    for d in &dirs {
        let r = run_case(d);
        loud(&r.table());
        if !r.passed() {
            let why = r.fatal.clone().unwrap_or_else(|| {
                let ids: Vec<&str> = r.failed_musts().iter().map(|x| x.id.as_str()).collect();
                format!("{} must expectations fail: {}", ids.len(), ids.join(", "))
            });
            failed.push(format!("{}: {why}", r.case));
        }
    }
    assert!(
        failed.is_empty(),
        "homebrew cases failed:\n{}",
        failed.join("\n")
    );
}

#[test]
fn synthetic_case_passes() {
    let r = run_case(&fixture("synthetic-lanternwarden"));
    let table = r.table();
    assert!(r.fatal.is_none(), "{table}");
    let failed: Vec<&Row> = r.rows.iter().filter(|x| !x.pass).collect();
    assert!(failed.is_empty(), "{table}");
    assert!(r.rows.len() > 30, "{table}");
}

/// A copy of the synthetic pack with one wrong number fails exactly the expectations that
/// look at that number.
#[test]
fn mutated_synthetic_case_fails_on_the_right_expectations() {
    let dir = fixture("synthetic-lanternwarden");
    let case = read_case(&dir.join("expected.json")).expect("case");
    let mut loaded = pack::load(&content_root(), &case, &dir).expect("pack");
    let flail = loaded
        .elements
        .iter_mut()
        .find(|e| e.name == "Lantern Flail")
        .expect("flail");
    let dmg = flail
        .setters
        .iter_mut()
        .find(|s| s.name == "damage")
        .expect("damage setter");
    dmg.value = "1d6".into();
    // Rebuild the store from the mutated element so the engine sees it too.
    loaded = pack::reload_with(&content_root(), &case, loaded).expect("reload");
    let r = run_loaded("synthetic-mutated", &case, &loaded);
    let mut failing: Vec<&str> = r
        .rows
        .iter()
        .filter(|x| !x.pass)
        .map(|x| x.id.as_str())
        .collect();
    failing.sort_unstable();
    assert_eq!(
        failing,
        [
            "el.weapon.lantern_flail",
            "lw-l3/attack.flail",
            "lw-l5/attack.flail_adorned"
        ],
        "{}",
        r.table()
    );
    assert!(!r.passed());
}

#[test]
fn case_without_imported_pack_fails_clearly() {
    let tmp = std::env::temp_dir().join(format!("borealis-hc-{}", std::process::id()));
    let case_dir = tmp.join("no-import-yet");
    std::fs::create_dir_all(&case_dir).expect("tmp");
    std::fs::copy(
        fixture("synthetic-lanternwarden").join("expected.json"),
        case_dir.join("expected.json"),
    )
    .expect("copy");
    let dirs = case_dirs(&tmp).expect("dirs");
    let r = run_case(&dirs[0]);
    let _ = std::fs::remove_dir_all(&tmp);
    assert!(!r.passed());
    let fatal = r.fatal.clone().expect("fatal");
    assert!(fatal.contains("no imported pack"), "{fatal}");
    assert!(r.table().contains("FAIL  (case)"));
}

#[test]
fn typos_in_expected_json_are_refused() {
    let raw = serde_json::json!({
        "elements": [{"id": "x", "level": "must", "class": "mechanised", "type": "Class",
                      "name": "X", "setter": {}}],
        "characters": [{"id": "c", "expect": [
            {"id": "a", "level": "must", "class": "mechanised", "check": "hit_points", "valeu": 3},
            {"id": "b", "level": "must", "class": "mechanised", "check": "hp"}
        ]}]
    });
    let lint = lint_keys(&raw);
    assert_eq!(lint.len(), 3, "{lint:?}");
}
