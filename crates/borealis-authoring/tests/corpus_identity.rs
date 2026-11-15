//! Spec 18.2 "Corpora" (WB-27a): the identity test of `form_identity.rs` over the local regression corpora the
//! regression gate (`tools/regression-gate/gate.py`) uses, converted the way the app converts them:
//!
//! * `BOREALIS_CORPUS_DIR`: the community Aurora element repositories (`<dir>/repos/*`, or `<dir>/*`), each converted
//!   by the folder import (`migrate_path`, custom origin) and loaded on top of the bundled packs;
//! * `BOREALIS_5ETOOLS_DIR`: a 5etools `data/` folder, imported by `import_folder` (with the bundled SRD packs), all
//!   packs loaded together on top of the bundled ones;
//! * `BOREALIS_HOMEBREW_CASES`: the stored homebrew import cases, each case's `imported/` pack.
//!
//! For every element: `form_to_elements(element_to_form(e)) == [e]` (as values and as JSON), also after the wire trip
//! (the form the UI receives, read back and merged with the backend's copy); for every append:
//! `form_to_append(append_to_form(a)) == a`. A corpus that is not there is skipped with its reason printed, so CI
//! (which has none) stays green. Exceptions would be listed with their reason in [`EXCEPTIONS`]; it is empty.
//!
//! The corpora are large: run them in release, with the gate's values, for example
//! `BOREALIS_CORPUS_DIR=../corpus BOREALIS_5ETOOLS_DIR=../corpus/5etools/data
//! BOREALIS_HOMEBREW_CASES=../specs/homebrew-cases cargo test --release -p borealis-authoring --test corpus_identity
//! -- --nocapture`. Counts go to `target/corpus_identity.txt`; content text is never printed (ids only).

mod support;

use borealis_authoring::form::{
    append_to_form, element_to_form, form_to_append, form_to_elements, merge_ui, to_ui,
    CatalogBlockCodec, FormContext, FormState,
};
use borealis_content::{Append, ContentFile, ContentStoreBuilder, Element, PackOrigin, ParsedPack};
use borealis_import_5etools::{import_folder, ImportOptions};
use borealis_migrate::{migrate_path, MigrateOptions};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use support::{bundled_packs, content_dir, pack_dirs, read_pack};

/// Elements or appends that cannot round trip, with the reason (corpus, id, why). Reviewed: none.
const EXCEPTIONS: &[(&str, &str, &str)] = &[];

/// Failures listed per corpus in the report.
const SHOWN: usize = 20;

/// The units of one corpus, or why it is not there.
type Source = fn() -> Result<Vec<Unit>, String>;

/// One corpus unit: content loaded together (one Aurora repository, all 5etools packs, one homebrew case).
struct Unit {
    corpus: &'static str,
    name: String,
    files: Vec<ContentFile>,
}

#[derive(Default)]
struct Tally {
    elements: usize,
    appends: usize,
    failures: Vec<String>,
}

fn env_dir(var: &str) -> Result<PathBuf, String> {
    let dir = std::env::var(var).map_err(|_| format!("{var} is not set"))?;
    let dir = PathBuf::from(dir);
    if dir.is_dir() {
        Ok(dir)
    } else {
        Err(format!("{var} is not a folder: {}", dir.display()))
    }
}

/// The Aurora repositories, each converted as the folder import converts it.
fn aurora_units() -> Result<Vec<Unit>, String> {
    let root = env_dir("BOREALIS_CORPUS_DIR")?;
    let repos = if root.join("repos").is_dir() {
        root.join("repos")
    } else {
        root
    };
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(&repos)
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    let mut out = Vec::new();
    for dir in dirs {
        let name = dir
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let opts = MigrateOptions {
            pack_id: Some(format!(
                "corpus-{}",
                name.to_lowercase()
                    .replace(|c: char| !c.is_ascii_alphanumeric(), "-")
            )),
            pack_name: Some(name.clone()),
            origin: PackOrigin::Custom,
            ..MigrateOptions::default()
        };
        let Ok(converted) = migrate_path(&dir, &opts) else {
            println!("  aurora {name}: does not convert (the migration suites cover it)");
            continue;
        };
        let files = converted
            .files
            .iter()
            .filter(|f| f.path != "pack.json" && f.path.ends_with(".json"))
            .filter_map(|f| ContentFile::from_json_str(&f.contents).ok())
            .collect();
        out.push(Unit {
            corpus: "aurora",
            name,
            files,
        });
    }
    Ok(out)
}

/// The 5etools folder, imported as the app imports it (one unit: the packs load together).
fn fivetools_units() -> Result<Vec<Unit>, String> {
    let dir = env_dir("BOREALIS_5ETOOLS_DIR")?;
    let options = ImportOptions {
        srd_dirs: vec![
            content_dir().join("srd-5.1"),
            content_dir().join("srd-5.2.1"),
        ],
        ..ImportOptions::default()
    };
    let result = import_folder(&dir, &options).map_err(|e| e.to_string())?;
    let files = result
        .packs
        .into_iter()
        .flat_map(|p| p.files.into_iter().map(|f| f.file))
        .collect();
    Ok(vec![Unit {
        corpus: "5etools",
        name: "data".into(),
        files,
    }])
}

/// The homebrew import cases: each case's `imported/` pack.
fn case_units() -> Result<Vec<Unit>, String> {
    let root = env_dir("BOREALIS_HOMEBREW_CASES")?;
    let mut out = Vec::new();
    for case in pack_dirs(&root)
        .into_iter()
        .filter(|d| d.file_name().is_some_and(|n| n == "imported"))
    {
        let pack = read_pack(&case);
        let name = case
            .parent()
            .and_then(Path::file_name)
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        out.push(Unit {
            corpus: "homebrew-cases",
            name,
            files: pack.files.into_iter().map(|(_, f)| f).collect(),
        });
    }
    Ok(out)
}

fn json<T: serde::Serialize>(x: &T) -> String {
    serde_json::to_string(x).expect("json")
}

/// The wire trip: what the UI receives, as JSON and back, merged with the backend's copy.
fn through_ui(form: &FormState) -> FormState {
    let wire = serde_json::to_string(&to_ui(form)).expect("ui json");
    let back: FormState = serde_json::from_str(&wire).expect("ui form reads");
    merge_ui(form, &back)
}

fn element_identity(e: &Element, cx: &FormContext<'_>) -> Result<(), String> {
    let form = element_to_form(e, cx);
    for (what, f) in [("identity", form.clone()), ("wire", through_ui(&form))] {
        let back = form_to_elements(&f, cx).map_err(|p| format!("{what}: problems {p:?}"))?;
        if back.len() != 1 || back[0] != *e || json(&back[0]) != json(e) {
            // local debugging only (the corpora stay on this machine): the two elements as JSON
            if std::env::var_os("BOREALIS_CORPUS_DIFF").is_some() {
                return Err(format!(
                    "{what}: differs\n  want {}\n  got  {}",
                    json(e),
                    json(&back)
                ));
            }
            return Err(format!("{what}: differs"));
        }
    }
    Ok(())
}

fn append_identity(a: &Append, cx: &FormContext<'_>) -> Result<(), String> {
    let back = form_to_append(&append_to_form(a, cx), cx).map_err(|p| format!("problems {p:?}"))?;
    if back != *a || json(&back) != json(a) {
        return Err("differs".into());
    }
    Ok(())
}

fn excepted(corpus: &str, id: &str) -> bool {
    EXCEPTIONS.iter().any(|(c, i, _)| *c == corpus && *i == id)
}

/// The identity test over one unit, its content loaded on top of the bundled packs (references resolve).
fn check_unit(unit: &Unit, bundled: &[ParsedPack]) -> Tally {
    let mut b = ContentStoreBuilder::new();
    for p in bundled {
        b = b.add_parsed_pack(p.clone());
    }
    for (i, f) in unit.files.iter().enumerate() {
        b = b.add_file(
            format!("{}/{i}.json", unit.name),
            PackOrigin::Custom,
            f.clone(),
        );
    }
    let store = b.build();
    let codec = CatalogBlockCodec;
    let cx = FormContext::from_store(&store, &codec);
    let mut t = Tally::default();
    for f in &unit.files {
        for e in &f.elements {
            t.elements += 1;
            if excepted(unit.corpus, &e.id) {
                continue;
            }
            if let Err(m) = element_identity(e, &cx) {
                t.failures.push(format!(
                    "{} {} {} ({}): {m}",
                    unit.corpus, unit.name, e.id, e.ty
                ));
            }
        }
        for a in &f.appends {
            t.appends += 1;
            if excepted(unit.corpus, &a.id) {
                continue;
            }
            if let Err(m) = append_identity(a, &cx) {
                t.failures.push(format!(
                    "{} {} append {}: {m}",
                    unit.corpus, unit.name, a.id
                ));
            }
        }
    }
    t
}

#[test]
fn local_corpora_round_trip_through_their_forms() {
    let bundled: Vec<ParsedPack> = bundled_packs()
        .iter()
        .map(|p| ParsedPack::read(&p.dir).expect("bundled pack"))
        .collect();
    let mut report = String::new();
    let mut failures = Vec::new();
    let sources: [(&str, Source); 3] = [
        ("aurora", aurora_units),
        ("5etools", fivetools_units),
        ("homebrew-cases", case_units),
    ];
    for (corpus, units) in sources {
        let units = match units() {
            Ok(u) => u,
            Err(why) => {
                let _ = writeln!(report, "corpus {corpus}: SKIPPED ({why})");
                continue;
            }
        };
        let tallies: Vec<Tally> = std::thread::scope(|s| {
            let handles: Vec<_> = units
                .iter()
                .map(|u| s.spawn(|| check_unit(u, &bundled)))
                .collect();
            handles
                .into_iter()
                .map(|h| h.join().expect("unit"))
                .collect()
        });
        let elements: usize = tallies.iter().map(|t| t.elements).sum();
        let appends: usize = tallies.iter().map(|t| t.appends).sum();
        let failed: Vec<String> = tallies.into_iter().flat_map(|t| t.failures).collect();
        let _ = writeln!(
            report,
            "corpus {corpus}: {} units, {elements} elements, {appends} appends, {} failures",
            units.len(),
            failed.len()
        );
        let mut by_type: BTreeMap<String, usize> = BTreeMap::new();
        for f in &failed {
            let ty = f
                .rsplit_once(" (")
                .map_or("append", |(_, t)| t.split(')').next().unwrap_or(""));
            *by_type.entry(ty.to_string()).or_default() += 1;
        }
        for (ty, n) in &by_type {
            let _ = writeln!(report, "  failing {ty}: {n}");
        }
        for f in failed.iter().take(SHOWN) {
            let shown = if std::env::var_os("BOREALIS_CORPUS_DIFF").is_some() {
                usize::MAX
            } else {
                400
            };
            let _ = writeln!(
                report,
                "  FAIL {}",
                f.chars().take(shown).collect::<String>()
            );
        }
        failures.extend(failed);
    }
    println!("{report}");
    let _ = std::fs::write(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/corpus_identity.txt"),
        &report,
    );
    assert!(
        failures.is_empty(),
        "{} failures (target/corpus_identity.txt)",
        failures.len()
    );
}
