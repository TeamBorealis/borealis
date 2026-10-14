//! Homebrew acceptance cases: judge an "import anything" result against expectations written
//! before the import ran.
//!
//! Real cases are third-party material and never live in this repository. Point
//! `BOREALIS_HOMEBREW_CASES` at a directory of case folders (`<case>/expected.json` plus
//! `<case>/imported/`, the pack the import wrote) and run
//! `cargo test -p borealis-assist --test homebrew_cases -- --nocapture`.
//! Unset, the env test is skipped; the synthetic case below always runs.

mod character;
mod checks;
mod pack;
mod schema;

use std::io::Write as _;
use std::path::{Path, PathBuf};

use borealis_engine::Engine;
use serde_json::Value;

use schema::{CaseFile, Level, Mech, SCHEMAS};

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
        "skill" => &["skill", "proficient", "expertise", "total", "advantage"],
        "initiative" => &["total", "advantage"],
        "features" => &["present", "absent"],
        "granted" => &["ids", "absent_ids", "names", "absent_names"],
        "resource" => &["name", "max", "absent"],
        "save_dc" => &["name", "value"],
        "choice" => &[
            "name_contains",
            "type",
            "owner",
            "options_include_ids",
            "options_include_names",
            "options_exclude_ids",
            "options_exclude_names",
            "count",
            "options",
            "min_options",
        ],
        "attack" => &["equip", "attack", "damage", "range", "proficient"],
        "ability_scores" | "size" | "armor_class" => &["value"],
        "speed" => &["walk", "climb", "swim", "fly", "burrow"],
        "vision" => &["darkvision", "blindsight", "tremorsense", "truesight"],
        "languages" => &["ids", "names", "text", "absent_ids"],
        "spellcasting" => &[
            "name",
            "present",
            "ability",
            "dc",
            "attack",
            "prepare_limit",
            "slots",
            "no_slots",
            "cantrips",
            "spells_include_ids",
            "spells_include_names",
            "spells_exclude_ids",
            "spells_exclude_names",
        ],
        "defenses" => &[
            "resistances",
            "immunities",
            "vulnerabilities",
            "condition_immunities",
            "absent",
        ],
        "advantage" => &["roll", "text", "absent"],
        "attunement" => &["attuned", "limit"],
        "item" => &["equip", "equipped", "attuned"],
        "any_of" => &["any_of"],
        _ => return None,
    })
}

const ELEMENT_KEYS: [&str; 9] = [
    "setters_contain",
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

/// Lint one character check (and the alternatives of an `any_of`).
fn lint_check(x: &Value, at: &str, out: &mut Vec<String>) {
    let check = x.get("check").and_then(Value::as_str).unwrap_or("");
    let Some(keys) = char_check_keys(check) else {
        out.push(format!("{at}: unknown check {check:?}"));
        return;
    };
    let mut allowed = keys.to_vec();
    allowed.push("check");
    unknown_keys(x, &allowed, at, out);
    if let Some(alts) = x.get("any_of").and_then(Value::as_array) {
        for (i, a) in alts.iter().enumerate() {
            lint_check(a, &format!("{at}.any_of[{i}]"), out);
        }
    }
}

const COUNT_KEYS: [&str; 6] = ["type", "setters", "supports_names", "equals", "min", "max"];

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
    for (i, e) in list("counts").iter().enumerate() {
        unknown_keys(e, &COUNT_KEYS, &format!("counts[{i}]"), &mut out);
    }
    for (i, e) in list("absent").iter().enumerate() {
        unknown_keys(e, &["type", "name"], &format!("absent[{i}]"), &mut out);
    }
    for (i, e) in list("report").iter().enumerate() {
        unknown_keys(e, &["source", "outcome"], &format!("report[{i}]"), &mut out);
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
            lint_check(x, &format!("{cid}.expect[{i}]"), &mut out);
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
    if !SCHEMAS.contains(&case.schema) {
        return Err(format!(
            "expected.json schema {}, this harness reads {SCHEMAS:?}",
            case.schema
        ));
    }
    let mut seen = std::collections::HashSet::new();
    let ids = case
        .elements
        .iter()
        .map(|x| x.head.id.clone())
        .chain(case.mentions.iter().map(|x| x.head.id.clone()))
        .chain(case.counts.iter().map(|x| x.head.id.clone()))
        .chain(case.absent.iter().map(|x| x.head.id.clone()))
        .chain(case.report.iter().map(|x| x.head.id.clone()))
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

/// Env var set on the child process that judges one case (see [`run_case_isolated`]).
const ONE_CASE_ENV: &str = "BOREALIS_HOMEBREW_ONE_CASE";
/// Stack of the thread a case runs on: deep content must not abort a whole run.
const CASE_STACK_BYTES: usize = 512 * 1024 * 1024;
/// Prefix of the child's one-line verdict on stderr.
const VERDICT: &str = "homebrew-case-verdict: ";
/// Harness self-test only: a case folder holding this file aborts its child process.
const ABORT_MARKER: &str = ".abort-for-harness-test";

/// Run one case on a thread with a large stack; a panic becomes the case's fatal failure.
fn run_case_guarded(dir: &Path) -> CaseReport {
    let owned = dir.to_path_buf();
    let joined = std::thread::Builder::new()
        .name("homebrew-case".into())
        .stack_size(CASE_STACK_BYTES)
        .spawn(move || run_case(&owned))
        .map_err(|e| format!("could not start the case thread: {e}"))
        .and_then(|h| {
            h.join().map_err(|p| {
                let msg = p
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| p.downcast_ref::<&str>().map(|s| (*s).to_string()))
                    .unwrap_or_else(|| "unknown panic".into());
                format!("the case panicked: {msg}")
            })
        });
    joined.unwrap_or_else(|e| CaseReport {
        case: dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        fatal: Some(e),
        rows: Vec::new(),
    })
}

/// Why a report failed, or None when it passed.
fn failure(r: &CaseReport) -> Option<String> {
    if r.passed() {
        return None;
    }
    Some(r.fatal.clone().unwrap_or_else(|| {
        let ids: Vec<&str> = r.failed_musts().iter().map(|x| x.id.as_str()).collect();
        format!("{} must expectations fail: {}", ids.len(), ids.join(", "))
    }))
}

/// Judge one case in a child process (this test binary, [`homebrew_one_case`]), so a case that
/// crashes the process (a stack overflow aborts) fails alone instead of ending the whole run.
/// Returns why it failed, or None.
fn run_case_isolated(dir: &Path) -> Option<String> {
    let exe = match std::env::current_exe() {
        Ok(e) => e,
        Err(e) => return Some(format!("cannot find the test binary: {e}")),
    };
    let out = std::process::Command::new(exe)
        .args([
            "--exact",
            "homebrew_one_case",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(ONE_CASE_ENV, dir)
        .output();
    let out = match out {
        Ok(o) => o,
        Err(e) => return Some(format!("cannot run the case process: {e}")),
    };
    let err = String::from_utf8_lossy(&out.stderr);
    let mut verdict = None;
    for line in err.lines() {
        match line.strip_prefix(VERDICT) {
            Some(v) => verdict = Some(v.to_string()),
            None => loud(line),
        }
    }
    match (out.status.success(), verdict) {
        (true, _) => None,
        (false, Some(v)) => Some(v),
        (false, None) => {
            let tail: Vec<&str> = err.lines().rev().take(3).collect();
            Some(format!(
                "the case process crashed ({}): {}",
                out.status,
                tail.into_iter().rev().collect::<Vec<_>>().join(" | ")
            ))
        }
    }
}

/// The child side of [`run_case_isolated`]: does nothing unless the parent named a case.
#[test]
fn homebrew_one_case() {
    let Some(dir) = std::env::var_os(ONE_CASE_ENV) else {
        return;
    };
    if Path::new(&dir).join(ABORT_MARKER).is_file() {
        std::process::abort();
    }
    let r = run_case_guarded(Path::new(&dir));
    loud(&r.table());
    if let Some(why) = failure(&r) {
        loud(&format!("{VERDICT}{}", why.replace('\n', " ")));
        panic!("{}: {why}", r.case);
    }
}

#[test]
fn homebrew_cases_from_env() {
    if std::env::var_os(ONE_CASE_ENV).is_some() {
        return;
    }
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
        if let Some(why) = run_case_isolated(d) {
            let name = d.file_name().map(|n| n.to_string_lossy().into_owned());
            failed.push(format!("{}: {why}", name.unwrap_or_default()));
        }
    }
    assert!(
        failed.is_empty(),
        "homebrew cases failed:\n{}",
        failed.join("\n")
    );
}

/// A case that crashes its process (as a stack overflow does) fails alone; the run goes on.
#[test]
fn a_crashing_case_fails_alone_and_others_still_run() {
    let tmp = std::env::temp_dir().join(format!("borealis-hc-abort-{}", std::process::id()));
    let crash = tmp.join("crashes");
    std::fs::create_dir_all(&crash).expect("tmp");
    std::fs::copy(
        fixture("synthetic-lanternwarden").join("expected.json"),
        crash.join("expected.json"),
    )
    .expect("copy");
    std::fs::write(crash.join(ABORT_MARKER), "").expect("marker");
    let crashed = run_case_isolated(&crash);
    let fine = run_case_isolated(&fixture("synthetic-lanternwarden"));
    let _ = std::fs::remove_dir_all(&tmp);
    let why = crashed.expect("the crashing case fails");
    assert!(why.contains("crashed"), "{why}");
    assert_eq!(fine, None, "the next case is still judged");
}

#[test]
fn a_panicking_case_thread_is_a_fatal_failure() {
    let r = run_case_guarded(Path::new("/nonexistent/borealis-homebrew-case"));
    assert!(!r.passed());
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
            {"id": "b", "level": "must", "class": "mechanised", "check": "hp"},
            {"id": "c", "level": "must", "class": "mechanised", "check": "any_of", "any_of": [
                {"check": "speed", "walk": 30},
                {"check": "speed", "wlak": 30}
            ]}
        ]}],
        "counts": [{"id": "n", "level": "must", "class": "mechanised", "type": "Spell", "equal": 3}],
        "absent": [{"id": "m", "level": "must", "class": "mechanised", "name": "X", "typ": "Y"}]
    });
    let lint = lint_keys(&raw);
    assert_eq!(lint.len(), 6, "{lint:?}");
}

/// Edit the loaded pack elements as JSON, then rebuild the store.
fn edit_pack(
    case: &CaseFile,
    loaded: pack::Loaded,
    edit: impl FnOnce(&mut Vec<Value>),
) -> pack::Loaded {
    let mut els: Vec<Value> = loaded
        .elements
        .iter()
        .map(|e| serde_json::to_value(e).expect("element json"))
        .collect();
    edit(&mut els);
    let elements = els
        .into_iter()
        .map(|v| serde_json::from_value(v).expect("element"))
        .collect();
    pack::reload_with(&content_root(), case, pack::Loaded { elements, ..loaded }).expect("reload")
}

fn element_mut<'a>(els: &'a mut [Value], name: &str) -> &'a mut Value {
    els.iter_mut()
        .find(|e| e["name"] == name)
        .unwrap_or_else(|| panic!("no element {name}"))
}

/// Replace the first rule of `name` matching `pick` (by JSON subset); `None` removes it.
fn set_rule(els: &mut [Value], name: &str, pick: Value, to: Option<Value>) {
    let rules = element_mut(els, name)["rules"]
        .as_array_mut()
        .expect("rules");
    let i = rules
        .iter()
        .position(|r| pack::subset(&pick, r))
        .unwrap_or_else(|| panic!("{name}: no rule like {pick}"));
    match to {
        Some(v) => rules[i] = v,
        None => {
            rules.remove(i);
        }
    }
}

fn set_setter(els: &mut [Value], name: &str, setter: &str, value: &str) {
    let setters = element_mut(els, name)["setters"]
        .as_array_mut()
        .expect("setters");
    let s = setters
        .iter_mut()
        .find(|s| s["name"] == setter)
        .unwrap_or_else(|| panic!("{name}: no setter {setter}"));
    s["value"] = Value::from(value);
}

/// Every schema-2 expectation type fails when the pack is wrong in the way it checks.
#[test]
fn schema2_expectations_fail_on_a_mutated_pack() {
    use serde_json::json;
    let dir = fixture("synthetic-lanternwarden");
    let case = read_case(&dir.join("expected.json")).expect("case");
    let mut loaded = pack::load(&content_root(), &case, &dir).expect("pack");
    if let Some(r) = loaded.report.as_mut() {
        for e in r.entries.iter_mut().filter(|e| e.outcome == "unsupported") {
            e.outcome = "skipped".into();
        }
    }
    let loaded = edit_pack(&case, loaded, |els| {
        let race = "Wickfolk";
        let stat = |n: &str, v: &str| json!({"kind": "stat", "name": n, "value": v});
        let grant = |id: &str, ty: &str| json!({"kind": "grant", "id": id, "type": ty});
        set_rule(
            els,
            race,
            stat("dexterity", "2"),
            Some(stat("dexterity", "1")),
        );
        set_rule(els, race, stat("innate speed:climb", "20"), None);
        let size = grant("ID_SIZE_MEDIUM", "Size");
        set_rule(els, race, json!({"id": "ID_SIZE_SMALL"}), Some(size));
        set_rule(els, race, json!({"id": "ID_VISION_DARKVISION"}), None);
        let elvish = grant("ID_LANGUAGE_ELVISH", "Language");
        set_rule(els, race, json!({"id": "ID_LANGUAGE_COMMON"}), Some(elvish));
        let poison = grant(
            "ID_INTERNAL_CONDITION_DAMAGE_RESISTANCE_POISON",
            "Condition",
        );
        set_rule(els, "Ember Skin", json!({"kind": "grant"}), Some(poison));
        set_rule(els, "Quiet Flame", json!({"kind": "roll_advantage"}), None);
        let ash = grant("ID_SYNTH_SPELL_ASHFALL", "Spell");
        set_rule(els, "Quiet Flame", json!({"kind": "grant"}), Some(ash));
        set_setter(els, "Ashfall", "level", "1");
        set_setter(els, "Ashfall", "school", "Illusion");
        set_setter(els, "Ember Ring", "attunement", "false");
        element_mut(els, "Bright Ward")
            .as_object_mut()
            .expect("feat")
            .remove("requirements");
        let embers = json!({"kind": "resource", "id": "wick_debt_embers", "name": "Embers",
            "max": {"type": "formula", "expr": "max(1, 2 + constitution:modifier)"},
            "recharge": ["long_rest"]});
        set_rule(els, "Kindle", json!({"kind": "resource"}), Some(embers));
        let mut sword = element_mut(els, "Lantern Flail").clone();
        sword["id"] = json!("ID_SYNTH_WEAPON_LANTERN_SWORD");
        sword["name"] = json!("Lantern Sword");
        els.push(sword);
    });
    let r = run_loaded("synthetic-mutated-2", &case, &loaded);
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
            "absent.lantern_sword",
            "count.spells_illusion",
            "count.spells_level_1",
            "el2.feat.bright_ward",
            "el2.spell.ashfall",
            "el2.trait.quiet_flame",
            "lw-cleric/spellcasting",
            "lw-feat-low-cha/choice.feat",
            "lw-l1/resource.embers",
            "lw-origin/ability_scores",
            "lw-origin/advantage",
            "lw-origin/any_of",
            "lw-origin/armor_class",
            "lw-origin/attunement",
            "lw-origin/defenses",
            "lw-origin/granted.names",
            "lw-origin/item.ring",
            "lw-origin/languages",
            "lw-origin/resource.embers",
            "lw-origin/resource.none",
            "lw-origin/size",
            "lw-origin/skill.stealth",
            "lw-origin/speed",
            "lw-origin/vision",
            "lw-wizard/choice.spellbook",
            "report.dim_lanterns",
        ],
        "{}",
        r.table()
    );
}

/// Pack names that resolve to nothing, or picks never offered, fail every check of the
/// character with a clear reason.
#[test]
fn unresolved_origin_and_picks_fail_the_character() {
    let dir = fixture("synthetic-lanternwarden");
    let mut case = read_case(&dir.join("expected.json")).expect("case");
    let loaded = pack::load(&content_root(), &case, &dir).expect("pack");
    let base = case
        .characters
        .iter()
        .find(|c| c.id == "lw-origin")
        .expect("lw-origin")
        .clone();
    let mut bad_race = base.clone();
    bad_race.id = "bad-race".into();
    bad_race.race = Some("Nobody".into());
    let mut bad_sub = base.clone();
    bad_sub.id = "bad-sub".into();
    bad_sub.subrace = Some("Path of Ash".into());
    let mut bad_pick = base;
    bad_pick.id = "bad-pick".into();
    bad_pick.picks = vec![schema::PickSpec {
        slot: Some("Feat".into()),
        option: "Bright Ward".into(),
    }];
    case.characters = vec![bad_race, bad_sub, bad_pick];
    let r = run_loaded("synthetic-unresolved", &case, &loaded);
    let detail = |id: &str| {
        r.rows
            .iter()
            .find(|x| x.id == format!("{id}/clean"))
            .map(|x| (x.pass, x.detail.clone()))
            .expect("row")
    };
    let (pass, d) = detail("bad-race");
    assert!(
        !pass && d.contains("no Race|Species named \"Nobody\""),
        "{d}"
    );
    let (pass, d) = detail("bad-sub");
    assert!(!pass && d.contains("no Sub Race"), "{d}");
    let (pass, d) = detail("bad-pick");
    assert!(!pass && d.contains("never offered up to level 1"), "{d}");
    assert!(r
        .rows
        .iter()
        .filter(|x| x.id.starts_with("bad-"))
        .all(|x| !x.pass));
}
