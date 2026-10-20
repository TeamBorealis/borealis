//! IM4 acceptance harness, stages 1 and 2 (specs/acceptance-imports-1.md), driven through the
//! real app state exactly as the UI drives it.
//!
//! Stage 1: every source element converts or gets a precise warning. `S` comes from independent
//! counters (`support/count.rs`), `I` (clean), `W` (warned) and `K` (skipped by design, closed
//! list) from the import report the install command returns; `U = S - I - W - K` must be 0. Two
//! installs and folder vs zip must give byte-identical packs.
//! Stage 2: every imported element is offered where it belongs, via `get_slot_options` (plus the
//! spellcasting block and `search_items`) on hosts built with the builder commands; elements of
//! types no select ever offers are declared non-offerable with a reason.
//!
//! Sources (each skips when its variable is unset; `BOREALIS_ACCEPT_STRICT=1` fails on a skip):
//! `a-user` the committed invented pack (always), `a-core` `BOREALIS_LIVE_INDEX_URL`,
//! `a-comm` `BOREALIS_CORPUS_DIR`, `f-full` `BOREALIS_5ETOOLS_DIR`, `f-url`
//! `BOREALIS_LIVE_5ETOOLS_URL`. `BOREALIS_ACCEPT_LIMIT` caps the class/subclass/feat hosts per
//! source (quick runs). Output: `METRIC\t<source>\t<stage>\t<type>\t<metric>\t<n>` and gate
//! `OUTCOME` lines on stderr; counts and ids only, never content text.

#[path = "../../tools/regression-gate/outcome.rs"]
mod outcome;
mod support;

use base64::Engine as _;
use borealis_content::{ContentStore, ContentStoreBuilder, Ruleset};
use borealis_lib::content_dto::{FiveToolsImportRequest, PackKind};
use borealis_lib::content_index::{Fetcher, HttpsFetcher};
use borealis_lib::content_packs::NullSink;
use borealis_lib::content_preview::{ImportFileInput, PreviewImportRequest};
use borealis_lib::picked_paths::PickPurpose;
use borealis_lib::state::AppState;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use support::app::{app_state, app_state_copy, Driver, TEST_SCORES};
use support::count::{self, AuroraCount};
use support::stage2::{stage2, Scope};
use support::{files, prereq, progress};

const SUITE: &str = "acceptance";
const BUNDLED: [&str; 3] = ["core", "srd-5.1", "srd-5.2.1"];
/// Element types placed by a dedicated host (spec type -> host table).
const HOSTED: &[&str] = &[
    "Class",
    "Archetype",
    "Race",
    "Sub Race",
    "Race Variant",
    "Species",
    "Background",
    "Feat",
    "Spell",
    "Item",
    "Weapon",
    "Armor",
    "Magic Item",
];
/// Types that are never offered by a select, by design (spec: Rule, Grants, Internal, ...).
const NON_OFFERABLE: &[(&str, &str)] = &[
    ("Grants", "Grants"),
    ("Internal", "Internal"),
    ("Support", "Support"),
    ("Source", "Source"),
    ("Information", "Information"),
    ("Rule", "Rule"),
];
/// 5etools arrays the rules import leaves out by decision (IM5: rules-only; monsters, adventures
/// and site data are not character content). Anything else unconverted counts in `U`.
const FIVE_OUT_OF_SCOPE: &[&str] = &[
    "monster",
    "legendaryGroup",
    // creature templates (`bestiary/template.json`) are applied to the creatures that copy
    // through them, not imported on their own
    "monsterTemplate",
    "legendaryGroupTemplate",
    "vehicle",
    "vehicleUpgrade",
    "object",
    "trap",
    "hazard",
    "table",
    "tableGroup",
    "encounter",
    "name",
    "deck",
    "card",
    "recipe",
    "reward",
    "cult",
    "boon",
    "deity",
    "psionic",
    "variantrule",
    "action",
    "facility",
    "makebrewCreatureTrait",
    "makebrewCreatureAction",
    "monsterfeatures",
    "adventure",
    "book",
    "crochetPattern",
    "languageScript",
    "itemTypeAdditionalEntries",
];
/// 5etools arrays the importer reports under another kind.
const FIVE_KIND_OF: &[(&str, &str)] = &[("baseitem", "item"), ("magicvariant", "item")];
/// Skip reasons that are by design (digit-free prefixes of the importer's skip notes).
const FIVE_SKIP_BY_DESIGN: &[&str] = &[
    "descriptive catalog entry covered by the core pack",
    "defined by the core pack",
    "duplicate of an entry that was already imported",
    // creatures the typed stat block model cannot hold exactly, each with a reason code
    // (borealis-import-5etools report::code): no challenge rating (summoned spirits whose
    // numbers follow the spell, level-scaled companions), a creature type that is a choice or
    // not one of the 14, a number printed as text, or a `_copy` target missing from the data
    "creature stat block: no challenge rating",
    "creature stat block: the creature type is a choice",
    "creature stat block: creature type",
    "creature stat block: hit points are not a number",
    "creature stat block: the str score is not a number",
    "creature stat block: challenge rating",
    "cannot resolve _copy: replaceArr: target not found",
    // TCE sidekick classes level a companion creature, not a character (IM5: creatures are not
    // character content); the importer skips them with code OUT_OF_SCOPE_SIDEKICK_CLASS
    "sidekick class",
];
/// Kinds the importer reports for elements it derives rather than reads: a magic variant applied
/// to each base item it fits ("+1 Longsword"). They are no entries of the data, so they stay out
/// of `S = I + W + K` (the variant itself is accounted by its generic item) and get metrics only.
const FIVE_DERIVED: &[&str] = &["specificVariant"];

/// Written straight to stderr (like `outcome::record`), so libtest's capture never swallows it.
fn metric(src: &str, stage: &str, ty: &str, name: &str, n: i64) {
    use std::io::Write as _;
    let line = format!("METRIC\t{src}\t{stage}\t{ty}\t{name}\t{n}\n");
    let _ = std::io::stderr().lock().write_all(line.as_bytes());
}

fn limit() -> usize {
    std::env::var("BOREALIS_ACCEPT_LIMIT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(usize::MAX)
}

/// `BOREALIS_ACCEPT_ONLY=a-comm,f-full`: run only these sources (the others skip-pass).
fn selected(src: &str) -> bool {
    std::env::var("BOREALIS_ACCEPT_ONLY")
        .map(|v| v.split(',').any(|s| s.trim() == src))
        .unwrap_or(true)
}

fn skip(src: &str, why: &str) {
    assert!(
        std::env::var_os("BOREALIS_ACCEPT_STRICT").is_none(),
        "{src}: {why} (BOREALIS_ACCEPT_STRICT set: a skip fails)"
    );
    eprintln!("SKIPPED {src}: {why}");
}

fn content_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content")
}

/// Installed pack directories under the app data dir: id -> (dir, single ruleset).
fn pack_dirs(data: &Path) -> BTreeMap<String, (PathBuf, Option<Ruleset>)> {
    let mut out = BTreeMap::new();
    let mut stack = vec![data.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.file_name().is_some_and(|n| n == "pack.json") {
                let v: serde_json::Value = std::fs::read_to_string(&p)
                    .ok()
                    .and_then(|t| serde_json::from_str(&t).ok())
                    .unwrap_or_default();
                let rs = v["rulesets"]
                    .as_array()
                    .filter(|a| a.len() == 1)
                    .and_then(|a| serde_json::from_value(a[0].clone()).ok());
                if let Some(id) = v["id"].as_str() {
                    out.insert(id.to_string(), (dir.clone(), rs));
                }
            }
        }
    }
    out
}

/// What a source installed: the full store (bundled + every installed pack) and its own elements.
struct Installed {
    store: ContentStore,
    ours: Vec<(String, String, Ruleset)>,
    digest: String,
}

fn installed(app_data: &Path, wanted: &dyn Fn(&str) -> bool) -> Installed {
    let mut b = ContentStoreBuilder::new();
    for p in BUNDLED {
        b = b
            .add_pack_dir(&content_dir().join(p))
            .expect("bundled pack");
    }
    let dirs = pack_dirs(app_data);
    let mut mine = Vec::new();
    let mut digest = String::new();
    for (id, (dir, rs)) in &dirs {
        b = b.add_pack_dir(dir).expect("installed pack loads");
        if wanted(id) {
            mine.push((format!("{id}/"), *rs));
            digest.push_str(&format!("{id}:{} ", files::digest_dir(dir)));
        }
    }
    let store = b.build();
    let ours = store
        .iter()
        .filter_map(|se| {
            let (_, rs) = mine.iter().find(|(p, _)| se.file.starts_with(p.as_str()))?;
            let rs = se.element.ruleset.or(*rs).unwrap_or(Ruleset::Dnd2014);
            Some((se.element.id.clone(), se.element.ty.clone(), rs))
        })
        .collect();
    Installed {
        store,
        ours,
        digest,
    }
}

// ---------------------------------------------------------------- stage 1, Aurora

/// Stage-1 tallies of one Aurora source, by type, on id sets.
#[derive(Default)]
struct AuroraTally {
    s: BTreeMap<String, String>,
    warned: BTreeSet<String>,
    /// source ids the import renamed (`normalized` infos name the source id)
    aliases: BTreeSet<String>,
    imprecise: usize,
    uncoded: usize,
    infos: usize,
    folder_errors: usize,
}

fn aurora_stage1(src: &str, t: &AuroraTally, ins: &Installed) {
    let mut ours: HashMap<&str, &str> = ins
        .ours
        .iter()
        .map(|(id, ty, _)| (id.as_str(), ty.as_str()))
        .collect();
    // an element the store resolves under its source id (renamed_from) is the source element
    for (id, ty, _) in &ins.ours {
        if let Some(old) = ins
            .store
            .element(id)
            .and_then(|e| e.renamed_from.as_deref())
        {
            ours.entry(old).or_insert(ty.as_str());
        }
    }
    let mut rows: BTreeMap<&str, [i64; 5]> = BTreeMap::new();
    for (id, ty) in &t.s {
        let r = rows.entry(ty.as_str()).or_default();
        r[0] += 1;
        if t.warned.contains(id) {
            r[2] += 1;
        } else if ours.contains_key(id.as_str()) || t.aliases.contains(id) {
            r[1] += 1;
        } else {
            r[4] += 1;
            outcome::record(
                SUITE,
                &format!("{src}/s1/{ty}/{id}"),
                false,
                "not imported, no warning",
            );
        }
    }
    let extra = ours.keys().filter(|id| !t.s.contains_key(**id)).count();
    for (ty, [s, i, w, k, u]) in &rows {
        for (name, n) in [("S", s), ("I", i), ("W", w), ("K", k), ("U", u)] {
            metric(src, "s1", ty, name, *n);
        }
        outcome::record(SUITE, &format!("{src}/s1/{ty}"), *u == 0, &format!("U={u}"));
    }
    metric(src, "s1", "*", "extra_imported", extra as i64);
    metric(src, "s1", "*", "imprecise_warnings", t.imprecise as i64);
    metric(src, "s1", "*", "folder_errors", t.folder_errors as i64);
    metric(src, "s1", "*", "warnings_without_code", t.uncoded as i64);
    metric(src, "s1", "*", "infos", t.infos as i64);
    outcome::record(
        SUITE,
        &format!("{src}/s1/coded_warnings"),
        t.uncoded == 0,
        "",
    );
    outcome::record(
        SUITE,
        &format!("{src}/s1/precise_warnings"),
        t.imprecise == 0,
        "",
    );
    outcome::record(
        SUITE,
        &format!("{src}/s1/folder_errors"),
        t.folder_errors == 0,
        "",
    );
}

/// Install one Aurora folder (or zip) through preview + install; fold its report into `t`.
fn aurora_install(app: &AppState, input: &AuroraInput, t: &mut AuroraTally) -> Option<String> {
    let request = match input {
        AuroraInput::Folder(dir) => {
            let token = app
                .picked
                .grant(dir.clone(), PickPurpose::ContentFolder)
                .expect("grant")
                .token;
            PreviewImportRequest {
                files: Vec::new(),
                folder: Some(token),
            }
        }
        AuroraInput::Zip(name, bytes) => PreviewImportRequest {
            files: vec![ImportFileInput {
                file_name: name.clone(),
                content_base64: base64::engine::general_purpose::STANDARD.encode(bytes),
            }],
            folder: None,
        },
    };
    let preview = match app.preview_import_pack(&request) {
        Ok(p) => p,
        Err(e) => {
            t.folder_errors += 1;
            eprintln!("  preview refused: {}", e.message);
            return None;
        }
    };
    let result = match app.install_import_preview(&preview.preview_id, &NullSink) {
        Ok(r) => r,
        Err(e) => {
            t.folder_errors += 1;
            eprintln!("  install refused: {}", e.message);
            return None;
        }
    };
    tally_report(&result.report, t);
    result.pack.map(|p| p.id)
}

/// Issue codes that are about a file, not an element (det-src-1/-2).
const FILE_LEVEL_CODES: &[&str] = &["FileSkipped", "UnsupportedConstruct"];

/// A precise finding names its source file and a closed code, and then its element and that
/// element's type, or, outside any element, a file-level code or its line in the file (an
/// `<info>` without `<update>`, D-20: the whole file is not loaded and each of its elements
/// gets `element_dropped`). An append names its target by id only (Aurora's `<append id>`
/// carries no type, `DataManager.cs:1006` finds the target by id), so a missing target has no
/// type to report: its file and target id are the whole location.
fn is_precise(issue: &borealis_lib::content_dto::MigrationIssue) -> bool {
    let code = format!("{:?}", issue.code);
    if issue.file.is_none() {
        return false;
    }
    match &issue.element {
        None => FILE_LEVEL_CODES.contains(&code.as_str()) || issue.line.is_some(),
        Some(_) => issue.element_type.is_some() || code == "AppendTargetMissing",
    }
}

#[test]
fn precise_findings_name_their_file_and_element_or_line() {
    use borealis_content::IssueCode;
    use borealis_lib::content_dto::MigrationIssue;
    let issue =
        |code, element: Option<&str>, ty: Option<&str>, file: Option<&str>, line| MigrationIssue {
            severity: "warning",
            code,
            message: String::new(),
            element: element.map(str::to_string),
            element_type: ty.map(str::to_string),
            file: file.map(str::to_string),
            line,
        };
    let f = Some("a.xml");
    let el = Some("ID_X");
    assert!(is_precise(&issue(
        IssueCode::DanglingReference,
        el,
        Some("Feat"),
        f,
        None
    )));
    assert!(!is_precise(&issue(
        IssueCode::DanglingReference,
        el,
        None,
        f,
        None
    )));
    assert!(!is_precise(&issue(
        IssueCode::DanglingReference,
        el,
        Some("Feat"),
        None,
        None
    )));
    // an append's missing target has no type
    assert!(is_precise(&issue(
        IssueCode::AppendTargetMissing,
        el,
        None,
        f,
        None
    )));
    assert!(!is_precise(&issue(
        IssueCode::AppendTargetMissing,
        None,
        None,
        f,
        None
    )));
    // outside any element: a file-level code, or the line of the construct
    assert!(is_precise(&issue(
        IssueCode::FileSkipped,
        None,
        None,
        f,
        None
    )));
    assert!(is_precise(&issue(
        IssueCode::MissingAttribute,
        None,
        None,
        f,
        Some(3)
    )));
    assert!(!is_precise(&issue(
        IssueCode::MissingAttribute,
        None,
        None,
        f,
        None
    )));
}

/// Folds one migration report into the tally: warnings by element id; precision by
/// [`is_precise`]; `normalized` infos record the source id as an alias.
fn tally_report(report: &borealis_lib::content_dto::MigrationReport, t: &mut AuroraTally) {
    for issue in &report.issues {
        let code = format!("{:?}", issue.code);
        if issue.severity == "info" {
            t.infos += 1;
            if code == "Normalized" {
                t.aliases.extend(issue.element.clone());
            }
            continue;
        }
        if matches!(code.as_str(), "Other" | "Unknown") {
            t.uncoded += 1;
        }
        if let Some(id) = &issue.element {
            t.warned.insert(id.clone());
        }
        if !is_precise(issue) {
            t.imprecise += 1;
            if t.imprecise <= 3 {
                eprintln!(
                    "  imprecise issue ({:?}, {code}, file {:?})",
                    issue.severity, issue.file
                );
            }
        }
    }
}

enum AuroraInput {
    Folder(PathBuf),
    Zip(String, Vec<u8>),
}

fn count_folder(dir: &Path, c: &mut AuroraCount) {
    for f in count::xml_files(dir) {
        if let Ok(text) = std::fs::read_to_string(&f) {
            c.add_file(&text);
        }
    }
}

// ---------------------------------------------------------------- stage 1, 5etools

fn five_request(app: &AppState, data: &Path) -> (FiveToolsImportRequest, Vec<String>) {
    let token = app
        .picked
        .grant(data.to_path_buf(), PickPurpose::FivetoolsFolder)
        .expect("grant")
        .token;
    let sources: Vec<String> = app
        .scan_5etools_folder(&token)
        .expect("scan")
        .sources
        .into_iter()
        .map(|s| s.abbr)
        .collect();
    (
        FiveToolsImportRequest {
            folder: token,
            sources: sources.clone(),
            include_fluff: true,
        },
        sources,
    )
}

fn five_digest(app_data: &Path) -> String {
    installed(app_data, &|id| id.starts_with("5etools-")).digest
}

fn five_stage1(src: &str, data: &Path, sources: &[String]) {
    let mut s: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
    for (array, files) in count::fivetools_entries(data) {
        let kind = FIVE_KIND_OF
            .iter()
            .find(|(a, _)| *a == array)
            .map_or(array.as_str(), |(_, k)| k);
        for (f, n) in files {
            *s.entry(kind.to_string()).or_default().entry(f).or_default() += n;
        }
    }
    let options = borealis_import_5etools::ImportOptions {
        include_fluff: true,
        srd_dirs: vec![
            content_dir().join("srd-5.1"),
            content_dir().join("srd-5.2.1"),
        ],
        ..borealis_import_5etools::ImportOptions::default().with_sources(sources)
    };
    let report = borealis_import_5etools::import_folder(data, &options)
        .expect("import")
        .report;
    let mut kinds: BTreeMap<String, [usize; 4]> = BTreeMap::new();
    for f in &report.files {
        for (k, c) in &f.kinds {
            let r = kinds.entry(k.clone()).or_default();
            r[0] += c.total;
            r[1] += c.converted;
            r[2] += c.partial;
            r[3] += c.skipped;
        }
    }
    let mut by_design: BTreeMap<String, usize> = BTreeMap::new();
    let mut other_skips: BTreeMap<String, usize> = BTreeMap::new();
    for n in &report.skipped {
        let cause: String = n
            .reason
            .chars()
            .filter(|c| !c.is_ascii_digit())
            .take(60)
            .collect();
        if FIVE_SKIP_BY_DESIGN.iter().any(|p| n.reason.starts_with(p)) {
            *by_design.entry(n.kind.clone()).or_default() += 1;
        } else {
            *other_skips
                .entry(format!("{} | {cause}", n.kind))
                .or_default() += 1;
        }
    }
    let imprecise = report
        .partial
        .iter()
        .filter(|n| {
            n.file.is_empty()
                || n.element.is_empty()
                || n.code.is_empty()
                || n.code == "UNCLASSIFIED"
        })
        .count();
    let all: BTreeSet<String> = s.keys().chain(kinds.keys()).cloned().collect();
    for ty in all {
        let sv = s.get(&ty).map_or(0, |m| m.values().sum::<usize>()) as i64;
        let [total, conv, part, _] = kinds.get(&ty).copied().unwrap_or_default();
        // entries the importer reported for the kind (R = S when each entry is reported once)
        metric(src, "s1", &ty, "R", total as i64);
        let (i, w) = ((conv - part) as i64, part as i64);
        if FIVE_DERIVED.contains(&ty.as_str()) {
            metric(src, "s1", &ty, "I", i);
            metric(src, "s1", &ty, "W", w);
            continue;
        }
        let k = if FIVE_OUT_OF_SCOPE.contains(&ty.as_str()) && conv == 0 {
            sv
        } else {
            by_design.get(&ty).copied().unwrap_or(0) as i64
        };
        let u = sv - i - w - k;
        for (name, n) in [("S", sv), ("I", i), ("W", w), ("K", k), ("U", u)] {
            metric(src, "s1", &ty, name, n);
        }
        outcome::record(SUITE, &format!("{src}/s1/{ty}"), u == 0, &format!("U={u}"));
    }
    println!("--- {src}: skips not by design (kind | cause)");
    for (c, n) in other_skips.iter().take(40) {
        println!("{n:>7}  {c}");
    }
    metric(src, "s1", "*", "imprecise_warnings", imprecise as i64);
    metric(src, "s1", "*", "run_warnings", report.warnings.len() as i64);
    metric(
        src,
        "s1",
        "*",
        "converter_errors",
        report.errors.len() as i64,
    );
    outcome::record(
        SUITE,
        &format!("{src}/s1/precise_warnings"),
        imprecise == 0,
        "",
    );
    outcome::record(
        SUITE,
        &format!("{src}/s1/converter_errors"),
        report.errors.is_empty(),
        "",
    );
}

// ---------------------------------------------------------------- sources

/// `name:bytes` of every content file of the installed (non-bundled) packs.
fn pack_files(data: &Path) -> String {
    pack_dirs(data)
        .values()
        .map(|(d, _)| {
            // pack.json and pack-meta.json record the input's name (folder vs file.zip)
            files::tree_entries(d, "", &|r| r != "pack.json" && r != "pack-meta.json")
                .iter()
                .map(|(n, b)| format!("{n}:{}", b.len()))
                .collect::<Vec<_>>()
                .join(",")
        })
        .collect::<Vec<_>>()
        .join(";")
}

fn a_user_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/support/a-user")
}

/// Stage 1 (and, with `with_stage2`, stage 2) of Aurora folders installed one after another.
/// Returns whether the stacked app's builder still offers classes and races.
fn aurora_folders(src: &str, dirs: &[PathBuf], zip_check: bool, with_stage2: bool) -> bool {
    let t0 = std::time::Instant::now();
    let (tmp, app) = app_state();
    let mut tally = AuroraTally::default();
    let mut c = AuroraCount::default();
    let mut ids = Vec::new();
    let mut no_content = 0;
    for dir in dirs {
        count_folder(dir, &mut c);
        let before = tally.folder_errors;
        let got = aurora_install(&app, &AuroraInput::Folder(dir.clone()), &mut tally);
        // a folder without one .xml file holds no Aurora content (an empty repository, an app,
        // character files, a lone remote .index): Aurora loads custom content from *.xml only
        // (DataManager.cs:934, 966; an .index is read by the online updater), so the refusal
        // "Nothing could be imported." is the right answer, not a folder error
        if got.is_none() && tally.folder_errors == before + 1 && count::xml_files(dir).is_empty() {
            tally.folder_errors = before;
            no_content += 1;
        }
        ids.extend(got);
    }
    tally.s = c.types.clone();
    metric(src, "s1", "*", "malformed_tags", c.malformed as i64);
    metric(src, "s1", "*", "no_content_folders", no_content);
    let data = tmp.path().join("data");
    let ins = installed(&data, &|id| ids.iter().any(|i| i == id));
    aurora_stage1(src, &tally, &ins);
    // rerun into a fresh app: byte-identical packs
    let (tmp2, app2) = app_state();
    let mut t2 = AuroraTally::default();
    for dir in dirs {
        aurora_install(&app2, &AuroraInput::Folder(dir.clone()), &mut t2);
    }
    let again = installed(&tmp2.path().join("data"), &|id| ids.iter().any(|i| i == id)).digest;
    outcome::record(
        SUITE,
        &format!("{src}/s1/rerun_identical"),
        again == ins.digest,
        "",
    );
    metric(
        src,
        "s1",
        "*",
        "rerun_identical",
        i64::from(again == ins.digest),
    );
    if zip_check {
        for dir in dirs {
            let name = dir.file_name().unwrap().to_string_lossy().into_owned();
            // a zip of the folder itself (top folder kept, as Finder/Explorer zip it) and of its contents
            for (shape, prefix) in [("nested", format!("{name}/")), ("flat", String::new())] {
                let entries = files::tree_entries(dir, &prefix, &|r| r.ends_with(".xml"));
                let (tmp3, app3) = app_state();
                let mut t3 = AuroraTally::default();
                let zip = AuroraInput::Zip(format!("{name}.zip"), files::zip_stored(&entries));
                let zid = aurora_install(&app3, &zip, &mut t3);
                let (zipped, folder) = (pack_files(&tmp3.path().join("data")), pack_files(&data));
                let same = zid.is_some() && zipped == folder;
                if !same && src == "a-user" {
                    for root in [tmp3.path().join("data"), data.clone()] {
                        for (d, _) in pack_dirs(&root).values() {
                            eprintln!(
                                "  meta: {}",
                                std::fs::read_to_string(d.join("pack-meta.json"))
                                    .unwrap_or_default()
                            );
                        }
                    }
                    eprintln!(
                        "  zip ({shape}) pack files: {zipped}\n  folder pack files: {folder}"
                    );
                }
                outcome::record(
                    SUITE,
                    &format!("{src}/s1/zip_identical_{shape}/{name}"),
                    same,
                    "",
                );
                metric(
                    src,
                    "s1",
                    "*",
                    &format!("zip_identical_{shape}"),
                    i64::from(same),
                );
            }
        }
    }
    let healthy = builder_health(src, &app);
    metric(src, "s1", "*", "wall_s", t0.elapsed().as_secs() as i64);
    if with_stage2 {
        stage2(src, &[&app], &ins, "ID_WOTC_", aurora_class_id, Scope::Full);
    }
    healthy
}

fn aurora_class_id(_: Ruleset, name: &str) -> String {
    format!(
        "ID_WOTC_PHB_CLASS_{}",
        name.to_uppercase().replace(' ', "_")
    )
}

/// Options of the L1 class and race/species slots per edition (0 = the builder is unusable).
fn builder_counts(app: &AppState) -> Vec<(Ruleset, usize, usize)> {
    let d = Driver::new(app);
    [Ruleset::Dnd2014, Ruleset::Dnd2024]
        .into_iter()
        .filter_map(|rs| {
            let h = d.create(rs, TEST_SCORES)?;
            let count = |tys: &[&str]| {
                h.snap
                    .slots
                    .iter()
                    .filter(|s| tys.iter().any(|t| s.element_type.eq_ignore_ascii_case(t)))
                    .map(|s| d.options(&h, &s.id).len())
                    .sum::<usize>()
            };
            let out = (rs, count(&["Class"]), count(&["Race", "Species"]));
            d.drop_host(h);
            Some(out)
        })
        .collect()
}

fn builder_health(src: &str, app: &AppState) -> bool {
    let counts = builder_counts(app);
    for (rs, classes, races) in &counts {
        let ed = if *rs == Ruleset::Dnd2024 {
            "2024"
        } else {
            "2014"
        };
        metric(
            src,
            "s2",
            "*",
            &format!("builder_class_options_{ed}"),
            *classes as i64,
        );
        metric(
            src,
            "s2",
            "*",
            &format!("builder_race_options_{ed}"),
            *races as i64,
        );
    }
    let ok = counts
        .iter()
        .any(|(rs, c, r)| *rs == Ruleset::Dnd2014 && *c > 0 && *r > 0);
    outcome::record(SUITE, &format!("{src}/s2/builder_usable"), ok, "");
    ok
}

#[test]
fn acceptance_a_user() {
    if !selected("a-user") {
        return eprintln!("SKIPPED a-user: not in BOREALIS_ACCEPT_ONLY");
    }
    run_big(|| {
        aurora_folders("a-user", &[a_user_dir()], true, true);
    });
}

#[test]
fn acceptance_a_comm() {
    if !selected("a-comm") {
        return eprintln!("SKIPPED a-comm: not in BOREALIS_ACCEPT_ONLY");
    }
    let Ok(root) = std::env::var("BOREALIS_CORPUS_DIR") else {
        return skip("a-comm", "BOREALIS_CORPUS_DIR not set");
    };
    let root = PathBuf::from(root);
    let repos = if root.join("repos").is_dir() {
        root.join("repos")
    } else {
        root
    };
    let base = "aurorabuilder__elements";
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(&repos)
        .expect("repos")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir() && !p.file_name().unwrap().to_string_lossy().starts_with('.'))
        .collect();
    dirs.sort_by_key(|p| (!p.ends_with(base), p.clone()));
    run_big(move || {
        // stage 1: every repo installed (base first); the stacked app's builder is checked too
        let (shard, of) = shard();
        // BOREALIS_ACCEPT_BISECT=1 names the repo that breaks the stacked builder (slow)
        if shard == 0
            && !aurora_folders("a-comm", &dirs, false, false)
            && std::env::var_os("BOREALIS_ACCEPT_BISECT").is_some()
        {
            bisect_stack(&dirs);
        }
        comm_stage2(&repos, &dirs, shard, of);
    });
}

/// Smallest prefix of `dirs` whose stack leaves the 2014 builder without classes or races:
/// names the repo that breaks it and the elements it shares ids with earlier repos.
fn bisect_stack(dirs: &[PathBuf]) {
    let broken = |n: usize| {
        let (_tmp, app) = app_state();
        for dir in &dirs[..n] {
            aurora_install(
                &app,
                &AuroraInput::Folder(dir.clone()),
                &mut AuroraTally::default(),
            );
        }
        !builder_counts(&app)
            .iter()
            .any(|(rs, c, r)| *rs == Ruleset::Dnd2014 && *c > 0 && *r > 0)
    };
    let (mut lo, mut hi) = (1, dirs.len());
    while lo < hi {
        let mid = (lo + hi) / 2;
        if broken(mid) {
            hi = mid;
        } else {
            lo = mid + 1;
        }
    }
    let culprit = dirs[lo - 1]
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    eprintln!("  stack breaks the builder at repo #{lo}: {culprit}");
    metric("a-comm-stack", "s2", "*", "breaking_prefix", lo as i64);
    // alone on the base (dirs[0]) too?
    let (_tmp, app) = app_state();
    for dir in [&dirs[0], &dirs[lo - 1]] {
        aurora_install(
            &app,
            &AuroraInput::Folder(dir.clone()),
            &mut AuroraTally::default(),
        );
    }
    let alone = builder_counts(&app);
    for (rs, c, r) in &alone {
        eprintln!("  base + {culprit} alone: {rs:?} classes {c} races {r}");
    }
    let ok_alone = alone
        .iter()
        .any(|(rs, c, r)| *rs == Ruleset::Dnd2014 && *c > 0 && *r > 0);
    metric(
        "a-comm-stack",
        "s2",
        "*",
        "breaking_repo_alone_ok",
        i64::from(ok_alone),
    );
    // the repo's name goes in an outcome id (metrics carry numbers only)
    outcome::record(
        SUITE,
        &format!("a-comm-stack/s2/breaking_repo/{culprit}"),
        false,
        "",
    );
}

/// Source label of the [`COMBO`] setup.
const COMBO_SRC: &str = "a-comm-combo";

/// The repos measured together as one realistic user setup (on top of A-core).
const COMBO: &[&str] = &[
    "aurorabuilder__elements",
    "FreshRoyal__elements-partnered",
    "Zenoverlordian__Aurora-Builder-KibblesTasty",
    "Zenoverlordian__Aurora-Builder-LaserLlama",
    "Idle-Handz__Aurora-Lights",
];

/// Stage 2 of A-comm as users install it: A-core (`BOREALIS_LIVE_INDEX_URL`; without it the
/// corpus's aurorabuilder__elements folder) plus one repo, for every repo; then [`COMBO`].
fn comm_stage2(repos: &Path, dirs: &[PathBuf], shard: usize, of: usize) {
    let (base_tmp, base_label) = match std::env::var("BOREALIS_LIVE_INDEX_URL") {
        Ok(url) => {
            let tmp = tempfile::tempdir().expect("temp dir");
            files::copy_tree(&core_base(&url).data, &tmp.path().join("data")).expect("copy");
            (tmp, "a-core")
        }
        Err(_) => {
            let (tmp, app) = app_state();
            let dir = repos.join("aurorabuilder__elements");
            aurora_install(&app, &AuroraInput::Folder(dir), &mut AuroraTally::default());
            (tmp, "aurorabuilder__elements")
        }
    };
    eprintln!("  a-comm stage 2 base: {base_label}");
    let template = base_tmp.path().join("data");
    // ids the base already has: a repo is measured on what it adds
    let base_ids: BTreeSet<String> = installed(&template, &|_| true)
        .store
        .iter()
        .map(|se| se.element.id.clone())
        .collect();
    let mut jobs: Vec<(String, Vec<PathBuf>)> = dirs
        .iter()
        .skip(shard)
        .step_by(of)
        .map(|d| {
            let repo = d.file_name().unwrap().to_string_lossy().into_owned();
            (format!("a-comm:{repo}"), vec![d.clone()])
        })
        .collect();
    if shard == 0 {
        let combo: Vec<PathBuf> = COMBO
            .iter()
            .map(|r| repos.join(r))
            .filter(|p| p.is_dir())
            .collect();
        // the combination is the longest setup by far: it starts first (on its own workers),
        // so the repos fill the other threads instead of it running alone at the end
        jobs.insert(0, (COMBO_SRC.into(), combo));
    }
    let t0 = std::time::Instant::now();
    let next = std::sync::atomic::AtomicUsize::new(0);
    let total = jobs.len();
    std::thread::scope(|sc| {
        for _ in 0..comm_threads() {
            std::thread::Builder::new()
                .stack_size(64 << 20)
                .spawn_scoped(sc, || {
                    borealis_engine::invariants::enable_for_thread();
                    loop {
                        let i = next.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        let Some((src, installs)) = jobs.get(i) else {
                            return;
                        };
                        let started = std::time::Instant::now();
                        progress(&format!("{src} start ({}/{total})", i + 1));
                        // an engine panic (settle invariants) fails this setup, not the run
                        let run = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            comm_one(src, installs, &template, &base_ids, base_label);
                        }));
                        if let Err(e) = run {
                            let why = e
                                .downcast_ref::<String>()
                                .map(String::as_str)
                                .or_else(|| e.downcast_ref::<&str>().copied())
                                .unwrap_or("panic");
                            let why: String = why.lines().take(2).collect::<Vec<_>>().join(" | ");
                            outcome::record(SUITE, &format!("{src}/s2/panic"), false, &why);
                            metric(src, "s2", "*", "panicked", 1);
                        }
                        progress(&format!("{src} done in {} s", started.elapsed().as_secs()));
                    }
                })
                .expect("a-comm worker");
        }
    });
    metric("a-comm", "s2", "*", "wall_s", t0.elapsed().as_secs() as i64);
}

/// `BOREALIS_ACCEPT_THREADS` (default 4): A-comm setups measured side by side, one app each.
fn comm_threads() -> usize {
    std::env::var("BOREALIS_ACCEPT_THREADS")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|n| *n > 0)
        .unwrap_or(COMM_THREADS)
}

/// `BOREALIS_ACCEPT_WORKERS` (default 3): stage-2 workers of a-core and f-full, one app each
/// (copies of the installed data dir).
fn workers() -> usize {
    std::env::var("BOREALIS_ACCEPT_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|n| *n > 0)
        .unwrap_or(WORKERS)
}

const WORKERS: usize = 3;

/// Default A-comm parallelism (within the resource guard: load < 15 with the other tests).
const COMM_THREADS: usize = 6;

/// Stage 2 of one A-comm setup: `installs` on a copy of the base, measured on what they add.
fn comm_one(
    src: &str,
    installs: &[PathBuf],
    template: &Path,
    base_ids: &BTreeSet<String>,
    base_label: &str,
) {
    let (tmp, app) = app_state_copy(template);
    let mut ids = Vec::new();
    for dir in installs {
        ids.extend(aurora_install(
            &app,
            &AuroraInput::Folder(dir.clone()),
            &mut AuroraTally::default(),
        ));
    }
    if ids.is_empty() {
        metric(src, "s2", "*", "install_refused", 1);
        return;
    }
    if !builder_health(src, &app) {
        eprintln!("  {src}: builder has no class/race options on {base_label}");
    }
    if std::env::var_os("BOREALIS_ACCEPT_COMM_S2").is_none() {
        return;
    }
    let mut ins = installed(&tmp.path().join("data"), &|id| ids.iter().any(|i| i == id));
    let all = ins.ours.len();
    ins.ours.retain(|o| !base_ids.contains(&o.0));
    metric(
        src,
        "s2",
        "*",
        "ids_already_in_base",
        (all - ins.ours.len()) as i64,
    );
    // the combination holds four big repos: its hosts run on workers() apps (copies of its
    // data dir), as a-core's do; a single repo keeps one app
    let extra: Vec<_> = if src == COMBO_SRC {
        (1..workers())
            .map(|_| app_state_copy(&tmp.path().join("data")))
            .collect()
    } else {
        Vec::new()
    };
    let apps: Vec<&AppState> = std::iter::once(&app)
        .chain(extra.iter().map(|x| &x.1))
        .collect();
    stage2(src, &apps, &ins, "ID_WOTC_", aurora_class_id, Scope::Repo);
}

/// A fetcher that keeps every file it downloaded (the stage-1 counter reads them).
struct Recording {
    files: Mutex<Vec<(String, Vec<u8>)>>,
}

impl Fetcher for Recording {
    fn get(&self, url: &str, max_bytes: usize) -> Result<Vec<u8>, String> {
        let r = HttpsFetcher.get(url, max_bytes);
        if let Ok(bytes) = &r {
            self.files
                .lock()
                .unwrap()
                .push((url.to_string(), bytes.clone()));
        }
        r
    }
}

/// A-core installed through Update indexes, as the app does it.
struct CoreInstall {
    app: AppState,
    fetched: Vec<(String, Vec<u8>)>,
    failed: usize,
    reports: Vec<borealis_lib::content_dto::MigrationReport>,
}

fn core_install_at(url: &str, data: PathBuf) -> CoreInstall {
    let app = support::app::app_state_at(data);
    app.add_index(url).expect("add index");
    let rec = Recording {
        files: Mutex::new(Vec::new()),
    };
    let result = app.update_indexes(&rec, &NullSink).expect("update");
    CoreInstall {
        app,
        fetched: rec.files.into_inner().unwrap(),
        failed: result.failed.len(),
        reports: result.reports,
    }
}

/// A-core installed once per process (shared by a-core and a-comm, which copy its data dir).
struct CoreBase {
    data: PathBuf,
    fetched: Vec<(String, Vec<u8>)>,
    failed: usize,
    reports: Vec<borealis_lib::content_dto::MigrationReport>,
}

static CORE_BASE: std::sync::OnceLock<CoreBase> = std::sync::OnceLock::new();

fn core_base(url: &str) -> &'static CoreBase {
    CORE_BASE.get_or_init(|| {
        let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("acceptance-core");
        let _ = std::fs::remove_dir_all(&dir);
        let data = dir.join("data");
        let c = core_install_at(url, data.clone());
        drop(c.app);
        CoreBase {
            data,
            fetched: c.fetched,
            failed: c.failed,
            reports: c.reports,
        }
    })
}

#[test]
fn acceptance_a_core() {
    if !selected("a-core") {
        return eprintln!("SKIPPED a-core: not in BOREALIS_ACCEPT_ONLY");
    }
    let Ok(url) = std::env::var("BOREALIS_LIVE_INDEX_URL") else {
        return skip("a-core", "BOREALIS_LIVE_INDEX_URL not set");
    };
    run_big(move || {
        let src = "a-core";
        let t0 = std::time::Instant::now();
        let base = core_base(&url);
        let (tmp, app) = app_state_copy(&base.data);
        let (fetched, failed, reports) = (&base.fetched, base.failed, &base.reports);
        let mut c = AuroraCount::default();
        for (_, bytes) in fetched {
            c.add_file(&String::from_utf8_lossy(bytes));
        }
        let mine = |id: &str| !BUNDLED.contains(&id) && !id.starts_with("5etools-");
        let non_bundled: Vec<String> = app
            .list_packs()
            .expect("packs")
            .into_iter()
            .filter(|p| p.kind != PackKind::Bundled)
            .map(|p| p.id)
            .collect();
        let ins = installed(&tmp.path().join("data"), &|id| {
            mine(id) && non_bundled.iter().any(|p| p == id)
        });
        let mut tally = AuroraTally {
            s: c.types.clone(),
            folder_errors: failed,
            ..AuroraTally::default()
        };
        for r in reports {
            tally_report(r, &mut tally);
        }
        metric(src, "s1", "*", "fetched_files", fetched.len() as i64);
        aurora_stage1(src, &tally, &ins);
        let tmp2 = tempfile::tempdir().expect("temp dir");
        drop(core_install_at(&url, tmp2.path().join("data")).app);
        let again = installed(&tmp2.path().join("data"), &|id| {
            mine(id) && non_bundled.iter().any(|p| p == id)
        })
        .digest;
        outcome::record(
            SUITE,
            &format!("{src}/s1/rerun_identical"),
            again == ins.digest,
            "",
        );
        metric(
            src,
            "s1",
            "*",
            "rerun_identical",
            i64::from(again == ins.digest),
        );
        builder_health(src, &app);
        metric(src, "s1", "*", "wall_s", t0.elapsed().as_secs() as i64);
        let extra: Vec<_> = (1..workers()).map(|_| app_state_copy(&base.data)).collect();
        let apps: Vec<&AppState> = std::iter::once(&app)
            .chain(extra.iter().map(|x| &x.1))
            .collect();
        stage2(src, &apps, &ins, "ID_WOTC_", aurora_class_id, Scope::Full);
    });
}

fn five_zip(data: &Path) -> Vec<u8> {
    let keep = |rel: &str| {
        let root_file = !rel.contains('/');
        let sub = ["class/", "spells/", "bestiary/", "homebrew/"]
            .iter()
            .any(|p| rel.starts_with(p));
        rel.ends_with(".json")
            && (root_file || sub || rel == "generated/gendata-spell-source-lookup.json")
    };
    files::zip_stored(&files::tree_entries(data, "data/", &keep))
}

#[test]
fn acceptance_f_full() {
    if !selected("f-full") {
        return eprintln!("SKIPPED f-full: not in BOREALIS_ACCEPT_ONLY");
    }
    let Ok(dir) = std::env::var("BOREALIS_5ETOOLS_DIR") else {
        return skip("f-full", "BOREALIS_5ETOOLS_DIR not set");
    };
    run_big(move || {
        let src = "f-full";
        let t0 = std::time::Instant::now();
        let data = PathBuf::from(dir);
        let (tmp, app) = app_state();
        let (request, sources) = five_request(&app, &data);
        let report = app.import_5etools(&request, &NullSink).expect("import");
        five_stage1(src, &data, &sources);
        metric(src, "s1", "*", "app_converted", report.converted as i64);
        metric(src, "s1", "*", "app_partial", report.partial.len() as i64);
        metric(src, "s1", "*", "app_skipped", report.skipped.len() as i64);
        let digest = five_digest(&tmp.path().join("data"));
        let (tmp2, app2) = app_state();
        let (r2, _) = five_request(&app2, &data);
        app2.import_5etools(&r2, &NullSink).expect("rerun");
        let same = five_digest(&tmp2.path().join("data")) == digest;
        outcome::record(SUITE, &format!("{src}/s1/rerun_identical"), same, "");
        metric(src, "s1", "*", "rerun_identical", i64::from(same));
        let rerun_same = same;
        let (tmp3, app3) = app_state();
        let zip = five_zip(&data);
        let zipped = app3
            .stage_5etools_zip(&ImportFileInput {
                file_name: "data.zip".into(),
                content_base64: base64::engine::general_purpose::STANDARD.encode(&zip),
            })
            .map_err(|e| e.message)
            .and_then(|picked| {
                let sources = app3
                    .scan_5etools_folder(&picked.token)
                    .map_err(|e| e.message)?;
                let req = FiveToolsImportRequest {
                    folder: picked.token,
                    sources: sources.sources.into_iter().map(|s| s.abbr).collect(),
                    include_fluff: true,
                };
                app3.import_5etools(&req, &NullSink).map_err(|e| e.message)
            });
        if let Err(e) = &zipped {
            eprintln!("  zip import refused: {e}");
        }
        let same = zipped.is_ok() && five_digest(&tmp3.path().join("data")) == digest;
        outcome::record(SUITE, &format!("{src}/s1/zip_identical"), same, "");
        metric(src, "s1", "*", "zip_identical", i64::from(same));
        metric(src, "s1", "*", "zip_bytes", zip.len() as i64);
        drop((tmp3, app3));
        let ins = installed(&tmp.path().join("data"), &|id| id.starts_with("5etools-"));
        metric(src, "s1", "*", "wall_s", t0.elapsed().as_secs() as i64);
        // the rerun app holds the same packs (rerun_identical): the 2024 edition runs on it
        let mut apps: Vec<&AppState> = if rerun_same {
            vec![&app, &app2]
        } else {
            vec![&app]
        };
        let extra: Vec<_> = (apps.len()..workers())
            .map(|_| app_state_copy(&tmp.path().join("data")))
            .collect();
        apps.extend(extra.iter().map(|x| &x.1));
        stage2(
            src,
            &apps,
            &ins,
            "ID_5ET_",
            prereq::class_id_5et,
            Scope::Full,
        );
        drop((tmp2, app2));
    });
}

#[test]
fn acceptance_f_url() {
    if !selected("f-url") {
        return eprintln!("SKIPPED f-url: not in BOREALIS_ACCEPT_ONLY");
    }
    let Ok(url) = std::env::var("BOREALIS_LIVE_5ETOOLS_URL") else {
        return skip("f-url", "BOREALIS_LIVE_5ETOOLS_URL not set");
    };
    run_big(move || {
        let src = "f-url";
        let (tmp, app) = app_state();
        let picked = app
            .stage_5etools_url(&HttpsFetcher, &url, &NullSink)
            .expect("download");
        // the folder the download was granted as (consumed by the import: counted first)
        let staged = app
            .picked
            .resolve(&picked.token, PickPurpose::FivetoolsFolder)
            .expect("staged folder");
        let sources: Vec<String> = app
            .scan_5etools_folder(&picked.token)
            .expect("scan")
            .sources
            .into_iter()
            .map(|s| s.abbr)
            .collect();
        let request = FiveToolsImportRequest {
            folder: picked.token,
            sources: sources.clone(),
            include_fluff: true,
        };
        five_stage1(src, &staged, &sources);
        app.import_5etools(&request, &NullSink).expect("import");

        let digest = five_digest(&tmp.path().join("data"));
        if let Ok(dir) = std::env::var("BOREALIS_5ETOOLS_DIR") {
            let (tmp2, app2) = app_state();
            let (r2, _) = five_request(&app2, Path::new(&dir));
            app2.import_5etools(&r2, &NullSink).expect("folder import");
            let same = five_digest(&tmp2.path().join("data")) == digest;
            outcome::record(SUITE, &format!("{src}/s1/url_equals_folder"), same, "");
            metric(src, "s1", "*", "url_equals_folder", i64::from(same));
        }
    });
}

/// `BOREALIS_ACCEPT_SHARD=i/n`: this process measures every n-th A-comm repo from i (stage 1,
/// the stack check and the combination run in shard 0 only). Unset = one shard.
fn shard() -> (usize, usize) {
    std::env::var("BOREALIS_ACCEPT_SHARD")
        .ok()
        .and_then(|v| {
            let (i, n) = v.split_once('/')?;
            let (i, n) = (i.trim().parse().ok()?, n.trim().parse::<usize>().ok()?);
            (n > 0 && i < n).then_some((i, n))
        })
        .unwrap_or((0, 1))
}

/// Run on a big stack (deep content recursion), propagating panics.
fn run_big<F: FnOnce() + Send + 'static>(f: F) {
    borealis_engine::invariants::enable_for_thread();
    std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(f)
        .expect("spawn")
        .join()
        .expect("acceptance thread");
}
